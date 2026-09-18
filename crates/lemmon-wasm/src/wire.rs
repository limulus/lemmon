//! The wire: the JSON shapes the shell and the core exchange, and the conversions
//! between them and the core's own types.
//!
//! Every call across the boundary carries its bulk data as one UTF-8 JSON string
//! (docs/architecture.md §"WASM API") — except a render's page, which crosses as bytes
//! beside a JSON header describing them ([`encode_render`]), since escaping and parsing
//! a page inside JSON costs more than the render. Each call has an input shape and each
//! result an output one. They are deliberately not the core's types: `deny_unknown_fields` makes a
//! malformed envelope a caller bug rather than a silently ignored field, and a diagnostic
//! goes out carrying both what the core works in — byte offsets — and the same two points
//! as zero-based UTF-16 LSP positions (D33), so neither `lem check` nor the language
//! server has to redo that conversion in JavaScript.
//!
//! Nothing here is `#[wasm_bindgen]`. These are plain Rust types, which is what lets the
//! boundary's tests exercise them without a `JsError` in sight.

use std::collections::BTreeMap;
use std::fmt;
use std::marker::PhantomData;
use std::rc::Rc;

use lemmon_vm::{
    Ambient, Claim, CodePayload, Data, Host, HostSpan, LineIndex, Loaded, Map, Payload,
    Position, Range, Rendered, RuntimeError, Severity, Source, Span, Spans, Value,
    repeated_path,
};
use serde::de::value::{MapAccessDeserializer, SeqAccessDeserializer};
use serde::de::{self, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};

/// A source file as `compile` receives it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FileInput {
    pub(crate) path: String,
    pub(crate) text: String,
    /// The record this file's content is, which types the host's document value in it
    /// (D30). Absent from a template, and from a document no collection matched.
    #[serde(default)]
    pub(crate) record: Option<String>,
    /// Where `record` was named — the `type:` key's value, as byte offsets into
    /// `text` — so a diagnostic about the record points at the key rather than at the
    /// front matter as a whole. The host holds the YAML, so the host says where.
    #[serde(default, rename = "recordSpan")]
    pub(crate) record_span: Option<SpanInput>,
}

/// A span as a host writes one: byte offsets into a file this call carries.
#[derive(Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SpanInput {
    start: u32,
    end: u32,
}

impl From<SpanInput> for Span {
    fn from(span: SpanInput) -> Self {
        Self::new(span.start, span.end)
    }
}

impl SpanInput {
    /// Why this span cannot be placed in `text`, if it cannot. A line index panics on
    /// an offset past the end of its text or inside a character, and a span's end
    /// before its start underflows its length in a release build, so a host's offsets
    /// are checked here, where the text is in hand, before anything keeps them.
    pub(crate) fn misplaced_in(&self, text: &str) -> Option<String> {
        let (start, end) = (self.start as usize, self.end as usize);
        if start > end {
            Some(format!("{start}..{end} that ends before it starts"))
        } else if end > text.len() {
            Some(format!(
                "{start}..{end}, past the end of a {}-byte file",
                text.len()
            ))
        } else if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            Some(format!(
                "{start}..{end} that does not fall on a character boundary"
            ))
        } else {
            None
        }
    }
}

/// The entries of a JSON object in the order they were written, each key once. A
/// declaration's names are taken in declaration order, and the loader refuses a name
/// given twice — both of which a map that sorts its keys, or keeps the last value for
/// a repeated one, would decide before the loader saw the text.
fn ordered<'de, D, V>(deserializer: D) -> Result<Vec<(String, V)>, D::Error>
where
    D: Deserializer<'de>,
    V: Deserialize<'de>,
{
    struct Ordered<V>(PhantomData<V>);

    impl<'de, V: Deserialize<'de>> Visitor<'de> for Ordered<V> {
        type Value = Vec<(String, V)>;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("an object")
        }

        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut entries = Vec::with_capacity(map.size_hint().unwrap_or(0));
            while let Some(key) = map.next_key::<String>()? {
                let value = map.next_value()?;
                entries.push((key, value));
            }
            each_once(&entries)?;
            Ok(entries)
        }
    }

    deserializer.deserialize_map(Ordered(PhantomData))
}

/// Refuses a key that `entries` holds twice.
fn each_once<V, E: de::Error>(entries: &[(String, V)]) -> Result<(), E> {
    match repeated_path(entries.iter().map(|(key, _)| key.as_str())) {
        Some(key) => Err(de::Error::custom(format!("`{key}` is given twice"))),
        None => Ok(()),
    }
}

/// The host's declaration as `compile` receives it: `{ambient, document?, entry?,
/// assets?, marks?}`, where each ambient name is bound to a declared type, to the
/// top-level bindings of a file, or to nothing at all — a name without a type (D30).
///
/// `assets` is the manifest the host's build produced, logical path to the URL it
/// shipped at. It rides here rather than on `loadData` because `asset("…")` is answered
/// while compiling (D10, D38), and being part of the declaration is what makes a
/// rebuild that re-hashes an asset a change to the declaration — which is what the
/// resident `Core` compares. `marks` rides here for the same reason (D42): a mark's name
/// and a code block's language are checked while compiling.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HostInput {
    /// In declaration order: the order the host's values load in (D37).
    #[serde(deserialize_with = "ordered")]
    pub(crate) ambient: Vec<(String, Option<AmbientBinding>)>,
    #[serde(default)]
    document: Option<String>,
    #[serde(default)]
    pub(crate) entry: Option<String>,
    /// In declaration order, like `ambient`: what a did-you-mean lists a near miss
    /// from, and the order a diagnostic shows.
    #[serde(default, deserialize_with = "ordered")]
    pub(crate) assets: Vec<(String, String)>,
    /// What the host's mark handlers claim (D42). Omitting the field declares no
    /// handlers at all, which is silence about every name rather than a warning at
    /// each — what a playground or an editor wants.
    #[serde(default)]
    marks: Option<Vec<ClaimInput>>,
}

/// One claim a host declares (D42): the name its handlers answer for, and, for a
/// producer, the keys it covers; the record a mark of the name carries; and which field
/// of that record holds a mark's key. A decorator's claim is the name alone. Several
/// claims for one name are merged, and two naming different payloads are refused.
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ClaimInput {
    name: String,
    #[serde(default)]
    keys: Option<Vec<String>>,
    #[serde(default)]
    payload: Option<String>,
    #[serde(default)]
    keyed_by: Option<String>,
}

impl ClaimInput {
    /// The claim as the core holds it.
    ///
    /// # Errors
    ///
    /// Where the claim names a key field but no payload to find it in.
    fn claim(&self) -> Result<Claim, String> {
        let mut claim = Claim::new();
        if let Some(keys) = &self.keys {
            claim = claim.with_keys(keys.iter().map(String::as_str));
        }
        match (&self.payload, &self.keyed_by) {
            (Some(payload), keyed_by) => {
                Ok(claim.with_payload(payload, keyed_by.as_deref()))
            }
            (None, Some(field)) => Err(format!(
                "the claim for `{}` keys its marks by `{field}` but names no payload to \
                 find that field in",
                self.name
            )),
            (None, None) => Ok(claim),
        }
    }
}

/// What one ambient name is bound to: a declared type by name, or the top-level
/// bindings of a file. `null` — a name the host does not type — is the `None` this sits
/// inside.
#[derive(Deserialize)]
#[serde(untagged)]
pub(crate) enum AmbientBinding {
    Named(String),
    Bindings {
        #[serde(rename = "bindingsOf")]
        bindings_of: String,
    },
}

/// # Errors
///
/// Where the host's claims cannot be taken as one declaration: a key field with no
/// payload, or two claims for one name naming different payloads or key fields.
impl TryFrom<HostInput> for Host {
    type Error = String;

    fn try_from(input: HostInput) -> Result<Self, String> {
        let mut host = Self::new();
        for (name, ambient) in input.ambient {
            host = host.bind(
                &name,
                match ambient {
                    None => Ambient::Untyped,
                    Some(AmbientBinding::Named(ty)) => Ambient::named(&ty),
                    Some(AmbientBinding::Bindings { bindings_of }) => {
                        Ambient::bindings(&bindings_of)
                    }
                },
            );
        }
        if let Some(document) = input.document {
            host = host.documents(&document);
        }
        if let Some(entry) = input.entry {
            host = host.rendered_through(&entry);
        }
        host = host.with_assets(
            input
                .assets
                .iter()
                .map(|(path, url)| (path.as_str(), url.as_str())),
        );
        match input.marks {
            Some(marks) => {
                let claims = marks
                    .iter()
                    .map(|input| Ok((input.name.as_str(), input.claim()?)))
                    .collect::<Result<Vec<_>, String>>()?;
                host.with_claims(claims)
            }
            None => Ok(host),
        }
    }
}

/// What `loadData` receives (D37): `{ambient?, sources?}`. `ambient` is bare JSON by
/// the name the host bound each value, and `sources` fulfils each `from` binding of the
/// bindings files by name — a list of entries, one per document, for a glob, and one
/// entry for a data file, a plugin, or an environment value. Which names may appear,
/// and which must, is the program's to say, so the loader checks them; what an entry
/// may hold is the wire's, so `deny_unknown_fields` does.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DataInput {
    #[serde(default, deserialize_with = "ordered")]
    ambient: Vec<(String, serde_json::Value)>,
    #[serde(default, deserialize_with = "sources")]
    sources: Vec<(String, SourceInput)>,
}

/// What fulfils one `from` binding on the wire: one entry, or a list of them.
enum SourceInput {
    One(LoadedInput),
    Many(Vec<LoadedInput>),
}

impl<'de> Deserialize<'de> for SourceInput {
    /// Read in one pass from whichever shape is there — an object is an entry, an
    /// array a list of them — rather than through an untagged enum, which would buffer
    /// the entry and report every fault in it as "no variant matched".
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Shape;

        impl<'de> Visitor<'de> for Shape {
            type Value = SourceInput;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an entry `{value, path?, spans?}` or a list of them")
            }

            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<SourceInput, A::Error> {
                LoadedInput::deserialize(MapAccessDeserializer::new(map))
                    .map(SourceInput::One)
            }

            fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<SourceInput, A::Error> {
                Vec::deserialize(SeqAccessDeserializer::new(seq)).map(SourceInput::Many)
            }
        }

        deserializer.deserialize_any(Shape)
    }
}

/// The entries of `sources`, in order and each once, with a fault in an entry named
/// for the binding it fulfils.
fn sources<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<(String, SourceInput)>, D::Error> {
    struct Sources;

    impl<'de> Visitor<'de> for Sources {
        type Value = Vec<(String, SourceInput)>;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("an object of sources by binding name")
        }

        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut entries = Vec::with_capacity(map.size_hint().unwrap_or(0));
            while let Some(name) = map.next_key::<String>()? {
                let source = map
                    .next_value::<SourceInput>()
                    .map_err(|e| de::Error::custom(format!("source `{name}`: {e}")))?;
                entries.push((name, source));
            }
            each_once(&entries)?;
            Ok(entries)
        }
    }

    deserializer.deserialize_map(Sources)
}

/// One value the host loaded, as an entry of `sources`: `{value, path?, spans?}`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LoadedInput {
    /// The value as parsed — a document's front matter with the `type:` key stripped.
    value: serde_json::Value,
    /// The file it came from: always for a document, a data file's path otherwise,
    /// and absent for a plugin or environment value.
    #[serde(default)]
    path: Option<String>,
    /// Where each field path stands in that file, spanning the key through the value;
    /// the empty path is the value as a whole.
    #[serde(default, deserialize_with = "ordered")]
    spans: Vec<(String, SpanWire)>,
}

impl DataInput {
    /// The envelope as the loader takes it. Which names it carries is left to the
    /// loader, which knows the program.
    ///
    /// # Errors
    ///
    /// A span in an entry of `sources` that ends before it starts, named for its
    /// binding and field.
    pub(crate) fn into_data(self) -> Result<Data, String> {
        let ambient = self
            .ambient
            .into_iter()
            .map(|(name, value)| (name, value_from_json(&value)))
            .collect();
        let mut sources = Vec::with_capacity(self.sources.len());
        for (name, source) in self.sources {
            let source = match source {
                SourceInput::One(entry) => Source::One(loaded(&name, entry)?),
                SourceInput::Many(entries) => Source::Many(
                    entries
                        .into_iter()
                        .map(|entry| loaded(&name, entry))
                        .collect::<Result<_, _>>()?,
                ),
            };
            sources.push((name, source));
        }
        Ok(Data { ambient, sources })
    }
}

/// One entry of `sources` under `name`, as the loader takes it.
fn loaded(name: &str, entry: LoadedInput) -> Result<Loaded, String> {
    let mut spans = Spans::new();
    for (field, span) in entry.spans {
        // The core holds no text to check these against (D37); what it can check is
        // that each span is a span, since `Span::new`'s own guard is a debug assertion
        // and an inverted range would reach an editor as sent.
        if span.start > span.end || span.range.start.after(&span.range.end) {
            let what = if field.is_empty() {
                "the value as a whole".to_owned()
            } else {
                format!("`{field}`")
            };
            return Err(format!(
                "source `{name}`: the span for {what} ends before it starts"
            ));
        }
        spans.insert(field, span.into());
    }
    Ok(Loaded {
        value: value_from_json(&entry.value),
        path: entry.path,
        spans,
    })
}

/// What `render` receives — the spike's `PageRef`: a path names the page until `plan()`
/// exists to mint richer references. An object, so those stay additive.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PageRef {
    pub(crate) path: String,
}

/// The boundary form of every diagnostic. A `code` would be an additive field; nothing
/// consumes one yet.
#[derive(Serialize)]
pub(crate) struct Diagnostic {
    pub(crate) path: String,
    pub(crate) span: SpanWire,
    severity: SeverityOut,
    message: String,
    labels: Vec<LabelOut>,
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum SeverityOut {
    Error,
    Warning,
}

impl From<Severity> for SeverityOut {
    fn from(severity: Severity) -> Self {
        match severity {
            Severity::Error => Self::Error,
            Severity::Warning => Self::Warning,
        }
    }
}

#[derive(Serialize)]
struct LabelOut {
    pub(crate) path: String,
    pub(crate) span: SpanWire,
    message: String,
}

/// A span as both of the things a consumer needs and a host supplies: the byte offsets
/// the core works in, and the same two points as LSP positions. One shape both ways —
/// what a diagnostic carries out, and what a host sends in for a file it holds and the
/// core never indexed (D37).
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SpanWire {
    start: u32,
    end: u32,
    range: RangeWire,
}

/// Zero-based line and UTF-16 `character`, exactly as the LSP defines a position — so a
/// language server can pass one straight through.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PositionWire {
    line: u32,
    character: u32,
}

impl PositionWire {
    /// Whether this position comes after `other`.
    fn after(&self, other: &Self) -> bool {
        (self.line, self.character) > (other.line, other.character)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RangeWire {
    start: PositionWire,
    end: PositionWire,
}

impl From<Position> for PositionWire {
    fn from(position: Position) -> Self {
        Self {
            line: position.line,
            character: position.character,
        }
    }
}

impl From<PositionWire> for Position {
    fn from(position: PositionWire) -> Self {
        Self {
            line: position.line,
            character: position.character,
        }
    }
}

impl From<SpanWire> for HostSpan {
    fn from(span: SpanWire) -> Self {
        Self {
            span: Span::new(span.start, span.end),
            range: Range {
                start: span.range.start.into(),
                end: span.range.end.into(),
            },
        }
    }
}

impl SpanWire {
    /// Places `span` in the file `index` describes.
    fn placed(span: lemmon_vm::Span, index: &LineIndex) -> Self {
        Self::at(span, index.range(span))
    }

    /// `span` with the positions `range` the producer already had for it.
    fn at(span: lemmon_vm::Span, range: lemmon_vm::Range) -> Self {
        Self {
            start: span.start,
            end: span.end,
            range: RangeWire {
                start: range.start.into(),
                end: range.end.into(),
            },
        }
    }

    /// `span` in the file at `path`: by the positions carried with it when the
    /// producer had them — a loader diagnostic about a file the host holds, which no
    /// index here describes (D37) — and by the file's index otherwise.
    fn carried_or_placed(
        span: lemmon_vm::Span,
        range: Option<lemmon_vm::Range>,
        path: &str,
        incoming: Option<&LineIndexes>,
        resident: &LineIndexes,
    ) -> Self {
        match range {
            Some(range) => Self::at(span, range),
            None => Self::placed(span, index_for(path, incoming, resident)),
        }
    }
}

/// A set of files' line indexes, by path — what places a span at the boundary. Shared
/// by reference, so that the set a linked program keeps is a copy of the resident one
/// in name only.
pub(crate) type LineIndexes = BTreeMap<String, Rc<LineIndex>>;

/// The index that places a span in `path`: the text of the call in progress if that call
/// carried the file, otherwise the text the file's span was produced from — the
/// resident unit's for a compile, the linked program's for a load or a render.
///
/// The two differ exactly when a recompile failed. That file keeps its previous unit, and
/// the unit's spans are offsets into the text that produced it — so a diagnostic the
/// relink raises about it has to be placed against *that* text, not the newer one at the
/// same path, which may be shorter than the span it is asked to place.
///
/// # Panics
///
/// If neither has the path. Every diagnostic names a file the core was handed and still
/// holds, so a miss is a bug in the core rather than anything a caller can provoke.
fn index_for<'a>(
    path: &str,
    incoming: Option<&'a LineIndexes>,
    resident: &'a LineIndexes,
) -> &'a LineIndex {
    incoming
        .and_then(|indexes| indexes.get(path))
        .or_else(|| resident.get(path))
        .map(|index| &**index)
        .unwrap_or_else(|| panic!("no line index for `{path}`, which a diagnostic names"))
}

impl Diagnostic {
    /// Places a diagnostic and each of its labels — a label by *its own* path, since it
    /// may point into a different file than the diagnostic does. `incoming` is the text
    /// of the call in progress, when the call carries files.
    pub(crate) fn placed(
        diagnostic: lemmon_vm::Diagnostic,
        incoming: Option<&LineIndexes>,
        resident: &LineIndexes,
    ) -> Self {
        Self {
            span: SpanWire::carried_or_placed(
                diagnostic.span,
                diagnostic.range,
                &diagnostic.path,
                incoming,
                resident,
            ),
            path: diagnostic.path,
            severity: diagnostic.severity.into(),
            message: diagnostic.message,
            labels: diagnostic
                .labels
                .into_iter()
                .map(|label| LabelOut {
                    span: SpanWire::carried_or_placed(
                        label.span,
                        label.range,
                        &label.path,
                        incoming,
                        resident,
                    ),
                    path: label.path,
                    message: label.message,
                })
                .collect(),
        }
    }

    /// A runtime failure is always an error, and the VM has one span to point at.
    pub(crate) fn placed_runtime(error: RuntimeError, resident: &LineIndexes) -> Self {
        Self {
            span: SpanWire::placed(error.span, index_for(&error.path, None, resident)),
            path: error.path,
            severity: SeverityOut::Error,
            message: error.message,
            labels: Vec::new(),
        }
    }
}

/// Everything a render returns except the page itself: the ranges the host's handlers
/// fill, what its collect blocks gathered, and the runtime diagnostic where it failed.
/// `deps` (∆xBO) is an additive field that arrives with that item.
///
/// `marks` and `fragments` are `null` exactly when the render failed, which is also when
/// there are no page bytes — an empty page is an empty section, not a missing one.
#[derive(Serialize)]
pub(crate) struct RenderMeta {
    pub(crate) marks: Option<Vec<MarkOut>>,
    pub(crate) fragments: Option<BTreeMap<String, Vec<FragmentOut>>>,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

/// One mark: the name to dispatch on, the key a producer is chosen by, what the
/// handler is given — the payload itself for a `mark()` call's, or the id of the one
/// `compile` reported for a code block — the byte range of the page it stands for, and
/// where the mark was made: `path` beside `span`, as a diagnostic carries them, so a
/// handler's own diagnostic can point at source (invariant 8). A field with nothing to
/// say is absent rather than null.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MarkOut {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload_id: Option<String>,
    start: u32,
    end: u32,
    path: String,
    span: SpanWire,
}

/// One collected fragment: its text, and the block that first gave it.
#[derive(Serialize)]
pub(crate) struct FragmentOut {
    text: String,
    path: String,
    span: SpanWire,
}

impl RenderMeta {
    /// What a render produced, placed against the files it came from.
    pub(crate) fn placed(
        rendered: &Rendered,
        files: &[String],
        resident: &LineIndexes,
    ) -> Self {
        let place = |file: u32, span: Span| {
            let path = &files[file as usize];
            (
                path.clone(),
                SpanWire::placed(span, index_for(path, None, resident)),
            )
        };
        let marks = rendered
            .marks
            .iter()
            .map(|mark| {
                let (path, span) = place(mark.file, mark.span);
                let (payload, payload_id) = match &mark.payload {
                    Some(Payload::Value(value)) => (Some(json_from_value(value)), None),
                    Some(Payload::Reported(id)) => (None, Some(id.to_string())),
                    None => (None, None),
                };
                MarkOut {
                    name: mark.name.to_string(),
                    key: mark.key.as_deref().map(str::to_owned),
                    payload,
                    payload_id,
                    start: mark.start,
                    end: mark.end,
                    path,
                    span,
                }
            })
            .collect();
        let fragments = rendered
            .fragments
            .iter()
            .map(|(name, fragments)| {
                let fragments = fragments
                    .iter()
                    .map(|fragment| {
                        let (path, span) = place(fragment.file, fragment.span);
                        FragmentOut {
                            text: fragment.text.to_string(),
                            path,
                            span,
                        }
                    })
                    .collect();
                (name.to_owned(), fragments)
            })
            .collect();
        Self {
            marks: Some(marks),
            fragments: Some(fragments),
            diagnostics: Vec::new(),
        }
    }

    /// A render that failed: its diagnostic, and no page to mark.
    pub(crate) fn failed(diagnostic: Diagnostic) -> Self {
        Self {
            marks: None,
            fragments: None,
            diagnostics: vec![diagnostic],
        }
    }
}

/// A render's reply: a four-byte little-endian length, that many bytes of `meta` as
/// JSON, then the page's UTF-8 bytes — nothing where the render failed.
///
/// The page crosses as bytes rather than inside the JSON because escaping it there and
/// parsing it back is more than half of a `render` call, and because the host splices
/// handler output at the marks' byte offsets, which a JSON string's escaping would move.
///
/// # Errors
///
/// If the metadata cannot be serialised, which is a bug in the shapes above.
pub(crate) fn encode_render(
    meta: &RenderMeta,
    html: Option<&str>,
) -> Result<Vec<u8>, String> {
    let meta = serde_json::to_vec(meta)
        .map_err(|e| format!("serialising the render result failed: {e}"))?;
    let length = u32::try_from(meta.len())
        .map_err(|_| "the render's metadata is too long to send".to_owned())?;
    let html = html.unwrap_or_default().as_bytes();
    let mut out = Vec::with_capacity(4 + meta.len() + html.len());
    out.extend_from_slice(&length.to_le_bytes());
    out.extend_from_slice(&meta);
    out.extend_from_slice(html);
    Ok(out)
}

/// A JSON value as the VM's: integral numbers become `Int` and the rest `Float`;
/// objects become insertion-ordered `Map`s in the key order the shell sent (which is
/// what a `lem-for` over a map walks). Every string arrives untrusted (`Str`) —
/// `raw()` is the only way to `html` (invariant 7).
/// A payload as the host's handler receives it (D42): what JSON has a form for as that
/// form; markup, a URL, an asset and a datetime (RFC 3339, in the offset it was written
/// in) as strings; a duration as its whole seconds; a record as the object of its
/// fields, in declared order, its type being the one the claim named.
///
/// A mark, a function, and a float that is not a finite number never reach here: the VM
/// refuses a payload holding any of them, at the call that made it.
fn json_from_value(value: &Value) -> serde_json::Value {
    use serde_json::Value as Json;
    let object = |map: &Map| {
        Json::Object(
            map.iter()
                .map(|(key, value)| (key.to_string(), json_from_value(value)))
                .collect(),
        )
    };
    match value {
        Value::Null => Json::Null,
        Value::Bool(b) => Json::Bool(*b),
        Value::Int(n) | Value::Duration(n) => Json::from(*n),
        Value::Float(f) => serde_json::Number::from_f64(*f).map_or_else(
            || unreachable!("the VM refuses a payload holding a float JSON cannot"),
            Json::Number,
        ),
        Value::Str(text) | Value::Html(text) | Value::Url(text) | Value::Asset(text) => {
            Json::String(text.to_string())
        }
        Value::Datetime(when) => Json::String(when.to_string()),
        Value::List(items) => Json::Array(items.iter().map(json_from_value).collect()),
        Value::Map(map) => object(map),
        Value::Record(record) => object(&record.fields),
        Value::Marked(_) | Value::Fn(_) => {
            unreachable!("the VM refuses a payload holding a mark or a function")
        }
    }
}

/// What `compile` returns (D42): the diagnostics about the files it was given, and what
/// their code blocks hand the host's producers — each payload once, whichever of the
/// files wrote it. ∆Qr2 adds each unit's compile-time facts beside them.
#[derive(Serialize)]
pub(crate) struct CompileOut {
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) payloads: Vec<PayloadOut>,
}

/// One code block's payload, named by the id a render's `code` mark carries.
#[derive(Serialize)]
pub(crate) struct PayloadOut {
    id: String,
    language: Option<String>,
    info: String,
    code: String,
}

impl From<&CodePayload> for PayloadOut {
    fn from(payload: &CodePayload) -> Self {
        Self {
            id: payload.id.to_string(),
            language: payload.language.as_deref().map(str::to_owned),
            info: payload.info.to_string(),
            code: payload.code.to_string(),
        }
    }
}

fn value_from_json(json: &serde_json::Value) -> Value {
    match json {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => match n.as_i64() {
            Some(i) => Value::Int(i),
            None => Value::Float(n.as_f64().expect("a JSON number reads as i64 or f64")),
        },
        serde_json::Value::String(s) => Value::str(s),
        serde_json::Value::Array(items) => {
            Value::list(items.iter().map(value_from_json).collect())
        }
        // A JSON object's keys are unique, so the map is built without the scan for a
        // repeated key that `collect` would make per entry.
        serde_json::Value::Object(entries) => {
            Value::map(Map::from_unique(entries.iter().map(|(key, value)| {
                (Rc::from(key.as_str()), value_from_json(value))
            })))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lemmon_vm::Datetime;

    fn range(line: u32, from: u32, to: u32) -> Range {
        Range {
            start: Position {
                line,
                character: from,
            },
            end: Position {
                line,
                character: to,
            },
        }
    }

    fn indexes(entries: &[(&str, &str)]) -> LineIndexes {
        entries
            .iter()
            .map(|(path, text)| ((*path).to_owned(), Rc::new(LineIndex::new(text))))
            .collect()
    }

    #[test]
    fn places_a_diagnostic_by_the_range_it_carries_when_no_index_has_its_path() {
        // A loader diagnostic names a file the host holds and the core never compiled:
        // without the carried range this would be the "no line index" panic.
        let diagnostic =
            lemmon_vm::Diagnostic::error("_data/photos.json", Span::new(12, 24), "bad")
                .placed_by_host(Some(range(1, 12, 24)))
                .with_placed_label(
                    "pages/a.md",
                    Span::new(4, 9),
                    Some(range(1, 0, 5)),
                    "here",
                );
        let placed = Diagnostic::placed(diagnostic, None, &LineIndexes::new());
        let json = serde_json::to_value(&placed).unwrap();
        assert_eq!(json["span"]["range"]["start"]["character"], 12);
        assert_eq!(json["labels"][0]["span"]["range"]["end"]["character"], 5);
    }

    #[test]
    fn places_a_diagnostic_by_its_files_index_when_it_carries_no_range() {
        let resident = indexes(&[("a.lem", "ab\ncd")]);
        let diagnostic = lemmon_vm::Diagnostic::error("a.lem", Span::new(3, 4), "bad");
        let placed = Diagnostic::placed(diagnostic, None, &resident);
        let json = serde_json::to_value(&placed).unwrap();
        assert_eq!(json["span"]["range"]["start"]["line"], 1);
        assert_eq!(json["span"]["range"]["start"]["character"], 0);
    }

    #[test]
    fn says_why_a_record_span_cannot_be_placed_in_its_text() {
        let text = "---\ntype: X\n---\nI’m\n";
        let span = |start, end| SpanInput { start, end };
        assert_eq!(span(10, 11).misplaced_in(text), None);
        assert_eq!(
            span(11, 10).misplaced_in(text).unwrap(),
            "11..10 that ends before it starts"
        );
        assert_eq!(
            span(9000, 9001).misplaced_in(text).unwrap(),
            format!("9000..9001, past the end of a {}-byte file", text.len())
        );
        // The `’` is three bytes; an offset inside it is a panic in the line index.
        assert_eq!(
            span(17, 18).misplaced_in(text).unwrap(),
            "17..18 that does not fall on a character boundary"
        );
    }

    fn data(json: &str) -> Result<Data, String> {
        serde_json::from_str::<DataInput>(json)
            .map_err(|e| e.to_string())?
            .into_data()
    }

    #[test]
    fn takes_a_list_of_entries_for_a_glob_and_one_entry_otherwise() {
        let data = data(
            r#"{"ambient": {"build": {"time": 1}}, "sources": {
                "posts": [{"path": "a.md", "value": {"title": "A"}, "spans": {
                    "title": {"start": 4, "end": 12, "range": {
                        "start": {"line": 1, "character": 0},
                        "end": {"line": 1, "character": 8}}}}}],
                "motto": {"value": "hi"}
            }}"#,
        )
        .unwrap();
        assert_eq!(
            data.ambient,
            vec![(
                "build".to_owned(),
                Value::map(Map::from_unique([(Rc::from("time"), Value::Int(1))]))
            )]
        );
        let [(posts, Source::Many(documents)), (motto, Source::One(one))] =
            data.sources.as_slice()
        else {
            panic!("{:?}", data.sources);
        };
        assert_eq!((posts.as_str(), motto.as_str()), ("posts", "motto"));
        assert_eq!(documents[0].path.as_deref(), Some("a.md"));
        let title = &documents[0].spans["title"];
        assert_eq!(title.span, Span::new(4, 12));
        assert_eq!(title.range.end.character, 8);
        assert_eq!((one.path.as_deref(), one.spans.len()), (None, 0));
        assert_eq!(one.value, Value::str("hi"));
    }

    #[test]
    fn takes_an_envelope_with_nothing_in_it() {
        let data = data("{}").unwrap();
        assert!(data.ambient.is_empty() && data.sources.is_empty());
    }

    #[test]
    fn refuses_an_entry_in_a_shape_no_source_has_naming_the_binding() {
        let unknown = data(r#"{"sources": {"posts": [{"value": 1, "nope": 2}]}}"#);
        assert!(
            unknown
                .as_ref()
                .is_err_and(|e| e.starts_with("source `posts`: unknown field `nope`")),
            "{unknown:?}"
        );
        let scalar = data(r#"{"sources": {"motto": "hi"}}"#);
        assert!(
            scalar.as_ref().is_err_and(|e| e.starts_with(
                "source `motto`: invalid type: string \"hi\", expected an entry"
            )),
            "{scalar:?}"
        );
        let inverted = data(
            r#"{"sources": {"motto": {"value": "hi", "path": "m.txt", "spans": {
                "": {"start": 2, "end": 0, "range": {
                    "start": {"line": 0, "character": 2},
                    "end": {"line": 0, "character": 0}}}}}}}"#,
        );
        assert_eq!(
            inverted.unwrap_err(),
            "source `motto`: the span for the value as a whole ends before it starts"
        );
        let backwards = data(
            r#"{"sources": {"motto": {"value": "hi", "path": "m.txt", "spans": {
                "x": {"start": 0, "end": 2, "range": {
                    "start": {"line": 1, "character": 0},
                    "end": {"line": 0, "character": 2}}}}}}}"#,
        );
        assert_eq!(
            backwards.unwrap_err(),
            "source `motto`: the span for `x` ends before it starts"
        );
    }

    #[test]
    fn refuses_a_name_given_twice_at_every_level_of_the_envelope() {
        let twice = |json: &str| data(json).unwrap_err();
        assert!(
            twice(r#"{"ambient": {"page": 1, "page": 2}}"#)
                .contains("`page` is given twice")
        );
        assert!(
            twice(r#"{"sources": {"posts": [], "posts": []}}"#)
                .contains("`posts` is given twice")
        );
        assert!(
            twice(
                r#"{"sources": {"m": {"value": 1, "path": "m", "spans": {
                    "": {"start": 0, "end": 1, "range": {
                        "start": {"line": 0, "character": 0},
                        "end": {"line": 0, "character": 1}}},
                    "": {"start": 0, "end": 1, "range": {
                        "start": {"line": 0, "character": 0},
                        "end": {"line": 0, "character": 1}}}}}}}"#
            )
            .contains("`` is given twice")
        );
        // A repeated field inside an entry is the entry's own refusal, which only a
        // single pass over the text can make.
        assert!(
            twice(r#"{"sources": {"m": {"value": 1, "value": 2}}}"#)
                .contains("duplicate field `value`")
        );
    }

    #[test]
    fn sends_a_payload_as_the_json_a_handler_reads() {
        // Each kind a payload can hold, in one record: its fields in declared order,
        // markup and URLs and assets as the strings they are, a datetime in the offset it
        // was written in, and a duration as its whole seconds.
        let fields = [
            ("none", Value::Null),
            ("yes", Value::Bool(true)),
            ("n", Value::Int(3)),
            ("x", Value::Float(1.5)),
            ("s", Value::str("a")),
            ("h", Value::html("<b>")),
            ("u", Value::url("/a/")),
            ("asset", Value::asset("/a.1.css")),
            (
                "at",
                Value::Datetime(Datetime::parse("2026-09-17T10:30:00-07:00").unwrap()),
            ),
            ("read", Value::Duration(90)),
            ("list", Value::list(vec![Value::Int(1)])),
            (
                "map",
                Value::map([(Rc::from("k"), Value::str("v"))].into_iter().collect()),
            ),
        ];
        let record = Value::record(
            "Spec",
            fields
                .into_iter()
                .map(|(name, value)| (Rc::from(name), value))
                .collect(),
        );
        assert_eq!(
            serde_json::to_string(&json_from_value(&record)).unwrap(),
            r#"{"none":null,"yes":true,"n":3,"x":1.5,"s":"a","h":"<b>","u":"/a/","asset":"/a.1.css","at":"2026-09-17T10:30:00-07:00","read":90,"list":[1],"map":{"k":"v"}}"#
        );
    }

    #[test]
    fn keeps_the_hosts_names_in_declaration_order_and_each_once() {
        let host = Host::try_from(
            serde_json::from_str::<HostInput>(
                r#"{"ambient": {"site": {"bindingsOf": "site.lem"}, "brand": null, "a": "A"}}"#,
            )
            .unwrap(),
        )
        .unwrap();
        let names: Vec<&str> = host.names().collect();
        assert_eq!(names, ["site", "brand", "a"]);
        let twice =
            serde_json::from_str::<HostInput>(r#"{"ambient": {"a": null, "a": null}}"#);
        assert!(
            twice
                .as_ref()
                .is_err_and(|e| e.to_string().contains("`a` is given twice")),
            "{:?}",
            twice.err().map(|e| e.to_string())
        );
    }

    #[test]
    fn round_trips_a_span_through_the_one_wire_shape() {
        // What a host sends in is what a diagnostic sends out, byte for byte.
        let json = r#"{"start":4,"end":12,"range":{"start":{"line":1,"character":0},"end":{"line":1,"character":8}}}"#;
        let span: SpanWire = serde_json::from_str(json).unwrap();
        let host: HostSpan = span.into();
        let out = SpanWire::at(host.span, host.range);
        assert_eq!(serde_json::to_string(&out).unwrap(), json);
    }
}

//! The wire: the JSON shapes the shell and the core exchange, and the conversions
//! between them and the core's own types.
//!
//! Every call across the boundary carries its bulk data as one UTF-8 JSON string
//! (docs/architecture.md §"WASM API"), so each has an input shape and each result has an
//! output one. They are deliberately not the core's types: `deny_unknown_fields` makes a
//! malformed envelope a caller bug rather than a silently ignored field, and a diagnostic
//! goes out carrying both what the core works in — byte offsets — and the same two points
//! as zero-based UTF-16 LSP positions (D33), so neither `lem check` nor the language
//! server has to redo that conversion in JavaScript.
//!
//! Nothing here is `#[wasm_bindgen]`. These are plain Rust types, which is what lets the
//! boundary's tests exercise them without a `JsError` in sight.

use std::collections::BTreeMap;
use std::rc::Rc;

use lemmon_vm::{Ambient, Host, LineIndex, Map, Position, RuntimeError, Severity, Value};
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

impl From<SpanInput> for lemmon_vm::Span {
    fn from(span: SpanInput) -> Self {
        Self::new(span.start, span.end)
    }
}

/// The host's declaration as `compile` receives it: `{ambient, document?, entry?}`,
/// where each ambient name is bound to a declared type, to the top-level bindings of a
/// file, or to nothing at all — a name without a type (D30).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HostInput {
    pub(crate) ambient: BTreeMap<String, Option<AmbientBinding>>,
    #[serde(default)]
    document: Option<String>,
    #[serde(default)]
    pub(crate) entry: Option<String>,
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

impl From<HostInput> for Host {
    fn from(input: HostInput) -> Self {
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
        host
    }
}

/// What `loadData` receives — the spike's `DataInput`: the ambient records only. The
/// envelope is an object so `records`, data files, and the asset manifest are additive
/// fields later.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DataInput {
    pub(crate) ambient: AmbientInput,
}

/// The three ambient records and nothing else — an unknown name here is a caller bug,
/// not data to carry silently (invariant 10).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AmbientInput {
    pub(crate) site: Option<serde_json::Value>,
    pub(crate) page: Option<serde_json::Value>,
    pub(crate) build: Option<serde_json::Value>,
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
    pub(crate) span: SpanOut,
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
    pub(crate) span: SpanOut,
    message: String,
}

/// A span as both of the things a consumer needs: the byte offsets the core works in,
/// and the two LSP positions an editor or a printed diagnostic wants.
#[derive(Serialize)]
pub(crate) struct SpanOut {
    start: u32,
    end: u32,
    range: RangeOut,
}

/// Zero-based line and UTF-16 `character`, exactly as the LSP defines a position — so a
/// language server can pass one straight through.
#[derive(Serialize)]
struct PositionOut {
    line: u32,
    character: u32,
}

#[derive(Serialize)]
struct RangeOut {
    start: PositionOut,
    end: PositionOut,
}

impl From<Position> for PositionOut {
    fn from(position: Position) -> Self {
        Self {
            line: position.line,
            character: position.character,
        }
    }
}

impl SpanOut {
    /// Places `span` in the file `index` describes.
    fn placed(span: lemmon_vm::Span, index: &LineIndex) -> Self {
        let range = index.range(span);
        Self {
            start: span.start,
            end: span.end,
            range: RangeOut {
                start: range.start.into(),
                end: range.end.into(),
            },
        }
    }
}

/// Every resident file's line index, by path — what places a span at the boundary.
pub(crate) type LineIndexes = BTreeMap<String, LineIndex>;

/// The index that places a span in `path`: the text of the call in progress if that call
/// carried the file, otherwise the text the resident unit at that path was compiled from.
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
        .unwrap_or_else(|| panic!("no line index for `{path}`, which a diagnostic names"))
}

impl Diagnostic {
    /// Places a compile diagnostic and each of its labels — a label by *its own* path,
    /// since it may point into a different file than the diagnostic does.
    pub(crate) fn placed(
        diagnostic: lemmon_vm::Diagnostic,
        incoming: &LineIndexes,
        resident: &LineIndexes,
    ) -> Self {
        let index = index_for(&diagnostic.path, Some(incoming), resident);
        Self {
            span: SpanOut::placed(diagnostic.span, index),
            path: diagnostic.path,
            severity: diagnostic.severity.into(),
            message: diagnostic.message,
            labels: diagnostic
                .labels
                .into_iter()
                .map(|label| LabelOut {
                    span: SpanOut::placed(
                        label.span,
                        index_for(&label.path, Some(incoming), resident),
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
            span: SpanOut::placed(error.span, index_for(&error.path, None, resident)),
            path: error.path,
            severity: SeverityOut::Error,
            message: error.message,
            labels: Vec::new(),
        }
    }
}

/// What `render` returns: the page's HTML, or the runtime diagnostic instead.
/// `channels` and `deps` are additive fields that arrive with `∆RGQ`.
#[derive(Serialize)]
pub(crate) struct RenderResult {
    pub(crate) html: Option<String>,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

/// A JSON value as the VM's: integral numbers become `Int` and the rest `Float`;
/// objects become insertion-ordered `Map`s in the key order the shell sent (which is
/// what a `lem-for` over a map walks). Every string arrives untrusted (`Str`) —
/// `raw()` is the only way to `html` (invariant 7).
pub(crate) fn value_from_json(json: &serde_json::Value) -> Value {
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
        serde_json::Value::Object(entries) => Value::map(
            entries
                .iter()
                .map(|(key, value)| (Rc::from(key.as_str()), value_from_json(value)))
                .collect::<Map>(),
        ),
    }
}

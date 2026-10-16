//! What the VM owes the golden corpus in `tests/corpus`.
//!
//! One expectation per `render/` case — the rendered HTML with its diagnostics trailer
//! (`.html`) — plus the structural gate that keeps the corpus honest for this stage:
//! every `Op` emitted by some case, so that an instruction cannot exist without a case
//! that exercises it.
//!
//! A `render/` or `document/` case supplies its own data with `lem-let` literals;
//! `page`/`site` are bound to small fixed records here, and named by an untyped host, so
//! that the ambient names can be exercised without a declaration. The `host/` group is
//! where a declaration is the subject: its cases compile against the environment files
//! in `host/environment/`, as a site compiles against the shell's (D30), and a document
//! there is loaded as the shell loads it (`readPage` in `@limulus/lemmon-shell`) — its
//! front matter parsed with positions, its `type:` key read, and the document handed to
//! the loader as the one member of the collection whose glob matched it (D37). The
//! harness plays the shell: it owns YAML and globs, and the core sees values and spans.
//!
//! Run `npm run corpus:bless` to rewrite the expectations from what the compiler and VM
//! currently produce, then review the diff — a case whose trailer grows lines is a
//! regression, not a new baseline.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::rc::Rc;
use std::sync::OnceLock;

use glob::Pattern;
use lemmon_corpus::Case;
use lemmon_syntax::spans::at;
use lemmon_syntax::{DIAGNOSTICS_TRAILER, FrontMatter};
use lemmon_vm::{
    Ambient, Claim, Data, FromBinding, Host, HostSpan, LineIndex, Loaded, Map, Mark,
    Module, Op, Payload, Rendered, RuntimeError, Source, SourceFile, SourceKind, Span,
    Spans, Value, compile, field_path, from_bindings, is_document, load, render,
    render_page,
};
use saphyr::{LoadableYamlNode, MarkedYaml, Scalar, YamlData};

const GROUP: &str = "render";

/// The host a `render/` or `document/` case compiles against: the three names the
/// standard shell binds, none of them typed. That is a host a site really has — the
/// Phase 0 spike's — and what any document no collection matched sees. It claims the
/// three names the shell's own bundler handles, and `code` as a highlighter does (D42),
/// so a case can place a mark under any of them, and a case that misspells a name or a
/// language warns.
fn untyped_host() -> Host {
    Host::untyped(["site", "page", "build"])
        .with_claims([
            ("styles", Claim::new()),
            ("head", Claim::new()),
            ("scripts", Claim::new()),
            // A highlighter's claim: the languages one producer covers, the record a
            // mark of the name carries, and the field it is keyed by (D42).
            (
                "code",
                Claim::new()
                    .with_keys(["bash", "css", "html", "sh"])
                    .with_payload("Code", Some("language")),
            ),
        ])
        .expect("claims that agree")
}

/// The declaration the `host/` group's environment files back: `page` a `Page`, `build`
/// a `Build`, `site` the record of the bindings `site.lem` declares, every page
/// rendered through its `layout` field and placed by its `url`.
fn declared_host() -> Host {
    declared_host_reading(SITE)
}

/// [`declared_host`], with `site` read from the bindings of the file at `bindings`.
fn declared_host_reading(bindings: &str) -> Host {
    Host::new()
        .bind("page", Ambient::named("Page"))
        .bind("build", Ambient::named("Build"))
        .bind("site", Ambient::bindings(bindings))
        .documents("page")
        .rendered_through("layout")
        .addressed_by("url")
        // The harness plays esbuild: a real shell bundles, hashes, and hands the map
        // over at `compile` (D38). Two entries are enough to show a hit, a near miss,
        // and that the URL a page carries is the hashed one and not the path asked for.
        .with_assets(ASSETS.iter().copied())
}

/// The manifest the `host/` corpus compiles against.
const ASSETS: &[(&str, &str)] = &[
    ("styles/site.css", "/assets/immutable/site.3f9a1c72.css"),
    (
        "scripts/gallery.js",
        "/assets/immutable/gallery.8b20d4e1.js",
    ),
];

const AMBIENT: &str = "host/environment/ambient.lem";
const SITE: &str = "host/environment/site.lem";

/// Where a case is a bindings file of its own: the declaration reads `site` from the
/// case in place of the environment's `site.lem`, which the compile leaves out, since a
/// `from` binding stands nowhere else.
const BINDINGS_CASES: &str = "host/bindings/";

/// The directory the environment's globs are relative to — the `host/` group is the
/// site's content root, so `pages/*.md` names `host/pages/*.md`.
const CONTENT_ROOT: &str = "host/";

/// The collections `site.lem` declares, as `(glob, element record)`: what a document
/// with no `type:` key is compiled as when a glob matches it (D30). Read from the
/// environment compiled on its own — which invariant 11 says no case can change — so
/// the record a case is compiled as cannot drift from the collection the loader files
/// it under.
fn collections() -> &'static [(String, String)] {
    static COLLECTIONS: OnceLock<Vec<(String, String)>> = OnceLock::new();
    COLLECTIONS.get_or_init(|| {
        let host = declared_host();
        let compiled = compile(&environment(), &host);
        // A broken environment is every `host/` case's problem, and each shows its
        // diagnostics; here it simply declares no collections.
        let Some(module) = compiled.output else {
            return Vec::new();
        };
        from_bindings(&module, &host)
            .into_iter()
            .filter_map(|binding| match binding {
                FromBinding {
                    kind: SourceKind::File(pattern),
                    collects: Some(element),
                    ..
                } => Some((pattern.to_string(), element.to_string())),
                _ => None,
            })
            .collect()
    })
}

/// The environment files, read from the corpus.
fn environment() -> Vec<SourceFile> {
    [AMBIENT, SITE]
        .iter()
        .map(|environment| {
            let text = fs::read_to_string(lemmon_corpus::root().join(environment))
                .unwrap_or_else(|e| panic!("read {environment}: {e}"));
            SourceFile::new(*environment, text)
        })
        .collect()
}

/// The shell's own value: what a build knows about itself.
const BUILD: &str = r#"{"time": "2026-09-08T12:00:00Z", "generator": "lemmon"}"#;

/// The data file `site.lem` loads its photo index from, as the shell would read it.
const PHOTOS: &str = r#"{
  "sunset": {"caption": "A horseshoe crab at sunset", "placement": "hero"},
  "molt": {"caption": "A shed exoskeleton", "placement": "left"}
}"#;
const PHOTOS_PATH: &str = "_data/photos.json";

/// The environment the shell passes values from, by name.
const ENV: &[(&str, &str)] = &[("SITE_URL", "https://limulus.net")];

/// The ambient records a `render/` or `document/` case may read. Deliberately short of
/// what every case names: a field missing from the data is how those cases reach a
/// runtime error at all.
fn globals() -> Vec<(&'static str, Value)> {
    let site: Map = [(Rc::from("name"), Value::str("limulus.net"))]
        .into_iter()
        .collect();
    vec![("page", page()), ("site", Value::map(site))]
}

/// The untyped `page` a case reads when no loaded document stands in for it.
fn page() -> Value {
    Value::map(
        [
            (Rc::from("url"), Value::str("/feed/the-rebound/")),
            (Rc::from("title"), Value::str("The Rebound")),
        ]
        .into_iter()
        .collect(),
    )
}

/// Renders a compiled case: its output, or the runtime error as a diagnostic line.
///
/// The output is the page with its marks shown where they stand, then each fragment it
/// collected under a `--- name` line of its own, in name order and then in the order the
/// render reached them, then under `--- payloads` every payload the page's code marks
/// name, in the order it first names each. The sections stand above the diagnostics
/// trailer, which is found by its last header, so they pass through blessing as page
/// text does.
fn rendered(
    module: &Module,
    result: Result<Rendered, RuntimeError>,
) -> (String, Option<String>) {
    match result {
        Ok(Rendered {
            html,
            marks,
            fragments,
        }) => {
            let mut out = marked(&html, &marks);
            for (name, fragments) in fragments.iter() {
                for fragment in fragments {
                    let _ = write!(out, "\n--- {name}\n{}", fragment.text);
                }
            }
            let mut named: Vec<&str> = Vec::new();
            for mark in &marks {
                if let Some(Payload::Reported(id)) = &mark.payload
                    && !named.contains(&&**id)
                {
                    named.push(id);
                }
            }
            if !named.is_empty() {
                out.push_str("\n--- payloads");
            }
            for id in named {
                let payload = module
                    .payloads
                    .iter()
                    .find(|payload| &*payload.id == id)
                    .expect("the module holds every payload a mark names");
                let language = payload
                    .language
                    .as_deref()
                    .map_or_else(|| "null".to_owned(), |word| format!("{word:?}"));
                let _ = write!(
                    out,
                    "\n#{id} {{language: {language}, info: {:?}, code: {:?}}}",
                    payload.info, payload.code
                );
            }
            (out, None)
        }
        Err(error) => (String::new(), Some(error.to_string())),
    }
}

/// The page with each mark shown where it stands: `⟦mark styles⟧` where its range is
/// empty, `⟦mark toc⟧…⟦/mark⟧` around default content. A mark's key follows its name, and
/// its payload the key, so a case shows what the host's handler is dispatched on and
/// given. Written back to front, so every offset is still the one the render gave; marks
/// at one offset keep the order they were printed in, which is the order they arrive in.
fn marked(html: &str, marks: &[Mark]) -> String {
    let mut out = html.to_owned();
    for mark in marks.iter().rev() {
        let at = |offset: u32| usize::try_from(offset).expect("an offset in this page");
        if mark.end > mark.start {
            out.insert_str(at(mark.end), "⟦/mark⟧");
        }
        let mut head = format!("⟦mark {}", mark.name);
        if let Some(key) = &mark.key {
            head.push_str(&format!(" {key}"));
        }
        match &mark.payload {
            Some(Payload::Value(value)) => {
                head.push_str(&format!(" {}", shown_value(value)))
            }
            // Named by id, which the `--- payloads` section below the page resolves.
            Some(Payload::Reported(id)) => head.push_str(&format!(" #{id}")),
            None => {}
        }
        head.push('⟧');
        out.insert_str(at(mark.start), &head);
    }
    out
}

/// A payload as the corpus shows it: what the handler is given, written the way a
/// template wrote it rather than the way the VM holds it. A record keeps its declared
/// name, since that is what the claim named.
fn shown_value(value: &Value) -> String {
    let entries = |map: &Map| {
        map.iter()
            .map(|(key, value)| format!("{key}: {}", shown_value(value)))
            .collect::<Vec<_>>()
            .join(", ")
    };
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(b) => b.to_string(),
        Value::Int(n) => n.to_string(),
        // `{:?}`, not `to_string`: `1.0` must not be blessed as `1`, or a change that
        // turned a float into an int would show nothing in the diff a reader judges.
        Value::Float(f) => format!("{f:?}"),
        Value::Duration(seconds) => format!("{seconds}s"),
        Value::Str(text) | Value::Html(text) | Value::Url(text) | Value::Asset(text) => {
            format!("{text:?}")
        }
        Value::Datetime(when) => format!("{when:?}"),
        Value::List(items) => format!(
            "[{}]",
            items.iter().map(shown_value).collect::<Vec<_>>().join(", ")
        ),
        Value::Map(map) => format!("{{{}}}", entries(map)),
        Value::Record(record) => {
            format!("{} {{{}}}", record.name, entries(&record.fields))
        }
        // Neither can be a payload: the compiler refuses a function field and the VM a
        // mark inside one, so a case that reaches here is a case about that refusal.
        Value::Marked(_) | Value::Fn(_) => format!("<{}>", value.type_name()),
    }
}

/// The output and diagnostics of a case in the shape the `.html` expectation takes:
/// the render, then `--- diagnostics` and one entry per diagnostic — warnings too — in
/// the shape `.cst` uses for the parser's.
fn shown(out: &str, diagnostics: &[String]) -> String {
    let mut shown = out.to_owned();
    let _ = writeln!(shown, "\n{DIAGNOSTICS_TRAILER}");
    for line in diagnostics {
        let _ = writeln!(shown, "{line}");
    }
    shown
}

/// A case that compiles alone, against a host that names its ambient values and types
/// none of them.
fn html(case: &Case) -> String {
    let path = case.path();
    let compiled = compile(&[SourceFile::new(&path, &case.source)], &untyped_host());
    let mut diagnostics: Vec<String> = compiled
        .diagnostics
        .iter()
        .map(ToString::to_string)
        .collect();
    let mut out = String::new();
    if let Some(module) = compiled.output {
        let (html, error) = rendered(&module, render(&module, &path, &globals()));
        out = html;
        diagnostics.extend(error);
    }
    shown(&out, &diagnostics)
}

/// A `host/` case, compiled with the environment the declaration describes — the two
/// files skipped where the case is one of them, since one compile may not name a file
/// twice, and `site.lem` skipped where the case is a bindings file of its own — then
/// loaded and rendered as the shell would: a page as the one member of its
/// collection, or on its own in `pages` when its `type:` key typed it and no glob holds
/// it, with the values the shell derives (`url`, `inputPath`) added to its front matter;
/// the shell's own `build`; and an untyped `page` for whatever is not a loaded page,
/// which is what a page no collection matched renders with. A `.lem` is a page when it
/// opens with front matter (D41), a `.md` always. A loaded page renders through its
/// record's `layout`, with its own output as the body; anything else as its own output.
fn typed_html(case: &Case) -> String {
    let path = case.path();
    let own_bindings = path.starts_with(BINDINGS_CASES);
    let host = declared_host_reading(if own_bindings { &path } else { SITE });
    let page_shaped = is_document(&path) || front_matter(&case.source).is_some();
    let document = page_shaped.then(|| PageInput::of(case));
    let mut files: Vec<SourceFile> = environment()
        .into_iter()
        .filter(|environment| {
            environment.path != path && !(own_bindings && environment.path == SITE)
        })
        .collect();
    let mut file = SourceFile::new(&path, &case.source);
    if let Some((record, span)) = document.as_ref().and_then(|d| d.record.as_ref()) {
        file = file.with_record(record);
        if let Some(span) = span {
            file = file.named_at(*span);
        }
    }
    files.push(file);
    let compiled = compile(&files, &host);
    let mut diagnostics: Vec<String> = compiled
        .diagnostics
        .iter()
        .map(ToString::to_string)
        .collect();
    let mut out = String::new();
    if let Some(module) = compiled.output {
        let data = envelope(&module, &host, &path, document.as_ref());
        let loaded =
            load(&module, &host, data).unwrap_or_else(|bug| panic!("{path}: {bug}"));
        diagnostics.extend(loaded.diagnostics.iter().map(ToString::to_string));
        if let Some(store) = loaded.output {
            let globals = store.globals(&host, &path);
            let result = match store.page(&path) {
                Some(page) => render_page(&module, page, host.entry(), &globals),
                None => render(&module, &path, &globals),
            };
            let (html, error) = rendered(&module, result);
            out = html;
            diagnostics.extend(error);
        }
    }
    shown(&out, &diagnostics)
}

/// What the shell hands `loadData` for one case: `build` and an untyped `page` as its
/// own values, every `from` binding of the environment fulfilled — the case's page as
/// the one member of the glob that matched it, the fixtures above for the rest — and,
/// in `pages`, the case's page when it has a record and no glob matched it.
fn envelope(
    module: &Module,
    host: &Host,
    path: &str,
    document: Option<&PageInput>,
) -> Data {
    let mut sources = Vec::new();
    let mut in_glob = false;
    for binding in from_bindings(module, host) {
        let source = match (&binding.kind, &binding.collects) {
            (SourceKind::File(pattern), Some(_)) => {
                let member = document
                    .filter(|_| matches(pattern, path))
                    .map(|document| document.loaded(path));
                in_glob |= member.is_some();
                Source::Many(member.into_iter().collect())
            }
            (SourceKind::File(file), None) => {
                assert_eq!(
                    **file, *PHOTOS_PATH,
                    "no fixture for the data file `{file}`"
                );
                let parsed = Parsed::of(PHOTOS, 0..PHOTOS.len(), false);
                Source::One(Loaded {
                    value: parsed.value,
                    path: Some(PHOTOS_PATH.to_owned()),
                    spans: parsed.spans,
                })
            }
            (SourceKind::Env(name), _) => {
                let value = ENV
                    .iter()
                    .find(|(key, _)| **key == **name)
                    .unwrap_or_else(|| {
                        panic!("no fixture for the environment value `{name}`")
                    })
                    .1;
                Source::One(Loaded {
                    value: Value::str(value),
                    path: None,
                    spans: Spans::new(),
                })
            }
            (SourceKind::Plugin(name), _) => panic!("no fixture for the plugin `{name}`"),
        };
        sources.push((binding.name.to_string(), source));
    }
    Data {
        ambient: vec![
            (
                "build".to_owned(),
                Parsed::of(BUILD, 0..BUILD.len(), false).value,
            ),
            ("page".to_owned(), page()),
        ],
        sources,
        pages: document
            .filter(|document| !in_glob && document.record.is_some())
            .map(|document| document.loaded(path))
            .into_iter()
            .collect(),
    }
}

/// Whether the glob `pattern`, relative to the content root, matches the case at `path`.
fn matches(pattern: &str, path: &str) -> bool {
    let relative = path
        .strip_prefix(CONTENT_ROOT)
        .unwrap_or_else(|| panic!("`{path}` is not under `{CONTENT_ROOT}`"));
    Pattern::new(pattern)
        .unwrap_or_else(|e| panic!("bad glob `{pattern}` in the environment: {e}"))
        .matches(relative)
}

/// A page case — a document, or a template with front matter — as the shell would hand
/// it over: its front matter as a value with positions, the record it is compiled as,
/// and where that was named.
struct PageInput {
    /// The front matter with the `type:` key stripped; an empty mapping without one.
    value: Value,
    spans: Spans,
    /// The record, from the `type:` key — the one raw key a host reads before a document
    /// has a type (D30), with the span of its value — or from the collection whose
    /// glob matched, which names no place in the file; else none.
    record: Option<(String, Option<Span>)>,
}

impl PageInput {
    fn of(case: &Case) -> Self {
        let path = case.path();
        let parsed = front_matter(&case.source);
        let (value, spans, mut record) = parsed.map_or_else(
            || (Value::map(Map::default()), Spans::new(), None),
            |parsed| {
                let record = parsed.record.map(|(record, span)| (record, Some(span)));
                (parsed.value, parsed.spans, record)
            },
        );
        if record.is_none() {
            record = collections()
                .iter()
                .find(|(pattern, _)| matches(pattern, &path))
                .map(|(_, element)| (element.clone(), None));
        }
        Self {
            value,
            spans,
            record,
        }
    }

    /// The page as loaded: its front matter with the values the shell derives — `url`
    /// from the path without its extension, `inputPath` the path itself — added where
    /// the front matter gave none.
    fn loaded(&self, path: &str) -> Loaded {
        let Value::Map(front) = &self.value else {
            return Loaded {
                value: self.value.clone(),
                path: Some(path.to_owned()),
                spans: self.spans.clone(),
            };
        };
        let mut entries: Vec<(Rc<str>, Value)> = front
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let stem = path
            .strip_suffix(".md")
            .or_else(|| path.strip_suffix(".lem"))
            .unwrap_or(path);
        for (key, derived) in [
            ("url", Value::str(format!("/{stem}/"))),
            ("inputPath", Value::str(path)),
        ] {
            if front.get(key).is_none() {
                entries.push((Rc::from(key), derived));
            }
        }
        Loaded {
            value: Value::map(entries.into_iter().collect()),
            path: Some(path.to_owned()),
            spans: self.spans.clone(),
        }
    }
}

/// The front matter of `source`, parsed, when it opens with a closed block — found by
/// the one rule the core applies (`lemmon_syntax::front_matter`), so the harness and
/// the parsers never disagree about which files have it. Spans are offsets into
/// `source`. An unclosed block is the parser's error, and nothing to parse here.
fn front_matter(source: &str) -> Option<Parsed> {
    match lemmon_syntax::front_matter(source) {
        FrontMatter::Closed { inner, .. } => Some(Parsed::of(
            source,
            inner.start as usize..inner.end as usize,
            true,
        )),
        FrontMatter::None | FrontMatter::Unclosed(_) => None,
    }
}

/// A YAML (or JSON) text as a value with positions, as the shell's parser hands them to
/// the core: every entry keyed by its field path, spanning the key through the value.
struct Parsed {
    value: Value,
    spans: Spans,
    /// The `type:` key's value and the span of that value, when the text is front
    /// matter and names one — read and stripped here, as the shell reads it (D30).
    record: Option<(String, Span)>,
}

impl Parsed {
    /// Parses the bytes `at` of `file`, placing every span in `file`; `front_matter`
    /// says whether a top-level `type:` key is the shell's to read and strip.
    fn of(file: &str, at: std::ops::Range<usize>, front_matter: bool) -> Self {
        let text = &file[at.clone()];
        let documents = MarkedYaml::load_from_str(text)
            .unwrap_or_else(|e| panic!("front matter does not parse: {e}"));
        let offset = u32::try_from(at.start).expect("a short file");
        let mut converter = Converter {
            index: LineIndex::new(file),
            // saphyr marks positions in characters; the core works in bytes.
            bytes: text
                .char_indices()
                .map(|(byte, _)| u32::try_from(byte).expect("a short file") + offset)
                .chain(std::iter::once(
                    u32::try_from(text.len()).expect("a short file") + offset,
                ))
                .collect(),
            spans: Spans::new(),
            record: None,
            front_matter,
        };
        let value = documents.first().map_or_else(
            || Value::map(Map::default()),
            |root| converter.convert(root, ""),
        );
        Self {
            value,
            spans: converter.spans,
            record: converter.record,
        }
    }
}

/// The walk behind [`Parsed`]: YAML nodes to values, with each entry's span recorded
/// under its field path.
struct Converter {
    index: LineIndex,
    /// Each character's byte offset in the file, and one past the last.
    bytes: Vec<u32>,
    spans: Spans,
    record: Option<(String, Span)>,
    front_matter: bool,
}

impl Converter {
    /// The file span of the characters `start..end` of the parsed text.
    fn span(&self, start: usize, end: usize) -> Span {
        Span::new(self.bytes[start], self.bytes[end])
    }

    fn place(&mut self, path: &str, start: usize, end: usize) {
        let span = self.span(start, end);
        let range = self.index.range(span);
        self.spans.insert(path.to_owned(), HostSpan { span, range });
    }

    /// `node` as a value, recording each entry's span under its field path.
    fn convert(&mut self, node: &MarkedYaml<'_>, path: &str) -> Value {
        let join = |key: &str| field_path(path, key);
        match &node.data {
            YamlData::Value(scalar) => {
                self.place(path, node.span.start.index(), node.span.end.index());
                match scalar {
                    Scalar::Null => Value::Null,
                    Scalar::Boolean(b) => Value::Bool(*b),
                    Scalar::Integer(i) => Value::Int(*i),
                    Scalar::FloatingPoint(f) => Value::Float(f.into_inner()),
                    Scalar::String(s) => Value::str(s),
                }
            }
            YamlData::Sequence(items) => {
                let values: Vec<Value> = items
                    .iter()
                    .enumerate()
                    .map(|(i, item)| self.convert(item, &join(&i.to_string())))
                    .collect();
                self.place(path, node.span.start.index(), node.span.end.index());
                Value::list(values)
            }
            YamlData::Mapping(entries) => {
                let mut map: Vec<(Rc<str>, Value)> = Vec::with_capacity(entries.len());
                let (mut first, mut last) = (usize::MAX, 0);
                for (key, value) in entries {
                    let YamlData::Value(Scalar::String(name)) = &key.data else {
                        panic!("a front-matter key is not a string: {key:?}");
                    };
                    let (start, end) = (key.span.start.index(), value.span.end.index());
                    first = first.min(start);
                    last = last.max(end);
                    // The shell reads `type:` before the document has a type, and the
                    // core never sees the key (D37); the record is named by the value,
                    // which is what a diagnostic about it points at.
                    if self.front_matter
                        && path.is_empty()
                        && &**name == "type"
                        && let YamlData::Value(Scalar::String(record)) = &value.data
                    {
                        let span =
                            self.span(value.span.start.index(), value.span.end.index());
                        self.record = Some((record.to_string(), span));
                        continue;
                    }
                    let field = join(name);
                    let converted = self.convert(value, &field);
                    // An entry spans its key through its value, whatever the value's
                    // own span was recorded as.
                    self.place(&field, start, end);
                    map.push((Rc::from(&**name), converted));
                }
                if first <= last {
                    self.place(path, first, last);
                } else {
                    self.place(path, node.span.start.index(), node.span.end.index());
                }
                Value::map(map.into_iter().collect())
            }
            other => {
                panic!("front matter uses a YAML feature the corpus does not: {other:?}")
            }
        }
    }
}

#[test]
fn renders_every_case_as_expected() {
    lemmon_corpus::check(GROUP, "html", html);
}

#[test]
fn renders_every_document_as_expected() {
    lemmon_corpus::check("document", "html", html);
}

#[test]
fn renders_every_case_against_the_declared_host_as_expected() {
    lemmon_corpus::check("host", "html", typed_html);
}

#[test]
fn places_front_matter_in_bytes_past_a_wide_character() {
    // saphyr counts characters and the core counts bytes: an `é` before a key costs
    // one of the former and two of the latter.
    let src = "---\ntitle: café\ntopic: x\n---\nbody";
    let parsed = front_matter(src).unwrap();
    let topic = parsed.spans["topic"];
    assert_eq!(topic.span, at(src, "topic: x"));
    assert_eq!(parsed.spans["title"].span, at(src, "title: café"));
    // The range is placed in the file, as the span is: `topic:` is the file's third
    // line, not the front matter's second.
    assert_eq!(
        (topic.range.start.line, topic.range.start.character),
        (2, 0)
    );
    assert_eq!(topic.range.end.character, 8);
}

#[test]
fn reads_the_record_from_the_type_keys_value_however_it_is_quoted() {
    let src = "---\ntype: \"Article\"\ntitle: x\n---\n";
    let parsed = front_matter(src).unwrap();
    assert_eq!(
        parsed.record,
        Some(("Article".to_owned(), at(src, "\"Article\"")))
    );
    assert!(!parsed.spans.contains_key("type"));
    let Value::Map(front) = &parsed.value else {
        panic!("a mapping");
    };
    assert!(front.get("type").is_none());
    let bare_src = "---\ntype: Til\n---\n";
    let bare = front_matter(bare_src).unwrap();
    assert_eq!(bare.record, Some(("Til".to_owned(), at(bare_src, "Til"))));
}

/// Every op some case compiles to. Read from the modules rather than from a trace so
/// that an op behind a branch no case takes still counts as covered by the corpus —
/// the VM's own tests cover execution paths.
fn emitted_ops() -> BTreeSet<&'static str> {
    lemmon_corpus::cases(GROUP)
        .iter()
        .filter_map(|case| {
            compile(
                &[SourceFile::new(case.path(), &case.source)],
                &untyped_host(),
            )
            .output
        })
        .flat_map(|module: Module| {
            module
                .functions
                .iter()
                .flat_map(|f| f.code.iter().map(Op::name).collect::<Vec<_>>())
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn emits_every_op_from_some_case() {
    let emitted = emitted_ops();
    let missing: Vec<&str> = Op::NAMES
        .iter()
        .copied()
        .filter(|name| !emitted.contains(name))
        .collect();
    assert!(
        missing.is_empty(),
        "no render case compiles to {}: add one",
        missing.join(", ")
    );
}

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
//! there is loaded the way the shell will load it (∆4tG) — its front matter parsed with
//! positions, its `type:` key read, and the document handed to the loader as the one
//! member of the collection whose glob matched it (D37). The harness plays the shell:
//! it owns YAML and globs, and the core sees values and spans.
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
use lemmon_vm::{
    Ambient, Data, Decls as _, Fulfilled, Host, HostSpan, LineIndex, Loaded, Map, Module,
    Op, Source, SourceFile, SourceKind, Span, Spans, Value, collects, compile, field_path,
    is_document, load, render,
};
use saphyr::{LoadableYamlNode, MarkedYaml, Scalar, YamlData};

const GROUP: &str = "render";

/// The host a `render/` or `document/` case compiles against: the three names the
/// standard shell binds, none of them typed. That is a host a site really has — the
/// Phase 0 spike's — and what any document no collection matched sees.
fn untyped_host() -> Host {
    Host::untyped(["site", "page", "build"])
}

/// The declaration the `host/` group's environment files back: `page` a `Page`, `build`
/// a `Build`, `site` the record of the bindings `site.lem` declares, and every page
/// rendered through its `layout` field.
fn declared_host() -> Host {
    Host::new()
        .bind("page", Ambient::named("Page"))
        .bind("build", Ambient::named("Build"))
        .bind("site", Ambient::bindings(SITE))
        .documents("page")
        .rendered_through("layout")
}

const AMBIENT: &str = "host/environment/ambient.lem";
const SITE: &str = "host/environment/site.lem";

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
        let mut collections = Vec::new();
        for plan in &module.bindings {
            for step in &plan.steps {
                let Fulfilled::ByHost(kind @ SourceKind::File(pattern)) = &step.fulfilled
                else {
                    continue;
                };
                let ty = module
                    .field(&plan.record, &step.name)
                    .expect("a binding is a field of its record");
                if let Some(element) = collects(&module, &host, kind, &ty) {
                    collections.push((pattern.to_string(), element.to_string()));
                }
            }
        }
        collections
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
fn rendered(
    module: &Module,
    path: &str,
    globals: &[(&str, Value)],
) -> (String, Option<String>) {
    match render(module, path, globals) {
        Ok(html) => (html, None),
        Err(error) => (String::new(), Some(error.to_string())),
    }
}

/// The output and diagnostics of a case in the shape the `.html` expectation takes:
/// the render, then `--- diagnostics` and one entry per diagnostic — warnings too — in
/// the shape `.cst` uses for the parser's.
fn shown(out: &str, diagnostics: &[String]) -> String {
    let mut shown = out.to_owned();
    let _ = writeln!(shown, "\n--- diagnostics");
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
        let (html, error) = rendered(&module, &path, &globals());
        out = html;
        diagnostics.extend(error);
    }
    shown(&out, &diagnostics)
}

/// A `host/` case, compiled with the environment the declaration describes — the two
/// files skipped where the case is one of them, since one compile may not name a file
/// twice — then loaded and rendered as the shell would: a document as the one member of
/// its collection, with the values the shell derives (`url`, `inputPath`) added to its
/// front matter; the shell's own `build`; and an untyped `page` for whatever is not a
/// loaded document, which is what a document no collection matched renders with.
fn typed_html(case: &Case) -> String {
    let path = case.path();
    let host = declared_host();
    let document = is_document(&path).then(|| Document::of(case));
    let mut files: Vec<SourceFile> = environment()
        .into_iter()
        .filter(|environment| environment.path != path)
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
            let (html, error) = rendered(&module, &path, &globals);
            out = html;
            diagnostics.extend(error);
        }
    }
    shown(&out, &diagnostics)
}

/// What the shell hands `loadData` for one case: `build` and an untyped `page` as its
/// own values, and every `from` binding of the environment fulfilled — the case's
/// document as the one member of the glob that matched it, the fixtures above for the
/// rest.
fn envelope(module: &Module, host: &Host, path: &str, document: Option<&Document>) -> Data {
    let mut sources = Vec::new();
    for plan in &module.bindings {
        for step in &plan.steps {
            let Fulfilled::ByHost(kind) = &step.fulfilled else {
                continue;
            };
            let ty = module
                .field(&plan.record, &step.name)
                .expect("a binding is a field of its record");
            let source = match kind {
                SourceKind::File(pattern)
                    if collects(module, host, kind, &ty).is_some() =>
                {
                    let member = document
                        .filter(|_| matches(pattern, path))
                        .map(|document| document.loaded(path));
                    Source::Many(member.into_iter().collect())
                }
                SourceKind::File(file) => {
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
                SourceKind::Env(name) => {
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
                SourceKind::Plugin(name) => panic!("no fixture for the plugin `{name}`"),
            };
            sources.push((step.name.to_string(), source));
        }
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

/// A document case as the shell would hand it over: its front matter as a value with
/// positions, the record it is compiled as, and where that was named.
struct Document {
    /// The front matter with the `type:` key stripped; an empty mapping without one.
    value: Value,
    spans: Spans,
    /// The record, from the `type:` key — the one raw key a host reads before a document
    /// has a type (D30), with the span of its value — or from the collection whose
    /// glob matched, which names no place in the file; else none.
    record: Option<(String, Option<Span>)>,
}

impl Document {
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

    /// The document as loaded: its front matter with the values the shell derives —
    /// `url` from the path, `inputPath` the path itself — added where the front matter
    /// gave none.
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
        let stem = path.strip_suffix(".md").unwrap_or(path);
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

/// The front matter of `source`, parsed, when it opens with a `---` line closed by
/// another. Spans are offsets into `source`, where the YAML starts after the opening
/// line.
fn front_matter(source: &str) -> Option<Parsed> {
    let body = source.strip_prefix("---\n")?;
    let end = body
        .find("\n---\n")
        .or_else(|| body.strip_suffix("\n---").map(|s| s.len()))?;
    Some(Parsed::of(source, 4..4 + end, true))
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
    let parsed = front_matter("---\ntitle: café\ntopic: x\n---\nbody").unwrap();
    let topic = parsed.spans["topic"];
    assert_eq!(topic.span, Span::new(17, 25));
    assert_eq!(parsed.spans["title"].span, Span::new(4, 16));
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
    let parsed = front_matter("---\ntype: \"Article\"\ntitle: x\n---\n").unwrap();
    assert_eq!(
        parsed.record,
        Some(("Article".to_owned(), Span::new(10, 19)))
    );
    assert!(!parsed.spans.contains_key("type"));
    let Value::Map(front) = &parsed.value else {
        panic!("a mapping");
    };
    assert!(front.get("type").is_none());
    let bare = front_matter("---\ntype: Til\n---\n").unwrap();
    assert_eq!(bare.record, Some(("Til".to_owned(), Span::new(10, 13))));
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

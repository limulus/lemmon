//! The loader behind `loadData`: what fulfils the typed content model (∆iCW, D37,
//! `docs/language.md` §Schemas, §Ambient types).
//!
//! A host hands the loader plain values — the JSON it parsed from front matter, data
//! files, plugin hooks, and its environment — with positions for the ones that have a
//! file behind them. The loader validates each against the type the program declared
//! for it, tags every record with its declared type so `is` and `lem-match` reach loaded
//! data (D35), and runs the module's plans: a record's `by` lookups and then its
//! defaults as it is built, the bindings file's computed bindings as the ambient value
//! is assembled. The plans are already in dependency order — [`link`](crate::link)
//! refused a cycle — so the loader walks each front to back and reasons about nothing.
//!
//! Two kinds of problem come out. A load diagnostic is about the data — a field of the
//! wrong type, a key no schema declares, a string no index resolves — and points at the
//! host's positions, or at the declaration of what was expected when the host had none
//! (invariant 8). A caller bug — a source no binding asked for, a document the program
//! never compiled — is an `Err`, since it has no data to point at.

mod place;
mod validate;

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use crate::bytecode::Module;
use crate::compile::{Compiled, Diagnostic, repeated_path};
use crate::decls::Decls;
use crate::host::{Ambient, Host};
use crate::plan::{Fulfilled, Read, SourceKind};
use crate::types::Type;
use crate::value::{Map, Value};
use crate::vm::invoke;

use place::{At, Place};
pub use place::{HostSpan, Spans, field_path};

/// One value the host loaded: what it parsed, the file it came from when there is
/// one, and the positions in that file (D37).
#[derive(Clone, Debug, PartialEq)]
pub struct Loaded {
    /// The value as parsed — objects as maps, every string untrusted — before any
    /// validation. A document's is its front matter with the `type:` key stripped.
    pub value: Value,
    /// The file the value came from: always for a document, which the program must
    /// have compiled; a data file's path otherwise; `None` for a plugin or environment
    /// source.
    pub path: Option<String>,
    /// Where each field path stands in that file. Empty when there is no file.
    pub spans: Spans,
}

/// What fulfils one `from` binding: one value for a data file, plugin, or environment
/// source, and one value per document for a glob.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    One(Loaded),
    Many(Vec<Loaded>),
}

/// Everything a host hands `loadData`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Data {
    /// The host's own values, by the name it bound each: `build`; `page` for a
    /// document no collection matched; whatever an untyped host names. A value the
    /// host reads from a file's bindings is built here, not carried.
    pub ambient: Vec<(String, Value)>,
    /// What fulfils each `from` binding of the bindings files, by binding name.
    pub sources: Vec<(String, Source)>,
}

/// What a load produced: what every render binds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Store {
    /// Every ambient value, by name, in the host's order: validated and tagged where
    /// the host typed it, assembled from its bindings where the host reads it from a
    /// file, and as the host sent it where the host said nothing about it.
    pub ambient: Vec<(Rc<str>, Value)>,
    /// Every loaded document's record, by the document's path — what a render of that
    /// document binds the host's document value to.
    pub documents: BTreeMap<String, Value>,
}

impl Store {
    /// What a render of the file at `path` binds: every ambient value, with the host's
    /// document value replaced by the file's own record when the file is a loaded
    /// document. The value the envelope carried under that name, if any, stands only
    /// where no loaded document does (D37).
    #[must_use]
    pub fn globals<'a>(&'a self, host: &'a Host, path: &str) -> Vec<(&'a str, Value)> {
        let mut globals: Vec<(&str, Value)> = self
            .ambient
            .iter()
            .map(|(name, value)| (&**name, value.clone()))
            .collect();
        if let Some((document, record)) = host.document().zip(self.documents.get(path)) {
            match globals.iter_mut().find(|(name, _)| *name == document) {
                Some(slot) => slot.1 = record.clone(),
                None => globals.push((document, record.clone())),
            }
        }
        globals
    }
}

/// Validates and assembles `data` against `module` for `host`.
///
/// Every load diagnostic is reported — the store comes back only when none of them is
/// an error, as a compile's output does.
///
/// # Errors
///
/// A caller bug, with no data to point at: an ambient name the host does not bind, or
/// one the host builds from a file; a source no `from` binding asked for, or one
/// fulfilled in the wrong shape; a document the program never compiled, or compiled
/// with no record.
pub fn load(module: &Module, host: &Host, data: Data) -> Result<Compiled<Store>, String> {
    let Data {
        mut ambient,
        mut sources,
    } = data;
    // The file table is a list; a load looks every document up in it, so it is
    // indexed once here rather than scanned per document.
    let files: Files<'_> = module
        .files
        .iter()
        .enumerate()
        .map(|(i, path)| (path.as_str(), i))
        .collect();
    check_envelope(module, host, &files, &ambient, &sources)?;
    let mut loader = Loader {
        module,
        host,
        files,
        document_type: host.document_type(),
        ambient: Vec::new(),
        incomplete: false,
        documents: BTreeMap::new(),
        diagnostics: Vec::new(),
    };
    loader.own_values(&mut ambient);
    for plan in &module.bindings {
        loader.bindings(plan, &mut sources);
    }
    let Loader {
        ambient,
        documents,
        diagnostics,
        ..
    } = loader;
    Ok(Compiled::new(
        Some(Store { ambient, documents }),
        diagnostics,
    ))
}

/// Refuses an envelope the program cannot be loaded from before anything is loaded,
/// so that a rejected call changes nothing and the load proper never has to ask.
fn check_envelope(
    module: &Module,
    host: &Host,
    files: &Files<'_>,
    ambient: &[(String, Value)],
    sources: &[(String, Source)],
) -> Result<(), String> {
    if let Some(name) = repeated_path(ambient.iter().map(|(name, _)| name.as_str())) {
        return Err(format!("`{name}` is carried twice"));
    }
    if let Some(name) = repeated_path(sources.iter().map(|(name, _)| name.as_str())) {
        return Err(format!("`{name}` is loaded twice"));
    }
    for (name, _) in ambient {
        match host.binding(name) {
            None => return Err(format!("the host binds no `{name}`")),
            Some(Ambient::Bindings(file)) => {
                return Err(format!(
                    "`{name}` is built from the bindings of `{file}`, not carried in the \
                     envelope"
                ));
            }
            Some(Ambient::Untyped | Ambient::Named(_)) => {}
        }
    }
    for name in host.names() {
        let Some(Ambient::Named(ty)) = host.binding(name) else {
            continue;
        };
        // A binding to a type no file declares is reported by the compile at the first
        // read of the name (D30); a program that never reads it compiles clean, and
        // only here would the type be needed.
        if module.declaration(ty).is_none() {
            return Err(format!(
                "the host binds `{name}` to `{ty}`, which no file declares"
            ));
        }
        let carried = ambient.iter().any(|(n, _)| n == name);
        if !carried && host.document() != Some(name) {
            return Err(format!(
                "the host binds `{name}`, and the envelope carries no `{name}`"
            ));
        }
    }
    let mut paths: BTreeSet<&str> = BTreeSet::new();
    for (name, source) in sources {
        let step = module
            .bindings
            .iter()
            .flat_map(|plan| plan.steps.iter().map(move |step| (plan, step)))
            .find(|(_, step)| &*step.name == name);
        let Some((plan, step)) = step else {
            return Err(format!("no `from` binding is named `{name}`"));
        };
        let Fulfilled::ByHost(kind) = &step.fulfilled else {
            return Err(format!("`{name}` is computed, not loaded"));
        };
        let ty = module
            .field(&plan.record, name)
            .expect("a binding is a field of its record");
        match (collects(module, host, kind, &ty), source) {
            (Some(_), Source::Many(documents)) => {
                for document in documents {
                    let Some(path) = document.path.as_deref() else {
                        return Err(format!("a document in `{name}` has no path"));
                    };
                    let Some(compiled) =
                        files.get(path).and_then(|&i| module.documents[i].as_ref())
                    else {
                        return Err(format!("no compiled document at `{path}`"));
                    };
                    if compiled.record.is_none() {
                        return Err(format!(
                            "`{path}` was compiled with no record, so it belongs to no \
                             collection"
                        ));
                    }
                    if !paths.insert(path) {
                        return Err(format!("`{path}` is loaded twice"));
                    }
                }
            }
            (Some(_), Source::One(_)) => {
                return Err(format!("`{name}` collects documents, so it takes a list"));
            }
            (None, Source::Many(_)) => {
                return Err(format!("`{name}` is one value, not a list"));
            }
            // Positions are placed in a file; with no file to place them in they would
            // be dropped unread, and a diagnostic about the value would point at its
            // binding instead of at the key the host had a position for.
            (None, Source::One(loaded)) => {
                if loaded.path.is_none() && !loaded.spans.is_empty() {
                    return Err(format!(
                        "`{name}` carries positions and no path to place them in"
                    ));
                }
            }
        }
    }
    for plan in &module.bindings {
        for step in &plan.steps {
            if matches!(step.fulfilled, Fulfilled::ByHost(_))
                && !sources.iter().any(|(name, _)| *name == *step.name)
            {
                return Err(format!("nothing was loaded for `{}`", step.name));
            }
        }
    }
    Ok(())
}

/// The record a `from` binding collects, when its source `kind` and type `ty` make it
/// a glob: a file source whose type is a list of the host's document type or of a
/// record extending it. Any other file source is a data file — the rule a host tells
/// the two apart by (`docs/language.md` §Ambient types), so it lives here for every
/// host to ask rather than to repeat.
#[must_use]
pub fn collects(
    module: &Module,
    host: &Host,
    kind: &SourceKind,
    ty: &Type,
) -> Option<Rc<str>> {
    let SourceKind::File(_) = kind else {
        return None;
    };
    let Type::List(element) = ty else {
        return None;
    };
    let Type::Named(record) = &**element else {
        return None;
    };
    let document = host.document_type()?;
    module.is_a(record, &document).then(|| record.clone())
}

/// The module's file table by path.
type Files<'a> = BTreeMap<&'a str, usize>;

/// The load in progress.
struct Loader<'a> {
    module: &'a Module,
    host: &'a Host,
    files: Files<'a>,
    /// The type every page-backed record extends, if the host types its documents.
    document_type: Option<Rc<str>>,
    /// The ambient values so far — what a loader-run function reads as globals. A
    /// bindings value stands here as the record built so far while its plan runs.
    ambient: Vec<(Rc<str>, Value)>,
    /// Whether a value a loader-run function may read — an ambient value, a binding —
    /// failed and is missing. No function runs past that point: it could only fault
    /// after the error already reported.
    incomplete: bool,
    documents: BTreeMap<String, Value>,
    diagnostics: Vec<Diagnostic>,
}

/// The document a record is being loaded from, for the `path` and `body` its defaults
/// take.
struct Doc<'a> {
    path: &'a str,
    body: &'a Rc<str>,
}

impl Loader<'_> {
    /// The first ambient value among `reads` that is not loaded yet — a read no
    /// function should be run over, since the loader has an order and this is not it.
    fn unloaded(&self, reads: &[Read]) -> Option<Rc<str>> {
        reads.iter().find_map(|read| match read {
            Read::Ambient { value, .. }
                if !self.ambient.iter().any(|(bound, _)| bound == value) =>
            {
                Some(value.clone())
            }
            Read::Ambient { .. } | Read::Own(_) => None,
        })
    }

    /// The ambient values as the VM takes them.
    fn globals(&self) -> Vec<(&str, Value)> {
        self.ambient
            .iter()
            .map(|(name, value)| (&**name, value.clone()))
            .collect()
    }

    /// Binds `name` to `value`, replacing what it was bound to before.
    fn bind(&mut self, name: &Rc<str>, value: Value) {
        match self.ambient.iter_mut().find(|(bound, _)| bound == name) {
            Some(slot) => slot.1 = value,
            None => self.ambient.push((name.clone(), value)),
        }
    }

    /// The host's own values, in the host's order. One the host typed is validated as
    /// that type and tagged; one it left untyped is bound as it came; its document
    /// value is bound as it came too, since the envelope's is only ever read where no
    /// loaded document stands in for it — a document no collection matched, whose
    /// `page` is unknown-typed (D30).
    fn own_values(&mut self, ambient: &mut Vec<(String, Value)>) {
        for name in self.host.names() {
            let Some(binding) = self.host.binding(name).cloned() else {
                unreachable!("`names` yields bound names");
            };
            let taken = ambient
                .iter()
                .position(|(n, _)| n == name)
                .map(|i| ambient.remove(i).1);
            let name: Rc<str> = Rc::from(name);
            match binding {
                Ambient::Bindings(_) => {}
                Ambient::Named(ty) if self.host.document() != Some(&name) => {
                    let value = taken.expect("checked by `check_envelope`");
                    let at = At::root(None, &name, self.type_place(&ty));
                    match self.value(value, &Type::Named(ty), &at, None) {
                        Some(value) => self.bind(&name, value),
                        None => self.incomplete = true,
                    }
                }
                Ambient::Untyped | Ambient::Named(_) => {
                    if let Some(value) = taken {
                        self.bind(&name, value);
                    }
                }
            }
        }
    }

    /// Runs one bindings plan: each binding in load order, the host's sources
    /// validated as the binding's type and the computed ones invoked, with the value
    /// rebuilt from the bindings so far after each — a later binding's function reads
    /// the earlier ones as fields of it.
    fn bindings(
        &mut self,
        plan: &crate::plan::Bindings,
        sources: &mut Vec<(String, Source)>,
    ) {
        let file = &self.module.files[plan.file as usize];
        let mut fields: Vec<(Rc<str>, Value)> = Vec::new();
        self.bind(&plan.value, Value::record(&plan.record, Map::default()));
        for step in &plan.steps {
            let declared = Place::resident(file, step.span);
            let ty = self
                .module
                .field(&plan.record, &step.name)
                .expect("a binding is a field of its record");
            let value = match &step.fulfilled {
                Fulfilled::ByHost(kind) => {
                    let taken = sources
                        .iter()
                        .position(|(name, _)| **name == *step.name)
                        .map(|i| sources.remove(i).1)
                        .expect("checked by `check_envelope`");
                    match (collects(self.module, self.host, kind, &ty), taken) {
                        (Some(element), Source::Many(documents)) => {
                            let mut records = Vec::with_capacity(documents.len());
                            let mut failed = false;
                            for document in documents {
                                match self.document(&document, &element, &step.name) {
                                    Some(record) => records.push(record),
                                    None => failed = true,
                                }
                            }
                            (!failed).then(|| Value::list(records))
                        }
                        (None, Source::One(loaded)) => {
                            let table = loaded.path.as_deref().map(|p| (p, &loaded.spans));
                            let at = At::root(table, &step.name, declared);
                            self.value(loaded.value.clone(), &ty, &at, None)
                        }
                        _ => unreachable!("checked by `check_envelope`"),
                    }
                }
                // A value that failed stops a computed binding as it stops a record's
                // plan: the function could only fault after the real error.
                Fulfilled::Computed { .. } if self.incomplete => None,
                Fulfilled::Computed { reads, .. } if self.unloaded(reads).is_some() => {
                    let value = self.unloaded(reads).expect("checked");
                    self.diagnostics.push(declared.error(format!(
                        "`{}` reads `{value}`, which is not loaded yet: the files the host \
                         reads bindings from load in its declaration order",
                        step.name
                    )));
                    None
                }
                Fulfilled::Computed { func, .. } => {
                    match invoke(self.module, *func, Vec::new(), &self.globals()) {
                        Ok(value) => Some(value),
                        Err(error) => {
                            self.diagnostics.push(
                                Diagnostic::error(error.path, error.span, error.message)
                                    .with_label(
                                        &declared.path,
                                        declared.span,
                                        format!("computing `{}`", step.name),
                                    ),
                            );
                            None
                        }
                    }
                }
            };
            match value {
                Some(value) => fields.push((step.name.clone(), value)),
                None => self.incomplete = true,
            }
            self.bind(&plan.value, self.assembled(&plan.record, &fields));
        }
    }

    /// The record `name` built from `fields`, in declaration order — the order the
    /// fields were computed in is the plan's business, not the value's.
    fn assembled(&self, name: &Rc<str>, fields: &[(Rc<str>, Value)]) -> Value {
        let map = Map::from_unique(self.module.fields(name).into_iter().filter_map(
            |(field, _)| {
                fields
                    .iter()
                    .find(|(name, _)| *name == field)
                    .map(|(_, value)| (field.clone(), value.clone()))
            },
        ));
        Value::record(name, map)
    }

    /// Loads one document of a glob binding collecting `element`: its front matter as
    /// the record it was compiled as, which must extend `element`, with its path and
    /// rendered prose in reach of the record's defaults. The record is kept by path for
    /// the render that binds it.
    fn document(
        &mut self,
        loaded: &Loaded,
        element: &Rc<str>,
        binding: &str,
    ) -> Option<Value> {
        let path = loaded.path.as_deref().expect("checked by `check_envelope`");
        let compiled = self.files[path];
        let compiled = self.module.documents[compiled]
            .as_ref()
            .expect("checked by `check_envelope`");
        let record = compiled
            .record
            .clone()
            .expect("checked by `check_envelope`");
        if !self.module.is_a(&record, element) {
            self.diagnostics
                .push(Place::resident(path, compiled.record_span).error(format!(
                    "`{record}` does not extend `{element}`, the type `{binding}` collects"
                )));
            return None;
        }
        let at = At::root(Some((path, &loaded.spans)), "", self.type_place(&record));
        let Value::Map(given) = &loaded.value else {
            self.diagnostics.push(at.error(format!(
                "expected the front matter as a mapping of `{record}` fields, got `{}`",
                loaded.value.type_name()
            )));
            return None;
        };
        let doc = Doc {
            path,
            body: &compiled.body,
        };
        let value = self.record(given, &record, &at, Some(&doc))?;
        self.documents.insert(path.to_owned(), value.clone());
        Some(value)
    }

    /// Where the type `name` is declared.
    fn type_place(&self, name: &str) -> Place {
        let declared = self
            .module
            .declared(name)
            .expect("a linked module declares every type it names");
        Place::resident(
            &self.module.files[declared.file as usize],
            declared.decl.name_span,
        )
    }

    /// Where the field `field` of the record `record` is declared: in the nearest
    /// declaration of its chain that has it.
    fn field_place(&self, record: &str, field: &str) -> Place {
        let (decl, found) = self
            .module
            .field_in(record, field)
            .expect("a record declares the fields it is built from");
        let file = self
            .module
            .declared_in(&decl.name)
            .expect("a declaration has a file");
        Place::resident(&self.module.files[file as usize], found.span)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use lemmon_syntax::{LineIndex, Span};

    use super::*;
    use crate::testing::{all_of, at};
    use crate::{SourceFile, compile};

    /// The standard shell's declaration: `page` a `Page`, `build` a `Build`, `site`
    /// the bindings of `site.lem`, pages rendered through `layout`.
    pub(crate) fn shell() -> Host {
        Host::new()
            .bind("page", Ambient::named("Page"))
            .bind("build", Ambient::named("Build"))
            .bind("site", Ambient::bindings("site.lem"))
            .documents("page")
            .rendered_through("layout")
    }

    pub(crate) const AMBIENT: &str = "<lem-type Page>\n  url: string\n  draft: bool = false\n</lem-type>\n\
         <lem-type Build>\n  generator: string\n</lem-type>";

    /// Compiles `files` against `host`, expecting a clean compile.
    pub(crate) fn module(host: &Host, files: &[(&str, &str)]) -> Module {
        let files: Vec<SourceFile> = files
            .iter()
            .map(|(path, text)| SourceFile::new(*path, *text))
            .collect();
        compile(&files, host).expect_output()
    }

    /// A document file, compiled as `record`.
    pub(crate) fn document(path: &str, text: &str, record: &str) -> SourceFile {
        SourceFile::new(path, text).with_record(record)
    }

    /// An object value from `entries`.
    pub(crate) fn object(entries: &[(&str, Value)]) -> Value {
        Value::map(
            entries
                .iter()
                .map(|(key, value)| (Rc::from(*key), value.clone()))
                .collect(),
        )
    }

    /// A span table over `text`: each `(path, span)` placed by a line index over it, as
    /// a host would.
    pub(crate) fn spans(text: &str, entries: &[(&str, Span)]) -> Spans {
        let index = LineIndex::new(text);
        entries
            .iter()
            .map(|(path, span)| {
                (
                    (*path).to_owned(),
                    HostSpan {
                        span: *span,
                        range: index.range(*span),
                    },
                )
            })
            .collect()
    }

    pub(crate) fn build() -> (String, Value) {
        (
            "build".to_owned(),
            object(&[("generator", Value::str("lemmon"))]),
        )
    }

    /// The diagnostics of a load, as strings.
    pub(crate) fn errors(module: &Module, host: &Host, data: Data) -> Vec<String> {
        load(module, host, data)
            .unwrap()
            .diagnostics
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    fn one(value: Value) -> Source {
        Source::One(Loaded {
            value,
            path: None,
            spans: Spans::new(),
        })
    }

    #[test]
    fn validates_a_typed_ambient_value_and_tags_it() {
        let module = module(&shell(), &[("ambient.lem", AMBIENT), ("site.lem", "")]);
        let data = Data {
            ambient: vec![build()],
            sources: Vec::new(),
        };
        let store = load(&module, &shell(), data).unwrap().expect_output();
        let names: Vec<&str> = store.ambient.iter().map(|(n, _)| &**n).collect();
        assert_eq!(names, ["build", "site"]);
        let Value::Record(build) = &store.ambient[0].1 else {
            panic!("a tagged record");
        };
        assert_eq!(&*build.ty, "Build");
        assert_eq!(build.fields.get("generator"), Some(&Value::str("lemmon")));
    }

    #[test]
    fn reports_a_typed_ambient_value_at_the_declaration_the_host_has_no_positions_for() {
        let module = module(&shell(), &[("ambient.lem", AMBIENT), ("site.lem", "")]);
        let data = Data {
            ambient: vec![(
                "build".to_owned(),
                object(&[("generator", Value::Int(3)), ("extra", Value::Null)]),
            )],
            sources: Vec::new(),
        };
        assert_eq!(
            errors(&module, &shell(), data),
            [
                format!(
                    "ambient.lem:{}: `Build` has no field `extra`",
                    at(AMBIENT, "Build")
                ),
                format!(
                    "ambient.lem:{}: expected `string` for `build.generator`, got `int`",
                    at(AMBIENT, "generator")
                ),
            ]
        );
    }

    #[test]
    fn passes_the_document_value_and_an_untyped_value_through_as_sent() {
        let module = module(&shell(), &[("ambient.lem", AMBIENT), ("site.lem", "")]);
        let page = object(&[("title", Value::str("unmatched"))]);
        let data = Data {
            ambient: vec![build(), ("page".to_owned(), page.clone())],
            sources: Vec::new(),
        };
        let store = load(&module, &shell(), data).unwrap().expect_output();
        assert_eq!(store.ambient[0].1, page);
        let untyped = Host::untyped(["site", "page"]);
        let bare = super::tests::module(&untyped, &[("a.lem", "x")]);
        let data = Data {
            ambient: vec![("site".to_owned(), Value::Int(1))],
            sources: Vec::new(),
        };
        let store = load(&bare, &untyped, data).unwrap().expect_output();
        assert_eq!(store.ambient, [(Rc::from("site"), Value::Int(1))]);
    }

    #[test]
    fn refuses_an_envelope_the_host_did_not_describe() {
        let module = module(&shell(), &[("ambient.lem", AMBIENT), ("site.lem", "")]);
        let refused = |data: Data| load(&module, &shell(), data).unwrap_err();
        assert_eq!(
            refused(Data {
                ambient: vec![build(), ("nope".to_owned(), Value::Null)],
                sources: Vec::new(),
            }),
            "the host binds no `nope`"
        );
        assert_eq!(
            refused(Data {
                ambient: vec![build(), ("site".to_owned(), Value::Null)],
                sources: Vec::new(),
            }),
            "`site` is built from the bindings of `site.lem`, not carried in the envelope"
        );
        assert_eq!(
            refused(Data::default()),
            "the host binds `build`, and the envelope carries no `build`"
        );
        assert_eq!(
            refused(Data {
                ambient: vec![build()],
                sources: vec![("posts".to_owned(), one(Value::Null))],
            }),
            "no `from` binding is named `posts`"
        );
    }

    const SITE: &str = "<lem-type Article extends Page>\n  title: string\n  layout: (Article, html): html\n</lem-type>\n\
         <lem-let posts: [Article] from \"pages/*.md\" />\n\
         <lem-let motto: string from env(\"MOTTO\") />\n\
         <lem-let count: int = 1 />";

    fn site_module() -> Module {
        let files = [
            SourceFile::new("ambient.lem", AMBIENT),
            SourceFile::new("site.lem", SITE),
            document("pages/a.md", "---\ntitle: A\n---\nbody", "Article"),
        ];
        compile(&files, &shell()).expect_output()
    }

    fn page(path: &str, value: Value) -> Loaded {
        Loaded {
            value,
            path: Some(path.to_owned()),
            spans: Spans::new(),
        }
    }

    #[test]
    fn binds_a_loaded_documents_record_under_the_name_the_host_declared() {
        let store = Store {
            ambient: vec![
                (Rc::from("build"), Value::Int(1)),
                (Rc::from("entry"), Value::str("sent")),
            ],
            documents: [("pages/a.md".to_owned(), Value::str("record"))]
                .into_iter()
                .collect(),
        };
        // The name is the host's, not `page`: the record replaces the envelope's value
        // in place, keeping the host's order.
        let host = Host::untyped(["build", "entry"]).documents("entry");
        assert_eq!(
            store.globals(&host, "pages/a.md"),
            vec![("build", Value::Int(1)), ("entry", Value::str("record"))]
        );
        // No loaded document at the path: the envelope's value stands.
        assert_eq!(
            store.globals(&host, "pages/b.md")[1],
            ("entry", Value::str("sent"))
        );
        // The envelope carried no document value: the record is bound anyway.
        let uncarried = Store {
            ambient: vec![(Rc::from("build"), Value::Int(1))],
            documents: store.documents.clone(),
        };
        assert_eq!(
            uncarried.globals(&host, "pages/a.md"),
            vec![("build", Value::Int(1)), ("entry", Value::str("record"))]
        );
        // A host that names no document value binds what was loaded and nothing else.
        assert_eq!(
            store.globals(&Host::untyped(["build", "entry"]), "pages/a.md")[1],
            ("entry", Value::str("sent"))
        );
    }

    #[test]
    fn refuses_sources_in_the_wrong_shape() {
        let module = site_module();
        let refused = |sources: Vec<(String, Source)>| {
            load(
                &module,
                &shell(),
                Data {
                    ambient: vec![build()],
                    sources,
                },
            )
            .unwrap_err()
        };
        let motto = || ("motto".to_owned(), one(Value::str("m")));
        assert_eq!(
            refused(vec![("posts".to_owned(), one(Value::Null)), motto()]),
            "`posts` collects documents, so it takes a list"
        );
        assert_eq!(
            refused(vec![
                ("posts".to_owned(), Source::Many(vec![])),
                ("motto".to_owned(), Source::Many(vec![])),
            ]),
            "`motto` is one value, not a list"
        );
        assert_eq!(
            refused(vec![("posts".to_owned(), Source::Many(vec![]))]),
            "nothing was loaded for `motto`"
        );
        assert_eq!(
            refused(vec![
                ("posts".to_owned(), Source::Many(vec![])),
                motto(),
                ("count".to_owned(), one(Value::Int(2))),
            ]),
            "`count` is computed, not loaded"
        );
        let mut nameless = page("pages/a.md", Value::Null);
        nameless.path = None;
        assert_eq!(
            refused(vec![
                ("posts".to_owned(), Source::Many(vec![nameless])),
                motto()
            ]),
            "a document in `posts` has no path"
        );
        let unplaced = Loaded {
            value: Value::str("m"),
            path: None,
            spans: spans("m", &[("", all_of("m"))]),
        };
        assert_eq!(
            refused(vec![
                ("posts".to_owned(), Source::Many(vec![])),
                ("motto".to_owned(), Source::One(unplaced)),
            ]),
            "`motto` carries positions and no path to place them in"
        );
        assert_eq!(
            refused(vec![
                (
                    "posts".to_owned(),
                    Source::Many(vec![page("pages/b.md", Value::Null)])
                ),
                motto(),
            ]),
            "no compiled document at `pages/b.md`"
        );
        let twice = vec![
            page("pages/a.md", object(&[])),
            page("pages/a.md", object(&[])),
        ];
        assert_eq!(
            refused(vec![("posts".to_owned(), Source::Many(twice)), motto()]),
            "`pages/a.md` is loaded twice"
        );
    }

    #[test]
    fn refuses_a_document_compiled_with_no_record() {
        let files = [
            SourceFile::new("ambient.lem", AMBIENT),
            SourceFile::new("site.lem", SITE),
            SourceFile::new("pages/a.md", "body"),
        ];
        let module = compile(&files, &shell()).expect_output();
        let data = Data {
            ambient: vec![build()],
            sources: vec![
                (
                    "posts".to_owned(),
                    Source::Many(vec![page("pages/a.md", object(&[]))]),
                ),
                ("motto".to_owned(), one(Value::str("m"))),
            ],
        };
        assert_eq!(
            load(&module, &shell(), data).unwrap_err(),
            "`pages/a.md` was compiled with no record, so it belongs to no collection"
        );
    }

    #[test]
    fn assembles_the_bindings_value_in_declaration_order_and_keeps_documents_by_path() {
        let module = site_module();
        let text = "---\ntitle: A\n---\nbody";
        let front = object(&[
            ("title", Value::str("A")),
            ("layout", Value::str("f")),
            ("url", Value::str("/a/")),
        ]);
        let files = [
            SourceFile::new("ambient.lem", AMBIENT),
            SourceFile::new(
                "site.lem",
                format!(
                    "{SITE}<lem-fn f(a: Article, body: html)><lem.a.title><lem.body></lem-fn>"
                ),
            ),
            document("pages/a.md", text, "Article"),
        ];
        let module_with_layout = compile(&files, &shell()).expect_output();
        drop(module);
        let data = Data {
            ambient: vec![build()],
            sources: vec![
                ("motto".to_owned(), one(Value::str("m"))),
                (
                    "posts".to_owned(),
                    Source::Many(vec![Loaded {
                        value: front,
                        path: Some("pages/a.md".to_owned()),
                        spans: spans(text, &[("", at(text, "title: A"))]),
                    }]),
                ),
            ],
        };
        let store = load(&module_with_layout, &shell(), data)
            .unwrap()
            .expect_output();
        let Value::Record(site) = &store.ambient[1].1 else {
            panic!("site is a record");
        };
        assert_eq!(&*site.ty, "site.lem");
        let fields: Vec<&str> = site.fields.iter().map(|(k, _)| &**k).collect();
        assert_eq!(fields, ["posts", "motto", "count"]);
        assert_eq!(site.fields.get("count"), Some(&Value::Int(1)));
        let Some(Value::List(posts)) = site.fields.get("posts") else {
            panic!("posts is a list");
        };
        assert_eq!(posts.len(), 1);
        assert_eq!(store.documents.get("pages/a.md"), Some(&posts[0]));
        let Value::Record(article) = &posts[0] else {
            panic!("a tagged record");
        };
        assert_eq!(&*article.ty, "Article");
        assert_eq!(article.fields.get("draft"), Some(&Value::Bool(false)));
        assert!(matches!(article.fields.get("layout"), Some(Value::Fn(_))));
    }

    #[test]
    fn reports_a_document_whose_record_is_not_what_the_binding_collects() {
        const NOTE: &str = "---\ntype: Note\n---\n";
        let site = format!(
            "{SITE}<lem-type Note extends Page>\n  layout: (Note, html): html\n</lem-type>"
        );
        let files = [
            SourceFile::new("ambient.lem", AMBIENT),
            SourceFile::new("site.lem", &site),
            document("pages/n.md", NOTE, "Note").named_at(at(NOTE, "Note")),
        ];
        let module = compile(&files, &shell()).expect_output();
        let data = Data {
            ambient: vec![build()],
            sources: vec![
                ("motto".to_owned(), one(Value::str("m"))),
                (
                    "posts".to_owned(),
                    Source::Many(vec![page("pages/n.md", object(&[]))]),
                ),
            ],
        };
        assert_eq!(
            errors(&module, &shell(), data),
            [format!(
                "pages/n.md:{}: `Note` does not extend `Article`, the type `posts` \
                 collects",
                at(NOTE, "Note")
            )]
        );
    }

    #[test]
    fn reports_front_matter_that_is_no_mapping() {
        let module = site_module();
        let data = Data {
            ambient: vec![build()],
            sources: vec![
                ("motto".to_owned(), one(Value::str("m"))),
                (
                    "posts".to_owned(),
                    Source::Many(vec![page("pages/a.md", Value::list(vec![]))]),
                ),
            ],
        };
        assert_eq!(
            errors(&module, &shell(), data),
            [format!(
                "site.lem:{}: expected the front matter as a mapping of `Article` \
                 fields, got `list`",
                at(SITE, "<lem-type «Article» extends")
            )]
        );
    }

    #[test]
    fn runs_a_computed_binding_after_what_it_reads_and_reports_its_fault() {
        let site = "<lem-let b: int = a + 1 />\n<lem-let a: int = 1 />\n\
                    <lem-let c: string = [\"x\"][a + 1] />";
        let module = module(&shell(), &[("ambient.lem", AMBIENT), ("site.lem", site)]);
        let data = Data {
            ambient: vec![build()],
            sources: Vec::new(),
        };
        let result = load(&module, &shell(), data).unwrap();
        let shown: Vec<String> =
            result.diagnostics.iter().map(ToString::to_string).collect();
        assert_eq!(
            shown,
            [format!(
                "site.lem:{}: index 2 is out of range for a list of 1\n  \
                 site.lem:{}: computing `c`",
                at(site, "[\"x\"][a + 1]"),
                at(site, "c")
            )]
        );
        assert!(result.output.is_none());
    }

    #[test]
    fn loads_a_data_file_binding_at_the_hosts_positions() {
        let site = "<lem-type Photo>\n  caption: string\n</lem-type>\n\
                    <lem-let photos: {string: Photo} from \"_data/photos.json\" />";
        let module = module(&shell(), &[("ambient.lem", AMBIENT), ("site.lem", site)]);
        let json = "{\"sunset\": {\"caption\": 3}}";
        let data = Data {
            ambient: vec![build()],
            sources: vec![(
                "photos".to_owned(),
                Source::One(Loaded {
                    value: object(&[("sunset", object(&[("caption", Value::Int(3))]))]),
                    path: Some("_data/photos.json".to_owned()),
                    spans: spans(
                        json,
                        &[
                            ("", all_of(json)),
                            ("sunset", at(json, "\"sunset\": {\"caption\": 3}")),
                            ("sunset.caption", at(json, "\"caption\": 3")),
                        ],
                    ),
                }),
            )],
        };
        let result = load(&module, &shell(), data).unwrap();
        assert_eq!(
            result.diagnostics[0].to_string(),
            format!(
                "_data/photos.json:{}: expected `string` for `photos.sunset.caption`, \
                 got `int`",
                at(json, "\"caption\": 3")
            )
        );
        assert_eq!(
            result.diagnostics[0]
                .range
                .map(|r| (r.start.character, r.end.character)),
            Some((12, 24))
        );
    }

    #[test]
    fn refuses_a_name_carried_twice_and_a_host_type_no_file_declares() {
        let module = module(&shell(), &[("ambient.lem", AMBIENT), ("site.lem", "")]);
        let twice = Data {
            ambient: vec![build(), build()],
            sources: Vec::new(),
        };
        assert_eq!(
            load(&module, &shell(), twice).unwrap_err(),
            "`build` is carried twice"
        );
        // `Build` is bound but never read, so the compile had nothing to report; the
        // loader is the first to need the declaration.
        let unread = module_of_untyped_build();
        let data = Data {
            ambient: vec![build()],
            sources: Vec::new(),
        };
        assert_eq!(
            load(&unread, &shell(), data).unwrap_err(),
            "the host binds `build` to `Build`, which no file declares"
        );
    }

    fn module_of_untyped_build() -> Module {
        let ambient = "<lem-type Page>\n  url: string\n</lem-type>";
        module(&shell(), &[("ambient.lem", ambient), ("site.lem", "")])
    }

    #[test]
    fn refuses_a_source_named_twice() {
        let module = site_module();
        let data = Data {
            ambient: vec![build()],
            sources: vec![
                ("motto".to_owned(), one(Value::str("m"))),
                ("motto".to_owned(), one(Value::str("m"))),
                ("posts".to_owned(), Source::Many(vec![])),
            ],
        };
        assert_eq!(
            load(&module, &shell(), data).unwrap_err(),
            "`motto` is loaded twice"
        );
    }

    #[test]
    fn skips_a_computed_binding_once_an_earlier_value_failed() {
        // `count` reads nothing that failed, but the load has already failed: running
        // it could add nothing but a fault after the real error.
        let module = site_module();
        let data = Data {
            ambient: vec![("build".to_owned(), object(&[("generator", Value::Int(1))]))],
            sources: vec![
                ("motto".to_owned(), one(Value::str("m"))),
                ("posts".to_owned(), Source::Many(vec![])),
            ],
        };
        assert_eq!(
            errors(&module, &shell(), data),
            [format!(
                "ambient.lem:{}: expected `string` for `build.generator`, got `int`",
                at(AMBIENT, "generator")
            )]
        );
    }
}

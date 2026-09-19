//! The loader behind `loadData`: what fulfils the typed content model (∆iCW, D37,
//! `docs/language.md` §Schemas, §Ambient types).
//!
//! A host hands the loader plain values — the JSON it parsed from front matter, data
//! files, plugin hooks, and its environment — with positions for the ones that have a
//! file behind them. The loader validates each against the type the program declared
//! for it, tags every record with its declared type so `is` and `lem-match` reach loaded
//! data (D35), and runs the module's plans: a record's `by` lookups and then its
//! defaults as it is built, the bindings file's computed bindings as the ambient value
//! is assembled. The plans are already in dependency order — [`link`](crate::link())
//! refused a cycle — so the loader walks each front to back and reasons about nothing.
//!
//! Two kinds of problem come out. A load diagnostic is about the data — a field of the
//! wrong type, a key no schema declares, a string no index resolves — and points at the
//! host's positions, or at the declaration of what was expected when the host had none
//! (invariant 8). A caller bug — a source no binding asked for, a page the program
//! never compiled — is an `Err`, since it has no data to point at.

mod place;
mod validate;

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use lemmon_syntax::Span;

use crate::bytecode::Module;
use crate::decls::Decls;
use crate::diagnostics::{Compiled, Diagnostic};
use crate::host::{Ambient, Host};
use crate::plan::{Fulfilled, SourceKind};
use crate::repeated_path;
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
    /// validation. A page's is its front matter with the `type:` key stripped.
    pub value: Value,
    /// The file the value came from: always for a page, which the program must have
    /// compiled; a data file's path otherwise; `None` for a plugin or environment
    /// source.
    pub path: Option<String>,
    /// Where each field path stands in that file. Empty when there is no file.
    pub spans: Spans,
}

/// What fulfils one `from` binding: one value for a data file, plugin, or environment
/// source, and one value per page for a glob — a document, or a template with front
/// matter (D41).
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
    /// The front matter of every page no glob holds — a document or template whose
    /// `type:` key named its record — each with its path. Loaded after every binding,
    /// in this order, so a `by` index in one may read a map the site loaded.
    pub pages: Vec<Loaded>,
}

/// What a load produced: what every render binds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Store {
    /// Every ambient value, by name, in the host's order: validated and tagged where
    /// the host typed it, assembled from its bindings where the host reads it from a
    /// file, and as the host sent it where the host said nothing about it.
    pub ambient: Vec<(Rc<str>, Value)>,
    /// Every loaded page, in load order: each collection's members as its binding
    /// loaded them, the bindings in their link order, then the pages the envelope
    /// carried on their own, in its order.
    pub pages: Vec<LoadedPage>,
}

/// One page as loaded: its path, the record its front matter was loaded as — what a
/// render of the page binds the host's document value to — and where a diagnostic
/// about it after the load can point.
#[derive(Clone, Debug, PartialEq)]
pub struct LoadedPage {
    pub path: String,
    pub record: Value,
    /// Where each field path of its front matter stands in the file (D37).
    pub spans: Spans,
    /// Where its record was named, for what the table cannot place — a field a default
    /// derived.
    pub record_span: Span,
}

/// One page as `plan()` lists it (D43): its path, and its url where the host names the
/// field it is read from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedPage {
    pub path: String,
    pub url: Option<String>,
}

impl Store {
    /// The loaded page at `path`, if one is.
    #[must_use]
    pub fn page(&self, path: &str) -> Option<&LoadedPage> {
        self.pages.iter().find(|page| page.path == path)
    }

    /// Every loaded page in load order, each with the url its record holds in the
    /// field `host` names — none where the host names no field.
    #[must_use]
    pub fn plan(&self, host: &Host) -> Vec<PlannedPage> {
        self.pages
            .iter()
            .map(|page| PlannedPage {
                path: page.path.clone(),
                url: host
                    .url()
                    .and_then(|field| page_url(page, field))
                    .map(ToString::to_string),
            })
            .collect()
    }

    /// What a render of the file at `path` binds: every ambient value, with the host's
    /// document value replaced by the file's own record when the file is a loaded
    /// page. The value the envelope carried under that name, if any, stands only
    /// where no loaded page does (D37).
    #[must_use]
    pub fn globals<'a>(&'a self, host: &'a Host, path: &str) -> Vec<(&'a str, Value)> {
        let mut globals: Vec<(&str, Value)> = self
            .ambient
            .iter()
            .map(|(name, value)| (&**name, value.clone()))
            .collect();
        if let Some((document, page)) = host.document().zip(self.page(path)) {
            match globals.iter_mut().find(|(name, _)| *name == document) {
                Some(slot) => slot.1 = page.record.clone(),
                None => globals.push((document, page.record.clone())),
            }
        }
        globals
    }
}

/// The url of a loaded page: what the field `field` of its record holds, when that is
/// a url. The link step saw the document type declare the field as `url`, and a load
/// validated the record against it, so a loaded page without one is a bug rather than
/// an author's mistake — which is why this answers `None` and its callers say so, and
/// none of them `expect` (invariant 8).
fn page_url<'a>(page: &'a LoadedPage, field: &str) -> Option<&'a Rc<str>> {
    let Value::Record(record) = &page.record else {
        return None;
    };
    match record.fields.get(field) {
        Some(Value::Url(url)) => Some(url),
        _ => None,
    }
}

/// Where a diagnostic about the url of `page` goes: the table's entry for `field`,
/// else the page's own value, else where its record was named — as for a url a
/// default derived, which no key in the file gave.
fn url_place(page: &LoadedPage, field: &str) -> Place {
    let declared = Place::resident(&page.path, page.record_span);
    At::root(Some((&page.path, &page.spans)), "", declared.clone())
        .child(field, declared)
        .place()
}

/// Every loaded page checked against the others for the url in `field` (D43): one url
/// is one page, so two at one url is an error at the second page's url, labelled at
/// the first's. A page whose record holds no url is an error too, at the same place.
fn url_diagnostics(pages: &[LoadedPage], field: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut first: BTreeMap<&str, &LoadedPage> = BTreeMap::new();
    for page in pages {
        let Some(url) = page_url(page, field) else {
            diagnostics.push(url_place(page, field).error(format!(
                "`{}` holds no url in `{field}`, so it stands nowhere",
                page.path
            )));
            continue;
        };
        match first.get(&**url) {
            None => {
                first.insert(url, page);
            }
            Some(taken) => {
                let there = url_place(taken, field);
                diagnostics.push(
                    url_place(page, field)
                        .error(format!(
                            "`{}` and `{}` are both at `{url}`",
                            page.path, taken.path
                        ))
                        .with_placed_label(
                            there.path,
                            there.span,
                            there.range,
                            "the first page at this url",
                        ),
                );
            }
        }
    }
    diagnostics
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
/// fulfilled in the wrong shape; a page the program never compiled, or compiled with
/// no record, or carried twice.
pub fn load(module: &Module, host: &Host, data: Data) -> Result<Compiled<Store>, String> {
    let Data {
        mut ambient,
        mut sources,
        pages,
    } = data;
    // The file table is a list; a load looks every page up in it, so it is indexed
    // once here rather than scanned per page.
    let files: Files<'_> = module
        .files
        .iter()
        .enumerate()
        .map(|(i, path)| (path.as_str(), i))
        .collect();
    check_envelope(module, host, &files, &ambient, &sources, &pages)?;
    let mut loader = Loader {
        module,
        host,
        files,
        document_type: host.document_type(),
        ambient: Vec::new(),
        incomplete: false,
        pages: Vec::new(),
        diagnostics: Vec::new(),
    };
    loader.own_values(&mut ambient);
    for plan in &module.bindings {
        loader.bindings(plan, &mut sources);
    }
    // A page no glob holds is still a page of the site — it is loaded like a member,
    // after every binding, since its `by` indexes may read maps the site loaded.
    for loaded in pages {
        loader.page(loaded, None);
    }
    if let Some(field) = host.url() {
        let repeated = url_diagnostics(&loader.pages, field);
        loader.diagnostics.extend(repeated);
    }
    let Loader {
        ambient,
        pages,
        diagnostics,
        ..
    } = loader;
    Ok(Compiled::new(Some(Store { ambient, pages }), diagnostics))
}

/// Refuses an envelope the program cannot be loaded from before anything is loaded,
/// so that a rejected call changes nothing and the load proper never has to ask.
fn check_envelope(
    module: &Module,
    host: &Host,
    files: &Files<'_>,
    ambient: &[(String, Value)],
    sources: &[(String, Source)],
    pages: &[Loaded],
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
    // Every page the envelope carries, in a glob or on its own: a page is loaded once,
    // since one path is one render, and there is one record it binds.
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
            (Some(_), Source::Many(members)) => {
                for member in members {
                    check_page(module, files, &mut paths, member, name)?;
                }
            }
            (Some(_), Source::One(_)) => {
                return Err(format!("`{name}` collects pages, so it takes a list"));
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
    for loaded in pages {
        check_page(module, files, &mut paths, loaded, "pages")?;
    }
    Ok(())
}

/// Refuses one page of the envelope — carried in `holder`, a glob binding or `pages`
/// — that cannot be loaded: it has no path, nothing page-shaped was compiled there,
/// what was compiled has no record, or the same path was carried before.
fn check_page<'a>(
    module: &Module,
    files: &Files<'_>,
    paths: &mut BTreeSet<&'a str>,
    loaded: &'a Loaded,
    holder: &str,
) -> Result<(), String> {
    let Some(path) = loaded.path.as_deref() else {
        return Err(format!("a page in `{holder}` has no path"));
    };
    let Some(compiled) = files.get(path).and_then(|&i| module.documents[i].as_ref()) else {
        return Err(format!("no compiled page at `{path}`"));
    };
    if compiled.record.is_none() {
        return Err(format!(
            "`{path}` was compiled with no record, so it is no page to load"
        ));
    }
    if !paths.insert(path) {
        return Err(format!("`{path}` is loaded twice"));
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
    /// Every page loaded so far, in load order.
    pages: Vec<LoadedPage>,
    diagnostics: Vec<Diagnostic>,
}

/// The page a record is being loaded from, for the `path` and `body` its defaults
/// take. A page template has no body until it renders — `None`, which the link step
/// keeps out of every default's reach by refusing a template whose record reads it.
struct Doc<'a> {
    path: &'a str,
    body: Option<&'a Rc<str>>,
}

impl Loader<'_> {
    /// The first of the ambient values `reads` that is not loaded yet — a read no
    /// function should be run over, since the loader has an order and this is not it.
    fn unloaded(&self, reads: &[Rc<str>]) -> Option<Rc<str>> {
        reads
            .iter()
            .find(|value| !self.ambient.iter().any(|(bound, _)| bound == *value))
            .cloned()
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
                        (Some(element), Source::Many(members)) => {
                            let mut records = Vec::with_capacity(members.len());
                            let mut failed = false;
                            for member in members {
                                match self.page(member, Some((&element, &step.name))) {
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
                Fulfilled::Computed { func, reads } => match self.unloaded(reads) {
                    Some(value) => {
                        self.diagnostics.push(declared.error(format!(
                            "`{}` reads `{value}`, which is not loaded yet: the files the \
                             host reads bindings from load in its declaration order",
                            step.name
                        )));
                        None
                    }
                    None => match invoke(self.module, *func, Vec::new(), &self.globals()) {
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
                    },
                },
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

    /// Loads one page: its front matter as the record it was compiled as, with its
    /// path and, for a document, its rendered prose in reach of the record's defaults.
    /// A member of a glob binding's `collection` — the element type it collects and
    /// the binding's name — must be of a record extending that type. The record is
    /// kept, in load order, for the render that binds it, with the positions the host
    /// sent for what is said about the page after it loaded.
    fn page(
        &mut self,
        loaded: Loaded,
        collection: Option<(&Rc<str>, &str)>,
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
        if let Some((element, binding)) = collection
            && !self.module.is_a(&record, element)
        {
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
            body: compiled.body.as_ref(),
        };
        let value = self.record(given, &record, &at, Some(&doc))?;
        self.pages.push(LoadedPage {
            path: path.to_owned(),
            record: value.clone(),
            spans: loaded.spans,
            record_span: compiled.record_span,
        });
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
    use lemmon_syntax::spans::{all_of, at};
    use lemmon_syntax::{LineIndex, Span};

    use super::*;
    use crate::rendered::{Mark, Marked};
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
    fn takes_marked_markup_where_a_host_supplies_html() {
        // A Rust host hands the loader `Value`s, and a render's own output is among what
        // it may hand back: marked markup is `html` like any other (D42), and nothing
        // about carrying a mark makes it another type.
        const MARKED: &str = "<lem-type Page>\n  url: string\n</lem-type>\n\
                              <lem-type Build>\n  banner: html\n</lem-type>";
        let module = module(&shell(), &[("ambient.lem", MARKED), ("site.lem", "")]);
        let banner = Value::Marked(Rc::new(Marked {
            text: Rc::from("<b>hi</b>"),
            marks: Box::from([Mark {
                name: Rc::from("head"),
                key: None,
                payload: None,
                start: 0,
                end: 9,
                file: 0,
                span: Span::new(0, 1),
            }]),
        }));
        let data = Data {
            ambient: vec![("build".to_owned(), object(&[("banner", banner.clone())]))],
            sources: Vec::new(),
            pages: Vec::new(),
        };
        let store = load(&module, &shell(), data).unwrap().expect_output();
        let Value::Record(build) = &store.ambient[0].1 else {
            panic!("a tagged record");
        };
        assert_eq!(build.fields.get("banner"), Some(&banner));
    }

    #[test]
    fn validates_a_typed_ambient_value_and_tags_it() {
        let module = module(&shell(), &[("ambient.lem", AMBIENT), ("site.lem", "")]);
        let data = Data {
            ambient: vec![build()],
            sources: Vec::new(),
            pages: Vec::new(),
        };
        let store = load(&module, &shell(), data).unwrap().expect_output();
        let names: Vec<&str> = store.ambient.iter().map(|(n, _)| &**n).collect();
        assert_eq!(names, ["build", "site"]);
        let Value::Record(build) = &store.ambient[0].1 else {
            panic!("a tagged record");
        };
        assert_eq!(&*build.name, "Build");
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
            pages: Vec::new(),
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
            pages: Vec::new(),
        };
        let store = load(&module, &shell(), data).unwrap().expect_output();
        assert_eq!(store.ambient[0].1, page);
        let untyped = Host::untyped(["site", "page"]);
        let bare = super::tests::module(&untyped, &[("a.lem", "x")]);
        let data = Data {
            ambient: vec![("site".to_owned(), Value::Int(1))],
            sources: Vec::new(),
            pages: Vec::new(),
        };
        let store = load(&bare, &untyped, data).unwrap().expect_output();
        assert_eq!(store.ambient, [(Rc::from("site"), Value::Int(1))]);
    }

    #[test]
    fn reports_a_computed_binding_that_reads_a_later_bindings_file_before_running_it() {
        // The files the host reads bindings from load in its declaration order; a
        // read of a later one, through a call or not, is refused before the function
        // could fault over the value's absence.
        let host = Host::new()
            .bind("site", Ambient::bindings("site.lem"))
            .bind("data", Ambient::bindings("data.lem"));
        let site = "<lem-let tag: string = motto() />\n\
                    <lem-fn motto(): string = data.motto />";
        let files = [
            ("site.lem", site),
            ("data.lem", "<lem-let motto: string = \"m\" />"),
        ];
        let module = module(&host, &files);
        let data = Data {
            ambient: Vec::new(),
            sources: Vec::new(),
            pages: Vec::new(),
        };
        assert_eq!(
            errors(&module, &host, data),
            [format!(
                "site.lem:{}: `tag` reads `data`, which is not loaded yet: the files the \
                 host reads bindings from load in its declaration order",
                at(site, "tag")
            )]
        );
    }

    #[test]
    fn refuses_an_envelope_the_host_did_not_describe() {
        let module = module(&shell(), &[("ambient.lem", AMBIENT), ("site.lem", "")]);
        let refused = |data: Data| load(&module, &shell(), data).unwrap_err();
        assert_eq!(
            refused(Data {
                ambient: vec![build(), ("nope".to_owned(), Value::Null)],
                sources: Vec::new(),
                pages: Vec::new(),
            }),
            "the host binds no `nope`"
        );
        assert_eq!(
            refused(Data {
                ambient: vec![build(), ("site".to_owned(), Value::Null)],
                sources: Vec::new(),
                pages: Vec::new(),
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
                pages: Vec::new(),
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
    fn binds_a_loaded_pages_record_under_the_name_the_host_declared() {
        let store = Store {
            ambient: vec![
                (Rc::from("build"), Value::Int(1)),
                (Rc::from("entry"), Value::str("sent")),
            ],
            pages: vec![LoadedPage {
                path: "pages/a.md".to_owned(),
                record: Value::str("record"),
                spans: Spans::new(),
                record_span: Span::new(0, 0),
            }],
        };
        // The name is the host's, not `page`: the record replaces the envelope's value
        // in place, keeping the host's order.
        let host = Host::untyped(["build", "entry"]).documents("entry");
        assert_eq!(
            store.globals(&host, "pages/a.md"),
            vec![("build", Value::Int(1)), ("entry", Value::str("record"))]
        );
        // No loaded page at the path: the envelope's value stands.
        assert_eq!(
            store.globals(&host, "pages/b.md")[1],
            ("entry", Value::str("sent"))
        );
        // The envelope carried no document value: the record is bound anyway.
        let uncarried = Store {
            ambient: vec![(Rc::from("build"), Value::Int(1))],
            pages: store.pages.clone(),
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
                    pages: Vec::new(),
                },
            )
            .unwrap_err()
        };
        let motto = || ("motto".to_owned(), one(Value::str("m")));
        assert_eq!(
            refused(vec![("posts".to_owned(), one(Value::Null)), motto()]),
            "`posts` collects pages, so it takes a list"
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
            "a page in `posts` has no path"
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
            "no compiled page at `pages/b.md`"
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
    fn refuses_a_page_compiled_with_no_record() {
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
            pages: Vec::new(),
        };
        assert_eq!(
            load(&module, &shell(), data).unwrap_err(),
            "`pages/a.md` was compiled with no record, so it is no page to load"
        );
    }

    #[test]
    fn assembles_the_bindings_value_in_declaration_order_and_keeps_the_page() {
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
            pages: Vec::new(),
        };
        let store = load(&module_with_layout, &shell(), data)
            .unwrap()
            .expect_output();
        let Value::Record(site) = &store.ambient[1].1 else {
            panic!("site is a record");
        };
        assert_eq!(&*site.name, "site.lem");
        let fields: Vec<&str> = site.fields.iter().map(|(k, _)| &**k).collect();
        assert_eq!(fields, ["posts", "motto", "count"]);
        assert_eq!(site.fields.get("count"), Some(&Value::Int(1)));
        let Some(Value::List(posts)) = site.fields.get("posts") else {
            panic!("posts is a list");
        };
        assert_eq!(posts.len(), 1);
        assert_eq!(
            store.page("pages/a.md").map(|page| &page.record),
            Some(&posts[0])
        );
        let Value::Record(article) = &posts[0] else {
            panic!("a tagged record");
        };
        assert_eq!(&*article.name, "Article");
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
            pages: Vec::new(),
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
            pages: Vec::new(),
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
            pages: Vec::new(),
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
            pages: Vec::new(),
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
            pages: Vec::new(),
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
            pages: Vec::new(),
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
            pages: Vec::new(),
        };
        assert_eq!(
            load(&module, &shell(), data).unwrap_err(),
            "`motto` is loaded twice"
        );
    }

    /// A site whose `Article` indexes a map the site computes once `notes` is loaded,
    /// so `posts` — declared first — loads last, and a page's load reads what the
    /// bindings loaded first. Each schema's layout takes its own type: one taking
    /// `Page` would make every page schema reachable from `notes`, `Article` and its
    /// index included, and close a cycle.
    const INDEXED: &str = "<lem-type Photo>\n  caption: string\n</lem-type>\n\
         <lem-type Article extends Page>\n  title: string\n  \
         hero: Photo? by site.photos\n  layout: (Article, html): html = f\n</lem-type>\n\
         <lem-type Note extends Page>\n  layout: (Note, html): html = g\n</lem-type>\n\
         <lem-fn f(a: Article, b: html)><lem.a.url><lem.b></lem-fn>\n\
         <lem-fn g(n: Note, b: html)><lem.n.url><lem.b></lem-fn>\n\
         <lem-let posts: [Article] from \"pages/*.md\" />\n\
         <lem-let notes: [Note] from \"notes/*.md\" />\n\
         <lem-let photos: {string: Photo} = {\"sunset\": {\"caption\": \
         notes.length() gt 0 ? \"Sunset over notes\" : \"Sunset\"}} />";

    /// The `INDEXED` site with a member of each collection and a page no glob holds
    /// — a document and a template — compiled under the shell host.
    fn indexed_module() -> Module {
        let files = [
            SourceFile::new("ambient.lem", AMBIENT),
            SourceFile::new("site.lem", INDEXED),
            document("pages/a.md", "---\ntitle: A\n---\nbody", "Article"),
            document("notes/n.md", "---\n---\nnote", "Note"),
            document(
                "about.md",
                "---\ntype: Article\ntitle: About\n---\n",
                "Article",
            ),
            document(
                "home.lem",
                "---\ntype: Article\ntitle: Home\n---\n<lem.page.title>",
                "Article",
            ),
        ];
        compile(&files, &shell()).expect_output()
    }

    fn paths(store: &Store) -> Vec<&str> {
        store.pages.iter().map(|page| page.path.as_str()).collect()
    }

    #[test]
    fn loads_a_page_template_as_a_member_of_its_collection() {
        // A template with front matter is a member like a document: its record is
        // built from the front matter, bound at its path for the render, and its
        // `body` is null, which no default of `Article` reads.
        let module = indexed_module();
        let data = Data {
            ambient: vec![build()],
            sources: vec![
                (
                    "posts".to_owned(),
                    Source::Many(vec![page(
                        "home.lem",
                        object(&[("title", Value::str("Home")), ("url", Value::str("/"))]),
                    )]),
                ),
                ("notes".to_owned(), Source::Many(vec![])),
            ],
            pages: Vec::new(),
        };
        let store = load(&module, &shell(), data).unwrap().expect_output();
        let Some(Value::Record(home)) = store.page("home.lem").map(|page| &page.record)
        else {
            panic!("a tagged record");
        };
        assert_eq!(&*home.name, "Article");
        assert_eq!(home.fields.get("title"), Some(&Value::str("Home")));
        assert_eq!(home.fields.get("draft"), Some(&Value::Bool(false)));
        let host = shell();
        let globals = store.globals(&host, "home.lem");
        assert_eq!(globals.last().map(|(name, _)| *name), Some("page"));
        assert_eq!(
            globals.last().map(|(_, v)| v),
            store.page("home.lem").map(|page| &page.record)
        );
    }

    #[test]
    fn loads_a_page_no_glob_holds_after_the_bindings() {
        // `about.md` resolves `hero` through `site.photos`, which the bindings loaded
        // before any page in `pages` was; and its record is kept after the members.
        let module = indexed_module();
        let data = Data {
            ambient: vec![build()],
            sources: vec![
                (
                    "posts".to_owned(),
                    Source::Many(vec![page(
                        "pages/a.md",
                        object(&[("title", Value::str("A")), ("url", Value::str("/a/"))]),
                    )]),
                ),
                ("notes".to_owned(), Source::Many(vec![])),
            ],
            pages: vec![page(
                "about.md",
                object(&[
                    ("title", Value::str("About")),
                    ("hero", Value::str("sunset")),
                    ("url", Value::str("/about/")),
                ]),
            )],
        };
        let store = load(&module, &shell(), data).unwrap().expect_output();
        assert_eq!(paths(&store), ["pages/a.md", "about.md"]);
        let Some(Value::Record(about)) = store.page("about.md").map(|page| &page.record)
        else {
            panic!("a tagged record");
        };
        let Some(Value::Record(hero)) = about.fields.get("hero") else {
            panic!("a resolved photo: {:?}", about.fields.get("hero"));
        };
        assert_eq!(hero.fields.get("caption"), Some(&Value::str("Sunset")));
    }

    #[test]
    fn reports_a_page_no_glob_holds_at_the_hosts_positions() {
        let module = indexed_module();
        let text = "---\ntype: Article\ntitle: 3\n---\n";
        let data = Data {
            ambient: vec![build()],
            sources: vec![
                ("posts".to_owned(), Source::Many(vec![])),
                ("notes".to_owned(), Source::Many(vec![])),
            ],
            pages: vec![Loaded {
                value: object(&[("title", Value::Int(3)), ("url", Value::str("/about/"))]),
                path: Some("about.md".to_owned()),
                spans: spans(text, &[("title", at(text, "title: 3"))]),
            }],
        };
        assert_eq!(
            errors(&module, &shell(), data),
            [format!(
                "about.md:{}: expected `string` for `title`, got `int`",
                at(text, "title: 3")
            )]
        );
    }

    #[test]
    fn refuses_a_page_the_envelope_cannot_load() {
        // Caller bugs, each with no data to point at: the checks a glob's members get,
        // and one path carried twice, whichever way.
        let files = [
            SourceFile::new("ambient.lem", AMBIENT),
            SourceFile::new("site.lem", SITE),
            document("pages/a.md", "---\ntitle: A\n---\nbody", "Article"),
            SourceFile::new("layout.lem", "<lem-fn g()>x</lem-fn>"),
            SourceFile::new("free.lem", "---\n---\nx"),
        ];
        let module = compile(&files, &shell()).expect_output();
        let refused = |members: Vec<Loaded>, pages: Vec<Loaded>| {
            load(
                &module,
                &shell(),
                Data {
                    ambient: vec![build()],
                    sources: vec![
                        ("posts".to_owned(), Source::Many(members)),
                        ("motto".to_owned(), one(Value::str("m"))),
                    ],
                    pages,
                },
            )
            .unwrap_err()
        };
        let mut nameless = page("pages/a.md", object(&[]));
        nameless.path = None;
        assert_eq!(
            refused(vec![], vec![nameless]),
            "a page in `pages` has no path"
        );
        assert_eq!(
            refused(vec![], vec![page("layout.lem", object(&[]))]),
            "no compiled page at `layout.lem`"
        );
        assert_eq!(
            refused(vec![], vec![page("nowhere.md", object(&[]))]),
            "no compiled page at `nowhere.md`"
        );
        assert_eq!(
            refused(vec![], vec![page("free.lem", object(&[]))]),
            "`free.lem` was compiled with no record, so it is no page to load"
        );
        let a = || page("pages/a.md", object(&[]));
        assert_eq!(
            refused(vec![], vec![a(), a()]),
            "`pages/a.md` is loaded twice"
        );
        assert_eq!(
            refused(vec![a()], vec![a()]),
            "`pages/a.md` is loaded twice"
        );
    }

    #[test]
    fn leaves_a_page_the_envelope_does_not_carry_unloaded() {
        // Not an error: the resident core cannot forget a unit the shell deleted or
        // held back, so the page is simply absent, and a render of it binds whatever
        // the envelope carried for the document value — here nothing.
        let module = site_module();
        let data = Data {
            ambient: vec![build()],
            sources: vec![
                ("posts".to_owned(), Source::Many(vec![])),
                ("motto".to_owned(), one(Value::str("m"))),
            ],
            pages: Vec::new(),
        };
        let store = load(&module, &shell(), data).unwrap().expect_output();
        assert_eq!(store.page("pages/a.md"), None);
        assert!(
            store
                .globals(&shell(), "pages/a.md")
                .iter()
                .all(|(name, _)| *name != "page")
        );
    }

    #[test]
    fn keeps_pages_in_load_order_across_collections_and_the_envelope() {
        // `posts` is declared first and loads last among the collections: its records
        // index `photos`, computed from `notes`, so link order is not declaration
        // order — and the store follows the load, then takes `pages` in the envelope's
        // order.
        let module = indexed_module();
        let steps: Vec<&str> = module.bindings[0]
            .steps
            .iter()
            .map(|step| &*step.name)
            .collect();
        assert_eq!(steps, ["notes", "photos", "posts"]);
        let data = Data {
            ambient: vec![build()],
            sources: vec![
                (
                    "posts".to_owned(),
                    Source::Many(vec![page(
                        "pages/a.md",
                        object(&[("title", Value::str("A")), ("url", Value::str("/a/"))]),
                    )]),
                ),
                (
                    "notes".to_owned(),
                    Source::Many(vec![page(
                        "notes/n.md",
                        object(&[("url", Value::str("/n/"))]),
                    )]),
                ),
            ],
            pages: vec![
                page(
                    "home.lem",
                    object(&[("title", Value::str("Home")), ("url", Value::str("/"))]),
                ),
                page(
                    "about.md",
                    object(&[
                        ("title", Value::str("About")),
                        ("url", Value::str("/about/")),
                    ]),
                ),
            ],
        };
        let store = load(&module, &shell(), data).unwrap().expect_output();
        assert_eq!(
            paths(&store),
            ["notes/n.md", "pages/a.md", "home.lem", "about.md"]
        );
    }

    /// A document type placing pages by `url`, and two page schemas: `Article`, whose
    /// url its front matter gives, and `Note`, whose url a default derives.
    const PLACED: &str = "<lem-type Page>\n  url: url\n</lem-type>\n\
         <lem-type Build>\n  generator: string\n</lem-type>";
    const ROUTED: &str = "<lem-type Article extends Page>\n  title: string\n  \
         layout: (Article, html): html = f\n</lem-type>\n\
         <lem-type Note extends Page>\n  url: url = url(\"/n/\")\n  \
         layout: (Note, html): html = g\n</lem-type>\n\
         <lem-fn f(a: Article, b: html)><lem.b></lem-fn>\n\
         <lem-fn g(n: Note, b: html)><lem.b></lem-fn>\n\
         <lem-let posts: [Article] from \"pages/*.md\" />\n\
         <lem-let notes: [Note] from \"notes/*.md\" />";
    const HOME: &str = "---\ntype: Article\ntitle: Home\nurl: /a/\n---\n";
    const ABOUT: &str = "---\ntype: Article\ntitle: About\nurl: /a/\n---\n";
    const NOTE: &str = "---\n---\nnote";

    /// The shell, placing pages by `url`.
    fn placed() -> Host {
        shell().addressed_by("url")
    }

    /// The `ROUTED` site with a member of each collection, a second note, and two
    /// page templates no glob holds, compiled under the placing host.
    fn routed_module() -> Module {
        let files = [
            SourceFile::new("ambient.lem", PLACED),
            SourceFile::new("site.lem", ROUTED),
            document(
                "pages/a.md",
                "---\ntitle: A\nurl: /a/\n---\nbody",
                "Article",
            ),
            document("notes/n.md", NOTE, "Note"),
            document("notes/m.md", NOTE, "Note"),
            document("home.lem", HOME, "Article"),
            document("about.lem", ABOUT, "Article"),
        ];
        compile(&files, &placed()).expect_output()
    }

    /// A page with its front matter's `url` placed by a table over `text`.
    fn placed_page(path: &str, text: &str, front: &[(&str, Value)]) -> Loaded {
        Loaded {
            value: object(front),
            path: Some(path.to_owned()),
            spans: spans(text, &[("url", at(text, "url: «/a/»"))]),
        }
    }

    fn article(url: &str) -> Value {
        object(&[("title", Value::str("A")), ("url", Value::str(url))])
    }

    #[test]
    fn refuses_two_pages_at_one_url_across_a_glob_and_the_envelope() {
        // `pages/a.md` came with no positions, so the label falls to where its record
        // was named; `home.lem` came with them, so the error is at its `url:` key.
        let module = routed_module();
        let data = Data {
            ambient: vec![build()],
            sources: vec![
                (
                    "posts".to_owned(),
                    Source::Many(vec![page("pages/a.md", article("/a/"))]),
                ),
                ("notes".to_owned(), Source::Many(vec![])),
            ],
            pages: vec![placed_page(
                "home.lem",
                HOME,
                &[("title", Value::str("Home")), ("url", Value::str("/a/"))],
            )],
        };
        let a = module.document("pages/a.md").expect("a page source");
        assert_eq!(
            errors(&module, &placed(), data),
            [format!(
                "home.lem:{}: `home.lem` and `pages/a.md` are both at `/a/`\n  \
                 pages/a.md:{}: the first page at this url",
                at(HOME, "url: «/a/»"),
                a.record_span
            )]
        );
    }

    #[test]
    fn refuses_two_pages_at_one_url_the_envelope_carried_written() {
        let module = routed_module();
        let data = Data {
            ambient: vec![build()],
            sources: vec![
                ("posts".to_owned(), Source::Many(vec![])),
                ("notes".to_owned(), Source::Many(vec![])),
            ],
            pages: vec![
                placed_page(
                    "home.lem",
                    HOME,
                    &[("title", Value::str("Home")), ("url", Value::str("/a/"))],
                ),
                placed_page(
                    "about.lem",
                    ABOUT,
                    &[("title", Value::str("About")), ("url", Value::str("/a/"))],
                ),
            ],
        };
        assert_eq!(
            errors(&module, &placed(), data),
            [format!(
                "about.lem:{}: `about.lem` and `home.lem` are both at `/a/`\n  \
                 home.lem:{}: the first page at this url",
                at(ABOUT, "url: «/a/»"),
                at(HOME, "url: «/a/»")
            )]
        );
    }

    #[test]
    fn refuses_two_pages_whose_defaults_derive_one_url() {
        // Neither note's front matter has a `url:` key for the table to place, so
        // both ends point at where each record was named.
        let module = routed_module();
        let data = Data {
            ambient: vec![build()],
            sources: vec![
                ("posts".to_owned(), Source::Many(vec![])),
                (
                    "notes".to_owned(),
                    Source::Many(vec![
                        page("notes/n.md", object(&[])),
                        page("notes/m.md", object(&[])),
                    ]),
                ),
            ],
            pages: Vec::new(),
        };
        let named = |path: &str| module.document(path).expect("a page source").record_span;
        assert_eq!(
            errors(&module, &placed(), data),
            [format!(
                "notes/m.md:{}: `notes/m.md` and `notes/n.md` are both at `/n/`\n  \
                 notes/n.md:{}: the first page at this url",
                named("notes/m.md"),
                named("notes/n.md")
            )]
        );
    }

    #[test]
    fn plans_every_loaded_page_in_store_order_with_its_url() {
        let module = routed_module();
        let data = Data {
            ambient: vec![build()],
            sources: vec![
                (
                    "posts".to_owned(),
                    Source::Many(vec![page("pages/a.md", article("/a/"))]),
                ),
                (
                    "notes".to_owned(),
                    Source::Many(vec![page("notes/n.md", object(&[]))]),
                ),
            ],
            pages: vec![page("home.lem", article("/home/"))],
        };
        let store = load(&module, &placed(), data).unwrap().expect_output();
        let planned = |host: &Host| -> Vec<(String, Option<String>)> {
            store
                .plan(host)
                .into_iter()
                .map(|page| (page.path, page.url))
                .collect()
        };
        assert_eq!(
            planned(&placed()),
            [
                ("pages/a.md".to_owned(), Some("/a/".to_owned())),
                ("notes/n.md".to_owned(), Some("/n/".to_owned())),
                ("home.lem".to_owned(), Some("/home/".to_owned())),
            ]
        );
        // A host that names no url field places nothing.
        assert_eq!(
            planned(&shell()),
            [
                ("pages/a.md".to_owned(), None),
                ("notes/n.md".to_owned(), None),
                ("home.lem".to_owned(), None),
            ]
        );
    }

    #[test]
    fn checks_no_url_for_a_host_that_names_no_url_field() {
        let module = routed_module();
        let data = Data {
            ambient: vec![build()],
            sources: vec![
                (
                    "posts".to_owned(),
                    Source::Many(vec![page("pages/a.md", article("/a/"))]),
                ),
                ("notes".to_owned(), Source::Many(vec![])),
            ],
            pages: vec![page("home.lem", article("/a/"))],
        };
        assert_eq!(errors(&module, &shell(), data), Vec::<String>::new());
    }

    #[test]
    fn reports_a_loaded_page_that_holds_no_url_rather_than_trusting_the_link() {
        // Nothing a load builds can reach here — the link step saw the document type
        // declare the field, and the record was validated against it — so this is
        // the check that the pass would say so rather than trap, were it to happen.
        let unplaced = LoadedPage {
            path: "x.md".to_owned(),
            record: Value::record("Article", Map::default()),
            spans: Spans::new(),
            record_span: Span::new(4, 11),
        };
        let shown: Vec<String> = url_diagnostics(&[unplaced], "url")
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            shown,
            ["x.md:4..11: `x.md` holds no url in `url`, so it stands nowhere"]
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
            pages: Vec::new(),
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

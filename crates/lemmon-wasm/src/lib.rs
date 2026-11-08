//! The `wasm-bindgen` boundary: the only part of the core the shell can see.
//!
//! One resident [`Core`] per worker holds the compiled units, the linked module, and the
//! loaded data across calls (docs/architecture.md §"WASM API"): few large calls, each
//! carrying its bulk data as one UTF-8 JSON string. `compile` takes only the files that
//! changed, compiles each into its own unit against the resident signatures, and
//! relinks; `loadData` validates the host's data against the program and keeps what it
//! builds; `plan` lists the pages it loaded; `render` runs one file's entry over it, a
//! loaded page's through its layout (D43) or, asked for its body, without (D46).
//!
//! Diagnostics come back as a JSON array of
//! `{path, span: {start, end, range}, severity, message, labels}`, every one pointing at
//! source (invariant 8) and every label too. `span` carries both what the core works in —
//! byte offsets — and `range`, the same two points as zero-based UTF-16 LSP positions, so
//! neither `lem check` nor the language server has to redo that conversion in JavaScript,
//! where a byte offset is the awkward direction to convert from. A
//! thrown `JsError` is reserved for caller bugs
//! — a malformed envelope, rendering before a compile has succeeded, rendering a path
//! the program does not have — which have no source span to point at.

use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;

use lemmon_vm::{
    ExportIndex, Host, LineIndex, LoadedPage, Module, SourceFile, Store, Unit, Value,
    compile_unit, from_bindings, link, load, prepare, render, render_page, repeated_path,
};
use wasm_bindgen::JsError;
use wasm_bindgen::prelude::wasm_bindgen;

mod wire;

use wire::{
    CodePayloadOut, CompileOut, DataInput, Diagnostic, FileInput, FromBindingOut,
    HostInput, LineIndexes, PageRef, PlannedOut, RenderMeta, encode_render,
};

/// The version of the crates this `.wasm` was built from, which stays `0.0.0`: the crates
/// are never published, and a release picks its version after the `.wasm` is built, so
/// the version a build names is `@limulus/lemmon-core`'s, read from its `package.json`.
/// `createCore` still calls this first, as the cheapest call that traps in a poisoned
/// instance.
#[wasm_bindgen]
#[must_use]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// The resident core — one per worker, nothing shared. Units persist across `compile`
/// calls so each call carries only the files that changed (D25).
#[wasm_bindgen]
pub struct Core {
    /// Compiled units by path — ordered, so every relink is deterministic whatever
    /// order the compiles came in.
    units: BTreeMap<String, Unit>,
    /// What every project-flat name means across the resident units: offered to as
    /// each file compiles, and always a description of `units` (`∆e5i`).
    index: ExportIndex,
    /// The linked program, with what a load or a render of it needs: absent until a
    /// compile succeeds, and left as it was by one that fails — stale until fixed, with
    /// the failure reported, never guessed around.
    linked: Option<Linked>,
    /// What the host declared about the ambient environment: resident like the units,
    /// since a `compile` of one changed file is compiled in the same environment as the
    /// files already held. Replaced whenever a call carries a declaration.
    host: Host,
    /// For each resident unit, an index over the text *that unit* was compiled from —
    /// which is not always the latest text at that path, since a file whose recompile
    /// failed keeps its previous unit. Updated in lockstep with `units`, so it grows
    /// with them and no faster. Line starts and each line's non-ASCII characters, not
    /// the text: an all-ASCII file costs a handful of bytes per line.
    unit_indexes: LineIndexes,
}

/// A program a clean relink installed, kept with everything that was true of it when
/// it linked. The resident units, indexes, and declaration all move on as later calls
/// compile — a call that compiles a file cleanly and then fails to link leaves the
/// unit newer than the program — so a load or a render, whose spans and names come
/// from the program, reads these and not the resident ones.
struct Linked {
    module: Module,
    /// The declaration the program linked under.
    host: Host,
    /// An index over the text each unit of `module` was compiled from.
    indexes: LineIndexes,
    /// What the last successful `loadData` built against `module` — every ambient
    /// value, validated and tagged where the host typed it, and each loaded document's
    /// record — bound at every render. `None` until a load succeeds, and left as it was
    /// by a load that reports an error: stale until fixed, with the failure reported.
    /// The store is a function of the program and the data, so it lives and dies with
    /// the program: a new program binds nothing until data is loaded against it.
    store: Option<Store>,
}

#[wasm_bindgen]
impl Core {
    /// A fresh core with nothing compiled or loaded.
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new() -> Self {
        Self {
            units: BTreeMap::new(),
            index: ExportIndex::new(),
            linked: None,
            host: Host::new(),
            unit_indexes: BTreeMap::new(),
        }
    }

    /// Compiles the given files — only the changed ones — into resident units and
    /// relinks the program. `files` is JSON `[{path, text, record?}]`, where `record`
    /// names the schema a document's content is (D30). The result is JSON
    /// `{diagnostics, payloads, sources}`: the diagnostics about those files (and about
    /// any resident unit the relink found stale) — none of them an error on success,
    /// since a program is still produced when the only diagnostics are warnings — and
    /// the payload of every code block those files write, `{id, language, info, code}`,
    /// each once (D42). A resident file's warnings and payloads are reported by the call
    /// that compiled it, not again by later calls: a host keeps the payloads by id,
    /// which is what a render's `code` mark names. `sources` is every `from` binding of
    /// the program a clean relink installed, `{name, kind, literal, collects?, path,
    /// span}` in the order its bindings file declares them — what the host fulfils in
    /// `loadData`, and which documents it compiles as which record (`∆muD`) — and empty
    /// when the call installed no program.
    ///
    /// `host` is the ambient declaration, JSON `{ambient, document?, entry?, url?,
    /// neighbours?, assets?, marks?}`: it is resident too, so a call that changes
    /// nothing about the environment omits it, and one that carries it replaces what the
    /// core holds. A core given none has no ambient values at all.
    ///
    /// A compile that installs a new program drops the data loaded against the old
    /// one: `loadData` again before `render`, or an ambient read fails as not bound.
    ///
    /// # Errors
    ///
    /// A malformed `files` or `host` payload, files naming the same path twice, or a
    /// `recordSpan` that is not a span of its file's text — caller bugs; template
    /// problems are diagnostics.
    pub fn compile(
        &mut self,
        files: &str,
        host: Option<String>,
    ) -> Result<String, JsError> {
        self.compile_json(files, host.as_deref())
            .map_err(|m| JsError::new(&m))
    }

    /// Loads the data every render binds, validating it against the program (D37).
    /// `input` is JSON `{ambient?, sources?, pages?}` — the envelope `DataInput` in
    /// `wire.rs` defines and D37 §"The envelope grows additively" settles, `pages`
    /// carrying every page no glob holds (D41), and any page's entry a generator's
    /// `for:` pair as the host parsed it, which the load expands into the generator's
    /// pages (D43). The result is the JSON array of load diagnostics, each placed by the
    /// positions the host sent or at the declaration that expected the value. On an
    /// error the previous data stays bound, stale until fixed. A page the envelope does
    /// not carry is not loaded: `plan` omits it, and `render` of it is refused.
    ///
    /// # Errors
    ///
    /// Caller bugs with no source span to carry: a malformed `input` payload; a load
    /// before any compile has succeeded, or after the declaration changed without a
    /// compile succeeding under it; and an envelope the program cannot be loaded from —
    /// a name the host does not bind, a source no `from` binding asked for, positions
    /// with no file to place them in, a document the program never compiled.
    #[wasm_bindgen(js_name = loadData)]
    pub fn load_data(&mut self, input: &str) -> Result<String, JsError> {
        self.load_data_json(input).map_err(|m| JsError::new(&m))
    }

    /// Lists every page the last successful load built, in load order (D43): JSON
    /// `[{path, index?, url?}]`, each collection's pages as its binding loaded them,
    /// then the envelope's `pages` in their order, a generator's pages where it stood
    /// with their `index` in its list's order, each with the url the host-named field
    /// holds. After a load that reported an error it lists the stale data, as `render`
    /// binds it.
    ///
    /// # Errors
    ///
    /// A call before any load has succeeded — before any compile, or after a compile
    /// installed a new program and dropped the data — a caller bug with no source
    /// span to carry.
    pub fn plan(&self) -> Result<String, JsError> {
        self.plan_json().map_err(|m| JsError::new(&m))
    }

    /// Renders one page. `page` is JSON `{path, index?, body?}`, `index` naming one page
    /// of a generator as `plan()` lists it; the result is bytes — a four-byte
    /// little-endian length, that many bytes of JSON `{marks, fragments, diagnostics}`,
    /// then the page's UTF-8 bytes.
    ///
    /// The page crosses as bytes rather than inside the JSON because escaping and
    /// parsing it there costs more than the render, and because a mark's offsets name
    /// the page's own bytes (D42). A failed render carries the runtime diagnostic, null
    /// marks and fragments, and no page section at all — which is how a caller tells it
    /// from a page that rendered empty.
    ///
    /// A loaded page renders through its record's entry field, with the host's
    /// document value bound to that record and its own output passed as the body
    /// (D43); any other file renders its own output, reading the value the envelope
    /// carried, if it did. With `body`, a loaded page renders its own output and calls
    /// no entry field — what its layout would have been given, its marks and fragments
    /// as they stand in it (D46) — and a file that is no page renders as it always does.
    ///
    /// # Errors
    ///
    /// A malformed `page` payload, rendering before any compile has succeeded, a path
    /// the program has no file for, or a page — a file compiled with a record — that
    /// the last load did not make; a generator's path with no `index`, an `index` past
    /// the pages it made, or one for a file that is no generator: caller bugs with no
    /// source span to carry.
    pub fn render(&self, page: &str) -> Result<Vec<u8>, JsError> {
        self.render_bytes(page).map_err(|m| JsError::new(&m))
    }
}

impl Default for Core {
    fn default() -> Self {
        Self::new()
    }
}

impl Core {
    // The logic behind each shim, in plain Rust types so host-target tests never touch
    // a `JsError`.

    fn compile_json(
        &mut self,
        files_json: &str,
        host_json: Option<&str>,
    ) -> Result<String, String> {
        let files: Vec<FileInput> = serde_json::from_str(files_json)
            .map_err(|e| format!("malformed `compile` input: {e}"))?;
        let host = host_json
            .map(|json| {
                serde_json::from_str::<HostInput>(json)
                    .map_err(|e| e.to_string())
                    .and_then(Host::try_from)
                    .map_err(|e| format!("malformed `compile` host: {e}"))
            })
            .transpose()?;
        // Rejected before anything is offered: the second offer of a path withdraws the
        // first's names, leaving the resident index describing neither file.
        if let Some(path) = repeated_path(files.iter().map(|file| file.path.as_str())) {
            return Err(format!("malformed `compile` input: `{path}` given twice"));
        }
        // A record span is the host's offsets into text this call carries, so it is
        // checked here, once, rather than trusted by every diagnostic placed at it.
        for file in &files {
            if let Some(problem) = file
                .record_span
                .and_then(|span| span.misplaced_in(&file.text))
            {
                return Err(format!(
                    "malformed `compile` input: `{}` has a `recordSpan` of {problem}",
                    file.path
                ));
            }
        }
        // Installed once the call is past everything that would refuse it: a rejected
        // call is a caller bug, and leaves the resident core exactly as it was.
        if let Some(host) = host {
            self.host = host;
        }
        // Indexed up front, before `prepare`, so that a file which fails to even parse
        // can still have its own diagnostics placed.
        let incoming: LineIndexes = files
            .iter()
            .map(|file| (file.path.clone(), Rc::new(LineIndex::new(&file.text))))
            .collect();
        let mut diagnostics: Vec<lemmon_vm::Diagnostic> = Vec::new();
        // Every unit that goes resident reports its payloads now, whether or not the
        // program relinks: a unit is not recompiled once resident, so a later call that
        // fixes another file would never report them (D42).
        let mut payloads: Vec<CodePayloadOut> = Vec::new();
        let mut reported: HashSet<Rc<str>> = HashSet::new();
        let mut prepared = Vec::new();
        for file in &files {
            let mut source = SourceFile::new(file.path.clone(), file.text.clone());
            source.record = file.record.clone();
            source.record_span = file.record_span.map(Into::into);
            match prepare(&source, &self.host) {
                Ok(p) => prepared.push(p),
                Err(errors) => diagnostics.extend(errors),
            }
        }
        // The index already holds every resident file's names; offering the incoming
        // files now is what puts a collision's blame on the file just handed in, since
        // a name a resident file owns stays that file's. The refusal is reported by the
        // compile below, so a collision is reported once, by the call that made it.
        // Every incoming file is withdrawn before any is offered, so a name that moved
        // from one of them to another is free when its new home offers it.
        for p in &prepared {
            self.index.withdraw(p.path());
        }
        for p in &prepared {
            self.index.offer(p.path(), p.exports());
            self.index.offer_types(p.path(), p.types());
        }
        for p in &prepared {
            let compiled = compile_unit(p, &self.index, &self.host);
            diagnostics.extend(compiled.diagnostics);
            if let Some(unit) = compiled.output {
                for payload in &unit.payloads {
                    if reported.insert(payload.id.clone()) {
                        payloads.push(CodePayloadOut::from(payload));
                    }
                }
                // In lockstep: the index kept for a unit is the one over the text that
                // produced it, so the unit's spans stay placeable for as long as it is
                // resident — including after a later recompile of that path fails.
                self.unit_indexes
                    .insert(unit.path.clone(), incoming[&unit.path].clone());
                self.units.insert(unit.path.clone(), unit);
            }
        }
        // What the host loads for the program a clean relink installs; nothing from a
        // compile that installs none, since the program it would describe is not the
        // one a load runs against.
        let mut sources: Vec<FromBindingOut> = Vec::new();
        // A file that failed keeps its previous unit, so the program stays renderable
        // — stale until fixed — and the module is only replaced by a clean relink.
        // Warnings are not failures: a program that only warns is the new program.
        if diagnostics.iter().any(lemmon_vm::Diagnostic::is_error) {
            // A file that yielded no unit was offered to the index anyway, so the index
            // now describes functions no resident unit has — indices a later clean
            // relink would resolve calls against. Back to what the units say.
            self.index = ExportIndex::of(self.units.values());
        } else {
            let units: Vec<Unit> = self.units.values().cloned().collect();
            let relinked = link(&units, &self.index, &self.host);
            diagnostics.extend(relinked.diagnostics);
            if let Some(module) = relinked.output {
                sources = from_bindings(&module, &self.host)
                    .into_iter()
                    .map(|binding| FromBindingOut::placed(binding, &self.unit_indexes))
                    .collect();
                self.linked = Some(Linked {
                    module,
                    host: self.host.clone(),
                    indexes: self.unit_indexes.clone(),
                    store: None,
                });
            }
        }
        debug_assert_eq!(
            self.index,
            ExportIndex::of(self.units.values()),
            "the index describes the resident units"
        );
        debug_assert!(
            self.units.keys().eq(self.unit_indexes.keys()),
            "an index for every resident unit and no others"
        );
        let diagnostics: Vec<Diagnostic> = diagnostics
            .into_iter()
            .map(|d| Diagnostic::placed(d, Some(&incoming), &self.unit_indexes))
            .collect();
        serde_json::to_string(&CompileOut {
            diagnostics,
            payloads,
            sources,
        })
        .map_err(|e| format!("serialising the compile result failed: {e}"))
    }

    fn load_data_json(&mut self, input_json: &str) -> Result<String, String> {
        let input: DataInput = serde_json::from_str(input_json)
            .map_err(|e| format!("malformed `loadData` input: {e}"))?;
        let Some(linked) = self.linked.as_mut() else {
            return Err(
                "nothing has compiled successfully yet — `loadData` needs a `compile` first"
                    .to_owned(),
            );
        };
        // A failed compile keeps the declaration its units compiled under, so the
        // resident declaration can be newer than the program's. An envelope built for
        // the newer one cannot be loaded against the older program, and saying so beats
        // blaming the envelope for a compile that has not succeeded yet.
        if self.host != linked.host {
            return Err(
                "the ambient declaration changed since the program last linked: \
                        `compile` successfully before `loadData`"
                    .to_owned(),
            );
        }
        let data = input
            .into_data()
            .map_err(|e| format!("malformed `loadData` input: {e}"))?;
        let loaded = load(&linked.module, &linked.host, data).map_err(|e| {
            format!("the program cannot be loaded from this `loadData` envelope: {e}")
        })?;
        if let Some(store) = loaded.output {
            linked.store = Some(store);
        }
        // A load diagnostic about a file the host holds carries the host's positions
        // and needs no index; one at a declaration names a file of the program.
        let diagnostics: Vec<Diagnostic> = loaded
            .diagnostics
            .into_iter()
            .map(|d| Diagnostic::placed(d, None, &linked.indexes))
            .collect();
        serde_json::to_string(&diagnostics)
            .map_err(|e| format!("serialising diagnostics failed: {e}"))
    }

    fn plan_json(&self) -> Result<String, String> {
        let store = self
            .linked
            .as_ref()
            .and_then(|linked| linked.store.as_ref().map(|store| (linked, store)));
        let Some((linked, store)) = store else {
            return Err(
                "nothing is loaded — `plan` needs a `loadData` that succeeded against \
                 the program"
                    .to_owned(),
            );
        };
        let pages: Vec<PlannedOut> = store
            .plan(&linked.host)
            .into_iter()
            .map(PlannedOut::from)
            .collect();
        serde_json::to_string(&pages)
            .map_err(|e| format!("serialising the plan failed: {e}"))
    }

    fn render_bytes(&self, page_json: &str) -> Result<Vec<u8>, String> {
        let page: PageRef = serde_json::from_str(page_json)
            .map_err(|e| format!("malformed `render` input: {e}"))?;
        let linked = self.linked.as_ref().ok_or(
            "nothing has compiled successfully yet — `render` needs a `compile` first",
        )?;
        // Checked here so the VM's unknown-path panic is unreachable from the boundary.
        if linked.module.entry(&page.path).is_none() {
            return Err(format!("no compiled file at `{}`", page.path));
        }
        let path = &page.path;
        // A page — a file compiled with a record — renders bound to the record the
        // load built for it. With none, the envelope's value would stand in, typed by
        // the checker as a record it is not (invariant 10): that is a caller bug, not
        // a render that happens to read nothing.
        let is_page = linked
            .module
            .document(path)
            .is_some_and(|source| source.record.is_some());
        if page.index.is_some() && !is_page {
            return Err(no_generator(path));
        }
        let store = linked.store.as_ref();
        let loaded = match store {
            Some(store) => loaded_page(store, path, page.index, is_page)?,
            None if is_page => {
                return Err(format!(
                    "`{path}` is a page, and nothing is loaded for it to bind: `loadData` \
                     first"
                ));
            }
            None => None,
        };
        let globals: Vec<(&str, Value)> = store
            .map(|store| store.globals(&linked.host, path, page.index))
            .unwrap_or_default();
        // A body is the page's own output, what its entry would have been given: the
        // same run with no entry to call, so ids, fragments and marks are the body's
        // exactly as they stand in the page (D46).
        let entry = if page.body { None } else { linked.host.entry() };
        let rendered = match store.zip(loaded) {
            Some((store, loaded)) => {
                render_page(&linked.module, loaded, &store.pages, entry, &globals)
            }
            None => render(&linked.module, &page.path, &globals),
        };
        match rendered {
            Ok(rendered) => {
                let meta =
                    RenderMeta::placed(&rendered, &linked.module.files, &linked.indexes);
                encode_render(&meta, Some(&rendered.html))
            }
            Err(error) => {
                let diagnostic = Diagnostic::placed_runtime(error, &linked.indexes);
                encode_render(&RenderMeta::failed(diagnostic), None)
            }
        }
    }
}

/// The refusal of an `index` for a file that is no generator.
fn no_generator(path: &str) -> String {
    format!("`{path}` is no generator, so `render` takes no `index` for it")
}

/// The page `store` holds at `path` and `index`, for a render of it — `None` for a file
/// that is no page, `is_page` saying whether it is one — or why the reference names no
/// page the last load made: a generator's path with no index, an index past the pages
/// it made, an index for a page that is no generator's, or a page the load did not
/// make. The store keeps no count of what a generator made; it is counted here, from
/// the pages that share the path.
fn loaded_page<'s>(
    store: &'s Store,
    path: &str,
    index: Option<u32>,
    is_page: bool,
) -> Result<Option<&'s LoadedPage>, String> {
    if let Some(found) = store.page(path, index) {
        return Ok(Some(found));
    }
    let made = store
        .pages
        .iter()
        .filter(|page| page.path == path && page.generated.is_some())
        .count();
    match index {
        None if made > 0 => Err(format!(
            "`{path}` is a generator: `render` takes the `index` of one of its pages, as \
             `plan()` lists them"
        )),
        Some(index) if made > 0 => Err(format!(
            "`{path}` has no page {index}: the last load made {made} from its `for:`"
        )),
        Some(_) if store.page(path, None).is_some() => Err(no_generator(path)),
        _ if is_page => Err(format!(
            "`{path}` is a page the last load did not make: `plan()` lists the pages it did"
        )),
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The diagnostics of a `compile` reply, `{diagnostics, payloads}`.
    fn diagnostics(json: &str) -> serde_json::Value {
        let reply: serde_json::Value = serde_json::from_str(json).expect("a JSON reply");
        reply["diagnostics"].clone()
    }

    /// The payloads of a `compile` reply.
    fn payloads(json: &str) -> serde_json::Value {
        let reply: serde_json::Value = serde_json::from_str(json).expect("a JSON reply");
        reply["payloads"].clone()
    }

    /// The host most tests compile against: the three names the standard shell binds,
    /// none of them typed. A core given no declaration at all has no ambient values, so
    /// a test that reads one says so (D30).
    const UNTYPED: &str = r#"{"ambient": {"site": null, "page": null, "build": null}}"#;

    fn compile_clean_with(core: &mut Core, files: &str, host: Option<&str>) {
        let result = core.compile_json(files, host).unwrap();
        assert_eq!(
            diagnostics(&result),
            serde_json::json!([]),
            "expected a clean compile"
        );
    }

    fn compile_clean(core: &mut Core, files: &str) {
        compile_clean_with(core, files, Some(UNTYPED));
    }

    /// The declaration a typed host makes: `Page` is declared in a file it compiles
    /// with the program, and a document says which record its content is.
    const TYPED: &str = r#"{"ambient": {"page": "Page"}, "document": "page", "entry": "layout", "url": "url"}"#;

    const SCHEMA: &str = r#"[
        {"path": "ambient.lem", "text": "<lem-type Page>\n  url: url\n</lem-type>"},
        {"path": "schema.lem", "text": "<lem-type Article extends Page>\n  title: string\n  layout: (Article, html): html\n</lem-type>"}
    ]"#;

    /// A typed site: the host reads `site` from `site.lem`, whose bindings collect the
    /// documents of `posts/*.md` as `Post`s, load a data file, and take an environment
    /// value. `posts/a.md` is one such document.
    const SITE_HOST: &str = r#"{"ambient": {"page": "Page", "site": {"bindingsOf": "site.lem"}}, "document": "page", "entry": "layout", "url": "url"}"#;

    /// A page template no glob holds, typed by its own key as a `Post`.
    const HOME_LEM: &str = "---\ntype: Post\ntitle: Home\n---\n<lem.page.title>";

    /// The typed site with `home.lem` compiled beside it, and the envelope carrying
    /// the template in `pages` with the url given.
    fn site_core_with_home() -> (Core, serde_json::Value) {
        let mut core = site_core();
        compile_clean_with(
            &mut core,
            &serde_json::json!([{"path": "home.lem", "text": HOME_LEM, "record": "Post"}])
                .to_string(),
            None,
        );
        let mut envelope = loaded();
        envelope["pages"] = serde_json::json!([{"path": "home.lem", "value": {"title": "Home", "url": "/"}}]);
        (core, envelope)
    }

    /// The pages `plan` lists, as `(path, url)`.
    fn planned(core: &Core) -> Vec<(String, Option<String>)> {
        let plan: serde_json::Value =
            serde_json::from_str(&core.plan_json().unwrap()).expect("a JSON reply");
        plan.as_array()
            .expect("an array")
            .iter()
            .map(|page| {
                (
                    page["path"].as_str().expect("a path").to_owned(),
                    page["url"].as_str().map(str::to_owned),
                )
            })
            .collect()
    }

    const AMBIENT_LEM: &str = "<lem-type Page>\n  url: url\n</lem-type>";

    /// Twelve lines; the `motto` binding is the last, line 11.
    const SITE_LEM: &str = "<lem-type Photo>\n  caption: string\n</lem-type>\n\
        <lem-type Post extends Page>\n  title: string\n  slug: string = path\n  \
        layout: (Post, html): html = post\n</lem-type>\n\
        <lem-fn post(p: Post, body: html)><h1><lem.p.title></h1><lem.body></lem-fn>\n\
        <lem-let posts: [Post] from \"posts/*.md\" />\n\
        <lem-let photos: {string: Photo} from \"_data/photos.json\" />\n\
        <lem-let motto: string from env(\"MOTTO\") />";

    const POST_MD: &str = "---\ntitle: A\nurl: /a/\n---\n<lem.page.title>\n<lem.page.slug>\n<lem.site.motto>\n";

    fn site_files() -> String {
        serde_json::json!([
            {"path": "ambient.lem", "text": AMBIENT_LEM},
            {"path": "site.lem", "text": SITE_LEM},
            {"path": "posts/a.md", "text": POST_MD, "record": "Post"},
        ])
        .to_string()
    }

    /// A core with the typed site compiled clean.
    fn site_core() -> Core {
        let mut core = Core::new();
        compile_clean_with(&mut core, &site_files(), Some(SITE_HOST));
        core
    }

    /// The envelope that loads the typed site cleanly; a test varies it by path.
    fn loaded() -> serde_json::Value {
        serde_json::json!({"ambient": {}, "sources": {
            "posts": [{"path": "posts/a.md", "value": {"title": "A", "url": "/a/"}}],
            "photos": {"path": "_data/photos.json", "value": {}},
            "motto": {"value": "hi"}
        }})
    }

    /// Loads `envelope`, returning its diagnostics: `loadData` replies with the array.
    fn load(core: &mut Core, envelope: &serde_json::Value) -> serde_json::Value {
        serde_json::from_str(&core.load_data_json(&envelope.to_string()).unwrap())
            .expect("a JSON reply")
    }

    /// A span as a host writes one, `start..end` running from one position to another.
    fn span(start: u32, end: u32, from: (u32, u32), to: (u32, u32)) -> serde_json::Value {
        serde_json::json!({"start": start, "end": end, "range": {
            "start": {"line": from.0, "character": from.1},
            "end": {"line": to.0, "character": to.1},
        }})
    }

    /// A render's reply taken apart: its metadata, and the page bytes after it — `None`
    /// where the render failed and sent no page section at all.
    fn decode(reply: &[u8]) -> (serde_json::Value, Option<String>) {
        let (header, rest) = reply.split_at(4);
        let length = u32::from_le_bytes(header.try_into().expect("four bytes"));
        let (meta, html) = rest.split_at(length as usize);
        let meta: serde_json::Value = serde_json::from_slice(meta).unwrap();
        let page = String::from_utf8(html.to_vec()).expect("a page is UTF-8");
        let failed = meta["marks"].is_null();
        assert_eq!(
            failed,
            page.is_empty() && meta["diagnostics"][0].is_object()
        );
        (meta, (!failed).then_some(page))
    }

    fn rendered(core: &Core, path: &str) -> (serde_json::Value, Option<String>) {
        let reply = core
            .render_bytes(&format!("{{\"path\": \"{path}\"}}"))
            .unwrap();
        decode(&reply)
    }

    fn html_of(core: &Core, path: &str) -> String {
        let (meta, html) = rendered(core, path);
        assert_eq!(meta["diagnostics"], serde_json::json!([]));
        html.expect("html on success")
    }

    #[test]
    fn reports_the_crate_version() {
        assert_eq!(version(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn compiles_loads_and_renders_across_the_boundary() {
        let mut core = Core::new();
        compile_clean(
            &mut core,
            r#"[
                {"path": "lib.lem", "text": "<lem-fn shout(s: string): string = s + \"!\" />"},
                {"path": "page.lem", "text": "<lem.shout(page.title)> on <lem.site.name>"}
            ]"#,
        );
        let loaded = core
            .load_data_json(
                r#"{"ambient": {"page": {"title": "Hi"}, "site": {"name": "limulus.net"}}}"#,
            )
            .unwrap();
        assert_eq!(loaded, "[]");
        assert_eq!(html_of(&core, "page.lem"), "Hi! on limulus.net");
    }

    #[test]
    fn lets_a_declaration_move_between_two_files_in_one_compile() {
        // The new home comes first in the batch, while the old one still owns the
        // name: withdrawing every incoming file before offering any is what keeps
        // this from being a collision — or a panic in the new home's compile.
        let mut core = Core::new();
        compile_clean(
            &mut core,
            r#"[{"path":"a.lem","text":"<lem-type Article>\n  title: string\n</lem-type>\n<lem-fn f(): int = 1 />"},{"path":"page.lem","text":"<lem-fn card(a: Article): string = a.title />"}]"#,
        );
        compile_clean(
            &mut core,
            r#"[{"path":"types.lem","text":"<lem-type Article>\n  title: string\n</lem-type>\n<lem-fn f(): int = 2 />"},{"path":"a.lem","text":""}]"#,
        );
        assert_eq!(core.index.lookup_type("Article").unwrap().path, "types.lem");
        assert!(core.index.defined_in("f", "types.lem"));
    }

    #[test]
    fn recompiles_only_the_changed_file_and_keeps_the_rest_resident() {
        let mut core = Core::new();
        compile_clean(
            &mut core,
            r#"[
                {"path": "lib.lem", "text": "<lem-fn n(): int = 1 />"},
                {"path": "page.lem", "text": "<lem.n()>"}
            ]"#,
        );
        core.load_data_json(r#"{"ambient": {}}"#).unwrap();
        assert_eq!(html_of(&core, "page.lem"), "1");
        compile_clean(
            &mut core,
            r#"[{"path": "lib.lem", "text": "<lem-fn n(): int = 2 />"}]"#,
        );
        assert_eq!(html_of(&core, "page.lem"), "2");
    }

    #[test]
    fn keeps_the_last_good_program_when_a_recompile_fails() {
        let mut core = Core::new();
        compile_clean(&mut core, r#"[{"path": "page.lem", "text": "ok"}]"#);
        core.load_data_json(r#"{"ambient": {}}"#).unwrap();
        let result = core
            .compile_json(r#"[{"path": "page.lem", "text": "<lem-if x>"}]"#, None)
            .unwrap();
        let reported = diagnostics(&result);
        assert_eq!(reported[0]["path"], "page.lem");
        assert!(reported[0]["span"]["end"].is_number(), "{reported:?}");
        assert_eq!(reported[0]["severity"], "error");
        assert_eq!(reported[0]["labels"], serde_json::json!([]));
        // Stale until fixed: the broken compile replaced neither unit nor module.
        assert_eq!(html_of(&core, "page.lem"), "ok");
    }

    #[test]
    fn replaces_the_program_when_a_compile_only_warns() {
        let mut core = Core::new();
        compile_clean(&mut core, r#"[{"path": "page.lem", "text": "old"}]"#);
        core.load_data_json(r#"{"ambient": {}}"#).unwrap();
        let result = core
            .compile_json(
                r#"[{"path": "page.lem", "text": "<lem-let raw = 1 />new<lem.raw>"}]"#,
                None,
            )
            .unwrap();
        let reported = diagnostics(&result);
        assert_eq!(reported.as_array().map(Vec::len), Some(1), "{reported:?}");
        assert_eq!(reported[0]["severity"], "warning");
        assert_eq!(reported[0]["span"]["start"], 9);
        assert_eq!(html_of(&core, "page.lem"), "new1");
    }

    #[test]
    fn reports_a_relink_that_a_resident_unit_no_longer_satisfies() {
        let mut core = Core::new();
        compile_clean(
            &mut core,
            r#"[
                {"path": "lib.lem", "text": "<lem-fn n(): int = 1 />"},
                {"path": "page.lem", "text": "<lem.n()>"}
            ]"#,
        );
        core.load_data_json(r#"{"ambient": {}}"#).unwrap();
        // lib.lem drops `n`: its own compile is clean, but page.lem's resident unit
        // still imports it, so the relink reports where page.lem uses the name.
        let result = core
            .compile_json(
                r#"[{"path": "lib.lem", "text": "no functions here"}]"#,
                None,
            )
            .unwrap();
        let reported = diagnostics(&result);
        assert_eq!(reported[0]["path"], "page.lem");
        assert!(
            reported[0]["message"]
                .as_str()
                .unwrap()
                .contains("undefined name `n`"),
            "{reported:?}"
        );
        assert_eq!(html_of(&core, "page.lem"), "1");
    }

    #[test]
    fn reports_a_caller_left_stale_by_a_signature_change_it_did_not_see() {
        let mut core = Core::new();
        compile_clean(
            &mut core,
            r#"[
                {"path": "lib.lem", "text": "<lem-fn pair(a: string, b: string): string = a + \"-\" + b />"},
                {"path": "page.lem", "text": "<lem.pair(b: \"B\", a: \"A\")>"}
            ]"#,
        );
        core.load_data_json(r#"{"ambient": {}}"#).unwrap();
        assert_eq!(html_of(&core, "page.lem"), "A-B");
        // Only lib.lem is recompiled, with its parameters swapped. page.lem reordered
        // its named arguments against the old list, so relinking it would silently
        // render `B-A`. It has to be told to recompile instead.
        let result = core
            .compile_json(
                r#"[{"path": "lib.lem", "text": "<lem-fn pair(b: string, a: string): string = a + \"-\" + b />"}]"#,
             None)
            .unwrap();
        let reported = diagnostics(&result);
        assert_eq!(reported[0]["path"], "page.lem");
        assert!(
            reported[0]["message"]
                .as_str()
                .unwrap()
                .contains("compiled against `pair(a: string, b: string): string`"),
            "{reported:?}"
        );
        // The last good program is what still renders, unchanged.
        assert_eq!(html_of(&core, "page.lem"), "A-B");
        // Recompiling the caller too is what clears it.
        compile_clean(
            &mut core,
            r#"[{"path": "page.lem", "text": "<lem.pair(b: \"B\", a: \"A\")>"}]"#,
        );
        assert_eq!(html_of(&core, "page.lem"), "A-B");
    }

    #[test]
    fn blames_the_file_just_compiled_for_a_collision_whatever_its_path_sorts_as() {
        let mut core = Core::new();
        compile_clean(
            &mut core,
            r#"[{"path": "z.lem", "text": "<lem-fn f(): int = 1 />"}]"#,
        );
        // `a.lem` sorts before the resident `z.lem`; the error still belongs to the
        // file that just introduced the name.
        let result = core
            .compile_json(
                r#"[{"path": "a.lem", "text": "<lem-fn f(): int = 2 />"}]"#,
                None,
            )
            .unwrap();
        let reported = diagnostics(&result);
        assert_eq!(reported[0]["path"], "a.lem");
        assert_eq!(reported[0]["labels"][0]["path"], "z.lem", "{reported:?}");
    }

    #[test]
    fn reports_a_definition_that_collides_with_a_resident_one() {
        let mut core = Core::new();
        compile_clean(
            &mut core,
            r#"[{"path": "a.lem", "text": "<lem-fn f(): int = 1 />"}]"#,
        );
        let result = core
            .compile_json(
                r#"[{"path": "b.lem", "text": "<lem-fn f(): int = 2 />"}]"#,
                None,
            )
            .unwrap();
        let reported = diagnostics(&result);
        assert_eq!(reported[0]["path"], "b.lem");
        assert_eq!(reported[0]["message"], "`f` is already defined");
        // The first definition crosses as a label, path and span intact.
        assert_eq!(reported[0]["labels"][0]["path"], "a.lem");
        assert_eq!(reported[0]["labels"][0]["span"]["start"], 8);
        assert_eq!(reported[0]["labels"][0]["message"], "first defined here");
    }

    /// The column is UTF-16 code units from the start of the line, so the `’` before the
    /// tag costs one column while costing three bytes. Asserting both is what tells the
    /// two apart: a position quietly rendered from the byte offset would read 11.
    #[test]
    fn places_a_diagnostic_at_its_line_and_column_not_its_byte_offset() {
        let mut core = Core::new();
        let result = core
            .compile_json(
                r#"[{"path": "page.lem", "text": "<p>ok</p>\nI’m <lem.nope>"}]"#,
                None,
            )
            .unwrap();
        let reported = diagnostics(&result);
        assert_eq!(reported[0]["path"], "page.lem");
        assert_eq!(reported[0]["span"]["start"], 21);
        assert_eq!(reported[0]["span"]["range"]["start"]["line"], 1);
        assert_eq!(reported[0]["span"]["range"]["start"]["character"], 9);
        assert_eq!(reported[0]["span"]["range"]["end"]["character"], 13);
    }

    /// A label is placed in the file *it* names, not the one the diagnostic is reported
    /// against. The two files put their `f` on different lines, so placing the label
    /// against `b.lem` would show up as line 0.
    #[test]
    fn places_a_label_in_the_file_it_points_into() {
        let mut core = Core::new();
        compile_clean(
            &mut core,
            r#"[{"path": "a.lem", "text": "<p>x</p>\n<lem-fn f(): int = 1 />"}]"#,
        );
        let result = core
            .compile_json(
                r#"[{"path": "b.lem", "text": "<lem-fn f(): int = 2 />"}]"#,
                None,
            )
            .unwrap();
        let reported = diagnostics(&result);
        assert_eq!(reported[0]["span"]["range"]["start"]["line"], 0);
        assert_eq!(reported[0]["labels"][0]["path"], "a.lem");
        assert_eq!(
            reported[0]["labels"][0]["span"]["range"]["start"]["line"],
            1
        );
        assert_eq!(
            reported[0]["labels"][0]["span"]["range"]["start"]["character"],
            8
        );
    }

    /// The case the resident indexes exist for: a relink blames a file the *reporting*
    /// call never received, whose text is long gone. Dropping an index with its call
    /// would panic here.
    #[test]
    fn places_a_diagnostic_about_a_file_this_call_never_received() {
        let mut core = Core::new();
        compile_clean(
            &mut core,
            r#"[
                {"path": "lib.lem", "text": "<lem-fn n(): int = 1 />"},
                {"path": "page.lem", "text": "<p>hi</p>\n<lem.n()>"}
            ]"#,
        );
        let result = core
            .compile_json(
                r#"[{"path": "lib.lem", "text": "no functions here"}]"#,
                None,
            )
            .unwrap();
        let reported = diagnostics(&result);
        assert_eq!(reported[0]["path"], "page.lem");
        assert_eq!(reported[0]["span"]["range"]["start"]["line"], 1);
        assert_eq!(reported[0]["span"]["range"]["start"]["character"], 5);
    }

    /// A file whose recompile fails keeps its previous unit, whose spans are offsets into
    /// the text that *produced* it. Placing them against the newer text at that path
    /// points at the wrong bytes — or, when the new text is shorter, past its end.
    #[test]
    fn places_a_stale_unit_s_span_against_the_text_that_produced_it() {
        let mut core = Core::new();
        compile_clean(
            &mut core,
            r#"[
                {"path": "lib.lem", "text": "<lem-fn n(): int = 1 />"},
                {"path": "page.lem", "text": "<p>a long first line</p>\n<lem.n()>"}
            ]"#,
        );
        // page.lem now fails to compile, so it keeps the unit above — but its text at
        // this path is henceforth this much shorter one.
        let failed = core
            .compile_json(r#"[{"path": "page.lem", "text": "<lem.nope>"}]"#, None)
            .unwrap();
        assert!(!diagnostics(&failed).as_array().unwrap().is_empty());
        // Now a clean compile relinks, and the stale unit still imports `n`.
        let result = core
            .compile_json(
                r#"[{"path": "lib.lem", "text": "no functions here"}]"#,
                None,
            )
            .unwrap();
        let reported = diagnostics(&result);
        assert_eq!(reported[0]["path"], "page.lem");
        assert_eq!(reported[0]["span"]["range"]["start"]["line"], 1);
        assert_eq!(reported[0]["span"]["range"]["start"]["character"], 5);
    }

    #[test]
    fn reports_a_collision_once_and_never_takes_the_refused_definition() {
        let mut core = Core::new();
        compile_clean(
            &mut core,
            r#"[{"path": "a.lem", "text": "<lem-fn f(): string = \"a\" />"}]"#,
        );
        let result = core
            .compile_json(
                r#"[{"path": "b.lem", "text": "<lem-fn f(): string = \"b\" />"}]"#,
                None,
            )
            .unwrap();
        let reported = diagnostics(&result);
        assert_eq!(
            reported.as_array().expect("an array").len(),
            1,
            "{reported:?}"
        );
        // The refused file compiled to nothing, so the program has no b.lem to render.
        assert!(core.render_bytes(r#"{"path": "b.lem"}"#).is_err());
        // And a later call is not told again about a collision it did not make: the
        // call that introduced it is the one that reported it.
        compile_clean(&mut core, r#"[{"path": "page.lem", "text": "<lem.f()>"}]"#);
        assert_eq!(html_of(&core, "page.lem"), "a");
    }

    #[test]
    fn keeps_the_index_describing_the_units_a_failed_compile_left_resident() {
        let mut core = Core::new();
        compile_clean(
            &mut core,
            r#"[
                {"path": "lib.lem", "text": "<lem-fn f(): string = \"one\" />"},
                {"path": "page.lem", "text": "<lem.f()>"}
            ]"#,
        );
        // The edit defines a function ahead of `f`, moving `f` to another index in the
        // unit, and fails — so the resident unit, where `f` is still the first, stands.
        let result = core
            .compile_json(
                r#"[{"path": "lib.lem", "text": "<lem-fn g(): string = \"g\" /><lem-fn f(): string = \"two\" /><lem.nope()>"}]"#,
             None)
            .unwrap();
        let reported = diagnostics(&result);
        assert_eq!(reported[0]["path"], "lib.lem", "{reported:?}");
        // Relinking the caller has to resolve `f` against the unit that is there, not
        // against the indices of the one that never landed.
        compile_clean(&mut core, r#"[{"path": "page.lem", "text": "<lem.f()>"}]"#);
        assert_eq!(html_of(&core, "page.lem"), "one");
    }

    #[test]
    fn refuses_a_call_that_names_one_file_twice_and_keeps_the_program() {
        let mut core = Core::new();
        compile_clean(
            &mut core,
            r#"[{"path": "page.lem", "text": "<lem-fn f(): string = \"one\" /><lem.f()>"}]"#,
        );
        // A caller bug, not a template problem: it has no span to point at, so it comes
        // back as a thrown error — and nothing was offered, so the core is untouched.
        let refused = core.compile_json(
            r#"[{"path": "a.lem", "text": "<lem-fn g(): int = 1 />"},
                {"path": "a.lem", "text": "<lem-fn h(): int = 2 />"}]"#,
            None,
        );
        assert_eq!(
            refused.unwrap_err(),
            "malformed `compile` input: `a.lem` given twice"
        );
        assert_eq!(html_of(&core, "page.lem"), "one");
    }

    #[test]
    fn maps_json_data_onto_the_vms_values() {
        let mut core = Core::new();
        compile_clean(
            &mut core,
            concat!(
                r#"[{"path": "page.lem", "text": "#,
                r#""<lem.page.i>|<lem.page.f>|<lem-if page.b>yes</lem-if>|"#,
                r#"<lem.page.opt ?? \"none\">|"#,
                r#"<lem-for x in page.list><lem.x>,</lem-for>|"#,
                r#"<lem-for k, v in page.map><lem.k>=<lem.v>;</lem-for>"}]"#,
            ),
        );
        core.load_data_json(
            r#"{"ambient": {"page": {
                "i": 3, "f": 1.5, "b": true, "opt": null,
                "list": [1, 2],
                "map": {"z": 1, "a": 2}
            }}}"#,
        )
        .unwrap();
        // `z` before `a`: the shell's key order survives into the map (preserve_order).
        assert_eq!(html_of(&core, "page.lem"), "3|1.5|yes|none|1,2,|z=1;a=2;");
    }

    #[test]
    fn prints_a_float_from_data_and_front_matter_as_the_host_wrote_it() {
        // Coordinates from limulus.net's photos, in the shortest text that reads back as
        // their double — what `JSON.stringify` sends. A fast, inexact reader lands three
        // of the five on the double one unit in the last place away, and prints that.
        let site = "<lem-type Photo>\n  latitude: float\n  longitude: float\n</lem-type>\n\
            <lem-type Post extends Page>\n  longitude: float\n  \
            layout: (Post, html): html = post\n</lem-type>\n\
            <lem-fn post(p: Post, body: html)><lem.p.longitude>|<lem.body></lem-fn>\n\
            <lem-let posts: [Post] from \"posts/*.md\" />\n\
            <lem-let photos: {string: Photo} from \"_data/photos.json\" />";
        let post = "---\nlongitude: -110.76918888888889\nurl: /a/\n---\n\
            <lem-for alias, photo in site.photos><lem.alias> \
            <lem.photo.latitude>,<lem.photo.longitude>;</lem-for>";
        let mut core = Core::new();
        compile_clean_with(
            &mut core,
            &serde_json::json!([
                {"path": "ambient.lem", "text": AMBIENT_LEM},
                {"path": "site.lem", "text": site},
                {"path": "posts/a.md", "text": post, "record": "Post"},
            ])
            .to_string(),
            Some(SITE_HOST),
        );
        let reply = core
            .load_data_json(
                r#"{"sources": {
                    "posts": [{"path": "posts/a.md",
                        "value": {"longitude": -110.76918888888889, "url": "/a/"}}],
                    "photos": {"path": "_data/photos.json", "value": {
                        "split": {"latitude": 32.44250555555555,
                            "longitude": -110.78919166666667},
                        "all-in-all-by-beth-lipman": {"latitude": 42.150394444444444,
                            "longitude": -77.05377222222222}}}
                }}"#,
            )
            .unwrap();
        assert_eq!(reply, "[]");
        assert_eq!(
            html_of(&core, "posts/a.md"),
            "-110.76918888888889|split 32.44250555555555,-110.78919166666667;\
            all-in-all-by-beth-lipman 42.150394444444444,-77.05377222222222;\n"
        );
    }

    /// The declaration a host with handlers makes: the three names the shell's own
    /// bundler claims (D42).
    const CLAIMING: &str = r#"{"ambient": {"site": null, "page": null, "build": null},
        "marks": [{"name": "styles"}, {"name": "head"}, {"name": "scripts"}]}"#;

    #[test]
    fn sends_the_page_as_bytes_beside_its_marks_and_fragments() {
        // The offsets name the page's own bytes, so a mark after text no ASCII could
        // hold is where the host must splice — not where a character count would put it.
        let mut core = Core::new();
        let text = "<lem-fn card()>\n  <p>café — señor</p>\n  <lem-style>\n    .card {}\n                      </lem-style>\n</lem-fn>\n<lem.card()><lem.mark(\"styles\")>";
        let files = serde_json::json!([{"path": "page.lem", "text": text}]).to_string();
        compile_clean_with(&mut core, &files, Some(CLAIMING));
        let (meta, html) = rendered(&core, "page.lem");
        let html = html.expect("a page");
        assert_eq!(html, "<p>café — señor</p>");
        let mark = &meta["marks"][0];
        assert_eq!(
            (&mark["name"], &mark["start"], &mark["end"], &mark["path"]),
            (
                &serde_json::json!("styles"),
                &serde_json::json!(html.len()),
                &serde_json::json!(html.len()),
                &serde_json::json!("page.lem")
            )
        );
        // The span is the call, placed for an editor as a diagnostic's is (D33).
        let at = text.find("mark(").expect("the call") as u32;
        assert_eq!(mark["span"]["start"], at);
        assert_eq!(mark["span"]["range"]["start"]["line"], 6);
        let fragment = &meta["fragments"]["styles"][0];
        assert_eq!(fragment["text"], ".card {}");
        assert_eq!(fragment["path"], "page.lem");
        assert_eq!(
            fragment["span"]["start"],
            text.find("<lem-style>").expect("the block") as u32
        );
    }

    #[test]
    fn sends_an_empty_page_as_an_empty_section_rather_than_none() {
        let mut core = Core::new();
        compile_clean(&mut core, r#"[{"path": "page.lem", "text": ""}]"#);
        let (meta, html) = rendered(&core, "page.lem");
        assert_eq!(html.as_deref(), Some(""));
        assert_eq!(meta["marks"], serde_json::json!([]));
        assert_eq!(meta["fragments"], serde_json::json!({}));
    }

    #[test]
    fn stales_a_unit_whose_mark_the_declaration_stops_claiming() {
        // What the claims said was answered while the file compiled, so a plugin list
        // that changes under it leaves exactly the files that mark stale (D42, D25).
        let mut core = Core::new();
        let files = r#"[{"path": "page.lem", "text": "<lem.mark(\"styles\")>"}]"#;
        compile_clean_with(&mut core, files, Some(CLAIMING));
        // Another file arrives under a declaration whose handlers claim nothing: the
        // page's unit is resident and marks, so relinking it reports.
        let other = r#"[{"path": "other.lem", "text": "<p>b</p>"}]"#;
        let diagnostics = diagnostics(&core.compile_json(other, Some(UNTYPED)).unwrap());
        assert_eq!(diagnostics[0]["path"], "page.lem");
        assert!(
            diagnostics[0]["message"]
                .as_str()
                .is_some_and(|m| m.contains("no handler claims `styles` now")),
            "{diagnostics}"
        );
        // The same change says nothing about a file that marks nothing.
        let mut core = Core::new();
        compile_clean_with(&mut core, other, Some(CLAIMING));
        compile_clean_with(&mut core, other, Some(UNTYPED));
    }

    #[test]
    fn renders_a_runtime_failure_as_a_diagnostic_not_a_throw() {
        let mut core = Core::new();
        compile_clean(
            &mut core,
            r#"[{"path": "page.lem", "text": "<lem.1 / 0>"}]"#,
        );
        let (result, html) = rendered(&core, "page.lem");
        // A failed render sends no page section, and says so with null marks and
        // fragments rather than by its length: an empty page is a page.
        assert_eq!(html, None);
        assert_eq!(result["marks"], serde_json::Value::Null);
        assert_eq!(result["fragments"], serde_json::Value::Null);
        assert_eq!(result["diagnostics"][0]["path"], "page.lem");
        assert_eq!(result["diagnostics"][0]["span"]["start"], 5);
        assert_eq!(
            result["diagnostics"][0]["span"]["range"]["start"]["line"],
            0
        );
        assert_eq!(
            result["diagnostics"][0]["span"]["range"]["start"]["character"],
            5
        );
    }

    #[test]
    fn refuses_to_render_what_it_cannot_have_compiled() {
        let mut core = Core::new();
        let error = core.render_bytes(r#"{"path": "page.lem"}"#).unwrap_err();
        assert!(error.contains("`compile` first"), "{error}");
        compile_clean(&mut core, r#"[{"path": "page.lem", "text": "ok"}]"#);
        let error = core.render_bytes(r#"{"path": "other.lem"}"#).unwrap_err();
        assert_eq!(error, "no compiled file at `other.lem`");
    }

    #[test]
    fn refuses_malformed_envelopes_with_a_throwable_message() {
        let mut core = Core::new();
        assert!(
            core.compile_json("not json", None)
                .unwrap_err()
                .starts_with("malformed `compile` input:")
        );
        assert!(
            core.load_data_json(r#"{"nope": 1}"#)
                .unwrap_err()
                .starts_with("malformed `loadData` input:")
        );
        assert!(
            core.render_bytes(r#"{}"#)
                .unwrap_err()
                .starts_with("malformed `render` input:")
        );
    }

    #[test]
    fn loads_the_documents_of_a_glob_and_binds_each_at_render_as_its_own_record() {
        let mut core = site_core();
        assert_eq!(load(&mut core, &loaded()), serde_json::json!([]));
        // `title` was validated, `slug`'s default ran over the document's path, and
        // `motto` came through `site` — all bound at the render of that document.
        let html = html_of(&core, "posts/a.md");
        assert!(html.contains("A\nposts/a.md\nhi"), "{html}");
    }

    /// The `sources` of a `compile` reply.
    fn sources(json: &str) -> serde_json::Value {
        let reply: serde_json::Value = serde_json::from_str(json).expect("a JSON reply");
        reply["sources"].clone()
    }

    #[test]
    fn reports_the_from_bindings_of_the_program_it_installs() {
        let mut core = Core::new();
        let reply = core.compile_json(&site_files(), Some(SITE_HOST)).unwrap();
        let reported = sources(&reply);
        let shown: Vec<(&str, &str, &str, Option<&str>)> = reported
            .as_array()
            .expect("an array")
            .iter()
            .map(|source| {
                (
                    source["name"].as_str().unwrap(),
                    source["kind"].as_str().unwrap(),
                    source["literal"].as_str().unwrap(),
                    source["collects"].as_str(),
                )
            })
            .collect();
        assert_eq!(
            shown,
            [
                ("posts", "file", "posts/*.md", Some("Post")),
                ("photos", "file", "_data/photos.json", None),
                ("motto", "env", "MOTTO", None),
            ]
        );
        // A data file's `collects` is absent rather than null, and the binding is
        // placed at its name, in bytes and positions both.
        let motto = &reported[2];
        assert!(motto.get("collects").is_none(), "{motto}");
        assert_eq!(motto["path"], "site.lem");
        let (start, end) = (
            motto["span"]["start"].as_u64().unwrap() as usize,
            motto["span"]["end"].as_u64().unwrap() as usize,
        );
        assert_eq!(&SITE_LEM[start..end], "motto");
        assert_eq!(motto["span"]["range"]["start"]["line"], 11);
    }

    #[test]
    fn places_a_source_by_the_index_of_the_bindings_file_it_stands_in() {
        // The host reads `data` from `data.lem`, and a `site.lem` beside it is a plain
        // template of one line: placed by any index but `data.lem`'s, the binding on
        // that file's fifth line would land elsewhere, or past the end of the text.
        const DATA_LEM: &str = "<lem-type Photo>\n  caption: string\n</lem-type>\n\n\
            <lem-let photos: {string: Photo} from \"_data/photos.json\" />";
        let files = serde_json::json!([
            {"path": "ambient.lem", "text": AMBIENT_LEM},
            {"path": "site.lem", "text": "<lem-fn decoy()>x</lem-fn>"},
            {"path": "data.lem", "text": DATA_LEM},
        ]);
        let host = r#"{"ambient": {"page": "Page", "data": {"bindingsOf": "data.lem"}},
            "document": "page", "entry": "layout", "url": "url"}"#;
        let mut core = Core::new();
        let reply = core.compile_json(&files.to_string(), Some(host)).unwrap();
        assert_eq!(diagnostics(&reply), serde_json::json!([]));
        let photos = &sources(&reply)[0];
        assert_eq!(photos["path"], "data.lem");
        let (start, end) = (
            photos["span"]["start"].as_u64().unwrap() as usize,
            photos["span"]["end"].as_u64().unwrap() as usize,
        );
        assert_eq!(&DATA_LEM[start..end], "photos");
        assert_eq!(
            photos["span"]["range"]["start"],
            serde_json::json!({"line": 4, "character": 9})
        );
    }

    #[test]
    fn reports_the_sources_from_every_compile_that_installs_a_program() {
        // A call carrying only a page relinks the resident bindings file with it, so
        // the program it installs reports what the host loads for it — the same list.
        let mut core = site_core();
        let page = serde_json::json!([
            {"path": "posts/a.md", "text": POST_MD, "record": "Post"}
        ]);
        let reply = core.compile_json(&page.to_string(), None).unwrap();
        assert_eq!(diagnostics(&reply), serde_json::json!([]));
        assert_eq!(sources(&reply).as_array().map(Vec::len), Some(3));
    }

    #[test]
    fn reports_no_sources_from_a_compile_that_installs_no_program() {
        let mut core = site_core();
        let failed = core
            .compile_json(r#"[{"path": "posts/a.md", "text": "<lem-if x>"}]"#, None)
            .unwrap();
        assert_eq!(diagnostics(&failed)[0]["severity"], "error");
        assert_eq!(sources(&failed), serde_json::json!([]));
        // A program with no bindings file installs, and has nothing to load.
        let mut bare = Core::new();
        let reply = bare
            .compile_json(r#"[{"path": "page.lem", "text": "hi"}]"#, Some(UNTYPED))
            .unwrap();
        assert_eq!(reply, r#"{"diagnostics":[],"payloads":[],"sources":[]}"#);
    }

    #[test]
    fn binds_the_envelopes_document_value_only_where_no_loaded_document_stands() {
        let mut core = site_core();
        // A document no collection matched, compiled in the resident environment.
        compile_clean_with(
            &mut core,
            r#"[{"path": "loose.md", "text": "<lem.page.title>"}]"#,
            None,
        );
        let mut envelope = loaded();
        envelope["ambient"]["page"] = serde_json::json!({"title": "Loose"});
        assert_eq!(load(&mut core, &envelope), serde_json::json!([]));
        // The unmatched document reads the value the envelope carried; the loaded one
        // reads its own record, not the envelope's.
        assert!(html_of(&core, "loose.md").contains("Loose"));
        assert!(html_of(&core, "posts/a.md").contains("A\nposts/a.md"));
    }

    /// A load diagnostic about front matter is placed by the positions the host sent,
    /// as a data file's has to be: `_data/photos.json` was never compiled, so no index
    /// could place it, and the host's span comes back as sent.
    #[test]
    fn places_a_load_diagnostic_by_the_positions_the_host_sent() {
        let mut core = site_core();
        let mut envelope = loaded();
        let post = &mut envelope["sources"]["posts"][0];
        post["value"]["title"] = serde_json::json!(3);
        post["spans"] = serde_json::json!({"title": span(4, 12, (1, 0), (1, 8))});
        let photos = &mut envelope["sources"]["photos"];
        photos["value"] = serde_json::json!({"x": {"caption": 3}});
        photos["spans"] = serde_json::json!({"x.caption": span(20, 32, (2, 4), (2, 16))});
        let reported = load(&mut core, &envelope);
        assert_eq!(reported.as_array().map(Vec::len), Some(2), "{reported:?}");
        assert_eq!(reported[0]["path"], "posts/a.md");
        assert_eq!(reported[0]["span"], span(4, 12, (1, 0), (1, 8)));
        assert!(
            reported[0]["message"].as_str().unwrap().contains("string"),
            "{reported:?}"
        );
        assert_eq!(reported[1]["path"], "_data/photos.json");
        assert_eq!(reported[1]["span"], span(20, 32, (2, 4), (2, 16)));
    }

    #[test]
    fn places_a_load_diagnostic_at_the_declaration_when_the_host_sent_no_positions() {
        let mut core = site_core();
        // An environment value has no file behind it, so the error is at the `from`
        // binding — the last line of site.lem — placed by the program's index.
        let mut envelope = loaded();
        envelope["sources"]["motto"]["value"] = serde_json::json!(3);
        let reported = load(&mut core, &envelope);
        assert_eq!(reported[0]["path"], "site.lem", "{reported:?}");
        assert_eq!(reported[0]["span"]["range"]["start"]["line"], 11);
        assert_eq!(reported[0]["severity"], "error");
    }

    /// A program's spans are placed against the text it was linked from. The resident
    /// units run ahead of that whenever a call compiles a file cleanly and then fails
    /// to link; placed against their newer, shorter text, the old span would be past
    /// the end of the file — a panic in the line index, and a trap at the boundary.
    #[test]
    fn places_a_load_diagnostic_against_the_text_the_program_was_linked_from() {
        let mut core = site_core();
        let failed = core
            .compile_json(
                r#"[{"path": "site.lem", "text": "x"}, {"path": "broken.lem", "text": "<lem-if x>"}]"#,
                None,
            )
            .unwrap();
        assert!(!diagnostics(&failed).as_array().unwrap().is_empty());
        let mut envelope = loaded();
        envelope["sources"]["motto"]["value"] = serde_json::json!(3);
        let reported = load(&mut core, &envelope);
        assert_eq!(reported[0]["path"], "site.lem", "{reported:?}");
        assert_eq!(reported[0]["span"]["range"]["start"]["line"], 11);
    }

    #[test]
    fn keeps_the_last_good_data_when_a_load_reports_an_error() {
        let mut core = site_core();
        assert_eq!(load(&mut core, &loaded()), serde_json::json!([]));
        let mut envelope = loaded();
        envelope["sources"]["posts"][0]["value"]["title"] = serde_json::json!(3);
        let reported = load(&mut core, &envelope);
        assert_eq!(reported.as_array().map(Vec::len), Some(1), "{reported:?}");
        // Stale until fixed: the render still binds what the last good load built.
        assert!(html_of(&core, "posts/a.md").contains("A\nposts/a.md\nhi"));
    }

    #[test]
    fn keeps_the_loaded_data_when_a_compile_fails() {
        let mut core = site_core();
        assert_eq!(load(&mut core, &loaded()), serde_json::json!([]));
        let failed = core
            .compile_json(r#"[{"path": "broken.lem", "text": "<lem-if x>"}]"#, None)
            .unwrap();
        assert!(!diagnostics(&failed).as_array().unwrap().is_empty());
        // The program stands, and so does the data loaded against it.
        assert!(html_of(&core, "posts/a.md").contains("A\nposts/a.md\nhi"));
    }

    /// The store is a function of the program and the data. A compile that installs
    /// a new program — here one default changed; a relink also renumbers every
    /// function a stored `layout` names — drops it rather than binding what the old
    /// program computed.
    #[test]
    fn drops_the_loaded_data_when_a_compile_installs_a_new_program() {
        let mut core = site_core();
        assert_eq!(load(&mut core, &loaded()), serde_json::json!([]));
        let edited = SITE_LEM.replace("slug: string = path", "slug: string = \"/\" + path");
        assert_ne!(edited, SITE_LEM);
        compile_clean_with(
            &mut core,
            &serde_json::json!([{"path": "site.lem", "text": edited}]).to_string(),
            None,
        );
        // Until the data is loaded again nothing is bound: a render of the page is
        // refused rather than showing the old program's value, and so is a plan.
        let refused = core.render_bytes(r#"{"path": "posts/a.md"}"#).unwrap_err();
        assert_eq!(
            refused,
            "`posts/a.md` is a page, and nothing is loaded for it to bind: `loadData` first"
        );
        assert!(core.plan_json().unwrap_err().contains("`loadData`"));
        assert_eq!(load(&mut core, &loaded()), serde_json::json!([]));
        assert!(html_of(&core, "posts/a.md").contains("A\n/posts/a.md\nhi"));
    }

    #[test]
    fn renders_a_loaded_page_through_its_layout() {
        // The document's own output is the body its layout takes; a page template in
        // `pages` renders the same way (D43).
        let (mut core, envelope) = site_core_with_home();
        assert_eq!(load(&mut core, &envelope), serde_json::json!([]));
        assert_eq!(
            html_of(&core, "posts/a.md"),
            "<h1>A</h1>A\nposts/a.md\nhi\n"
        );
        assert_eq!(html_of(&core, "home.lem"), "<h1>Home</h1>Home");
    }

    #[test]
    fn plans_every_loaded_page_in_load_order_with_its_url() {
        let (mut core, envelope) = site_core_with_home();
        assert_eq!(load(&mut core, &envelope), serde_json::json!([]));
        assert_eq!(
            planned(&core),
            [
                ("posts/a.md".to_owned(), Some("/a/".to_owned())),
                ("home.lem".to_owned(), Some("/".to_owned())),
            ]
        );
        // A host that names no url field lists the pages and no url.
        let unplaced = SITE_HOST.replace(r#", "url": "url""#, "");
        assert_ne!(unplaced, SITE_HOST);
        compile_clean_with(&mut core, "[]", Some(&unplaced));
        assert_eq!(load(&mut core, &envelope), serde_json::json!([]));
        assert_eq!(
            planned(&core),
            [
                ("posts/a.md".to_owned(), None),
                ("home.lem".to_owned(), None)
            ]
        );
    }

    #[test]
    fn plans_nothing_before_a_load_has_succeeded_and_the_stale_data_after_one_fails() {
        let fresh = Core::new();
        assert!(fresh.plan_json().unwrap_err().contains("`loadData`"));
        let (mut core, envelope) = site_core_with_home();
        assert!(core.plan_json().unwrap_err().contains("`loadData`"));
        assert_eq!(load(&mut core, &envelope), serde_json::json!([]));
        let mut broken = envelope.clone();
        broken["pages"][0]["value"]["title"] = serde_json::json!(3);
        assert_eq!(load(&mut core, &broken).as_array().map(Vec::len), Some(1));
        // Stale until fixed: the plan is the last good load's, as a render binds it.
        assert_eq!(planned(&core).len(), 2);
    }

    #[test]
    fn refuses_to_render_a_page_the_last_load_did_not_carry() {
        // `home.lem` is compiled as a `Post` and the envelope leaves it out: a draft,
        // or a file the shell deleted. It is not loaded, so the plan omits it, and a
        // render of it would bind the envelope's untyped value as a `Post`.
        let (mut core, mut envelope) = site_core_with_home();
        envelope["pages"] = serde_json::json!([]);
        assert_eq!(load(&mut core, &envelope), serde_json::json!([]));
        assert_eq!(planned(&core).len(), 1);
        assert_eq!(
            core.render_bytes(r#"{"path": "home.lem"}"#).unwrap_err(),
            "`home.lem` is a page the last load did not make: `plan()` lists the pages it \
             did"
        );
        // A file compiled with no record is no page, and renders as before.
        assert_eq!(
            html_of(&core, "posts/a.md"),
            "<h1>A</h1>A\nposts/a.md\nhi\n"
        );
    }

    /// A generator over a list `site.lem` computes, whose pages take their title and
    /// url from their type's defaults, written so that each of its pages prints its
    /// own item.
    const TOPICS_LEM: &str =
        "---\ntype: Topic\nfor:\n  topic: site.topics\n---\n<lem.page.topic>";

    /// The typed site with a topic type, the lists a generator pages over, and the
    /// generator `topics.lem` compiled beside it; and the envelope carrying the
    /// generator in `pages` with its `for:` pair as a host parses it, placed in
    /// `TOPICS_LEM`.
    fn generator_core() -> (Core, serde_json::Value) {
        let site = format!(
            "{SITE_LEM}\n<lem-type Topic extends Page>\n  topic: string\n  \
             url: url = url(\"/t/\" + self.topic + \"/\")\n  \
             layout: (Topic, html): html = topical\n</lem-type>\n\
             <lem-fn topical(t: Topic, body: html)><lem.t.url>:<lem.body></lem-fn>\n\
             <lem-let topics: [string] = [\"git\", \"node\", \"rust\"] />\n\
             <lem-let none: [string] = [] />"
        );
        let mut core = Core::new();
        let files = serde_json::json!([
            {"path": "ambient.lem", "text": AMBIENT_LEM},
            {"path": "site.lem", "text": site},
            {"path": "posts/a.md", "text": POST_MD, "record": "Post"},
            {"path": "topics.lem", "text": TOPICS_LEM, "record": "Topic"},
            {"path": "plain.lem", "text": "<lem-fn plain()>x</lem-fn>"},
        ]);
        compile_clean_with(&mut core, &files.to_string(), Some(SITE_HOST));
        let mut envelope = loaded();
        envelope["pages"] = serde_json::json!([{"path": "topics.lem", "value": {}, "for": {
            "value": {"topic": "site.topics"},
            "spans": {
                "": span(23, 41, (3, 2), (3, 20)),
                "topic": span(23, 41, (3, 2), (3, 20)),
            },
            "valueSpans": {"topic": span(30, 41, (3, 9), (3, 20))},
        }}]);
        (core, envelope)
    }

    fn rendered_at(core: &Core, page: &serde_json::Value) -> String {
        let (meta, html) = decode(&core.render_bytes(&page.to_string()).unwrap());
        assert_eq!(meta["diagnostics"], serde_json::json!([]));
        html.expect("html on success")
    }

    #[test]
    fn plans_and_renders_each_page_a_generator_made_by_its_index() {
        let (mut core, envelope) = generator_core();
        assert_eq!(load(&mut core, &envelope), serde_json::json!([]));
        let plan: serde_json::Value =
            serde_json::from_str(&core.plan_json().unwrap()).unwrap();
        assert_eq!(
            plan,
            serde_json::json!([
                {"path": "posts/a.md", "url": "/a/"},
                {"path": "topics.lem", "index": 0, "url": "/t/git/"},
                {"path": "topics.lem", "index": 1, "url": "/t/node/"},
                {"path": "topics.lem", "index": 2, "url": "/t/rust/"},
            ])
        );
        let page = |index: u32| serde_json::json!({"path": "topics.lem", "index": index});
        assert_eq!(rendered_at(&core, &page(0)), "/t/git/:git");
        assert_eq!(rendered_at(&core, &page(1)), "/t/node/:node");
        assert_eq!(rendered_at(&core, &page(2)), "/t/rust/:rust");
    }

    #[test]
    fn refuses_a_reference_to_a_page_the_last_load_did_not_make() {
        let (mut core, mut envelope) = generator_core();
        assert_eq!(load(&mut core, &envelope), serde_json::json!([]));
        let refused = |core: &Core, page: serde_json::Value| {
            core.render_bytes(&page.to_string()).unwrap_err()
        };
        assert_eq!(
            refused(&core, serde_json::json!({"path": "topics.lem"})),
            "`topics.lem` is a generator: `render` takes the `index` of one of its pages, \
             as `plan()` lists them"
        );
        assert_eq!(
            refused(&core, serde_json::json!({"path": "topics.lem", "index": 3})),
            "`topics.lem` has no page 3: the last load made 3 from its `for:`"
        );
        for path in ["posts/a.md", "plain.lem"] {
            assert_eq!(
                refused(&core, serde_json::json!({"path": path, "index": 0})),
                format!("`{path}` is no generator, so `render` takes no `index` for it")
            );
        }
        // An empty list makes no page: the generator is then a page the load did not
        // make, with an index or without.
        envelope["pages"][0]["for"]["value"]["topic"] = serde_json::json!("site.none");
        assert_eq!(load(&mut core, &envelope), serde_json::json!([]));
        for page in [
            serde_json::json!({"path": "topics.lem"}),
            serde_json::json!({"path": "topics.lem", "index": 0}),
        ] {
            assert_eq!(
                refused(&core, page),
                "`topics.lem` is a page the last load did not make: `plan()` lists the \
                 pages it did"
            );
        }
        // With nothing loaded, a file that is no page still takes no index.
        let (fresh, _) = generator_core();
        assert_eq!(
            fresh
                .render_bytes(r#"{"path": "plain.lem", "index": 0}"#)
                .unwrap_err(),
            "`plain.lem` is no generator, so `render` takes no `index` for it"
        );
        assert!(
            fresh
                .render_bytes(r#"{"path": "topics.lem", "index": 0}"#)
                .unwrap_err()
                .contains("`loadData` first")
        );
    }

    /// A typed site whose post writes a style, an id and a code block, and whose layout
    /// writes an id of the same prefix and places the styles: what a body render must
    /// return as it stands in the page (D46). Loaded, with the claims declared.
    fn body_core() -> Core {
        let host = SITE_HOST.replace(
            r#""url": "url""#,
            r#""url": "url", "marks": [{"name": "styles"},
                {"name": "code", "keys": ["sh"], "payload": "Code", "keyedBy": "language"}]"#,
        );
        let site = SITE_LEM.replace(
            "<lem-fn post(p: Post, body: html)><h1><lem.p.title></h1><lem.body></lem-fn>",
            "<lem-fn post(p: Post, body: html)><h1 id=\"<lem.uniqueId(\"n\")>\">\
             <lem.p.title></h1><lem.body><lem.mark(\"styles\")></lem-fn>",
        );
        assert_ne!(site, SITE_LEM);
        let post = "---\ntitle: A\nurl: /a/\n---\n<lem-style>\n  p {}\n</lem-style>\n\
                    <lem.uniqueId(\"n\")>\n\n```sh\nls\n```\n";
        let files = serde_json::json!([
            {"path": "ambient.lem", "text": format!("{AMBIENT_LEM}\n{CODE}")},
            {"path": "site.lem", "text": site},
            {"path": "posts/a.md", "text": post, "record": "Post"},
        ]);
        let mut core = Core::new();
        compile_clean_with(&mut core, &files.to_string(), Some(&host));
        assert_eq!(load(&mut core, &loaded()), serde_json::json!([]));
        core
    }

    #[test]
    fn renders_a_loaded_pages_body_without_its_layout() {
        let core = body_core();
        let body = |core: &Core| {
            decode(
                &core
                    .render_bytes(r#"{"path": "posts/a.md", "body": true}"#)
                    .unwrap(),
            )
        };
        let (meta, html) = body(&core);
        let html = html.expect("a body");
        let block = "<pre><code class=\"language-sh\">ls\n</code></pre>";
        assert_eq!(html, format!("n-0\n{block}\n"));
        // The body's own mark, at the body's own bytes; the layout's `styles` is not
        // in it.
        let marks = meta["marks"].as_array().expect("marks");
        assert_eq!(marks.len(), 1, "{marks:?}");
        let start = html.find("<pre>").expect("the block");
        assert_eq!(
            (&marks[0]["name"], &marks[0]["start"], &marks[0]["end"]),
            (
                &serde_json::json!("code"),
                &serde_json::json!(start),
                &serde_json::json!(start + block.len())
            )
        );
        assert_eq!(meta["fragments"]["styles"][0]["text"], "p {}");
        // The page holds the same body, through its layout.
        assert_eq!(
            html_of(&core, "posts/a.md"),
            format!("<h1 id=\"n-1\">A</h1>{html}")
        );
    }

    #[test]
    fn numbers_a_bodys_ids_as_its_page_numbers_them() {
        // The page renders its body before its layout, in one run, so the body's ids
        // come first there as they do alone: a fragment link in a body read elsewhere
        // names the element it names on the page.
        let core = body_core();
        let (_, body) = decode(
            &core
                .render_bytes(r#"{"path": "posts/a.md", "body": true}"#)
                .unwrap(),
        );
        let page = html_of(&core, "posts/a.md");
        assert!(body.expect("a body").starts_with("n-0\n"));
        assert!(page.contains("</h1>n-0\n"), "{page}");
    }

    #[test]
    fn renders_a_generated_pages_body_by_its_index() {
        let (mut core, envelope) = generator_core();
        assert_eq!(load(&mut core, &envelope), serde_json::json!([]));
        let page = serde_json::json!({"path": "topics.lem", "index": 1, "body": true});
        assert_eq!(rendered_at(&core, &page), "node");
        let page = serde_json::json!({"path": "topics.lem", "body": true});
        assert!(
            core.render_bytes(&page.to_string())
                .unwrap_err()
                .contains("is a generator"),
            "a body names its page as a render does"
        );
    }

    #[test]
    fn renders_a_file_that_is_no_page_the_same_with_body() {
        // A document nothing typed is no page: it has no layout to leave out.
        let mut core = site_core();
        let loose = r#"[{"path": "loose.md", "text": "<lem.site.motto>\n"}]"#;
        compile_clean_with(&mut core, loose, None);
        assert_eq!(load(&mut core, &loaded()), serde_json::json!([]));
        let page = serde_json::json!({"path": "loose.md", "body": true});
        assert_eq!(rendered_at(&core, &page), "hi\n");
        assert_eq!(html_of(&core, "loose.md"), "hi\n");
    }

    #[test]
    fn places_a_generators_load_diagnostic_at_the_part_of_the_pair_it_is_about() {
        // The list alone for a list the site lacks; the entry for a field the type
        // lacks.
        let (mut core, mut envelope) = generator_core();
        envelope["pages"][0]["for"]["value"]["topic"] = serde_json::json!("site.topic");
        let reported = load(&mut core, &envelope);
        assert_eq!(
            reported[0]["message"],
            "`site.lem` has no field `topic`; did you mean `topics`?"
        );
        assert_eq!(reported[0]["span"], span(30, 41, (3, 9), (3, 20)));
        let (mut core, mut envelope) = generator_core();
        envelope["pages"][0]["for"]["value"] = serde_json::json!({"topik": "site.topics"});
        let pair = &mut envelope["pages"][0]["for"];
        pair["spans"]["topik"] = pair["spans"]["topic"].clone();
        let reported = load(&mut core, &envelope);
        assert_eq!(
            reported[0]["message"],
            "`Topic` has no field `topik`; did you mean `topic`?"
        );
        assert_eq!(reported[0]["span"], span(23, 41, (3, 2), (3, 20)));
    }

    #[test]
    fn refuses_a_for_pair_or_an_index_the_wire_cannot_take() {
        let (mut core, envelope) = generator_core();
        let mut unknown = envelope.clone();
        unknown["pages"][0]["for"]["list"] = serde_json::json!("site.topics");
        let error = core.load_data_json(&unknown.to_string()).unwrap_err();
        assert!(error.starts_with("malformed `loadData` input:"), "{error}");
        assert!(error.contains("unknown field `list`"), "{error}");
        let inverted = span(41, 23, (3, 20), (3, 2));
        for (table, field, named) in [
            ("spans", "topic", "`for.topic`"),
            ("spans", "", "`for` as a whole"),
            ("valueSpans", "topic", "the value of `for.topic`"),
        ] {
            let mut backwards = envelope.clone();
            backwards["pages"][0]["for"][table][field] = inverted.clone();
            assert_eq!(
                core.load_data_json(&backwards.to_string()).unwrap_err(),
                format!(
                    "malformed `loadData` input: page `topics.lem`: the span for {named} \
                     ends before it starts"
                )
            );
        }
        assert_eq!(load(&mut core, &envelope), serde_json::json!([]));
        for index in ["-1", "1.5", "\"1\""] {
            let error = core
                .render_bytes(&format!(r#"{{"path": "topics.lem", "index": {index}}}"#))
                .unwrap_err();
            assert!(error.starts_with("malformed `render` input:"), "{error}");
        }
    }

    /// [`SITE_HOST`] with a generated page's neighbours named.
    fn neighboured_host() -> String {
        SITE_HOST.replace(
            r#""url": "url""#,
            r#""url": "url", "neighbours": ["prev", "next"]"#,
        )
    }

    #[test]
    fn checks_the_neighbour_fields_on_a_compile_that_only_changed_the_declaration() {
        // Compiled clean while the host named no neighbours; the declaration that names
        // them recompiles nothing, and the relink alone judges the fields and the
        // defaults that read them.
        let (mut core, _) = generator_core();
        let files = serde_json::json!([{"path": "slides.lem", "text":
            "<lem-type Slide extends Page>\n  title: string\n  prev: Slide\n  \
             layout: (Slide, html): html = slid\n</lem-type>\n\
             <lem-fn slid(s: Slide, body: html)><lem.s.title><lem.body></lem-fn>"}]);
        compile_clean_with(&mut core, &files.to_string(), None);
        let reported =
            diagnostics(&core.compile_json("[]", Some(&neighboured_host())).unwrap());
        assert_eq!(reported.as_array().map(Vec::len), Some(1), "{reported:?}");
        assert_eq!(reported[0]["path"], "slides.lem");
        assert_eq!(
            reported[0]["message"],
            "`prev` holds a generated page's neighbour, a page of its own type: declare it \
             `prev: Slide?`, or an optional of a type `Slide` extends"
        );
        let (mut core, _) = generator_core();
        let files = serde_json::json!([{"path": "slides.lem", "text":
            "<lem-type Slide extends Page>\n  title: string\n  prev: Slide?\n  \
             blurb: string = self.prev?.title ?? \"\"\n  \
             layout: (Slide, html): html = slid\n</lem-type>\n\
             <lem-fn slid(s: Slide, body: html)><lem.s.blurb><lem.body></lem-fn>"}]);
        compile_clean_with(&mut core, &files.to_string(), None);
        let reported =
            diagnostics(&core.compile_json("[]", Some(&neighboured_host())).unwrap());
        assert_eq!(
            reported[0]["message"],
            "`blurb` reads `prev`, but `prev` is out of reach of a default: the loader \
             fills a page's neighbours once every page of its generator is built"
        );
    }

    #[test]
    fn renders_a_generated_page_reading_its_neighbours_through_the_table() {
        let (_, envelope) = generator_core();
        let site = format!(
            "{SITE_LEM}\n<lem-type Topic extends Page>\n  topic: string\n  \
             prev: Topic?\n  next: Topic?\n  \
             url: url = url(\"/t/\" + self.topic + \"/\")\n  \
             layout: (Topic, html): html = topical\n</lem-type>\n\
             <lem-fn topical(t: Topic, body: html)><lem.t.url>:<lem.body></lem-fn>\n\
             <lem-let topics: [string] = [\"git\", \"node\", \"rust\"] />"
        );
        let files = serde_json::json!([
            {"path": "ambient.lem", "text": AMBIENT_LEM},
            {"path": "site.lem", "text": site},
            {"path": "posts/a.md", "text": POST_MD, "record": "Post"},
            {"path": "topics.lem", "record": "Topic", "text":
             "---\ntype: Topic\nfor:\n  topic: site.topics\n---\n\
              <lem.page.prev?.topic ?? \"-\">|<lem.page.next?.topic ?? \"-\">"},
        ]);
        let mut core = Core::new();
        compile_clean_with(&mut core, &files.to_string(), Some(&neighboured_host()));
        assert_eq!(load(&mut core, &envelope), serde_json::json!([]));
        let page = |index: u32| serde_json::json!({"path": "topics.lem", "index": index});
        assert_eq!(rendered_at(&core, &page(0)), "/t/git/:-|node");
        assert_eq!(rendered_at(&core, &page(1)), "/t/node/:git|rust");
        assert_eq!(rendered_at(&core, &page(2)), "/t/rust/:node|-");
    }

    #[test]
    fn refuses_a_generated_page_as_a_payload_and_renders_on_after_it() {
        // With no entry field a page record holds no function, so the compiler lets a
        // page through as a payload; the reference its neighbour field holds is what
        // the VM refuses, at the call, rather than a trap the boundary cannot survive.
        let host = r#"{"ambient": {"page": "Page", "site": {"bindingsOf": "site.lem"}},
            "document": "page", "url": "url", "neighbours": ["prev", "next"]}"#;
        let site = "<lem-type Topic extends Page>\n  topic: string\n  prev: Topic?\n  \
             next: Topic?\n  url: url = url(\"/t/\" + self.topic + \"/\")\n</lem-type>\n\
             <lem-let topics: [string] = [\"git\", \"node\"] />";
        let mut core = Core::new();
        let files = serde_json::json!([
            {"path": "ambient.lem", "text": AMBIENT_LEM},
            {"path": "site.lem", "text": site},
            {"path": "topics.lem", "record": "Topic", "text":
             "---\ntype: Topic\nfor:\n  topic: site.topics\n---\n\
              <lem.mark(\"card\", page)>"},
            {"path": "plain.lem", "text": "plain"},
        ]);
        let reply = core.compile_json(&files.to_string(), Some(host)).unwrap();
        // `plain.lem` warns that its output never renders as a page; it renders as a
        // file all the same, which is all this needs of it.
        let reported = diagnostics(&reply);
        assert!(
            reported
                .as_array()
                .is_some_and(|all| all.iter().all(|d| d["severity"] == "warning")),
            "{reported:?}"
        );
        let envelope = serde_json::json!({"ambient": {}, "pages": [{"path": "topics.lem",
            "value": {}, "for": {"value": {"topic": "site.topics"}}}]});
        assert_eq!(load(&mut core, &envelope), serde_json::json!([]));
        let (meta, html) = decode(
            &core
                .render_bytes(r#"{"path": "topics.lem", "index": 0}"#)
                .unwrap(),
        );
        assert_eq!(html, None);
        assert_eq!(
            meta["diagnostics"][0]["message"],
            "a payload crosses to the handler as data, and a generated page's neighbours \
             have no form there"
        );
        // The instance survived it: another render runs.
        assert_eq!(html_of(&core, "plain.lem"), "plain");
    }

    #[test]
    fn refuses_two_pages_at_one_url_where_the_host_placed_the_second() {
        let (mut core, mut envelope) = site_core_with_home();
        let home = &mut envelope["pages"][0];
        home["value"]["url"] = serde_json::json!("/a/");
        home["spans"] = serde_json::json!({"url": span(20, 29, (3, 0), (3, 9))});
        let reported = load(&mut core, &envelope);
        assert_eq!(reported.as_array().map(Vec::len), Some(1), "{reported:?}");
        assert_eq!(reported[0]["path"], "home.lem");
        assert_eq!(reported[0]["span"], span(20, 29, (3, 0), (3, 9)));
        assert_eq!(
            reported[0]["message"],
            "`home.lem` and `posts/a.md` are both at `/a/`"
        );
        // The first page sent no positions, so its label is at where its record was
        // named — the front matter block — placed by the program's index.
        assert_eq!(reported[0]["labels"][0]["path"], "posts/a.md");
        assert_eq!(
            reported[0]["labels"][0]["span"]["range"]["start"]["line"],
            0
        );
    }

    #[test]
    fn checks_the_url_field_on_a_compile_that_only_changed_the_declaration() {
        // Nothing is recompiled: the relink alone sees the renamed field, and reports
        // it where `Page` is declared.
        let mut core = site_core();
        let renamed = SITE_HOST.replace(r#""url": "url""#, r#""url": "permalink""#);
        let reported = diagnostics(&core.compile_json("[]", Some(&renamed)).unwrap());
        assert_eq!(reported[0]["path"], "ambient.lem", "{reported:?}");
        assert_eq!(
            reported[0]["message"],
            "`Page` is the host's document type, so it declares where a page goes: \
             `permalink: url`"
        );
        assert_eq!(reported[0]["span"]["range"]["start"]["character"], 10);
    }

    /// A failed compile keeps the declaration its units compiled under, which is not
    /// the one the program linked under; an envelope built for the new declaration
    /// cannot be loaded against the old program, and the refusal says which.
    #[test]
    fn refuses_a_load_after_the_declaration_changed_without_a_compile_succeeding() {
        let mut core = site_core();
        // A failed compile that changes nothing about the declaration leaves the
        // program loadable.
        let failed = core
            .compile_json(r#"[{"path": "broken.lem", "text": "<lem-if x>"}]"#, None)
            .unwrap();
        assert!(!diagnostics(&failed).as_array().unwrap().is_empty());
        assert_eq!(load(&mut core, &loaded()), serde_json::json!([]));
        // One that changes it does not, until a compile succeeds under it.
        let changed = r#"{"ambient": {"page": "Page", "site": {"bindingsOf": "site.lem"}, "build": "Build"}, "document": "page", "entry": "layout"}"#;
        let failed = core
            .compile_json(
                r#"[{"path": "build.lem", "text": "<lem-type Build>\n  generator: strng\n</lem-type>"}]"#,
                Some(changed),
            )
            .unwrap();
        assert!(!diagnostics(&failed).as_array().unwrap().is_empty());
        let refused = core.load_data_json(&loaded().to_string()).unwrap_err();
        assert!(
            refused.starts_with(
                "the ambient declaration changed since the program last linked"
            ),
            "{refused}"
        );
        // The data loaded before it still renders.
        assert!(html_of(&core, "posts/a.md").contains("A\nposts/a.md\nhi"));
    }

    #[test]
    fn refuses_a_load_before_any_compile_has_succeeded() {
        let mut core = Core::new();
        let error = core.load_data_json(r#"{"ambient": {}}"#).unwrap_err();
        assert!(error.contains("`compile` first"), "{error}");
    }

    #[test]
    fn refuses_an_ambient_name_the_host_does_not_bind() {
        let mut core = site_core();
        let refused = core
            .load_data_json(r#"{"ambient": {"weather": 1}}"#)
            .unwrap_err();
        assert_eq!(
            refused,
            "the program cannot be loaded from this `loadData` envelope: the host binds \
             no `weather`"
        );
    }

    #[test]
    fn refuses_a_source_no_from_binding_asked_for() {
        let mut core = site_core();
        let mut envelope = loaded();
        envelope["sources"]["extra"] = serde_json::json!({"value": 1});
        let refused = core.load_data_json(&envelope.to_string()).unwrap_err();
        assert_eq!(
            refused,
            "the program cannot be loaded from this `loadData` envelope: no `from` \
             binding is named `extra`"
        );
    }

    #[test]
    fn keeps_the_last_good_data_when_an_envelope_is_refused() {
        let mut core = site_core();
        assert_eq!(load(&mut core, &loaded()), serde_json::json!([]));
        // A caller bug with no data to point at: a throw, and the data the last good
        // load built stays bound.
        assert!(
            core.load_data_json(r#"{"ambient": {"weather": 1}}"#)
                .is_err()
        );
        assert!(html_of(&core, "posts/a.md").contains("A\nposts/a.md\nhi"));
    }

    /// The files the host reads bindings from load in declaration order, so the order
    /// has to survive the boundary: `brand` reads `site`, which is declared first —
    /// and sorts last.
    #[test]
    fn loads_bindings_files_in_the_hosts_declaration_order() {
        let mut core = Core::new();
        let host = r#"{"ambient": {"site": {"bindingsOf": "site.lem"}, "brand": {"bindingsOf": "brand.lem"}}}"#;
        compile_clean_with(
            &mut core,
            r#"[
                {"path": "site.lem", "text": "<lem-let motto: string = \"m\" />"},
                {"path": "brand.lem", "text": "<lem-let tag: string = site.motto />"},
                {"path": "page.lem", "text": "<lem.brand.tag>"}
            ]"#,
            Some(host),
        );
        assert_eq!(
            load(&mut core, &serde_json::json!({})),
            serde_json::json!([])
        );
        assert_eq!(html_of(&core, "page.lem"), "m");
    }

    #[test]
    fn refuses_a_record_span_that_is_not_a_span_of_its_file() {
        const TEXT: &str = "---\ntype: Post\n---\nhi\n";
        let mut core = Core::new();
        let files = serde_json::json!([{
            "path": "a.md", "text": TEXT, "record": "Post",
            "recordSpan": {"start": 9000, "end": 9001},
        }])
        .to_string();
        let refused = core.compile_json(&files, Some(TYPED)).unwrap_err();
        assert_eq!(
            refused,
            format!(
                "malformed `compile` input: `a.md` has a `recordSpan` of 9000..9001, past \
                 the end of a {}-byte file",
                TEXT.len()
            )
        );
        // Refused before anything was installed, declaration included.
        compile_clean(&mut core, r#"[{"path": "page.lem", "text": "ok"}]"#);
        assert_eq!(html_of(&core, "page.lem"), "ok");
    }

    #[test]
    fn keeps_the_host_declaration_across_calls() {
        // The shell declares its environment once and then compiles changed files, so
        // a call that carries no declaration compiles against the resident one.
        let mut core = Core::new();
        compile_clean(&mut core, SCHEMA);
        compile_clean_with(
            &mut core,
            r##"[{"path": "post.md", "text": "<lem.page.title>", "record": "Article"}]"##,
            Some(TYPED),
        );
        let later = core
            .compile_json(
                r##"[{"path": "other.md", "text": "<lem.page.nope>", "record": "Article"}]"##,
                None,
            )
            .unwrap();
        assert!(later.contains("has no field `nope`"), "{later}");
    }

    #[test]
    fn points_a_record_diagnostic_at_the_key_the_host_named_it_at() {
        let mut core = Core::new();
        compile_clean(&mut core, SCHEMA);
        let result = core
            .compile_json(
                r##"[{"path": "post.md", "text": "---\ntype: Missive\n---\nhi", "record": "Missive", "recordSpan": {"start": 10, "end": 17}}]"##,
                Some(TYPED),
            )
            .unwrap();
        let reported = diagnostics(&result);
        assert_eq!(reported[0]["message"], "unknown type `Missive`");
        assert_eq!(reported[0]["span"]["start"], 10);
        assert_eq!(reported[0]["span"]["end"], 17);
        assert_eq!(reported[0]["span"]["range"]["start"]["line"], 1);
        assert_eq!(reported[0]["span"]["range"]["start"]["character"], 6);
    }

    #[test]
    fn reads_a_document_no_record_named_as_unknown_typed() {
        let mut core = Core::new();
        compile_clean(&mut core, SCHEMA);
        compile_clean_with(
            &mut core,
            r##"[{"path": "loose.md", "text": "<lem.page.whatever>"}]"##,
            Some(TYPED),
        );
    }

    /// A host whose highlighter claims `code` for two languages, with the payload record
    /// a code mark carries and the field it is keyed by, as `ambient.lem` declares it.
    const HIGHLIGHTING: &str = r#"{"ambient": {"site": null, "page": null, "build": null},
        "marks": [
            {"name": "code", "keys": ["sh", "ts"], "payload": "Code", "keyedBy": "language"},
            {"name": "code"}
        ]}"#;

    /// `Code` as the shell's `ambient.lem` declares it.
    const CODE: &str = "<lem-type Code>\n  language: string?\n  info: string\n  \
                        code: string\n</lem-type>";

    #[test]
    fn marks_a_code_block_over_the_element_comrak_wrote() {
        // The core highlights nothing (D42): the page holds comrak's plain block, a
        // `code` mark names its bytes and key, and the payload it names crossed once,
        // with `compile`, rather than with every render.
        let mut core = Core::new();
        let files = r#"[{"path": "post.md", "text": "```sh {1}\necho <hi>\n```\n"}]"#;
        let result = core.compile_json(files, Some(HIGHLIGHTING)).unwrap();
        assert_eq!(diagnostics(&result), serde_json::json!([]));
        let payloads = payloads(&result);
        let id = payloads[0]["id"].as_str().expect("an id").to_owned();
        assert_eq!(
            payloads,
            serde_json::json!([
                {"id": id, "language": "sh", "info": "sh {1}", "code": "echo <hi>\n"}
            ])
        );
        let (meta, html) = rendered(&core, "post.md");
        let html = html.expect("a page");
        let element = "<pre><code class=\"language-sh\">echo &lt;hi&gt;\n</code></pre>";
        assert_eq!(html, format!("{element}\n"));
        let mark = &meta["marks"][0];
        assert_eq!(
            (
                &mark["name"],
                &mark["key"],
                &mark["payloadId"],
                &mark["start"],
                &mark["end"]
            ),
            (
                &serde_json::json!("code"),
                &serde_json::json!("sh"),
                &serde_json::json!(id),
                &serde_json::json!(0),
                &serde_json::json!(element.len())
            )
        );
        // A reported payload is named, never carried: nothing about the code rides on
        // the render.
        assert!(mark.get("payload").is_none(), "{mark}");
    }

    #[test]
    fn sends_a_template_marks_key_and_payload_as_json() {
        // A `mark()` call's payload crosses inline, as the JSON a handler can read: a
        // record as its fields in declared order, and no field absent for being null.
        let mut core = Core::new();
        let page = format!(
            "{CODE}\n<lem.mark(\"code\", {{language: \"ts\", info: \"ts\", code: \"let x\"}})>"
        );
        let files = serde_json::json!([{"path": "page.lem", "text": page}]).to_string();
        compile_clean_with(&mut core, &files, Some(HIGHLIGHTING));
        let (meta, _) = rendered(&core, "page.lem");
        let mark = &meta["marks"][0];
        assert_eq!(mark["key"], "ts");
        assert_eq!(
            serde_json::to_string(&mark["payload"]).unwrap(),
            r#"{"language":"ts","info":"ts","code":"let x"}"#
        );
        assert!(mark.get("payloadId").is_none(), "{mark}");
    }

    #[test]
    fn reports_each_payload_once_and_those_of_a_file_that_waits_on_another() {
        // One block in two files is one payload. A file that compiles beside one that
        // does not stays resident without the program relinking, and is not compiled
        // again when the other is fixed — so its payloads go out now or never.
        let mut core = Core::new();
        let files = r#"[
            {"path": "a.md", "text": "```sh\nls\n```\n"},
            {"path": "b.md", "text": "```sh\nls\n```\n\n```ts\nx\n```\n"},
            {"path": "broken.lem", "text": "<lem-if x>"}
        ]"#;
        let result = core.compile_json(files, Some(HIGHLIGHTING)).unwrap();
        assert!(!diagnostics(&result).as_array().unwrap().is_empty());
        let reported = payloads(&result);
        let codes: Vec<&str> = reported
            .as_array()
            .unwrap()
            .iter()
            .map(|payload| payload["code"].as_str().unwrap())
            .collect();
        assert_eq!(codes, ["ls\n", "x\n"]);
        // Fixing the broken file reports nothing again: a.md and b.md are resident.
        let fixed = core
            .compile_json(r#"[{"path": "broken.lem", "text": "ok"}]"#, None)
            .unwrap();
        assert_eq!(payloads(&fixed), serde_json::json!([]));
    }

    #[test]
    fn refuses_claims_that_cannot_be_one_declaration() {
        // Two claims for one name naming different payloads, and a key field with no
        // payload to find it in, are caller bugs, and leave the resident core as it was.
        let mut core = Core::new();
        compile_clean(&mut core, r#"[{"path": "page.lem", "text": "ok"}]"#);
        for (host, message) in [
            (
                r#"{"ambient": {}, "marks": [
                    {"name": "code", "keys": ["sh"], "payload": "Code"},
                    {"name": "code", "keys": ["ts"], "payload": "Snippet"}]}"#,
                "the claims for `code` name two payloads, `Code` and `Snippet`",
            ),
            (
                r#"{"ambient": {}, "marks": [{"name": "code", "keyedBy": "language"}]}"#,
                "the claim for `code` keys its marks by `language` but names no payload",
            ),
        ] {
            let refused = core.compile_json("[]", Some(host)).unwrap_err();
            assert!(
                refused.starts_with("malformed `compile` host:")
                    && refused.contains(message),
                "{refused}"
            );
        }
        assert_eq!(
            core.host,
            Host::try_from(serde_json::from_str::<HostInput>(UNTYPED).unwrap()).unwrap()
        );
    }

    #[test]
    fn stales_a_resident_document_when_a_producer_starts_covering_its_language() {
        // The fact is per language: a highlighter that starts covering `rust` leaves the
        // document that names it stale and the one that names `sh` alone.
        let mut core = Core::new();
        compile_clean_with(
            &mut core,
            r#"[{"path": "a.md", "text": "```sh\nls\n```\n"}]"#,
            Some(HIGHLIGHTING),
        );
        let result = core
            .compile_json(
                r#"[{"path": "b.md", "text": "```rust\nfn main() {}\n```\n"}]"#,
                None,
            )
            .unwrap();
        assert!(
            diagnostics(&result)[0]["message"]
                .as_str()
                .is_some_and(|m| m.starts_with("no producer claims `rust` for `code`")),
            "{result}"
        );
        let wider = HIGHLIGHTING.replace(r#""ts"]"#, r#""ts", "rust"]"#);
        let relinked = core
            .compile_json(r#"[{"path": "c.lem", "text": "ok"}]"#, Some(&wider))
            .unwrap();
        let stale = diagnostics(&relinked);
        assert_eq!(stale.as_array().map(Vec::len), Some(1), "{stale}");
        assert_eq!(stale[0]["path"], "b.md");
        assert!(
            stale[0]["message"]
                .as_str()
                .is_some_and(|m| m.contains("a producer claims `rust` for `code` now")),
            "{stale}"
        );
    }

    #[test]
    fn refuses_a_malformed_host_declaration() {
        let mut core = Core::new();
        let refused = core.compile_json("[]", Some(r#"{"ambient": 3}"#));
        assert!(
            refused
                .as_ref()
                .is_err_and(|message| message.starts_with("malformed `compile` host")),
            "{refused:?}"
        );
    }

    #[test]
    fn keeps_the_resident_declaration_when_a_call_is_refused() {
        // A refused call is a caller bug: it compiles nothing, and it installs nothing
        // either, so the next call still compiles in the environment that was actually
        // accepted.
        let mut core = Core::new();
        compile_clean(&mut core, "[]");
        let refused = core.compile_json(
            r#"[{"path": "a.lem", "text": "x"}, {"path": "a.lem", "text": "y"}]"#,
            Some(TYPED),
        );
        assert!(refused.is_err(), "{refused:?}");
        compile_clean_with(
            &mut core,
            r#"[{"path": "page.lem", "text": "<lem.site.name>"}]"#,
            None,
        );
    }
}

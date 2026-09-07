//! The `wasm-bindgen` boundary: the only part of the core the shell can see.
//!
//! One resident [`Core`] per worker holds the compiled units, the linked module, and the
//! loaded data across calls (docs/architecture.md §"WASM API"): few large calls, each
//! carrying its bulk data as one UTF-8 JSON string. `compile` takes only the files that
//! changed, compiles each into its own unit against the resident signatures, and
//! relinks; `loadData` stores the ambient records; `render` runs one file's entry.
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

use std::collections::BTreeMap;
use std::rc::Rc;

use lemmon_vm::{
    ExportIndex, LineIndex, Map, Module, Position, RuntimeError, Severity, SourceFile,
    Unit, Value, compile_unit, link, prepare, render, repeated_path,
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsError;
use wasm_bindgen::prelude::wasm_bindgen;

/// The version of the core this `.wasm` was built from.
#[wasm_bindgen]
#[must_use]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// A source file as `compile` receives it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileInput {
    path: String,
    text: String,
}

/// What `loadData` receives — the spike's `DataInput`: the ambient records only. The
/// envelope is an object so `records`, data files, and the asset manifest are additive
/// fields later.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DataInput {
    ambient: AmbientInput,
}

/// The three ambient records and nothing else — an unknown name here is a caller bug,
/// not data to carry silently (invariant 10).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AmbientInput {
    site: Option<serde_json::Value>,
    page: Option<serde_json::Value>,
    build: Option<serde_json::Value>,
}

/// What `render` receives — the spike's `PageRef`: a path names the page until `plan()`
/// exists to mint richer references. An object, so those stay additive.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PageRef {
    path: String,
}

/// The boundary form of every diagnostic. A `code` would be an additive field; nothing
/// consumes one yet.
#[derive(Serialize)]
struct Diagnostic {
    path: String,
    span: SpanOut,
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
    path: String,
    span: SpanOut,
    message: String,
}

/// A span as both of the things a consumer needs: the byte offsets the core works in,
/// and the two LSP positions an editor or a printed diagnostic wants.
#[derive(Serialize)]
struct SpanOut {
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
type LineIndexes = BTreeMap<String, LineIndex>;

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
    fn placed(
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
    fn placed_runtime(error: RuntimeError, resident: &LineIndexes) -> Self {
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
struct RenderResult {
    html: Option<String>,
    diagnostics: Vec<Diagnostic>,
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
    /// The linked program: absent until a compile succeeds, and left as it was by one
    /// that fails — stale until fixed, with the failure reported, never guessed around.
    module: Option<Module>,
    /// The ambient records `loadData` stored, bound at every render.
    ambient: Vec<(String, Value)>,
    /// For each resident unit, an index over the text *that unit* was compiled from —
    /// which is not always the latest text at that path, since a file whose recompile
    /// failed keeps its previous unit. Updated in lockstep with `units`, so it grows
    /// with them and no faster. Line starts and each line's non-ASCII characters, not
    /// the text: an all-ASCII file costs a handful of bytes per line.
    unit_indexes: LineIndexes,
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
            module: None,
            ambient: Vec::new(),
            unit_indexes: BTreeMap::new(),
        }
    }

    /// Compiles the given files — only the changed ones — into resident units and
    /// relinks the program. `files` is JSON `[{path, text}]`; the result is the JSON
    /// array of diagnostics about those files (and about any resident unit the relink
    /// found stale) — none of them an error on success, since a program is still
    /// produced when the only diagnostics are warnings. A resident file's warnings are
    /// reported by the call that compiled it, not again by later calls.
    ///
    /// # Errors
    ///
    /// A malformed `files` payload, or one naming the same file twice — caller bugs;
    /// template problems are diagnostics.
    pub fn compile(&mut self, files: &str) -> Result<String, JsError> {
        self.compile_json(files).map_err(|m| JsError::new(&m))
    }

    /// Stores the data every render binds. `input` is JSON
    /// `{ambient: {site?, page?, build?}}`; the result is the JSON array of
    /// diagnostics — always empty until schemas exist to validate against (`∆iCW`).
    ///
    /// # Errors
    ///
    /// A malformed `input` payload.
    #[wasm_bindgen(js_name = loadData)]
    pub fn load_data(&mut self, input: &str) -> Result<String, JsError> {
        self.load_data_json(input).map_err(|m| JsError::new(&m))
    }

    /// Renders one page. `page` is JSON `{path}`; the result is JSON
    /// `{html, diagnostics}` — the page's HTML, or `null` with the runtime diagnostic.
    ///
    /// # Errors
    ///
    /// A malformed `page` payload, rendering before any compile has succeeded, or a
    /// path the program has no file for — caller bugs with no source span to carry.
    pub fn render(&self, page: &str) -> Result<String, JsError> {
        self.render_json(page).map_err(|m| JsError::new(&m))
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

    fn compile_json(&mut self, files_json: &str) -> Result<String, String> {
        let files: Vec<FileInput> = serde_json::from_str(files_json)
            .map_err(|e| format!("malformed `compile` input: {e}"))?;
        // Rejected before anything is offered: the second offer of a path withdraws the
        // first's names, leaving the resident index describing neither file.
        if let Some(path) = repeated_path(files.iter().map(|file| file.path.as_str())) {
            return Err(format!("malformed `compile` input: `{path}` given twice"));
        }
        // Indexed up front, before `prepare`, so that a file which fails to even parse
        // can still have its own diagnostics placed.
        let incoming: LineIndexes = files
            .iter()
            .map(|file| (file.path.clone(), LineIndex::new(&file.text)))
            .collect();
        let mut diagnostics: Vec<lemmon_vm::Diagnostic> = Vec::new();
        let mut prepared = Vec::new();
        for file in &files {
            match prepare(&SourceFile::new(&file.path, &file.text)) {
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
            let compiled = compile_unit(p, &self.index);
            diagnostics.extend(compiled.diagnostics);
            if let Some(unit) = compiled.output {
                // In lockstep: the index kept for a unit is the one over the text that
                // produced it, so the unit's spans stay placeable for as long as it is
                // resident — including after a later recompile of that path fails.
                self.unit_indexes
                    .insert(unit.path.clone(), incoming[&unit.path].clone());
                self.units.insert(unit.path.clone(), unit);
            }
        }
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
            let linked = link(&units, &self.index);
            diagnostics.extend(linked.diagnostics);
            if let Some(module) = linked.output {
                self.module = Some(module);
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
            .map(|d| Diagnostic::placed(d, &incoming, &self.unit_indexes))
            .collect();
        serde_json::to_string(&diagnostics)
            .map_err(|e| format!("serialising diagnostics failed: {e}"))
    }

    fn load_data_json(&mut self, input_json: &str) -> Result<String, String> {
        let input: DataInput = serde_json::from_str(input_json)
            .map_err(|e| format!("malformed `loadData` input: {e}"))?;
        let mut ambient = Vec::new();
        for (name, value) in [
            ("site", input.ambient.site),
            ("page", input.ambient.page),
            ("build", input.ambient.build),
        ] {
            if let Some(value) = value {
                ambient.push((name.to_owned(), value_from_json(&value)));
            }
        }
        self.ambient = ambient;
        // Nothing to validate against until schemas exist (`∆iCW`). Those diagnostics
        // will point into the host's YAML envelope rather than a resident file, so they
        // will need a line index this core does not hold — ∆iCW's to work out.
        Ok("[]".to_owned())
    }

    fn render_json(&self, page_json: &str) -> Result<String, String> {
        let page: PageRef = serde_json::from_str(page_json)
            .map_err(|e| format!("malformed `render` input: {e}"))?;
        let module = self.module.as_ref().ok_or(
            "nothing has compiled successfully yet — `render` needs a `compile` first",
        )?;
        // Checked here so the VM's unknown-path panic is unreachable from the boundary.
        if module.entry(&page.path).is_none() {
            return Err(format!("no compiled file at `{}`", page.path));
        }
        let globals: Vec<(&str, Value)> = self
            .ambient
            .iter()
            .map(|(name, value)| (name.as_str(), value.clone()))
            .collect();
        let result = match render(module, &page.path, &globals) {
            Ok(html) => RenderResult {
                html: Some(html),
                diagnostics: Vec::new(),
            },
            Err(error) => RenderResult {
                html: None,
                diagnostics: vec![Diagnostic::placed_runtime(error, &self.unit_indexes)],
            },
        };
        serde_json::to_string(&result)
            .map_err(|e| format!("serialising the render result failed: {e}"))
    }
}

/// A JSON value as the VM's: integral numbers become `Int` and the rest `Float`;
/// objects become insertion-ordered `Map`s in the key order the shell sent (which is
/// what a `lem-for` over a map walks). Every string arrives untrusted (`Str`) —
/// `raw()` is the only way to `html` (invariant 7).
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
        serde_json::Value::Object(entries) => Value::map(
            entries
                .iter()
                .map(|(key, value)| (Rc::from(key.as_str()), value_from_json(value)))
                .collect::<Map>(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagnostics(json: &str) -> serde_json::Value {
        serde_json::from_str(json).expect("diagnostics are JSON")
    }

    fn compile_clean(core: &mut Core, files: &str) {
        let result = core.compile_json(files).unwrap();
        assert_eq!(result, "[]", "expected a clean compile");
    }

    fn html_of(core: &Core, path: &str) -> String {
        let result = core
            .render_json(&format!("{{\"path\": \"{path}\"}}"))
            .unwrap();
        let result: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(result["diagnostics"], serde_json::json!([]));
        result["html"].as_str().expect("html on success").to_owned()
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
            .compile_json(r#"[{"path": "page.lem", "text": "<lem-if x>"}]"#)
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
            .compile_json(r#"[{"path": "lib.lem", "text": "no functions here"}]"#)
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
            )
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
            .compile_json(r#"[{"path": "a.lem", "text": "<lem-fn f(): int = 2 />"}]"#)
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
            .compile_json(r#"[{"path": "b.lem", "text": "<lem-fn f(): int = 2 />"}]"#)
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
            .compile_json(r#"[{"path": "page.lem", "text": "<p>ok</p>\nI’m <lem.nope>"}]"#)
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
            .compile_json(r#"[{"path": "b.lem", "text": "<lem-fn f(): int = 2 />"}]"#)
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
            .compile_json(r#"[{"path": "lib.lem", "text": "no functions here"}]"#)
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
            .compile_json(r#"[{"path": "page.lem", "text": "<lem.nope>"}]"#)
            .unwrap();
        assert!(!diagnostics(&failed).as_array().unwrap().is_empty());
        // Now a clean compile relinks, and the stale unit still imports `n`.
        let result = core
            .compile_json(r#"[{"path": "lib.lem", "text": "no functions here"}]"#)
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
            )
            .unwrap();
        let reported = diagnostics(&result);
        assert_eq!(
            reported.as_array().expect("an array").len(),
            1,
            "{reported:?}"
        );
        // The refused file compiled to nothing, so the program has no b.lem to render.
        assert!(core.render_json(r#"{"path": "b.lem"}"#).is_err());
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
            )
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
    fn renders_a_runtime_failure_as_a_diagnostic_not_a_throw() {
        let mut core = Core::new();
        compile_clean(
            &mut core,
            r#"[{"path": "page.lem", "text": "<lem.1 / 0>"}]"#,
        );
        let result = core.render_json(r#"{"path": "page.lem"}"#).unwrap();
        let result: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(result["html"], serde_json::Value::Null);
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
        let error = core.render_json(r#"{"path": "page.lem"}"#).unwrap_err();
        assert!(error.contains("`compile` first"), "{error}");
        compile_clean(&mut core, r#"[{"path": "page.lem", "text": "ok"}]"#);
        let error = core.render_json(r#"{"path": "other.lem"}"#).unwrap_err();
        assert_eq!(error, "no compiled file at `other.lem`");
    }

    #[test]
    fn refuses_malformed_envelopes_with_a_throwable_message() {
        let mut core = Core::new();
        assert!(
            core.compile_json("not json")
                .unwrap_err()
                .starts_with("malformed `compile` input:")
        );
        assert!(
            core.load_data_json(r#"{"ambient": {"weather": 1}}"#)
                .unwrap_err()
                .starts_with("malformed `loadData` input:")
        );
        assert!(
            core.render_json(r#"{}"#)
                .unwrap_err()
                .starts_with("malformed `render` input:")
        );
    }
}

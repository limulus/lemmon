//! The `wasm-bindgen` boundary: the only part of the core the shell can see.
//!
//! One resident [`Core`] per worker holds the compiled units, the linked module, and the
//! loaded data across calls (docs/architecture.md §"WASM API"): few large calls, each
//! carrying its bulk data as one UTF-8 JSON string. `compile` takes only the files that
//! changed, compiles each into its own unit against the resident signatures, and
//! relinks; `loadData` stores the ambient records; `render` runs one file's entry.
//!
//! Diagnostics come back as a JSON array of `{path, span: {start, end}, message}`, every
//! one pointing at source (invariant 8). A thrown `JsError` is reserved for caller bugs
//! — a malformed envelope, rendering before a compile has succeeded, rendering a path
//! the program does not have — which have no source span to point at.

use std::collections::BTreeMap;
use std::rc::Rc;

use lemmon_vm::{
    CompileError, ExportedSignature, Map, Module, PreparedFile, RuntimeError,
    SignatureTable, SourceFile, Unit, Value, compile_unit, link, prepare, render,
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

/// The boundary form of every diagnostic. `severity` and `code` arrive with the checker
/// (`∆C8d`) as additive optional fields.
#[derive(Serialize)]
struct Diagnostic {
    path: String,
    span: SpanOut,
    message: String,
}

#[derive(Serialize)]
struct SpanOut {
    start: u32,
    end: u32,
}

impl From<CompileError> for Diagnostic {
    fn from(error: CompileError) -> Self {
        Self {
            path: error.path,
            span: SpanOut {
                start: error.span.start,
                end: error.span.end,
            },
            message: error.message,
        }
    }
}

impl From<RuntimeError> for Diagnostic {
    fn from(error: RuntimeError) -> Self {
        Self {
            path: error.path,
            span: SpanOut {
                start: error.span.start,
                end: error.span.end,
            },
            message: error.message,
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
    /// The linked program: absent until a compile succeeds, and left as it was by one
    /// that fails — stale until fixed, with the failure reported, never guessed around.
    module: Option<Module>,
    /// The ambient records `loadData` stored, bound at every render.
    ambient: Vec<(String, Value)>,
}

#[wasm_bindgen]
impl Core {
    /// A fresh core with nothing compiled or loaded.
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new() -> Self {
        Self {
            units: BTreeMap::new(),
            module: None,
            ambient: Vec::new(),
        }
    }

    /// Compiles the given files — only the changed ones — into resident units and
    /// relinks the program. `files` is JSON `[{path, text}]`; the result is the JSON
    /// array of diagnostics, empty on success.
    ///
    /// # Errors
    ///
    /// A malformed `files` payload — a caller bug; template problems are diagnostics.
    pub fn compile(&mut self, files: &str) -> Result<String, JsError> {
        self.compile_json(files).map_err(|m| JsError::new(&m))
    }

    /// Stores the data every render binds. `input` is JSON
    /// `{ambient: {site?, page?, build?}}`; the result is the JSON array of
    /// diagnostics — always empty until schemas exist to validate against (`∆C8d`).
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
        let mut diagnostics: Vec<Diagnostic> = Vec::new();
        let mut prepared = Vec::new();
        for file in &files {
            match prepare(&SourceFile::new(&file.path, &file.text)) {
                Ok(p) => prepared.push(p),
                Err(errors) => diagnostics.extend(errors.into_iter().map(Into::into)),
            }
        }
        // The signature table spans the whole resident program: every unit the incoming
        // files do not replace, then the incoming files themselves. That order is what
        // puts a name collision's blame on the file just handed in — the table keeps the
        // first definition it sees, so the resident one wins and the edit is the error,
        // rather than whichever path happened to sort first.
        let replaced: Vec<&str> = prepared.iter().map(PreparedFile::path).collect();
        let mut exports: Vec<(&str, &[ExportedSignature])> = self
            .units
            .iter()
            .filter(|(path, _)| !replaced.contains(&path.as_str()))
            .map(|(path, unit)| (path.as_str(), unit.exports.as_slice()))
            .collect();
        exports.extend(prepared.iter().map(|p| (p.path(), p.exports())));
        let (table, table_errors) = SignatureTable::build(exports);
        diagnostics.extend(table_errors.into_iter().map(Into::into));
        for p in &prepared {
            match compile_unit(p, &table) {
                Ok(unit) => {
                    self.units.insert(unit.path.clone(), unit);
                }
                Err(errors) => diagnostics.extend(errors.into_iter().map(Into::into)),
            }
        }
        // A file that failed keeps its previous unit, so the program stays renderable
        // — stale until fixed — and the module is only replaced by a clean relink.
        if diagnostics.is_empty() {
            let units: Vec<Unit> = self.units.values().cloned().collect();
            match link(&units) {
                Ok(module) => self.module = Some(module),
                Err(errors) => diagnostics.extend(errors.into_iter().map(Into::into)),
            }
        }
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
        // Nothing to validate against until schemas exist (`∆C8d`).
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
                diagnostics: vec![error.into()],
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
        // Stale until fixed: the broken compile replaced neither unit nor module.
        assert_eq!(html_of(&core, "page.lem"), "ok");
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
                .contains("compiled against `pair(a, b)`"),
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
        assert!(
            reported[0]["message"]
                .as_str()
                .unwrap()
                .contains("already defined in z.lem"),
            "{reported:?}"
        );
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
        assert!(
            reported[0]["message"]
                .as_str()
                .unwrap()
                .contains("`f` is already defined in a.lem"),
            "{reported:?}"
        );
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

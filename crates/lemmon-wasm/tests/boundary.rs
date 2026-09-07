//! The boundary exercised across the real ABI — the `#[wasm_bindgen]` shims, `JsError`
//! included — under Node via `wasm-bindgen-test-runner` (`npm run rust:wasm:test`).
//! The logic behind the shims is covered host-side in the crate's unit tests; what this
//! proves is that the same calls survive the actual wasm32 boundary.
//!
//! Deliberately not behind `#![cfg(target_arch = "wasm32")]`: `#[wasm_bindgen_test]`
//! already expands to nothing off wasm32, so the cfg would not change what runs where —
//! it would only hide this file from `cargo clippy --all-targets` and from every editor
//! analysing the host target. Compiled everywhere, run under the wasm runner.

use lemmon_wasm::{Core, version};
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn reports_a_version_through_the_abi() {
    assert!(!version().is_empty());
}

#[wasm_bindgen_test]
fn compiles_loads_and_renders_through_the_abi() {
    let mut core = Core::new();
    let diagnostics = core
        .compile(r#"[{"path": "page.lem", "text": "hi <lem.site.name>"}]"#)
        .expect("a well-formed compile payload");
    assert_eq!(diagnostics, "[]");
    core.load_data(r#"{"ambient": {"site": {"name": "limulus.net"}}}"#)
        .expect("a well-formed loadData payload");
    let result = core
        .render(r#"{"path": "page.lem"}"#)
        .expect("a compiled page renders");
    assert!(result.contains("hi limulus.net"), "{result}");
}

#[wasm_bindgen_test]
fn throws_a_js_error_for_a_caller_bug() {
    let core = Core::new();
    assert!(core.render(r#"{"path": "never-compiled.lem"}"#).is_err());
}

/// A builtin taking a function re-enters the VM natively, once per nesting level
/// (`map` → `invoke` → the loop again). The frame cap bounds that recursion, so this is
/// where a wasm32 stack must be shown to reach the cap before it overflows: a
/// `lem-fn` that maps itself over a one-element list nests as deep as the VM allows.
#[wasm_bindgen_test]
fn reaches_the_depth_cap_through_nested_invokes_without_trapping() {
    let mut core = Core::new();
    let text = "<lem-fn dive(n: int): int = [n].map(dive).first() ?? 0 /><lem.dive(0)>";
    let files = serde_json::json!([{"path": "deep.lem", "text": text}]).to_string();
    let diagnostics = core.compile(&files).expect("a well-formed compile payload");
    assert_eq!(diagnostics, "[]");
    let result = core
        .render(r#"{"path": "deep.lem"}"#)
        .expect("a runtime failure is a diagnostic, not a throw");
    assert!(
        result.contains("call depth exceeded 256 frames"),
        "{result}"
    );
}

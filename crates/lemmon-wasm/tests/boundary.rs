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

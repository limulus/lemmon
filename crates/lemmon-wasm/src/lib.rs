//! The `wasm-bindgen` boundary: the only part of the core the shell can see.
//!
//! Scaffolding only. The real surface — `compile`, `loadData`, `plan`, `render` — and the
//! typed `@limulus/lemmon-core` wrapper around it arrive with `∆TNT`; see
//! docs/architecture.md §"WASM API". What exists here proves the crate builds for
//! `wasm32-unknown-unknown` as a `cdylib` and that the syntax → vm → wasm dependency
//! chain links.

use lemmon_vm::{RuntimeError, Span};
use wasm_bindgen::prelude::wasm_bindgen;

/// The version of the core this `.wasm` was built from.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// Renders a spanned runtime error the way the shell will receive one.
#[wasm_bindgen]
pub fn describe_error(start: u32, end: u32, message: String) -> String {
    RuntimeError::new(Span::new(start, end), message).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_the_crate_version() {
        assert_eq!(version(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn renders_a_runtime_error_across_the_boundary() {
        assert_eq!(
            describe_error(3, 8, "no such field".to_owned()),
            "3..8: no such field"
        );
    }
}

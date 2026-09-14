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

/// The host most of these compile against: the three names the standard shell binds,
/// none of them typed. A program compiled with no declaration at all has no ambient
/// values, so every test that reads one says so here (D30).
const UNTYPED: &str = r#"{"ambient": {"site": null, "page": null, "build": null}}"#;

#[wasm_bindgen_test]
fn reports_a_version_through_the_abi() {
    assert!(!version().is_empty());
}

#[wasm_bindgen_test]
fn compiles_loads_and_renders_through_the_abi() {
    let mut core = Core::new();
    let diagnostics = core
        .compile(
            r#"[{"path": "page.lem", "text": "hi <lem.site.name>"}]"#,
            Some(UNTYPED.to_owned()),
        )
        .expect("a well-formed compile payload");
    assert_eq!(diagnostics, "[]");
    core.load_data(r#"{"ambient": {"site": {"name": "limulus.net"}}}"#)
        .expect("a well-formed loadData payload");
    let result = core
        .render(r#"{"path": "page.lem"}"#)
        .expect("a compiled page renders");
    assert!(result.contains("hi limulus.net"), "{result}");
}

/// The declaration crosses as its own JSON string, and what it says reaches the
/// checker: `page` is an `Article` here, so a field the schema does not declare is a
/// compile error rather than something the VM finds at render (D30).
#[wasm_bindgen_test]
fn compiles_against_a_typed_host_through_the_abi() {
    let mut core = Core::new();
    let files = serde_json::json!([
        {"path": "ambient.lem", "text": "<lem-type Page>\n  url: url\n</lem-type>"},
        {
            "path": "schema.lem",
            "text": "<lem-type Article extends Page>\n  title: string\n                       layout: (Article, html): html\n</lem-type>",
        },
        {"path": "post.md", "text": "<lem.page.nope>", "record": "Article"},
    ])
    .to_string();
    let host = r#"{"ambient": {"page": "Page"}, "document": "page", "entry": "layout"}"#;
    let diagnostics = core
        .compile(&files, Some(host.to_owned()))
        .expect("a well-formed compile payload");
    assert!(diagnostics.contains("has no field `nope`"), "{diagnostics}");
}

#[wasm_bindgen_test]
fn throws_a_js_error_for_a_caller_bug() {
    let core = Core::new();
    assert!(core.render(r#"{"path": "never-compiled.lem"}"#).is_err());
}

/// A builtin taking a function re-enters the VM natively, once per nesting level
/// (`map` → `invoke` → the loop again). That recursion costs the machine's own stack,
/// which `build.rs` pins for this test binary as for the shipped `.wasm`, so this is
/// where the native cap must be shown to be reached before the stack runs out: a
/// `lem-fn` that maps itself over a one-element list nests as deep as the VM allows, and
/// the diagnostic proves it got there rather than trapping on the way. It is also what
/// fails if the pinned stack ever shrinks under what the cap needs.
#[wasm_bindgen_test]
fn reaches_the_depth_cap_through_nested_invokes_without_trapping() {
    let mut core = Core::new();
    let text = "<lem-fn dive(n: int): int = [n].map(dive).first() ?? 0 /><lem.dive(0)>";
    let files = serde_json::json!([{"path": "deep.lem", "text": text}]).to_string();
    let diagnostics = core
        .compile(&files, Some(UNTYPED.to_owned()))
        .expect("a well-formed compile payload");
    assert_eq!(diagnostics, "[]");
    let result = core
        .render(r#"{"path": "deep.lem"}"#)
        .expect("a runtime failure is a diagnostic, not a throw");
    assert!(
        result.contains("builtins nested more than 64 deep"),
        "{result}"
    );
}

/// The parser recurses once per nested block, and checking, lowering, and rendering what
/// it built recurse as deep again, all on the stack `build.rs` pins. Blocks cost more
/// stack per level than nested parentheses, lists, or negations, so the parser's cap is
/// shown to hold with them: two hundred `lem-if`s, the most it accepts, compile and render
/// without trapping. One level more is refused, which ties this test to the cap: raising
/// the cap fails it until it nests to the new one.
#[wasm_bindgen_test]
fn renders_blocks_nested_to_the_parser_cap_without_trapping() {
    let nested = |levels: usize| {
        let text = format!(
            "{}a{}",
            "<lem-if true>".repeat(levels),
            "</lem-if>".repeat(levels)
        );
        serde_json::json!([{"path": "deep.lem", "text": text}]).to_string()
    };
    let mut core = Core::new();
    let diagnostics = core
        .compile(&nested(200), Some(UNTYPED.to_owned()))
        .expect("a well-formed compile payload");
    assert_eq!(diagnostics, "[]");
    let result = core
        .render(r#"{"path": "deep.lem"}"#)
        .expect("a runtime failure is a diagnostic, not a throw");
    assert!(result.contains(r#""html":"a""#), "{result}");
    let diagnostics = core
        .compile(&nested(201), Some(UNTYPED.to_owned()))
        .expect("a well-formed compile payload");
    assert!(
        diagnostics.contains("`lem-if` is nested too deeply"),
        "{diagnostics}"
    );
}

/// Highlighting runs on the pinned stack too, over code an author controls: a fence of
/// many lines, one very long line, and brackets nested thousands deep, each a push onto
/// syntect's context stack and a span the HTML nests. syntect keeps both stacks on the
/// heap, and this is what shows it does under the real wasm32 build — a trap here would
/// leave the worker's instance unusable (D40).
#[wasm_bindgen_test]
fn highlights_large_long_and_deeply_nested_code_without_trapping() {
    let grammar = "name: Nest\nscope: source.nest\nfile_extensions: [nest]\ncontexts:\n  \
                   main:\n    - match: '\\('\n      push: group\n    \
                   - match: '\"[^\"]*\"'\n      scope: string\n  group:\n    \
                   - meta_scope: meta.group\n    - match: '\\)'\n      pop: true\n    \
                   - include: main\n";
    let host = serde_json::json!({
        "ambient": {"site": null, "page": null, "build": null},
        "grammars": [{"path": "nest.sublime-syntax", "text": grammar}],
    })
    .to_string();
    let code = [
        "(\"many lines\")\n".repeat(20_000),
        format!("{}\n", "\"a\" ".repeat(25_000)),
        format!("{}{}\n", "(".repeat(5_000), ")".repeat(5_000)),
    ]
    .concat();
    let text = format!("```nest\n{code}```\n");
    let files = serde_json::json!([{"path": "big.md", "text": text}]).to_string();
    let mut core = Core::new();
    let diagnostics = core
        .compile(&files, Some(host))
        .expect("a well-formed compile payload");
    assert_eq!(diagnostics, "[]");
    core.load_data(r#"{"ambient": {}}"#)
        .expect("a well-formed loadData payload");
    let result = core
        .render(r#"{"path": "big.md"}"#)
        .expect("a compiled page renders");
    assert!(result.contains("syn-meta syn-group"), "a highlighted page");
}

/// A load crosses with the host's positions and its diagnostics come back placed by
/// them: the data file was never compiled, so nothing but the carried range could
/// place an error in it (D37). An envelope the program cannot be loaded from throws.
#[wasm_bindgen_test]
fn loads_data_against_the_program_through_the_abi() {
    let mut core = Core::new();
    let files = serde_json::json!([
        {"path": "ambient.lem", "text": "<lem-type Page>\n  url: url\n</lem-type>"},
        {
            "path": "site.lem",
            "text": "<lem-type Photo>\n  caption: string\n</lem-type>\n<lem-let photos: {string: Photo} from \"_data/photos.json\" />",
        },
    ])
    .to_string();
    let host = r#"{"ambient": {"page": "Page", "site": {"bindingsOf": "site.lem"}}, "document": "page", "entry": "layout"}"#;
    let diagnostics = core
        .compile(&files, Some(host.to_owned()))
        .expect("a well-formed compile payload");
    assert_eq!(diagnostics, "[]");
    let reported = core
        .load_data(
            r#"{"ambient": {}, "sources": {"photos": {
                "path": "_data/photos.json",
                "value": {"x": {"caption": 3}},
                "spans": {"x.caption": {"start": 20, "end": 32, "range": {
                    "start": {"line": 2, "character": 4},
                    "end": {"line": 2, "character": 16}}}}}}}"#,
        )
        .expect("a well-formed loadData payload");
    assert!(
        reported.contains(r#""path":"_data/photos.json""#),
        "{reported}"
    );
    assert!(reported.contains(r#""line":2"#), "{reported}");
    assert!(core.load_data(r#"{"ambient": {"weather": 1}}"#).is_err());
}

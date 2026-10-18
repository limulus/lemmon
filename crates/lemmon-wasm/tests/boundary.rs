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

/// A render's reply as its two parts: the metadata JSON, and the page's text after it.
/// The page crosses as bytes beside the metadata rather than inside it (D42), so a test
/// that reads either says which it means.
fn decode(reply: &[u8]) -> (String, String) {
    let (header, rest) = reply.split_at(4);
    let length = u32::from_le_bytes(header.try_into().expect("four bytes")) as usize;
    let (meta, page) = rest.split_at(length);
    (
        String::from_utf8(meta.to_vec()).expect("metadata is UTF-8"),
        String::from_utf8(page.to_vec()).expect("a page is UTF-8"),
    )
}

/// The host most of these compile against: the three names the standard shell binds,
/// none of them typed. A program compiled with no declaration at all has no ambient
/// values, so every test that reads one says so here (D30).
const UNTYPED: &str = r#"{"ambient": {"site": null, "page": null, "build": null}}"#;

#[wasm_bindgen_test]
fn reports_a_version_through_the_abi() {
    assert!(!version().is_empty());
}

/// A code block crosses as a `code` mark naming, by id, the payload `compile` sent —
/// the id carried over the real ABI in both replies, so a host can pair them (D42).
#[wasm_bindgen_test]
fn names_a_code_blocks_payload_by_the_id_compile_sent() {
    let host = r#"{"ambient": {}, "marks": [
        {"name": "code", "keys": ["sh"], "payload": "Code", "keyedBy": "language"}]}"#;
    let mut core = Core::new();
    let reply = core
        .compile(
            r#"[{"path": "post.md", "text": "```sh\nls\n```\n"}]"#,
            Some(host.to_owned()),
        )
        .expect("a well-formed compile payload");
    let reply: serde_json::Value = serde_json::from_str(&reply).expect("JSON");
    assert_eq!(reply["diagnostics"], serde_json::json!([]));
    let payload = &reply["payloads"][0];
    assert_eq!(payload["code"], "ls\n");
    core.load_data(r#"{"ambient": {}}"#)
        .expect("a well-formed loadData payload");
    let (meta, _) = decode(
        &core
            .render(r#"{"path": "post.md"}"#)
            .expect("a compiled page renders"),
    );
    let meta: serde_json::Value = serde_json::from_str(&meta).expect("JSON");
    let mark = &meta["marks"][0];
    assert_eq!(
        (&mark["key"], &mark["payloadId"]),
        (&serde_json::json!("sh"), &payload["id"])
    );
}

/// What `compile` sends for files that compiled clean, write no code block, and have
/// no bindings file: `{diagnostics, payloads, sources}` with all three empty (D42,
/// ∆muD).
const CLEAN: &str = r#"{"diagnostics":[],"payloads":[],"sources":[]}"#;

/// The diagnostics of a `compile` reply, for a program whose bindings file reports
/// sources beside them.
fn diagnostics_of(reply: &str) -> serde_json::Value {
    let reply: serde_json::Value = serde_json::from_str(reply).expect("JSON");
    reply["diagnostics"].clone()
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
    assert_eq!(diagnostics, CLEAN);
    core.load_data(r#"{"ambient": {"site": {"name": "limulus.net"}}}"#)
        .expect("a well-formed loadData payload");
    let (meta, page) = decode(
        &core
            .render(r#"{"path": "page.lem"}"#)
            .expect("a compiled page renders"),
    );
    assert_eq!(page, "hi limulus.net");
    assert!(meta.contains(r#""diagnostics":[]"#), "{meta}");
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
    assert_eq!(diagnostics, CLEAN);
    let (meta, page) = decode(
        &core
            .render(r#"{"path": "deep.lem"}"#)
            .expect("a runtime failure is a diagnostic, not a throw"),
    );
    assert!(page.is_empty(), "a failed render sends no page: {page}");
    assert!(meta.contains("builtins nested more than 64 deep"), "{meta}");
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
    assert_eq!(diagnostics, CLEAN);
    let (_, page) = decode(
        &core
            .render(r#"{"path": "deep.lem"}"#)
            .expect("a runtime failure is a diagnostic, not a throw"),
    );
    assert_eq!(page, "a");
    let diagnostics = core
        .compile(&nested(201), Some(UNTYPED.to_owned()))
        .expect("a well-formed compile payload");
    assert!(
        diagnostics.contains("`lem-if` is nested too deeply"),
        "{diagnostics}"
    );
}

/// A page no glob holds crosses in `pages`, the plan lists what loaded with the url the
/// host-named field holds, and the page renders through its layout (D43).
#[wasm_bindgen_test]
fn plans_and_renders_loaded_pages_through_the_abi() {
    let mut core = Core::new();
    let files = serde_json::json!([
        {"path": "ambient.lem", "text": "<lem-type Page>\n  url: url\n</lem-type>"},
        {
            "path": "site.lem",
            "text": "<lem-type Post extends Page>\n  title: string\n  layout: (Post, html): html = post\n</lem-type>\n<lem-fn post(p: Post, body: html)><h1><lem.p.title></h1><lem.body></lem-fn>\n<lem-let posts: [Post] from \"posts/*.md\" />",
        },
        {"path": "home.lem", "text": "---\ntype: Post\ntitle: Home\n---\nhi", "record": "Post"},
    ])
    .to_string();
    let host = r#"{"ambient": {"page": "Page", "site": {"bindingsOf": "site.lem"}}, "document": "page", "entry": "layout", "url": "url"}"#;
    let reply = core
        .compile(&files, Some(host.to_owned()))
        .expect("a well-formed compile payload");
    assert_eq!(diagnostics_of(&reply), serde_json::json!([]));
    // The glob the host matches documents against, and the record it compiles them as.
    let reply: serde_json::Value = serde_json::from_str(&reply).expect("JSON");
    let posts = &reply["sources"][0];
    assert_eq!(
        (
            &posts["name"],
            &posts["kind"],
            &posts["literal"],
            &posts["collects"]
        ),
        (
            &serde_json::json!("posts"),
            &serde_json::json!("file"),
            &serde_json::json!("posts/*.md"),
            &serde_json::json!("Post")
        )
    );
    assert_eq!(posts["span"]["range"]["start"]["line"], 5);
    assert!(
        core.plan().is_err(),
        "a plan before any load is a caller bug"
    );
    let reported = core
        .load_data(
            r#"{"ambient": {}, "sources": {"posts": []},
                "pages": [{"path": "home.lem", "value": {"title": "Home", "url": "/"}}]}"#,
        )
        .expect("a well-formed loadData payload");
    assert_eq!(reported, "[]");
    assert_eq!(
        core.plan().expect("a plan after a load"),
        r#"[{"path":"home.lem","url":"/"}]"#
    );
    let (_, page) = decode(
        &core
            .render(r#"{"path": "home.lem"}"#)
            .expect("a loaded page renders"),
    );
    assert_eq!(page, "<h1>Home</h1>hi");
}

/// A generator's `for:` pair crosses on its page's entry, the plan lists each page it
/// made with its index, `render` takes the index of one, and a generated page reads its
/// neighbours through the page table the render was given (D43).
#[wasm_bindgen_test]
fn plans_and_renders_a_generators_pages_by_index_through_the_abi() {
    let mut core = Core::new();
    let files = serde_json::json!([
        {"path": "ambient.lem", "text": "<lem-type Page>\n  url: url\n</lem-type>"},
        {
            "path": "site.lem",
            "text": "<lem-type Topic extends Page>\n  topic: string\n  prev: Topic?\n  next: Topic?\n  url: url = url(\"/t/\" + self.topic + \"/\")\n  layout: (Topic, html): html = topical\n</lem-type>\n<lem-fn topical(t: Topic, body: html)><h1><lem.t.topic></h1><lem.body></lem-fn>\n<lem-let topics: [string] = [\"git\", \"node\"] />",
        },
        {"path": "topics.lem", "text": "---\ntype: Topic\nfor:\n  topic: site.topics\n---\nafter <lem.page.prev?.topic ?? \"nothing\">", "record": "Topic"},
    ])
    .to_string();
    let host = r#"{"ambient": {"page": "Page", "site": {"bindingsOf": "site.lem"}}, "document": "page", "entry": "layout", "url": "url", "neighbours": ["prev", "next"]}"#;
    let reply = core
        .compile(&files, Some(host.to_owned()))
        .expect("a well-formed compile payload");
    assert_eq!(diagnostics_of(&reply), serde_json::json!([]));
    let reported = core
        .load_data(
            r#"{"ambient": {}, "pages": [{"path": "topics.lem", "value": {},
                "for": {"value": {"topic": "site.topics"}}}]}"#,
        )
        .expect("a well-formed loadData payload");
    assert_eq!(reported, "[]");
    assert_eq!(
        core.plan().expect("a plan after a load"),
        r#"[{"path":"topics.lem","index":0,"url":"/t/git/"},{"path":"topics.lem","index":1,"url":"/t/node/"}]"#
    );
    let (_, page) = decode(
        &core
            .render(r#"{"path": "topics.lem", "index": 1}"#)
            .expect("a generated page renders"),
    );
    assert_eq!(page, "<h1>node</h1>after git");
    assert!(
        core.render(r#"{"path": "topics.lem"}"#).is_err(),
        "a generator's path alone names none of its pages"
    );
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
    let reply = core
        .compile(&files, Some(host.to_owned()))
        .expect("a well-formed compile payload");
    assert_eq!(diagnostics_of(&reply), serde_json::json!([]));
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

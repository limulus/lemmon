//! What the VM owes the golden corpus in `tests/corpus`.
//!
//! One expectation per `render/` case — the rendered HTML with its diagnostics trailer
//! (`.html`) — plus the structural gate that keeps the corpus honest for this stage:
//! every `Op` emitted by some case, so that an instruction cannot exist without a case
//! that exercises it.
//!
//! A case supplies its own data with `lem-let` literals; `page`/`site` are bound to
//! small fixed records here so that the ambient names can be exercised too. Run
//! `npm run corpus:bless` to rewrite the expectations from what the compiler and VM
//! currently produce, then review the diff — a case whose trailer grows lines is a
//! regression, not a new baseline.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::rc::Rc;

use lemmon_corpus::Case;
use lemmon_vm::{Map, Module, Op, SourceFile, Value, compile, render};

const GROUP: &str = "render";

/// The ambient records a render case may read.
fn globals() -> Vec<(&'static str, Value)> {
    let page: Map = [
        (Rc::from("url"), Value::str("/feed/the-rebound/")),
        (Rc::from("title"), Value::str("The Rebound")),
    ]
    .into_iter()
    .collect();
    let site: Map = [(Rc::from("name"), Value::str("limulus.net"))]
        .into_iter()
        .collect();
    vec![("page", Value::map(page)), ("site", Value::map(site))]
}

/// Renders a case: its output, then `--- diagnostics` and one entry per compile
/// diagnostic — warnings too — or for the runtime error, in the shape `.cst` uses for
/// the parser's.
fn html(case: &Case) -> String {
    let path = case.path();
    let files = [SourceFile::new(&path, &case.source)];
    let mut out = String::new();
    let compiled = compile(&files);
    let mut diagnostics: Vec<String> = compiled
        .diagnostics
        .iter()
        .map(ToString::to_string)
        .collect();
    if let Some(module) = compiled.output {
        match render(&module, &path, &globals()) {
            Ok(rendered) => out = rendered,
            Err(error) => diagnostics.push(error.to_string()),
        }
    }
    let _ = writeln!(out, "\n--- diagnostics");
    for line in diagnostics {
        let _ = writeln!(out, "{line}");
    }
    out
}

#[test]
fn renders_every_case_as_expected() {
    lemmon_corpus::check(GROUP, "html", html);
}

#[test]
fn renders_every_document_as_expected() {
    lemmon_corpus::check("document", "html", html);
}

/// Every op some case compiles to. Read from the modules rather than from a trace so
/// that an op behind a branch no case takes still counts as covered by the corpus —
/// the VM's own tests cover execution paths.
fn emitted_ops() -> BTreeSet<&'static str> {
    lemmon_corpus::cases(GROUP)
        .iter()
        .filter_map(|case| compile(&[SourceFile::new(case.path(), &case.source)]).output)
        .flat_map(|module: Module| {
            module
                .functions
                .iter()
                .flat_map(|f| f.code.iter().map(Op::name).collect::<Vec<_>>())
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn emits_every_op_from_some_case() {
    let emitted = emitted_ops();
    let missing: Vec<&str> = Op::NAMES
        .iter()
        .copied()
        .filter(|name| !emitted.contains(name))
        .collect();
    assert!(
        missing.is_empty(),
        "no render case compiles to {}: add one",
        missing.join(", ")
    );
}

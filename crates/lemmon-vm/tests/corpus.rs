//! What the VM owes the golden corpus in `tests/corpus`.
//!
//! One expectation per `render/` case — the rendered HTML with its diagnostics trailer
//! (`.html`) — plus the structural gate that keeps the corpus honest for this stage:
//! every `Op` emitted by some case, so that an instruction cannot exist without a case
//! that exercises it.
//!
//! A `render/` or `document/` case supplies its own data with `lem-let` literals;
//! `page`/`site` are bound to small fixed records here, and named by an untyped host, so
//! that the ambient names can be exercised without a declaration. The `host/` group is
//! where a declaration is the subject: its cases compile against the environment files
//! in `host/environment/`, as a site compiles against the shell's (D30).
//!
//! Run `npm run corpus:bless` to rewrite the expectations from what the compiler and VM
//! currently produce, then review the diff — a case whose trailer grows lines is a
//! regression, not a new baseline.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::rc::Rc;

use lemmon_corpus::Case;
use lemmon_vm::{
    Ambient, Host, Map, Module, Op, SourceFile, Value, compile, is_document, render,
};

const GROUP: &str = "render";

/// The host a `render/` or `document/` case compiles against: the three names the
/// standard shell binds, none of them typed. That is a host a site really has — the
/// Phase 0 spike's — and what any document no collection matched sees.
fn untyped_host() -> Host {
    Host::untyped(["site", "page", "build"])
}

/// The declaration the `host/` group's environment files back: `page` a `Page`, `build`
/// a `Build`, `site` the record of the bindings `site.lem` declares, and every page
/// rendered through its `layout` field.
fn declared_host() -> Host {
    Host::new()
        .bind("page", Ambient::named("Page"))
        .bind("build", Ambient::named("Build"))
        .bind("site", Ambient::bindings(SITE))
        .documents("page")
        .rendered_through("layout")
}

const AMBIENT: &str = "host/environment/ambient.lem";
const SITE: &str = "host/environment/site.lem";

/// The ambient records a `render/` or `document/` case may read. Deliberately short of
/// what every case names: a field missing from the data is how those cases reach a
/// runtime error at all.
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

/// The records a `host/` case reads: every field its schemas declare and its bindings
/// name, since data a host loads is untagged and unvalidated until ∆iCW and a field
/// that is missing there fails at render however well the file checked.
fn declared_globals() -> Vec<(&'static str, Value)> {
    let page: Map = [
        (Rc::from("url"), Value::str("/feed/the-rebound/")),
        (Rc::from("title"), Value::str("The Rebound")),
        (Rc::from("subhead"), Value::str("A podcast")),
        (
            Rc::from("hero"),
            Value::map(
                [(Rc::from("caption"), Value::str("A horseshoe crab"))]
                    .into_iter()
                    .collect(),
            ),
        ),
    ]
    .into_iter()
    .collect();
    let site: Map = [
        (Rc::from("name"), Value::str("limulus.net")),
        (Rc::from("tagline"), Value::str("Notes from limulus.net")),
    ]
    .into_iter()
    .collect();
    vec![("page", Value::map(page)), ("site", Value::map(site))]
}

/// Renders a case: its output, then `--- diagnostics` and one entry per compile
/// diagnostic — warnings too — or for the runtime error, in the shape `.cst` uses for
/// the parser's.
fn rendered(
    case: &Case,
    files: &[SourceFile],
    host: &Host,
    globals: &[(&str, Value)],
) -> String {
    let path = case.path();
    let mut out = String::new();
    let compiled = compile(files, host);
    let mut diagnostics: Vec<String> = compiled
        .diagnostics
        .iter()
        .map(ToString::to_string)
        .collect();
    if let Some(module) = compiled.output {
        match render(&module, &path, globals) {
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

/// A case that compiles alone, against a host that names its ambient values and types
/// none of them.
fn html(case: &Case) -> String {
    let files = [SourceFile::new(case.path(), &case.source)];
    rendered(case, &files, &untyped_host(), &globals())
}

/// A `host/` case, compiled with the environment the declaration describes — the two
/// files skipped where the case is one of them, since one compile may not name a file
/// twice.
fn typed_html(case: &Case) -> String {
    let path = case.path();
    let mut files: Vec<SourceFile> = [AMBIENT, SITE]
        .iter()
        .filter(|environment| **environment != path)
        .map(|environment| {
            let text = fs::read_to_string(lemmon_corpus::root().join(environment))
                .unwrap_or_else(|e| panic!("read {environment}: {e}"));
            SourceFile::new(*environment, text)
        })
        .collect();
    let mut file = SourceFile::new(&path, &case.source);
    file.record = record_of(case);
    files.push(file);
    rendered(case, &files, &declared_host(), &declared_globals())
}

/// The record a document case is compiled as: the `type:` key of its front matter,
/// which is the one raw key a host reads before a document has a type (D30). A shell
/// matches globs to decide this (∆4tG); a corpus case says it itself.
fn record_of(case: &Case) -> Option<String> {
    if !is_document(&case.path()) {
        return None;
    }
    let mut lines = case.source.lines();
    if lines.next() != Some("---") {
        return None;
    }
    // The key is read from the front matter and nowhere else: a `---` that is never
    // closed is not front matter, and a case that says `type:` in its prose is prose.
    let mut front = Vec::new();
    for line in lines {
        if line == "---" {
            return front
                .iter()
                .find_map(|line: &&str| line.strip_prefix("type:"))
                .map(|record| record.trim().to_owned());
        }
        front.push(line);
    }
    None
}

#[test]
fn renders_every_case_as_expected() {
    lemmon_corpus::check(GROUP, "html", html);
}

#[test]
fn renders_every_document_as_expected() {
    lemmon_corpus::check("document", "html", html);
}

#[test]
fn renders_every_case_against_the_declared_host_as_expected() {
    lemmon_corpus::check("host", "html", typed_html);
}

/// Every op some case compiles to. Read from the modules rather than from a trace so
/// that an op behind a branch no case takes still counts as covered by the corpus —
/// the VM's own tests cover execution paths.
fn emitted_ops() -> BTreeSet<&'static str> {
    lemmon_corpus::cases(GROUP)
        .iter()
        .filter_map(|case| {
            compile(
                &[SourceFile::new(case.path(), &case.source)],
                &untyped_host(),
            )
            .output
        })
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

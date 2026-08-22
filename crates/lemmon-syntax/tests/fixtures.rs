//! Snapshot tests: every fixture's dump must match its checked-in `.txt`, and parsing
//! arbitrary input must neither panic nor lose a byte.
//!
//! Set `UPDATE_FIXTURES=1` to rewrite the expected files from the current output; set
//! `DUMPS_DIR=<path>` to also copy them there.

use std::fs;
use std::path::{Path, PathBuf};

use lemmon_syntax::ast::{AstNode, Block, Expr, IfArm, Item, Params, TypeAnnotation};
use lemmon_syntax::{Span, SyntaxKind, SyntaxNode, dump, parse};
use proptest::prelude::*;

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn check_dir(group: &str) {
    let dir = fixtures_dir().join(group);
    let mut names: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("fixture dir")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "lem"))
        .collect();
    names.sort();
    assert!(!names.is_empty(), "no fixtures in {}", dir.display());
    let update = std::env::var_os("UPDATE_FIXTURES").is_some();
    let dumps_dir = std::env::var_os("DUMPS_DIR").map(PathBuf::from);
    let mut failures = Vec::new();
    for lem in names {
        let src = fs::read_to_string(&lem).expect("read fixture");
        let actual = dump(&src);
        let expected_path = lem.with_extension("txt");
        if update {
            fs::write(&expected_path, &actual).expect("write expected");
        }
        if let Some(dumps) = &dumps_dir {
            let out = dumps.join(group);
            fs::create_dir_all(&out).expect("create dumps dir");
            fs::write(out.join(expected_path.file_name().expect("name")), &actual)
                .expect("write dump");
        }
        let expected = fs::read_to_string(&expected_path).unwrap_or_default();
        if expected != actual {
            failures.push(format!(
                "{}:\n--- expected\n{expected}\n--- actual\n{actual}",
                lem.display()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn dumps_every_ok_fixture_as_expected() {
    check_dir("ok");
}

#[test]
fn dumps_every_broken_fixture_as_expected() {
    check_dir("broken");
}

#[test]
fn reports_no_diagnostics_for_ok_fixtures() {
    for entry in fs::read_dir(fixtures_dir().join("ok")).expect("dir") {
        let path = entry.expect("entry").path();
        if path.extension().is_some_and(|e| e == "lem") {
            let src = fs::read_to_string(&path).expect("read");
            assert_eq!(parse(&src).diagnostics, vec![], "{}", path.display());
        }
    }
}

/// Every node the typed views reach from the root, by byte range, so that a node kind
/// without a view — or an accessor that drops a child — shows up as a missing range.
fn visit_ast(src: &str) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    visit_items(parse(src).root().items(), &mut out);
    out.sort_unstable();
    out
}

fn range(node: &SyntaxNode) -> (u32, u32) {
    let Span { start, end } = node.text_range().into();
    (start, end)
}

fn visit_expr(e: Option<Expr>, out: &mut Vec<(u32, u32)>) {
    let Some(e) = e else { return };
    out.push(range(e.syntax()));
    match &e {
        Expr::Binary(b) => {
            visit_expr(b.lhs(), out);
            visit_expr(b.rhs(), out);
        }
        Expr::Unary(u) => visit_expr(u.operand(), out),
        Expr::Ternary(t) => {
            visit_expr(t.condition(), out);
            visit_expr(t.then(), out);
            visit_expr(t.otherwise(), out);
        }
        Expr::Call(c) => {
            visit_expr(c.callee(), out);
            if let Some(args) = c.args() {
                out.push(range(args.syntax()));
                args.positional().for_each(|a| visit_expr(Some(a), out));
                for named in args.named() {
                    out.push(range(named.syntax()));
                    visit_expr(named.value(), out);
                }
            }
        }
        Expr::Member(m) => visit_expr(m.object(), out),
        Expr::Index(i) => {
            visit_expr(i.object(), out);
            visit_expr(i.index(), out);
        }
        Expr::Lambda(l) => {
            if let Some(params) = l.params() {
                out.push(range(params.syntax()));
            }
            visit_expr(l.body(), out);
        }
        Expr::List(l) => l.elements().for_each(|e| visit_expr(Some(e), out)),
        Expr::Map(m) => {
            for entry in m.entries() {
                out.push(range(entry.syntax()));
                visit_expr(entry.value(), out);
            }
        }
        Expr::Paren(p) => visit_expr(p.inner(), out),
        Expr::TypeTest(t) => visit_expr(t.expr(), out),
        Expr::Literal(_) | Expr::Name(_) => {}
    }
}

fn visit_signature(
    params: Option<Params>,
    ret: Option<TypeAnnotation>,
    out: &mut Vec<(u32, u32)>,
) {
    if let Some(params) = params {
        out.push(range(params.syntax()));
        for param in params.params() {
            out.push(range(param.syntax()));
            if let Some(ty) = param.type_annotation() {
                out.push(range(ty.syntax()));
            }
        }
    }
    if let Some(ty) = ret {
        out.push(range(ty.syntax()));
    }
}

fn visit_block(block: Option<Block>, out: &mut Vec<(u32, u32)>) {
    if let Some(block) = block {
        visit_items(block.items(), out);
    }
}

fn visit_items(items: impl Iterator<Item = Item>, out: &mut Vec<(u32, u32)>) {
    for item in items {
        out.push(range(item.syntax()));
        match &item {
            Item::Text(_) | Item::Unknown(_) => {}
            Item::Access(a) => visit_expr(a.expr(), out),
            Item::If(tag) => {
                for IfArm {
                    condition, items, ..
                } in tag.arms()
                {
                    visit_expr(condition, out);
                    visit_items(items.into_iter(), out);
                }
            }
            Item::For(tag) => {
                if let Some(binding) = tag.binding() {
                    out.push(range(binding.syntax()));
                }
                visit_expr(tag.iterable(), out);
                visit_items(tag.items(), out);
                visit_items(tag.else_items().into_iter().flatten(), out);
            }
            Item::LetVoid(tag) => {
                if let Some(ty) = tag.type_annotation() {
                    out.push(range(ty.syntax()));
                }
                visit_expr(tag.value(), out);
            }
            Item::LetBlock(tag) => visit_block(tag.body(), out),
            Item::FnBlock(tag) => {
                visit_signature(tag.params(), tag.return_type(), out);
                visit_block(tag.body(), out);
            }
            Item::FnVoid(tag) => {
                visit_signature(tag.params(), tag.return_type(), out);
                visit_expr(tag.value(), out);
            }
            Item::Unsupported(tag) => visit_block(tag.body(), out),
        }
    }
}

/// Kinds that are reached only through their parent's accessors and have no view of
/// their own in the walk: structure, separators, and the opaque heads of tags the
/// parser does not parse.
fn is_structural(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::Root
            | SyntaxKind::Head
            | SyntaxKind::Block
            | SyntaxKind::CloseTag
            | SyntaxKind::ElseIfTag
            | SyntaxKind::ElseTag
    )
}

#[test]
fn reaches_every_node_of_every_ok_fixture_through_the_typed_views() {
    for entry in fs::read_dir(fixtures_dir().join("ok")).expect("dir") {
        let path = entry.expect("entry").path();
        if !path.extension().is_some_and(|e| e == "lem") {
            continue;
        }
        let src = fs::read_to_string(&path).expect("read");
        let mut expected: Vec<(u32, u32)> = parse(&src)
            .syntax()
            .descendants()
            .filter(|n| !is_structural(n.kind()))
            .map(|n| range(&n))
            .collect();
        expected.sort_unstable();
        assert_eq!(visit_ast(&src), expected, "{}", path.display());
    }
}

#[test]
fn walks_every_broken_fixture_without_panicking() {
    for entry in fs::read_dir(fixtures_dir().join("broken")).expect("dir") {
        let path = entry.expect("entry").path();
        if path.extension().is_some_and(|e| e == "lem") {
            let src = fs::read_to_string(&path).expect("read");
            let _ = visit_ast(&src);
        }
    }
}

/// Biased towards Lemmon syntax so that the generator actually reaches the parser's
/// interesting paths rather than producing text runs alone.
fn lemmon_like() -> impl Strategy<Value = String> {
    let atom = prop_oneof![
        Just("<lem."),
        Just("<lem-if "),
        Just("<lem-for "),
        Just("<lem-let "),
        Just("<lem-fn "),
        Just("<lem-else"),
        Just("<lem-else-if "),
        Just("<lem-match "),
        Just("<lem-iff "),
        Just("</lem-if>"),
        Just("</lem-for>"),
        Just("</lem-let>"),
        Just("</lem-fn>"),
        Just(">"),
        Just("/>"),
        Just(" x"),
        Just(" in "),
        Just(" = "),
        Just(": int"),
        Just("("),
        Just(")"),
        Just("["),
        Just("]"),
        Just("{"),
        Just("}"),
        Just(","),
        Just("?"),
        Just(":"),
        Just("|"),
        Just("||"),
        Just("??"),
        Just("+"),
        Just("lt "),
        Just("is T"),
        Just("\"s\""),
        Just("1.5"),
        Just("\n<p>t</p>\n"),
        Just("<"),
        Just("é"),
    ];
    prop::collection::vec(atom, 0..40).prop_map(|parts| parts.concat())
}

proptest! {
    #[test]
    fn never_panics_and_round_trips_arbitrary_text(src in "\\PC*") {
        let parse = parse(&src);
        prop_assert_eq!(parse.syntax().text().to_string(), src);
    }

    #[test]
    fn never_panics_and_round_trips_lemmon_like_input(src in lemmon_like()) {
        let parse = parse(&src);
        prop_assert_eq!(parse.syntax().text().to_string(), src.as_str());
        for d in &parse.diagnostics {
            prop_assert!(d.span.end as usize <= src.len());
        }
    }
}

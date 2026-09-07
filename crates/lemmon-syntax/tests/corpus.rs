//! What the syntax crate owes the golden corpus in `tests/corpus`.
//!
//! Two expectations per case — the tree with its diagnostics (`.cst`) and the
//! tag-boundary projection the future tree-sitter grammar has to agree with (`.tags`) —
//! plus the structural gates that keep the corpus honest: every `SyntaxKind` reached,
//! every node reached through the typed views, every projected boundary well nested.
//! (That no expectation is left orphaned by a rename is corpus-wide rather than
//! stage-specific, so `lemmon-corpus` gates it once for every stage.)
//!
//! Run `npm run corpus:bless` to rewrite the expectations from what the parser currently
//! produces, then review the diff — a clean case whose diagnostics trailer grows lines is
//! a regression, not a new baseline.

use std::collections::BTreeSet;

use lemmon_corpus::Case;
use lemmon_syntax::ast::{
    AstNode, Block, Expr, IfArm, Item, Params, TypeAnnotation, TypeExpr,
};
use lemmon_syntax::{
    Parse, Span, SyntaxKind, SyntaxNode, dump, parse, parse_document, tag_dump,
};

#[test]
fn dumps_every_case_as_expected() {
    lemmon_corpus::check("syntax", "cst", |case| dump(&parse(&case.source)));
}

#[test]
fn projects_the_tag_boundaries_of_every_case_as_expected() {
    lemmon_corpus::check("syntax", "tags", |case| tag_dump(&parse(&case.source)));
}

/// Documents get the same two expectations from their assembled tree: `.cst` shows
/// the segmentation — prose as text runs, each lem-block as its item — and `.tags` is
/// what a tree-sitter grammar with a markdown injection has to agree with.
#[test]
fn dumps_every_document_as_expected() {
    lemmon_corpus::check("document", "cst", |case| {
        dump(&parse_document(&case.source).parse)
    });
}

#[test]
fn projects_the_tag_boundaries_of_every_document_as_expected() {
    lemmon_corpus::check("document", "tags", |case| {
        tag_dump(&parse_document(&case.source).parse)
    });
}

/// Every case of every group with its tree — a template's parsed, a document's
/// assembled by the segmenter. The structural gates below run over both: a document's
/// tree is hand-built from spliced green nodes rather than produced by the parser, so
/// it is exactly the tree that most needs holding to the same shape rules.
fn every_case() -> Vec<(Case, Parse)> {
    ["syntax", "document"]
        .into_iter()
        .flat_map(lemmon_corpus::cases)
        .map(|case| {
            let parse = tree(&case);
            (case, parse)
        })
        .collect()
}

/// The tree of a case, by the group its input's extension puts it in.
fn tree(case: &Case) -> Parse {
    if case.input.extension().is_some_and(|e| e == "md") {
        parse_document(&case.source).parse
    } else {
        parse(&case.source)
    }
}

/// Whether a case is meant to parse clean. Derived rather than declared by a directory:
/// the checked-in `.cst` carries the diagnostics either way, so a case that starts
/// erroring shows up as lines appearing in a trailer that was empty.
fn is_clean(parse: &Parse) -> bool {
    parse.diagnostics.is_empty()
}

/// Every kind the corpus reaches, over every case — the gate that makes invariant 9
/// mechanical. Walks the tree rather than the dump text because `TextRun`/`Text` and
/// `CloseTagToken`/`CloseTag` share a printed name and would be conflated.
fn reached_kinds() -> BTreeSet<SyntaxKind> {
    every_case()
        .iter()
        .flat_map(|(_, parse)| {
            parse
                .syntax()
                .descendants_with_tokens()
                .map(|element| element.kind())
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Kinds no corpus case can reach, each for a reason that is a property of the design
/// rather than a gap in coverage.
///
/// `OpenInHead` is a placeholder: the scanner breaks on it and re-enters text mode at
/// the `<` rather than emitting it (`scanner.rs`, `HeadEnd::Reopened`), which the
/// scanner's own `never_emits_a_placeholder_kind` property test pins. It exists in
/// `SyntaxKind` only because that enumeration mirrors `TokenKind` one for one.
const UNREACHABLE: &[SyntaxKind] = &[SyntaxKind::OpenInHead];

#[test]
fn reaches_every_syntax_kind_from_some_case() {
    let reached = reached_kinds();
    let missing: Vec<String> = SyntaxKind::ALL
        .iter()
        .filter(|kind| !reached.contains(kind) && !UNREACHABLE.contains(kind))
        .map(|kind| format!("{kind:?}"))
        .collect();
    assert!(
        missing.is_empty(),
        "no corpus case reaches {}: add one, or add it to `UNREACHABLE` with the reason",
        missing.join(", ")
    );
}

#[test]
fn keeps_the_unreachable_list_to_kinds_that_are_really_unreachable() {
    let reached = reached_kinds();
    let stale: Vec<String> = UNREACHABLE
        .iter()
        .filter(|kind| reached.contains(kind))
        .map(|kind| format!("{kind:?}"))
        .collect();
    assert!(
        stale.is_empty(),
        "{} is reached by a corpus case; drop it from `UNREACHABLE`",
        stale.join(", ")
    );
}

/// Every node the typed views reach from the root, by byte range, so that a node kind
/// without a view — or an accessor that drops a child — shows up as a missing range.
fn visit_ast(parse: &Parse) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    visit_items(parse.root().items(), &mut out);
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
        Expr::Literal(_) | Expr::Name(_) | Expr::Variant(_) => {}
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
            visit_annotation(param.type_annotation(), out);
        }
    }
    visit_annotation(ret, out);
}

fn visit_annotation(annotation: Option<TypeAnnotation>, out: &mut Vec<(u32, u32)>) {
    if let Some(annotation) = annotation {
        out.push(range(annotation.syntax()));
        visit_type(annotation.ty(), out);
    }
}

fn visit_type(ty: Option<TypeExpr>, out: &mut Vec<(u32, u32)>) {
    let Some(ty) = ty else { return };
    out.push(range(ty.syntax()));
    match &ty {
        TypeExpr::Named(_) => {}
        TypeExpr::Optional(o) => visit_type(o.inner(), out),
        TypeExpr::List(l) => visit_type(l.element(), out),
        TypeExpr::Map(m) => {
            visit_type(m.key(), out);
            visit_type(m.value(), out);
        }
        TypeExpr::Fn(f) => {
            f.params().for_each(|t| visit_type(Some(t), out));
            visit_type(f.ret(), out);
        }
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
                visit_annotation(tag.type_annotation(), out);
                visit_expr(tag.value(), out);
            }
            Item::LetBlock(tag) => visit_block(tag.body(), out),
            Item::LetFrom(tag) => {
                visit_annotation(tag.type_annotation(), out);
                visit_expr(tag.source(), out);
            }
            Item::FnBlock(tag) => {
                visit_signature(tag.params(), tag.return_type(), out);
                visit_block(tag.body(), out);
            }
            Item::FnVoid(tag) => {
                visit_signature(tag.params(), tag.return_type(), out);
                visit_expr(tag.value(), out);
            }
            Item::TypeDecl(tag) => {
                for field in tag.fields() {
                    out.push(range(field.syntax()));
                    visit_annotation(field.type_annotation(), out);
                    visit_expr(field.index(), out);
                    visit_expr(field.default(), out);
                }
            }
            Item::EnumDecl(_) => {}
            Item::Match(tag) => {
                visit_expr(tag.subject(), out);
                // The block's own items are the layout text between the cases.
                visit_block(tag.block(), out);
                for case in tag.cases() {
                    out.push(range(case.syntax()));
                    if let Some(pattern) = case.pattern() {
                        out.push(range(pattern.syntax()));
                        if let Some(literal) = pattern.literal() {
                            out.push(range(literal.syntax()));
                        }
                    }
                    visit_block(case.body(), out);
                }
                if let Some(default) = tag.default() {
                    out.push(range(default.syntax()));
                    visit_block(default.body(), out);
                }
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
            | SyntaxKind::FieldList
            | SyntaxKind::CloseTag
            | SyntaxKind::ElseIfTag
            | SyntaxKind::ElseTag
    )
}

#[test]
fn reaches_every_node_of_every_clean_case_through_the_typed_views() {
    let cases = every_case();
    let clean: Vec<&(Case, Parse)> =
        cases.iter().filter(|(_, parse)| is_clean(parse)).collect();
    assert!(!clean.is_empty(), "no clean case to walk");
    for (case, parse) in clean {
        let mut expected: Vec<(u32, u32)> = parse
            .syntax()
            .descendants()
            .filter(|n| !is_structural(n.kind()))
            .map(|n| range(&n))
            .collect();
        expected.sort_unstable();
        assert_eq!(visit_ast(parse), expected, "{}", case.name);
    }
}

#[test]
fn walks_every_case_with_diagnostics_without_panicking() {
    let cases = every_case();
    let broken: Vec<&(Case, Parse)> =
        cases.iter().filter(|(_, parse)| !is_clean(parse)).collect();
    assert!(!broken.is_empty(), "no case with diagnostics to walk");
    for (_, parse) in broken {
        let _ = visit_ast(parse);
    }
}

/// What makes `.tags` usable as an agreement target: one root covering the whole file,
/// and boundaries that nest properly — each inside its parent, each after its previous
/// sibling ends. A grammar can be diffed against that; it cannot be diffed against
/// spans that overlap or run backwards.
///
/// Deliberately *not* asserted: that the boundaries tile every byte. Trivia between
/// items belongs to no boundary, and a void tag whose head the scanner cut ends at the
/// last token its head consumed rather than at the cut — see
/// `syntax/recovery/unterminated-string`, where the tag's own bytes 16..33 fall outside
/// `LetVoid@0..16`.
#[test]
fn projects_tag_boundaries_that_nest_and_never_overlap() {
    for (case, parse) in every_case() {
        let spans = boundary_spans(&tag_dump(&parse));
        let (root, rest) = spans.split_first().expect("a root line");
        assert_eq!(root.0, 0, "{}: root is not at depth zero", case.name);
        assert_eq!(
            root.1,
            (0, u32::try_from(case.source.len()).expect("fits")),
            "{}: root does not cover the file",
            case.name
        );
        // `open[d]` is the span of the boundary currently open at depth `d`, and
        // `after[d]` where its last child at depth `d + 1` ended.
        let mut open = vec![root.1];
        let mut after = vec![0u32];
        for &(depth, (start, end)) in rest {
            assert!(
                (1..=open.len()).contains(&depth),
                "{}: depth {depth} outside 1..={} below the root",
                case.name,
                open.len()
            );
            open.truncate(depth);
            after.truncate(depth + 1);
            let (parent_start, parent_end) = open[depth - 1];
            assert!(
                start >= parent_start && end <= parent_end,
                "{}: {start}..{end} escapes its parent {parent_start}..{parent_end}",
                case.name
            );
            assert!(
                start >= after[depth - 1],
                "{}: {start}..{end} overlaps a sibling ending at {}",
                case.name,
                after[depth - 1]
            );
            assert!(start <= end, "{}: {start}..{end} runs backwards", case.name);
            after[depth - 1] = end;
            open.push((start, end));
            after.push(start);
        }
    }
}

/// `(depth, (start, end))` for every line of a [`tag_dump`].
fn boundary_spans(dump: &str) -> Vec<(usize, (u32, u32))> {
    dump.lines()
        .map(|line| {
            let depth = (line.len() - line.trim_start().len()) / 2;
            let (_, span) = line.rsplit_once('@').expect("a span on every line");
            let (start, end) = span.split_once("..").expect("a range");
            (
                depth,
                (start.parse().expect("start"), end.parse().expect("end")),
            )
        })
        .collect()
}

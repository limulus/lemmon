//! The dump format: a tree and its diagnostics rendered as text.
//!
//! What the golden corpus compares against (`tests/corpus`, invariant 9). [`dump`] shows
//! every node and token with its byte range; [`tag_dump`] projects the same tree down to
//! the tag boundaries alone, which is the shape `tree-sitter-lemmon` must agree with
//! (∆5MM). Both take a [`Parse`] rather than source, so that a document's tree —
//! assembled by [`parse_document`](crate::parse_document), not parsed — dumps the same
//! way as a template's.
//!
//! Rendering is not parsing: nothing here is reached from [`parse`](super::parse), and
//! nothing in the parser reads it.

use super::Parse;
use crate::{Severity, Span, SyntaxKind, SyntaxNode};

/// Renders a tree and its diagnostics in the bake-off's dump format.
///
/// A test and debugging aid, not an API: one node or token per line, indented two
/// spaces per depth, then a blank line, `--- diagnostics`, and one diagnostic per line —
/// `start..end: message`, with `warning: ` before a warning's message and each label on
/// its own line below, indented, as `start..end: note`.
///
/// The spans here are byte offsets. What lands in a checked-in `.cst` is not: the corpus
/// harness renders the trailer's spans as `line:col` against the case's source, since
/// nobody can match a byte offset to a place in a file by eye. The tree above the
/// trailer keeps its byte ranges either way.
/// Takes a [`Parse`] rather than source so that a document's tree — assembled by
/// [`parse_document`](crate::parse_document), not parsed — dumps the same way.
#[must_use]
pub fn dump(parse: &Parse) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    for event in parse.syntax().preorder_with_tokens() {
        let element = match event {
            rowan::WalkEvent::Enter(element) => element,
            rowan::WalkEvent::Leave(_) => {
                depth -= 1;
                continue;
            }
        };
        let range = element.text_range();
        out.push_str(&"  ".repeat(depth));
        out.push_str(&element.kind().name());
        out.push_str(&format!(
            "@{}..{}",
            u32::from(range.start()),
            u32::from(range.end())
        ));
        if let rowan::NodeOrToken::Token(token) = element {
            out.push_str(&format!(" {}", quoted(token.text())));
        }
        out.push('\n');
        depth += 1;
    }
    out.push_str("\n--- diagnostics\n");
    for diagnostic in &parse.diagnostics {
        let Span { start, end } = diagnostic.span;
        let severity = match diagnostic.severity {
            Severity::Error => "",
            Severity::Warning => "warning: ",
        };
        out.push_str(&format!(
            "{start}..{end}: {severity}{}\n",
            diagnostic.message
        ));
        for label in &diagnostic.labels {
            let Span { start, end } = label.span;
            out.push_str(&format!("  {start}..{end}: {}\n", label.message));
        }
    }
    out
}

/// Renders the tag boundaries of a tree: every tag interior elided.
///
/// This is the agreement target invariant 9 names — what the future `tree-sitter-lemmon`
/// grammar has to reproduce for every corpus case. A context-free grammar can say where
/// a tag begins and ends; it cannot be held to the shape the Pratt parser gave a head.
/// So heads, expressions, and the `Block` wrapper are dropped and their tag descendants
/// lifted, while text runs stay.
///
/// `Error` is kept, as a node or a token, but only where items stand — directly under a
/// `Root` or a `Block`. There it marks bytes the parser could place in neither a tag nor
/// text, which a grammar does have to account for; inside a head it is tag interior like
/// any expression, and elided with the rest.
///
/// The projection is not a partition of the file: trivia between items belongs to no
/// boundary, and a void tag whose head the scanner cut ends at the last token its head
/// consumed rather than at the cut. Those gaps are visible in `tests/corpus/**/*.tags`
/// and are the parser's shape, not the projection's.
#[must_use]
pub fn tag_dump(parse: &Parse) -> String {
    let mut out = String::new();
    project(&parse.syntax(), 0, false, &mut out);
    out
}

/// Whether an element of `kind` is a tag boundary rather than tag interior — the
/// elements [`tag_dump`] keeps.
///
/// `in_items` says whether the element stands where items run, directly under a `Root`
/// or a `Block`. That is the only place an `Error` is a boundary, and it is why this
/// takes more than a kind: the same `Error`, node or token, is tag interior inside a
/// head. No other boundary is position-dependent.
fn is_boundary(kind: SyntaxKind, in_items: bool) -> bool {
    matches!(
        kind,
        SyntaxKind::Root
            | SyntaxKind::TextRun
            | SyntaxKind::AccessTag
            | SyntaxKind::IfTag
            | SyntaxKind::ElseIfTag
            | SyntaxKind::ElseTag
            | SyntaxKind::ForTag
            | SyntaxKind::LetVoid
            | SyntaxKind::LetBlock
            | SyntaxKind::LetFrom
            | SyntaxKind::FnBlock
            | SyntaxKind::FnVoid
            | SyntaxKind::TypeDecl
            | SyntaxKind::EnumDecl
            | SyntaxKind::MatchTag
            | SyntaxKind::CaseTag
            | SyntaxKind::DefaultTag
            | SyntaxKind::StyleBlock
            | SyntaxKind::HeadBlock
            | SyntaxKind::ScriptVoid
            | SyntaxKind::UnsupportedTag
            | SyntaxKind::UnknownTag
            | SyntaxKind::CloseTag
    ) || (in_items && kind == SyntaxKind::Error)
}

fn project(node: &SyntaxNode, depth: usize, in_items: bool, out: &mut String) {
    let depth = if is_boundary(node.kind(), in_items) {
        line(out, depth, node.kind(), node.text_range());
        depth + 1
    } else {
        depth
    };
    let items = matches!(node.kind(), SyntaxKind::Root | SyntaxKind::Block);
    for child in node.children_with_tokens() {
        match child {
            rowan::NodeOrToken::Node(child) => project(&child, depth, items, out),
            rowan::NodeOrToken::Token(token) => {
                if is_boundary(token.kind(), items) {
                    line(out, depth, token.kind(), token.text_range());
                }
            }
        }
    }
}

fn line(out: &mut String, depth: usize, kind: SyntaxKind, range: rowan::TextRange) {
    out.push_str(&"  ".repeat(depth));
    out.push_str(&kind.name());
    out.push_str(&format!(
        "@{}..{}\n",
        u32::from(range.start()),
        u32::from(range.end())
    ));
}

/// JSON-escapes `text` for the dump, truncating at 40 bytes (on a character boundary)
/// with an ellipsis.
fn quoted(text: &str) -> String {
    const LIMIT: usize = 40;
    let mut cut = LIMIT.min(text.len());
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    let mut out = String::from('"');
    for c in text[..cut].chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
    }
    out.push('"');
    if cut < text.len() {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Diagnostic, Span, parse};

    #[test]
    fn dumps_tokens_with_escaped_and_truncated_text() {
        assert_eq!(quoted("a\"b\\c\nd"), "\"a\\\"b\\\\c\\nd\"");
        let long = "x".repeat(50);
        assert_eq!(quoted(&long), format!("\"{}\"…", "x".repeat(40)));
    }

    #[test]
    fn truncates_on_a_character_boundary() {
        let text = format!("{}é", "x".repeat(39));
        assert_eq!(quoted(&text), format!("\"{}\"…", "x".repeat(39)));
    }

    #[test]
    fn dumps_an_empty_source_as_a_bare_root() {
        assert_eq!(dump(&parse("")), "Root@0..0\n\n--- diagnostics\n");
    }

    #[test]
    fn dumps_a_warning_and_its_labels_on_their_own_lines() {
        // The parser emits neither yet; the checker will, through the same trailer.
        let mut parsed = parse("");
        parsed.diagnostics.push(
            Diagnostic::warning("unused", Span::new(1, 2))
                .with_label(Span::new(0, 1), "bound here"),
        );
        assert_eq!(
            dump(&parsed),
            "Root@0..0\n\n--- diagnostics\n1..2: warning: unused\n  0..1: bound here\n"
        );
    }
}

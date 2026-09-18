//! Properties that hold for *any* input, not just the corpus's hand-written cases:
//! parsing never panics and never loses a byte, and every diagnostic points inside the
//! source.

use lemmon_syntax::{SyntaxKind, parse};
use proptest::prelude::*;

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
        Just("---\n"),
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

    #[test]
    fn front_matter_changes_nothing_about_how_the_body_parses(body in lemmon_like()) {
        // A closed block is parsed past: the body's tree and diagnostics are the ones
        // it gets on its own, placed after the block. A body that itself opens with a
        // rule is the one exception, and only in its diagnostics: alone it is an
        // unclosed block, after a block it is text, since a file has one front matter.
        prop_assume!(!body.starts_with("---"));
        const BLOCK: &str = "---\na: 1\n---\n";
        let alone = parse(&body);
        let after = parse(&format!("{BLOCK}{body}"));
        let mut children = after.syntax().children_with_tokens();
        let first = children.next().expect("the block");
        prop_assert_eq!(first.kind(), SyntaxKind::FrontMatter);
        prop_assert_eq!(first.text_range().len(), rowan::TextSize::of(BLOCK));
        let rest: Vec<String> = children.map(|c| c.to_string()).collect();
        let own: Vec<String> =
            alone.syntax().children_with_tokens().map(|c| c.to_string()).collect();
        prop_assert_eq!(rest, own);
        let by = u32::try_from(BLOCK.len()).expect("a short block");
        let shifted: Vec<_> =
            alone.diagnostics.into_iter().map(|d| d.shifted(by)).collect();
        prop_assert_eq!(after.diagnostics, shifted);
    }
}

//! Properties that hold for *any* input, not just the corpus's hand-written cases:
//! parsing never panics and never loses a byte, and every diagnostic points inside the
//! source.

use lemmon_syntax::parse;
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

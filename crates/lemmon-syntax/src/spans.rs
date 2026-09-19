//! A byte span named by the source text it covers, for the span assertions every
//! stage's tests make.
//!
//! It is here and not in `lemmon-corpus` with the rest of the shared test machinery
//! because `lemmon-corpus` depends on this crate: the [`Span`] it would hand back to this
//! crate's inline tests comes from a second copy of `lemmon-syntax`, a different type
//! from `crate::Span`, and the test does not compile. It is not a plain `#[cfg(test)]` module either, since that is compiled
//! only into this crate's own test build and `lemmon-vm`'s tests need it too.
//!
//! So the module is compiled under `cfg(test)` or the `testing` feature, which a
//! dependent turns on in its dev-dependency. It is test code, and no production build
//! has it.

use std::ops::Range;

use crate::{Span, index_u32};

/// The span of `needle` in `source`.
///
/// `needle` is literal source text that `source` must contain exactly once, so a test
/// asserts the span it means rather than the byte offsets it counted. Bracket part of
/// the needle with `«…»` to span only that part, leaving the rest as context that makes
/// the match unique — the only way to name one of several identical names, as
/// `at(src, "f(«a»: int")` names the first `a` of `f(a: int, a: int)`. An empty bracket
/// gives the zero-width span at that point.
///
/// A stray or doubled bracket needs no check of its own: it stays in the text looked
/// for, which `source` then does not contain.
///
/// # Panics
///
/// If a bracket is opened and never closed, or if `source` does not hold the needle
/// exactly once — a repeat would otherwise assert whichever occurrence came first.
pub fn at(source: &str, needle: &str) -> Span {
    let (text, marked) = unbracketed(needle);
    // Every start, not `match_indices`' non-overlapping ones: `--` occurs twice in
    // `---`, and a needle the source repeats has to be refused however the repeats sit.
    let mut starts = source
        .char_indices()
        .map(|(start, _)| start)
        .chain(std::iter::once(source.len()))
        .filter(|&start| source[start..].starts_with(&text));
    let found = match (starts.next(), starts.next()) {
        (Some(found), None) => found,
        (None, _) => panic!("`{text}` is not in the source"),
        // Two are already taken off the iterator; the rest are what is left.
        (Some(_), Some(_)) => panic!(
            "`{text}` is in the source {} times: bracket the span with `«…»` inside a \
             longer needle that is not",
            2 + starts.count()
        ),
    };
    let start = index_u32(found + marked.start);
    Span::new(start, start + index_u32(marked.len()))
}

/// The span of all of `source`: a diagnostic about a whole file, or what a host sends
/// for a value as a whole.
pub fn all_of(source: &str) -> Span {
    Span::new(0, index_u32(source.len()))
}

/// `needle` with its brackets removed, and the byte range they marked in what is left.
fn unbracketed(needle: &str) -> (String, Range<usize>) {
    let Some((before, rest)) = needle.split_once('«') else {
        return (needle.to_owned(), 0..needle.len());
    };
    let (marked, after) = rest
        .split_once('»')
        .unwrap_or_else(|| panic!("`{needle}` opens a span it never closes"));
    (
        format!("{before}{marked}{after}"),
        before.len()..before.len() + marked.len(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_a_needle_the_source_holds_once() {
        let src = "<lem-let raw: string = \"x\" />";
        assert_eq!(at(src, "raw"), Span::new(9, 12));
        assert_eq!(&src[9..12], "raw");
    }

    #[test]
    fn spans_only_what_the_brackets_hold() {
        let src = "<lem-fn f(a: int, a: int): int = a />";
        assert_eq!(at(src, "f(«a»: int"), Span::new(10, 11));
        assert_eq!(at(src, ", «a»: int"), Span::new(18, 19));
    }

    #[test]
    fn counts_the_bytes_a_multibyte_source_puts_before_the_needle() {
        // The offsets this replaces were counted through two entities and an é.
        let src = "&amp;&amp;é <lem.x> here\n";
        assert_eq!(at(src, "<lem.x>"), Span::new(13, 20));
    }

    #[test]
    fn gives_an_empty_bracket_the_zero_width_span_at_that_point() {
        assert_eq!(
            at("---\ntype: Nope\n---\nhi", "«»---\ntype"),
            Span::new(0, 0)
        );
    }

    #[test]
    fn spans_a_whole_source() {
        assert_eq!(all_of("{\"a\": 1}"), Span::new(0, 8));
        assert_eq!(all_of(""), Span::new(0, 0));
    }

    #[test]
    #[should_panic(expected = "`nope` is not in the source")]
    fn refuses_a_needle_the_source_does_not_hold() {
        at("<lem.x>", "nope");
    }

    #[test]
    #[should_panic(expected = "`t` is in the source 10 times")]
    fn refuses_a_needle_the_source_holds_more_than_once() {
        at(
            "<lem-type A>\n  t: string = self.nope\n</lem-type>\n\
             <lem-type B>\n  t: string = \"1\"\n  t: string = self.nope\n</lem-type>",
            "t",
        );
    }

    #[test]
    #[should_panic(expected = "`--` is in the source 2 times")]
    fn refuses_a_needle_whose_repeats_overlap() {
        // `match_indices` sees one `--` in `---` and would take the needle for unique.
        at("---", "--");
    }

    #[test]
    #[should_panic(expected = "`f(«a: int` opens a span it never closes")]
    fn refuses_a_bracket_that_is_never_closed() {
        at("<lem-fn f(a: int): int = a />", "f(«a: int");
    }

    #[test]
    #[should_panic(expected = "`lem«.x»` is not in the source")]
    fn leaves_a_doubled_bracket_in_the_text_it_looks_for() {
        // No check of its own: the second pair stays in the needle and finds nothing.
        at("<lem.x>", "«lem»«.x»");
    }
}

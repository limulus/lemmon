//! The did-you-mean policy: the candidate closest to a name, or none when nothing is
//! close enough to be worth suggesting.

/// The candidate closest to `name`, for a did-you-mean, or `None` when nothing given is
/// close enough to be worth suggesting.
///
/// Closeness is optimal string alignment distance — Levenshtein plus adjacent
/// transposition, so `fi` is one step from `if`, as a typist would expect — judged
/// *relative to length*: at most half the longer of the two names may differ. That
/// threshold is the whole point of the function. Without it every mistake gets an
/// answer, and a short or unrelated name (`-`, `xx`, `widget`) is met with whichever
/// candidate happens to sit nearest, which reads as the checker guessing rather than
/// helping. Comparison is case-insensitive: `<lem-ELSE>` is a recognisable `lem-else`,
/// and a `Title:` key a recognisable `title`.
///
/// Ties go to the candidate given first, so a caller with an order worth keeping — a
/// record's own fields before the ones it inherits — gets that order respected.
///
/// Shared rather than per-stage so that one suggestion policy covers the parser's
/// unknown tags and the checker's unknown fields alike; a stage that grew its own would
/// drift from this one silently.
#[must_use]
pub fn nearest<'a>(
    name: &str,
    candidates: impl IntoIterator<Item = &'a str>,
) -> Option<&'a str> {
    let name = name.to_lowercase();
    // Widths come from the lowercased forms, the same strings the distance is measured
    // over, so the threshold and the distance can never be counting different things.
    let width = name.chars().count();
    candidates
        .into_iter()
        .filter_map(|candidate| {
            let folded = candidate.to_lowercase();
            let longest = width.max(folded.chars().count());
            if longest == 0 {
                return None;
            }
            let distance = strsim::osa_distance(&name, &folded);
            (distance * 2 <= longest).then_some((
                distance * 1000 / longest,
                distance,
                candidate,
            ))
        })
        .min_by_key(|&(relative, distance, _)| (relative, distance))
        .map(|(_, _, candidate)| candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A handful of the parser's tag names — real candidates, short and similar, so the
    /// policy is exercised against the shapes it was tuned on rather than invented ones.
    /// Not the parser's whole list: that one is its own to test.
    const BUILTINS: [&str; 5] = ["if", "else", "else-if", "for", "let"];

    #[test]
    fn suggests_the_closest_candidate_to_a_typo() {
        assert_eq!(nearest("iff", BUILTINS), Some("if"));
        // Three of seven characters differ, which is as far as the threshold reaches.
        assert_eq!(nearest("elif", BUILTINS), Some("else-if"));
    }

    #[test]
    fn counts_a_transposition_as_one_step() {
        assert_eq!(nearest("fi", BUILTINS), Some("if"));
        assert_eq!(nearest("esle", BUILTINS), Some("else"));
    }

    #[test]
    fn suggests_nothing_when_no_candidate_is_close_enough() {
        assert_eq!(nearest("frobnicate", BUILTINS), None);
        assert_eq!(nearest("xx", BUILTINS), None);
        assert_eq!(nearest("-", BUILTINS), None);
    }

    #[test]
    fn ignores_case_on_both_sides() {
        assert_eq!(nearest("ELSE", BUILTINS), Some("else"));
        assert_eq!(nearest("title", ["Title", "topic"]), Some("Title"));
    }

    #[test]
    fn prefers_the_candidate_that_differs_by_least_of_its_length() {
        // One step from either, but that step is a third of `title` and a quarter of
        // `tit` — and the winner is given second, so order is not what decided it.
        assert_eq!(nearest("titl", ["tit", "title"]), Some("title"));
    }

    #[test]
    fn breaks_a_tie_towards_the_candidate_given_first() {
        assert_eq!(nearest("titl", ["title", "titly"]), Some("title"));
        assert_eq!(nearest("titl", ["titly", "title"]), Some("titly"));
    }

    /// Two empty names are also a zero-width comparison, which the relative threshold
    /// cannot divide by.
    #[test]
    fn suggests_nothing_when_there_is_nothing_to_suggest() {
        assert_eq!(nearest("title", []), None);
        assert_eq!(nearest("", BUILTINS), None);
        assert_eq!(nearest("", [""]), None);
    }

    #[test]
    fn measures_width_in_characters_rather_than_bytes() {
        // Two characters sharing nothing with two others, so nothing to suggest. Judged
        // by byte length the accented name would look four wide against two, and its
        // two differences would fit under the threshold as a false suggestion.
        assert_eq!(nearest("ää", ["xy"]), None);
    }
}

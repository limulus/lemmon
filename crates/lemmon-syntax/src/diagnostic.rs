use crate::Span;

/// How much a diagnostic matters: an error stops a file from compiling; a warning is
/// reported and the file compiles anyway.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Severity {
    Error,
    Warning,
}

/// A secondary span a diagnostic points at with a note of its own — where a name was
/// first defined, what a caller was compiled against. Spans are file-local, like the
/// diagnostic's; a stage whose spans cross files carries the path beside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    /// The bytes the note is about.
    pub span: Span,
    /// The note, phrased to read after the diagnostic's message.
    pub message: String,
}

/// Something wrong with the source, pointed at precisely.
///
/// A message and the span it is about, with a severity and any secondary spans worth
/// pointing at (∆VNk). Codes wait for a consumer. Every diagnostic carries a span — a
/// throw without one is a bug.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// Human-readable description of the problem.
    pub message: String,
    /// The bytes the problem is about.
    pub span: Span,
    pub severity: Severity,
    /// Secondary spans, in the order they should be read.
    pub labels: Vec<Label>,
}

impl Diagnostic {
    /// Creates an error about `span`.
    #[must_use]
    pub fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
            severity: Severity::Error,
            labels: Vec::new(),
        }
    }

    /// Creates a warning about `span`.
    #[must_use]
    pub fn warning(message: impl Into<String>, span: Span) -> Self {
        Self {
            severity: Severity::Warning,
            ..Self::new(message, span)
        }
    }

    /// Adds a secondary span with its note.
    #[must_use]
    pub fn with_label(mut self, span: Span, message: impl Into<String>) -> Self {
        self.labels.push(Label {
            span,
            message: message.into(),
        });
        self
    }

    /// The same diagnostic `by` bytes later in the file, labels included — for a parser
    /// that read a slice of the source and places what it found in the whole.
    #[must_use]
    pub fn shifted(mut self, by: u32) -> Self {
        self.span = Span::new(self.span.start + by, self.span.end + by);
        for label in &mut self.labels {
            label.span = Span::new(label.span.start + by, label.span.end + by);
        }
        self
    }

    /// Whether this diagnostic stops the file from compiling.
    #[must_use]
    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

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

    #[test]
    fn shifts_a_diagnostic_and_its_labels_together() {
        let shifted = Diagnostic::warning("w", Span::new(1, 2))
            .with_label(Span::new(0, 1), "here")
            .shifted(10);
        assert_eq!(shifted.span, Span::new(11, 12));
        assert_eq!(shifted.labels[0].span, Span::new(10, 11));
        assert_eq!(shifted.severity, Severity::Warning);
    }

    #[test]
    fn carries_its_message_and_span_as_an_error_with_no_labels() {
        let diagnostic = Diagnostic::new("unexpected character", Span::new(2, 3));
        assert_eq!(diagnostic.message, "unexpected character");
        assert_eq!(diagnostic.span, Span::new(2, 3));
        assert_eq!(diagnostic.severity, Severity::Error);
        assert!(diagnostic.is_error());
        assert_eq!(diagnostic.labels, Vec::new());
    }

    #[test]
    fn builds_a_warning_that_is_not_an_error() {
        let diagnostic = Diagnostic::warning("unused", Span::new(0, 1));
        assert_eq!(diagnostic.severity, Severity::Warning);
        assert!(!diagnostic.is_error());
    }

    #[test]
    fn keeps_labels_in_the_order_they_were_added() {
        let diagnostic = Diagnostic::new("redefined", Span::new(10, 11))
            .with_label(Span::new(0, 1), "first defined here")
            .with_label(Span::new(4, 5), "and again here");
        let notes: Vec<&str> = diagnostic
            .labels
            .iter()
            .map(|l| l.message.as_str())
            .collect();
        assert_eq!(notes, ["first defined here", "and again here"]);
        assert_eq!(diagnostic.labels[0].span, Span::new(0, 1));
    }

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

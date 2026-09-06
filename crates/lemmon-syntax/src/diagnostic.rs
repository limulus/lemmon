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

    /// Whether this diagnostic stops the file from compiling.
    #[must_use]
    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}

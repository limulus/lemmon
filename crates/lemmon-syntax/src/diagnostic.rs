use crate::Span;

/// Something wrong with the source, pointed at precisely.
///
/// Deliberately minimal: a message and the span it is about. Severity, codes, and
/// secondary spans arrive with the parser and checker, which are the first stages that
/// need them. Every diagnostic carries a span — a throw without one is a bug.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// Human-readable description of the problem.
    pub message: String,
    /// The bytes the problem is about.
    pub span: Span,
}

impl Diagnostic {
    /// Creates a diagnostic about `span`.
    #[must_use]
    pub fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carries_its_message_and_span() {
        let diagnostic = Diagnostic::new("unexpected character", Span::new(2, 3));
        assert_eq!(diagnostic.message, "unexpected character");
        assert_eq!(diagnostic.span, Span::new(2, 3));
    }
}

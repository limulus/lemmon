//! Types, checker, codegen, VM, and document mode for Lemmon.
//!
//! Scaffolding only: codegen and the VM arrive with `∆c8h`, the checker with `∆C8d`.

use std::fmt;

/// Re-exported so consumers of the VM — including the WASM boundary — get spans without
/// depending on the syntax crate directly.
pub use lemmon_syntax::Span;

/// An error raised while the VM is running, pointing at the source that caused it.
///
/// The span is not optional. A throw the shell cannot trace back to source is a bug, not
/// a degraded error (invariant 8).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeError {
    /// The source the failing instruction was compiled from.
    pub span: Span,
    /// What went wrong, phrased for the person who wrote the template.
    pub message: String,
}

impl RuntimeError {
    /// Creates a runtime error covering `span`.
    #[must_use]
    pub fn new(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
        }
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}..{}: {}",
            self.span.start, self.span.end, self.message
        )
    }
}

impl std::error::Error for RuntimeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_any_message_that_converts_into_a_string() {
        let error = RuntimeError::new(Span::new(0, 4), "undefined name");
        assert_eq!(error.message, "undefined name");
    }

    #[test]
    fn renders_its_span_alongside_the_message() {
        let error = RuntimeError::new(Span::new(12, 20), "not callable");
        assert_eq!(error.to_string(), "12..20: not callable");
    }

    #[test]
    fn keeps_the_span_it_was_built_with() {
        let error = RuntimeError::new(Span::new(7, 9), "boom");
        assert_eq!(error.span, Span::new(7, 9));
    }
}

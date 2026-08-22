//! Scanner, parser, CST/AST, and diagnostics for Lemmon.
//!
//! The scanner ([`scan`]) lexes a file into lossless tokens; the parser ([`parse`])
//! builds a rowan syntax tree over them and reports every problem with a [`Span`] — the
//! one type every later stage depends on.

mod diagnostic;
mod parser;
mod scanner;
mod syntax_kind;
mod token;

pub use diagnostic::Diagnostic;
pub use parser::{Parse, dump, parse};
pub use scanner::{ScanResult, scan};
pub use syntax_kind::{Lemmon, SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken};
pub use token::{Token, TokenKind};

/// A byte range into a source file.
///
/// Every Lemmon diagnostic — compile, load, and runtime alike — carries one, so this is
/// the single type the entire core depends on. Byte offsets, not character offsets: they
/// are what the scanner produces and what editors expect back over LSP.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    /// Byte offset of the first byte in the range.
    pub start: u32,
    /// Byte offset one past the last byte in the range.
    pub end: u32,
}

impl Span {
    /// Creates a span covering `start..end`.
    ///
    /// # Panics
    ///
    /// In debug builds, if `end` precedes `start`. Pinned here rather than in [`len`]
    /// because this is where the span is still traceable to whatever produced it: an
    /// inverted span underflows to roughly four billion in a release build instead.
    ///
    /// [`len`]: Self::len
    #[must_use]
    pub const fn new(start: u32, end: u32) -> Self {
        debug_assert!(start <= end, "span end precedes its start");
        Self { start, end }
    }

    /// The number of bytes the span covers.
    #[must_use]
    pub const fn len(&self) -> u32 {
        self.end - self.start
    }

    /// Whether the span covers no bytes, as a zero-width span at a point does.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_the_number_of_bytes_covered() {
        assert_eq!(Span::new(3, 10).len(), 7);
    }

    #[test]
    fn treats_a_zero_width_span_as_empty() {
        assert!(Span::new(4, 4).is_empty());
        assert_eq!(Span::new(4, 4).len(), 0);
    }

    #[test]
    fn treats_a_span_covering_bytes_as_non_empty() {
        assert!(!Span::new(4, 5).is_empty());
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "span end precedes its start")]
    fn rejects_a_span_whose_end_precedes_its_start() {
        let _ = Span::new(10, 3);
    }
}

//! What a compile has to say about a file, and what it hands back.
//!
//! A [`Diagnostic`] is one problem pointed at the bytes it is about (invariant 8), with
//! [`Label`]s for the secondary spans worth reading beside it — path-qualified, because a
//! duplicate definition's first occurrence may be in another file. A [`Compiled`] is what
//! a step produces: its output when nothing was an error, and every diagnostic it raised,
//! warnings included, which is why it is not a `Result`.
//!
//! These are the crate's vocabulary rather than the compiler's: the linker, document
//! mode, and the WASM boundary all speak them, and `lemmon-vm` re-exports all three.

use std::fmt;

use lemmon_syntax::{Range, Severity, Span};

/// Something a compile has to say about a file, pointed at precisely: an error the file
/// cannot compile past, or a warning it compiles with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// The file the problem is in.
    pub path: String,
    /// The bytes the problem is about.
    pub span: Span,
    /// The same two points as positions, when the producer had them: a loader
    /// diagnostic about a file the host holds — front matter, a data file — which no
    /// resident index can place, so the host's own positions travel with it (D37).
    /// `None` where an index over `path` places `span`.
    pub range: Option<Range>,
    pub severity: Severity,
    /// Human-readable description of the problem.
    pub message: String,
    /// Secondary spans worth reading with it, in reading order.
    pub labels: Vec<Label>,
}

/// A secondary span a [`Diagnostic`] points at, with a note of its own. Path-qualified,
/// because a duplicate definition's first occurrence may be in another file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    pub path: String,
    pub span: Span,
    /// As [`Diagnostic::range`]: the host's positions, for a span in a file it holds.
    pub range: Option<Range>,
    /// The note, phrased to read after the diagnostic's message.
    pub message: String,
}

impl Diagnostic {
    /// An error about `span` in the file at `path`.
    #[must_use]
    pub fn error(path: impl Into<String>, span: Span, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            span,
            range: None,
            severity: Severity::Error,
            message: message.into(),
            labels: Vec::new(),
        }
    }

    /// A warning about `span` in the file at `path`.
    #[must_use]
    pub fn warning(
        path: impl Into<String>,
        span: Span,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: Severity::Warning,
            ..Self::error(path, span, message)
        }
    }

    /// The same diagnostic carrying the host's positions for its span.
    #[must_use]
    pub const fn placed_by_host(mut self, range: Option<Range>) -> Self {
        self.range = range;
        self
    }

    /// Adds a secondary span with its note.
    #[must_use]
    pub fn with_label(
        self,
        path: impl Into<String>,
        span: Span,
        message: impl Into<String>,
    ) -> Self {
        self.with_placed_label(path, span, None, message)
    }

    /// Adds a secondary span with its note and, for a span in a file the host holds,
    /// the host's positions for it.
    #[must_use]
    pub fn with_placed_label(
        mut self,
        path: impl Into<String>,
        span: Span,
        range: Option<Range>,
        message: impl Into<String>,
    ) -> Self {
        self.labels.push(Label {
            path: path.into(),
            span,
            range,
            message: message.into(),
        });
        self
    }

    /// Whether this diagnostic stops the file from compiling.
    #[must_use]
    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }

    /// The parser's diagnostic about the file at `path`, whose labels are in the same file.
    pub(super) fn from_syntax(path: &str, diagnostic: &lemmon_syntax::Diagnostic) -> Self {
        Self {
            path: path.to_owned(),
            span: diagnostic.span,
            range: None,
            severity: diagnostic.severity,
            message: diagnostic.message.clone(),
            labels: diagnostic
                .labels
                .iter()
                .map(|label| Label {
                    path: path.to_owned(),
                    span: label.span,
                    range: None,
                    message: label.message.clone(),
                })
                .collect(),
        }
    }
}

/// `path:start..end: message`, `warning: ` before a warning's message, and each label on
/// its own indented line below.
///
/// Byte offsets, for a Rust-side reader who has the source open. The corpus trailer is
/// built from this and then has its spans rendered as `line:col` by the harness; how
/// `lem check` prints one is the shell's to decide (∆4tG).
impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let severity = match self.severity {
            Severity::Error => "",
            Severity::Warning => "warning: ",
        };
        write!(f, "{}:{}: {severity}{}", self.path, self.span, self.message)?;
        for label in &self.labels {
            write!(f, "\n  {}:{}: {}", label.path, label.span, label.message)?;
        }
        Ok(())
    }
}

impl std::error::Error for Diagnostic {}

/// What a compile step produces: its output when nothing was an error, and every
/// diagnostic it raised — warnings included, which is why this is not a `Result`.
#[derive(Clone, Debug, PartialEq)]
pub struct Compiled<T> {
    /// The output; `None` exactly when `diagnostics` holds an error.
    pub output: Option<T>,
    /// Every diagnostic, in source order.
    pub diagnostics: Vec<Diagnostic>,
}

impl<T> Compiled<T> {
    /// The one constructor: pairs `output` with `diagnostics`, dropping the output when
    /// any diagnostic is an error. A step that stopped before producing an output passes
    /// `None`, which it may only do when it has an error to show for it.
    pub(crate) fn new(output: Option<T>, diagnostics: Vec<Diagnostic>) -> Self {
        let failed = diagnostics.iter().any(Diagnostic::is_error);
        debug_assert!(
            output.is_some() || failed,
            "a step without output must have an error to explain it"
        );
        Self {
            output: output.filter(|_| !failed),
            diagnostics,
        }
    }

    /// The output, or a panic listing every diagnostic — for tests and harnesses.
    ///
    /// # Panics
    ///
    /// When the step had an error.
    #[must_use]
    pub fn expect_output(self) -> T {
        match self.output {
            Some(output) => output,
            None => {
                let shown: Vec<String> =
                    self.diagnostics.iter().map(ToString::to_string).collect();
                panic!("compile failed:\n{}", shown.join("\n"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_output_beside_warnings_and_drops_it_beside_an_error() {
        let warning = Diagnostic::warning("t.lem", Span::new(0, 1), "w");
        let error = Diagnostic::error("t.lem", Span::new(0, 1), "e");
        assert_eq!(
            Compiled::new(Some(1), vec![warning.clone()]).output,
            Some(1)
        );
        assert_eq!(
            Compiled::new(Some(1), vec![warning, error.clone()]).output,
            None
        );
        assert_eq!(Compiled::new(None::<i32>, vec![error]).output, None);
    }

    #[test]
    fn shows_a_warning_and_its_labels_in_the_trailer_shape() {
        let diagnostic = Diagnostic::warning("a.lem", Span::new(1, 2), "unused")
            .with_label("b.lem", Span::new(3, 4), "bound here");
        assert_eq!(
            diagnostic.to_string(),
            "a.lem:1..2: warning: unused\n  b.lem:3..4: bound here"
        );
        assert!(!diagnostic.is_error());
    }
}

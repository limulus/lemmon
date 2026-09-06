//! Codegen and the VM for Lemmon: the stages below the typed AST.
//!
//! [`compile`] lowers every file of a site to one [`Module`] of bytecode; [`render`]
//! runs a file's entry function and returns its HTML. The type checker (`∆C8d`) slots
//! into [`compile`] when it lands; until then this is an untyped VM, and what
//! `docs/language.md` calls type errors surface as spanned runtime errors.

mod builtins;
mod bytecode;
mod compile;
mod document;
mod exports;
mod unit;
mod value;
mod vm;

use std::fmt;

/// Re-exported so consumers of the VM — including the WASM boundary — get spans and
/// severities without depending on the syntax crate directly.
pub use lemmon_syntax::{Severity, Span};

pub use bytecode::{Function, Module, Op};
pub use compile::{
    Compiled, Diagnostic, Label, PreparedFile, compile, compile_unit, prepare,
    repeated_path,
};
pub use exports::{Export, ExportIndex};
pub use unit::{ExportedSignature, ImportRef, ImportedSignature, Unit, link};
pub use value::{Closure, Map, Value};
pub use vm::render;

/// One source file handed to [`compile`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceFile {
    /// The path the file is known by — what errors and [`render`] name it with.
    pub path: String,
    /// Its contents.
    pub text: String,
}

impl SourceFile {
    /// Creates a source file.
    pub fn new(path: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            text: text.into(),
        }
    }
}

/// An error raised while the VM is running, pointing at the source that caused it.
///
/// The span is not optional. A throw the shell cannot trace back to source is a bug, not
/// a degraded error (invariant 8).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeError {
    /// The file the failing instruction was compiled from.
    pub path: String,
    /// The source the failing instruction was compiled from.
    pub span: Span,
    /// What went wrong, phrased for the person who wrote the template.
    pub message: String,
}

impl RuntimeError {
    /// Creates a runtime error covering `span` in the file at `path`.
    #[must_use]
    pub fn new(path: impl Into<String>, span: Span, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            span,
            message: message.into(),
        }
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}..{}: {}",
            self.path, self.span.start, self.span.end, self.message
        )
    }
}

impl std::error::Error for RuntimeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_any_message_that_converts_into_a_string() {
        let error = RuntimeError::new("a.lem", Span::new(0, 4), "undefined name");
        assert_eq!(error.message, "undefined name");
    }

    #[test]
    fn renders_its_path_and_span_alongside_the_message() {
        let error = RuntimeError::new("a.lem", Span::new(12, 20), "not callable");
        assert_eq!(error.to_string(), "a.lem:12..20: not callable");
    }

    #[test]
    fn keeps_the_span_it_was_built_with() {
        let error = RuntimeError::new("a.lem", Span::new(7, 9), "boom");
        assert_eq!(error.span, Span::new(7, 9));
    }

    #[test]
    fn builds_a_source_file_from_anything_stringy() {
        let file = SourceFile::new("a.lem", String::from("x"));
        assert_eq!(file.path, "a.lem");
        assert_eq!(file.text, "x");
    }
}

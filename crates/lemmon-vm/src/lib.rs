//! Codegen and the VM for Lemmon: the stages below the typed AST.
//!
//! [`compile`] lowers every file of a site to one [`Module`] of bytecode, checking types
//! as it lowers (D31); [`load`] validates the data a host hands it against the module's
//! declarations and runs the module's plans, building what every render binds (D37);
//! [`render`] runs a file's entry function and returns its HTML. The VM is the runtime
//! for `any`: what the checker could not decide statically — every operation on a value
//! typed `any` — it checks at run time, with a span.

mod builtins;
mod bytecode;
mod closure;
mod compile;
mod datetime;
mod decls;
mod document;
mod exports;
mod host;
mod load;
mod plan;
#[cfg(test)]
mod testing;
mod types;
mod unit;
mod url;
mod value;
mod vm;

use std::fmt;

/// Re-exported so consumers of the VM — including the WASM boundary — get spans and
/// severities without depending on the syntax crate directly.
pub use lemmon_syntax::{LineIndex, Position, Range, Severity, Span};

pub use bytecode::{Declared, Document, Exported, Function, Module, Op};
pub use compile::{
    Compiled, Diagnostic, Label, PreparedFile, compile, compile_unit, prepare,
    repeated_path,
};
pub use datetime::Datetime;
pub use decls::{Declaration, Decls, FieldDecl, Kind, Shape, ShapeFact, TypeFact};
pub use exports::{Export, ExportIndex, TypeExport};
pub use host::{Ambient, Host};
pub use load::{Data, HostSpan, Loaded, Source, Spans, Store, collects, field_path, load};
pub use plan::{BindingStep, Bindings, Fulfilled, Index, RecordPlan, SourceKind, Step};
pub use types::Type;
pub use unit::{
    Access, BindingFn, Call, DefaultFn, ExportedSignature, ImportRef, ImportedSignature,
    IndexDecl, Param, Root, Supplied, Unit, Uses, link,
};
pub use value::{Closure, Map, Value};
pub use vm::{invoke, render};

/// Whether the file at `path` holds a document rather than a template: `.md` is a
/// document, anything else a template. The rule a site runs on, read from the path
/// alone — by the compiler deciding how to parse a file, and by a host deciding what an
/// ambient value is where nothing typed it.
#[must_use]
pub fn is_document(path: &str) -> bool {
    path.ends_with(".md")
}

/// One source file handed to [`compile`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceFile {
    /// The path the file is known by — what errors and [`render`] name it with.
    pub path: String,
    /// Its contents.
    pub text: String,
    /// The record this file's content is: the element type of the collection whose glob
    /// matched it, or the type its own `type:` key named — the host's to decide (D30).
    /// A document without one is unknown-typed and renders.
    pub record: Option<String>,
    /// Where in the file the record was named, when a `type:` key named it — what a
    /// diagnostic about the record points at. Without one it points at the front
    /// matter as a whole.
    pub record_span: Option<Span>,
}

impl SourceFile {
    /// Creates a source file the host said nothing about.
    pub fn new(path: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            text: text.into(),
            record: None,
            record_span: None,
        }
    }

    /// The same file, compiled as the record `name`.
    #[must_use]
    pub fn with_record(mut self, name: impl Into<String>) -> Self {
        self.record = Some(name.into());
        self
    }

    /// The same file, with the record named at `span` — the `type:` key's value.
    #[must_use]
    pub const fn named_at(mut self, span: Span) -> Self {
        self.record_span = Some(span);
        self
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
        write!(f, "{}:{}: {}", self.path, self.span, self.message)
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

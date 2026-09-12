//! Every surface a file offers another, read from its heads alone (invariant 11, D36).
//!
//! [`prepare`] parses a file, renders a document's prose so markdown never runs later,
//! and collects what the rest of the project compiles against: the signatures of its
//! top-level `lem-fn`s, the records and enums it declares, and — where the host binds an
//! ambient value to it — its top-level bindings as that value's fields. The result is a
//! [`PreparedFile`], which is an [`ExportIndex`](crate::ExportIndex)'s input and
//! [`compile_unit`](super::compile_unit)'s.
//!
//! Nothing here reads a body, and nothing here has a [`Compiler`](super::Compiler): a
//! signature comes from the head that declares it, so a file's own compiled unit never
//! depends on another file's body. What the collection cannot know alone — whether a name
//! another file also defines is this file's, whether a type any of it names is declared —
//! waits for the index and is asked in [`schema`](super::schema).

use std::rc::Rc;

use lemmon_syntax::ast::{EnumDecl, Item, Spanned, TypeDecl};
use lemmon_syntax::{DocumentParse, Parse, Span, parse_document};

use crate::SourceFile;
use crate::decls::{Declaration, FieldDecl, Shape};
use crate::document;
use crate::host::Host;
use crate::types::Type;
use crate::unit::{ExportedSignature, ImportRef, Param};

use super::annotations::Reserved;
use super::names::Names;
use super::{Diagnostic, TokenSpan, annotations, duplicate_named, index_u32, required};

/// A file parsed, its prose rendered, and its exports and declarations collected —
/// everything [`compile_unit`](super::compile_unit) needs that comes from this file alone.
pub struct PreparedFile {
    pub(super) path: String,
    /// The record this file's content is, as the host named it (D30).
    pub(super) record: Option<String>,
    /// Where the host says the record was named, when it can say.
    record_span: Option<Span>,
    pub(super) source: ParsedFile,
    /// A document's prose, rendered by comrak here so markdown never runs later.
    pub(super) chunks: Option<Vec<String>>,
    pub(super) exports: Vec<ExportedSignature>,
    pub(super) types: Vec<Declaration>,
    /// Every declared type name the signatures and declarations use, where it is
    /// written. Whether each is declared is the index's question, asked by
    /// [`compile_unit`](super::compile_unit) once every file's declarations are in.
    pub(super) type_uses: Vec<ImportRef>,
    /// What parsing warned about and what collecting the exports and declarations
    /// found (ambient names, same-file duplicates, reserved type names), reported by
    /// [`compile_unit`](super::compile_unit) with the rest of the file's.
    pub(super) diagnostics: Vec<Diagnostic>,
}

impl PreparedFile {
    /// The path the file is known by.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The top-level `lem-fn`s the file defines — an [`ExportIndex`](crate::ExportIndex)'s input.
    #[must_use]
    pub fn exports(&self) -> &[ExportedSignature] {
        &self.exports
    }

    /// The top-level `lem-type`s and `lem-enum`s the file declares — the
    /// [`ExportIndex`](crate::ExportIndex)'s other input.
    #[must_use]
    pub fn types(&self) -> &[Declaration] {
        &self.types
    }

    /// Where a diagnostic about the record this file was compiled as points: the
    /// `type:` key's value when the host said where that is, else the front matter,
    /// which is where such a key stands, and the start of the file for a template,
    /// which has none.
    pub(super) fn record_span(&self) -> Span {
        if let Some(span) = self.record_span {
            return span;
        }
        match &self.source {
            ParsedFile::Document(doc) => {
                doc.front_matter.unwrap_or_else(|| Span::new(0, 0))
            }
            ParsedFile::Template(_) => Span::new(0, 0),
        }
    }
}

/// Parses `file`, renders a document's prose, and collects its exported signatures and
/// declarations.
///
/// # Errors
///
/// The file's parse or prose diagnostics — a file that does not parse clean yields
/// nothing to compile. Parser warnings are kept in the prepared file and reported with
/// the unit's.
pub fn prepare(file: &SourceFile, host: &Host) -> Result<PreparedFile, Vec<Diagnostic>> {
    let source = ParsedFile::of(file);
    let mut diagnostics: Vec<Diagnostic> = source
        .parse()
        .diagnostics
        .iter()
        .map(|d| Diagnostic::from_syntax(&file.path, d))
        .collect();
    if diagnostics.iter().any(Diagnostic::is_error) {
        return Err(diagnostics);
    }
    // Prose is rendered before anything is lowered, and a document markdown rejected
    // fails the file the way a parse error does: its blocks may be fine, but where
    // their output would go is not known.
    let chunks = match &source {
        ParsedFile::Template(_) => None,
        ParsedFile::Document(doc) => {
            match document::render_prose(&file.path, &file.text, doc) {
                Ok(chunks) => Some(chunks),
                Err(errors) => {
                    diagnostics.extend(errors);
                    return Err(diagnostics);
                }
            }
        }
    };
    let mut type_uses = Vec::new();
    let (exports, export_diagnostics) =
        collect_exports(&file.path, source.parse(), &mut type_uses, host);
    diagnostics.extend(export_diagnostics);
    let (mut types, type_diagnostics) =
        collect_declarations(&file.path, source.parse(), &mut type_uses);
    diagnostics.extend(type_diagnostics);
    // The bindings of the file an ambient value reads are that value's fields, so they
    // are declared like any other cross-file surface and collected here, from the heads
    // alone, with everything else the index is offered (D36).
    if host.binds_from(&file.path) {
        let (bindings, binding_diagnostics) =
            collect_bindings(&file.path, source.parse(), &mut type_uses);
        types.push(bindings);
        diagnostics.extend(binding_diagnostics);
    }
    Ok(PreparedFile {
        path: file.path.clone(),
        record: file.record.clone(),
        record_span: file.record_span,
        source,
        chunks,
        exports,
        types,
        type_uses,
        diagnostics,
    })
}

/// Collects the signatures of a parsed file's top-level `lem-fn`s, assigning each its
/// unit-local function index — the entry is 0, so they count from 1. A name the rules
/// refuse ([`Names::bind_function`]: ambient, or repeated in this file) exports nothing.
///
/// A signature is read from the head alone, never the body (invariant 11): a block body
/// returns `html`, and an expression body must declare what it returns.
fn collect_exports(
    path: &str,
    parse: &Parse,
    uses: &mut Vec<ImportRef>,
    host: &Host,
) -> (Vec<ExportedSignature>, Vec<Diagnostic>) {
    let mut names = Names::new(path, None, host);
    let mut diagnostics = Vec::new();
    let mut exports: Vec<ExportedSignature> = Vec::new();
    for item in parse.root().items() {
        let (name, params, return_type, block) = match &item {
            Item::FnBlock(f) => (required(f.name()), f.params(), f.return_type(), true),
            Item::FnVoid(f) => (required(f.name()), f.params(), f.return_type(), false),
            _ => continue,
        };
        if !names.bind_function(name.text(), name.span()) {
            continue;
        }
        let params: Vec<Param> = params
            .map(|params| {
                params
                    .params()
                    .map(|p| Param {
                        name: Rc::from(required(p.name()).text()),
                        ty: annotations::resolve(
                            path,
                            &required(required(p.type_annotation()).ty()),
                            &mut diagnostics,
                            uses,
                        ),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let ret = match (return_type, block) {
            (None, true) => Type::Html,
            (Some(annotation), true) => {
                let declared = annotations::resolve(
                    path,
                    &required(annotation.ty()),
                    &mut diagnostics,
                    uses,
                );
                if declared != Type::Html {
                    diagnostics.push(Diagnostic::error(
                        path,
                        annotation.span(),
                        "a block-bodied function returns `html`: declare `: html` or nothing",
                    ));
                }
                Type::Html
            }
            (Some(annotation), false) => annotations::resolve(
                path,
                &required(annotation.ty()),
                &mut diagnostics,
                uses,
            ),
            (None, false) => {
                diagnostics.push(Diagnostic::error(
                    path,
                    name.span(),
                    format!(
                        "an expression-bodied function declares its return type: \
                         `<lem-fn {}(…): T = … />`",
                        name.text()
                    ),
                ));
                Type::Any
            }
        };
        exports.push(ExportedSignature {
            name: Rc::from(name.text()),
            params,
            ret,
            func: index_u32(exports.len() + 1),
            name_span: name.span(),
        });
    }
    diagnostics.extend(names.into_diagnostics());
    (exports, diagnostics)
}

/// Collects a parsed file's top-level `lem-type`s and `lem-enum`s. The rules that need
/// only this file are applied here — a reserved name (a builtin type's, or a spelling
/// the hints answer for) cannot be declared, a name declared twice in one file is a
/// duplicate, and so is a field or a variant repeated within one declaration — and a
/// refused declaration exports nothing. Whether a base or a field's type is declared,
/// and what it declares, waits for the index ([`Compiler::check_declarations`](super::Compiler::check_declarations)).
fn collect_declarations(
    path: &str,
    parse: &Parse,
    uses: &mut Vec<ImportRef>,
) -> (Vec<Declaration>, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    let mut declared: Vec<Declaration> = Vec::new();
    for item in parse.root().items() {
        let (name, shape) = match &item {
            Item::TypeDecl(decl) => (
                required(decl.name()),
                record_shape(path, decl, &mut diagnostics, uses),
            ),
            Item::EnumDecl(decl) => (
                required(decl.name()),
                enum_shape(path, decl, &mut diagnostics),
            ),
            _ => continue,
        };
        let refusal = match annotations::reserved(name.text()) {
            Some(Reserved::Builtin) => Some(format!(
                "`{}` is a builtin type and cannot be declared",
                name.text()
            )),
            Some(Reserved::Hint(hint)) => {
                Some(format!("`{}` cannot be declared: {hint}", name.text()))
            }
            None => None,
        };
        if let Some(message) = refusal {
            diagnostics.push(Diagnostic::error(path, name.span(), message));
            continue;
        }
        if let Some(first) = declared.iter().find(|d| &*d.name == name.text()) {
            diagnostics.push(duplicate_named(
                path,
                name.span(),
                name.text(),
                path,
                first.name_span,
            ));
            continue;
        }
        declared.push(Declaration {
            name: Rc::from(name.text()),
            name_span: name.span(),
            shape,
        });
    }
    (declared, diagnostics)
}

/// Collects the top-level bindings of the file an ambient value reads into the record
/// that value has: one field per `lem-let`, in binding order (D30, D36).
///
/// Read from the heads alone, like every other cross-file surface: a void binding
/// declares its type, a `from` binding already had to (∆1vS), and a capture body is
/// `html` by its shape. The record is named by the file's own path, which no `lem-type`
/// can collide with, so it needs no rule to keep it out of the type namespace.
fn collect_bindings(
    path: &str,
    parse: &Parse,
    uses: &mut Vec<ImportRef>,
) -> (Declaration, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    let mut fields: Vec<FieldDecl> = Vec::new();
    for item in parse.root().items() {
        let (name, ty) = match &item {
            Item::LetVoid(tag) => {
                let name = required(tag.name());
                // Without its annotation the binding is still a field, of the unknown
                // type, so a read of it costs no second diagnostic.
                let ty = match tag.type_annotation() {
                    Some(annotation) => annotations::resolve(
                        path,
                        &required(annotation.ty()),
                        &mut diagnostics,
                        uses,
                    ),
                    None => {
                        diagnostics.push(Diagnostic::error(
                            path,
                            name.span(),
                            format!(
                                "a binding the host reads declares its type: \
                                 `<lem-let {}: T = … />`",
                                name.text()
                            ),
                        ));
                        Type::Any
                    }
                };
                (name, ty)
            }
            Item::LetFrom(tag) => {
                let annotation = required(tag.type_annotation());
                let ty = annotations::resolve(
                    path,
                    &required(annotation.ty()),
                    &mut diagnostics,
                    uses,
                );
                (required(tag.name()), ty)
            }
            Item::LetBlock(tag) => (required(tag.name()), Type::Html),
            _ => continue,
        };
        // A name bound twice is reported where the second binding is lowered; the
        // record keeps the first, as the scope does.
        if fields.iter().any(|field| &*field.name == name.text()) {
            continue;
        }
        fields.push(FieldDecl {
            name: Rc::from(name.text()),
            ty,
            span: name.span(),
        });
    }
    (
        Declaration {
            name: Rc::from(path),
            // Nothing can spell this name, so no diagnostic ever points at where it
            // was declared — the file itself is the honest answer.
            name_span: Span::new(0, 0),
            shape: Shape::Record { base: None, fields },
        },
        diagnostics,
    )
}

/// A `lem-type`'s shape. A base that is a reserved name is refused here and dropped —
/// a declared one is a use, judged with the rest; a field repeated within the
/// declaration is a duplicate, its type still resolved so what it names is reported.
fn record_shape(
    path: &str,
    decl: &TypeDecl,
    diagnostics: &mut Vec<Diagnostic>,
    uses: &mut Vec<ImportRef>,
) -> Shape {
    let base = decl.base().and_then(|base| {
        let refusal = match annotations::reserved(base.text()) {
            Some(Reserved::Builtin) => {
                Some(format!("`{}` is a builtin type, not a record", base.text()))
            }
            Some(Reserved::Hint(hint)) => {
                Some(format!("`{}` is not a record: {hint}", base.text()))
            }
            None => None,
        };
        if let Some(message) = refusal {
            diagnostics.push(Diagnostic::error(path, base.span(), message));
            return None;
        }
        uses.push(ImportRef {
            name: Rc::from(base.text()),
            span: base.span(),
        });
        Some((Rc::from(base.text()), base.span()))
    });
    let mut fields: Vec<FieldDecl> = Vec::new();
    for field in decl.fields() {
        let name = required(field.name());
        let ty = annotations::resolve(
            path,
            &required(required(field.type_annotation()).ty()),
            diagnostics,
            uses,
        );
        if let Some(first) = fields.iter().find(|f| &*f.name == name.text()) {
            diagnostics.push(duplicate_named(
                path,
                name.span(),
                name.text(),
                path,
                first.span,
            ));
            continue;
        }
        fields.push(FieldDecl {
            name: Rc::from(name.text()),
            ty,
            span: name.span(),
        });
    }
    Shape::Record { base, fields }
}

/// A `lem-enum`'s shape; a variant repeated within the declaration is a duplicate.
fn enum_shape(path: &str, decl: &EnumDecl, diagnostics: &mut Vec<Diagnostic>) -> Shape {
    let mut variants: Vec<(Rc<str>, Span)> = Vec::new();
    for variant in decl.variants() {
        if let Some((_, first)) = variants.iter().find(|(v, _)| &**v == variant.text()) {
            diagnostics.push(duplicate_named(
                path,
                variant.span(),
                variant.text(),
                path,
                *first,
            ));
            continue;
        }
        variants.push((Rc::from(variant.text()), variant.span()));
    }
    Shape::Enum { variants }
}

/// A file as parsed: a template, or a document in document mode. Decided by the path,
/// as it is in a site — `.md` is a document, anything else a template.
pub(super) enum ParsedFile {
    Template(Parse),
    Document(DocumentParse),
}

impl ParsedFile {
    pub(super) fn of(file: &SourceFile) -> Self {
        if crate::is_document(&file.path) {
            Self::Document(parse_document(&file.text))
        } else {
            Self::Template(lemmon_syntax::parse(&file.text))
        }
    }

    /// The tree either way: a document's has its prose as text runs and its blocks as
    /// items, so every pass but the entry's reads it exactly like a template's.
    pub(super) fn parse(&self) -> &Parse {
        match self {
            Self::Template(parse) => parse,
            Self::Document(doc) => &doc.parse,
        }
    }
}

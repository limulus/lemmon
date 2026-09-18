//! Lowering from the typed AST to bytecode, checking types on the way (D31).
//!
//! The compiler reads only the `lemmon_syntax::ast` views — never `SyntaxKind` — and
//! runs only on files that parsed clean, so every `Option` an accessor returns is `Some`
//! here; [`required`] says so in one place. There is no separate checking pass: the one
//! walk that emits an expression's code is the walk that knows its type, so the checker's
//! rules are applied where the lowering they guard happens (`docs/language.md`
//! §Checking). What is typed `any` is checked by the VM at run time instead.
//!
//! What is left in this module is the two entry points and the machine they drive:
//! [`compile`] over a whole site, [`compile_unit`] over one prepared file, and the
//! [`Compiler`] itself — its frames, its emission, and its delegation to the resolver.
//! Everything else has a module:
//!
//! - [`surfaces`] reads what a file offers other files, from its heads alone, before any
//!   of this runs; [`schema`] asks the questions about those surfaces that need the
//!   index, and records the answers as the unit's facts.
//! - [`items`] lowers what stands in a body and [`whitespace`] decides what that body's
//!   literal text is; [`expr`] lowers heads, [`calls`] the argument lists among them, and
//!   [`narrowing`] what a condition proves for the arm it guards.
//! - [`rules`] holds the typing rules the walk asks — pure functions from types to a type
//!   or a message — [`annotations`] reads types as written, and [`names`] owns every rule
//!   about names, in one stated order.
//! - [`diagnostics`] holds what any of them has to say and what a step hands back.

mod annotations;
mod calls;
mod diagnostics;
mod expr;
mod items;
mod names;
mod narrowing;
mod rules;
mod schema;
mod surfaces;
mod whitespace;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

use lemmon_syntax::ast::{Expr, Item, Spanned};
use lemmon_syntax::{Parse, Span, SyntaxToken};

use crate::bytecode::{Function, Module, Op};
use crate::decls::{DeclKind, Decls, Shape};
use crate::exports::{Export, ExportIndex};
use crate::host::Host;
use crate::types::Type;
use crate::unit::{
    Access, Call, DefaultFn, ImportRef, ImportedSignature, IndexDecl, Root, Unit, Uses,
    link,
};
use crate::{SourceFile, Value};

pub use diagnostics::{Compiled, Diagnostic, Label};
pub(crate) use expr::{Runs, refused_at_load};
pub(crate) use names::duplicate_named;
use names::{BindingKind, Frame, Names, Resolved};
pub(crate) use rules::omittable;
use surfaces::ParsedFile;
pub use surfaces::{PreparedFile, prepare};
use whitespace::BodyKind;

/// Compiles every file of a site into one module: each file [`prepare`]d and compiled
/// into its own [`Unit`] against the index of every file's signatures, then [`link`]ed.
///
/// A file that does not parse clean yields no unit — nothing is lowered from a tree the
/// parser had to recover — but the other files still compile, and every problem found
/// in every file is reported, not just the first. A module is still produced when the
/// only diagnostics are warnings.
///
/// # Panics
///
/// If two of `files` share a path — a caller bug with no source span to point at
/// ([`repeated_path`]).
pub fn compile(files: &[SourceFile], host: &Host) -> Compiled<Module> {
    if let Some(path) = repeated_path(files.iter().map(|file| file.path.as_str())) {
        panic!("`compile` was given `{path}` twice");
    }
    let mut diagnostics = Vec::new();
    let mut prepared = Vec::new();
    for file in files {
        match prepare(file, host) {
            Ok(p) => prepared.push(p),
            Err(errs) => diagnostics.extend(errs),
        }
    }
    // Offered in file order, so the first definition of a name is the one the project
    // keeps and every later one is reported where its own file compiles.
    let mut index = ExportIndex::new();
    for p in &prepared {
        index.offer(&p.path, &p.exports);
        index.offer_types(&p.path, &p.types);
    }
    let mut units = Vec::new();
    for p in &prepared {
        let compiled = compile_unit(p, &index, host);
        diagnostics.extend(compiled.diagnostics);
        units.extend(compiled.output);
    }
    let output = if diagnostics.iter().any(Diagnostic::is_error) {
        None
    } else {
        // Every unit was compiled against this same index, so nothing can be
        // unresolved at link time.
        let linked = link(&units, &index, host);
        diagnostics.extend(linked.diagnostics);
        linked.output
    };
    // In source order, like the parser's diagnostics — the order the passes above happen
    // to find things in is an implementation detail nobody reading a file wants to see.
    let order = |diagnostic: &Diagnostic| {
        let file = files.iter().position(|f| f.path == diagnostic.path);
        (file, diagnostic.span.start, diagnostic.span.end)
    };
    diagnostics.sort_by_key(order);
    Compiled::new(output, diagnostics)
}

/// The first path `paths` names twice, if any.
///
/// One call may not name a file twice: what that file exports is then two answers, and
/// the [`ExportIndex`] holds one — offering the second withdraws the first, leaving the
/// first entry's own names gone from under it. Both entry points reject such a call
/// rather than compile something confused out of it, and neither has a span to report
/// it against (invariant 8), so each treats it as the caller bug it is.
pub fn repeated_path<'a>(paths: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    let mut seen: HashSet<&str> = HashSet::new();
    paths.into_iter().find(|path| !seen.insert(path))
}

/// Compiles one prepared file into its [`Unit`], resolving names against `index`. A
/// call to a function another file defines stays a symbolic import, and the signature
/// it was compiled against is recorded as one of the unit's facts; so is the
/// declaration of every type another file declares that the checking depended on.
///
/// A definition or declaration the index refused — another file already owns the name
/// — is reported here, so a collision is reported once, by the compile that introduced
/// it, and the refused file yields no unit. So is everything about a declaration that
/// needs every file's declarations in: an unknown type, a base that is not a record, a
/// cycle, a field redeclared with another type.
///
/// Every problem in the file comes back in source order; the unit only when none of them
/// is an error.
pub fn compile_unit(
    prepared: &PreparedFile,
    index: &ExportIndex,
    host: &Host,
) -> Compiled<Unit> {
    let seen = host.for_file(&prepared.path, prepared.record.as_deref());
    let mut compiler = Compiler {
        path: &prepared.path,
        unit: Unit {
            path: prepared.path.clone(),
            constants: Vec::new(),
            functions: Vec::new(),
            imports: Vec::new(),
            exports: prepared.exports.clone(),
            types: prepared.types.clone(),
            facts: Vec::new(),
            type_refs: Vec::new(),
            ambient_refs: Vec::new(),
            asset_refs: Vec::new(),
            claim_refs: Vec::new(),
            grammar_ref: prepared.grammar_ref.clone(),
            record: prepared.record.clone(),
            defaults: Vec::new(),
            indexes: Vec::new(),
            bindings: Vec::new(),
            uses: BTreeMap::new(),
            record_span: prepared.record_span(),
            body: prepared
                .chunks
                .as_ref()
                .map(|chunks| Rc::from(chunks.concat())),
        },
        strings: HashMap::new(),
        index,
        host,
        exports_bindings: host.binds_from(&prepared.path),
        names: Names::new(&prepared.path, Some(index), &seen),
        diagnostics: prepared.diagnostics.clone(),
        ctxs: Vec::new(),
        narrowings: Vec::new(),
        uses: None,
        top_bindings: Vec::new(),
    };
    let entry = compiler.reserve(&prepared.path);
    debug_assert_eq!(entry, 0, "the entry is a unit's first function");
    compiler.check_type_uses(&prepared.type_uses);
    compiler.check_record(prepared);
    compiler.check_declarations(&prepared.types);
    for export in &prepared.exports {
        let func = compiler.reserve(&export.name);
        debug_assert_eq!(
            func, export.func,
            "exports are reserved in collection order"
        );
        // Bound again now that the index says which file's definition of each name
        // the project kept — the stdlib-shadowing warning is this file's only then,
        // and a name the project gave to another file is this file's duplicate.
        compiler.names.bind_function(&export.name, export.name_span);
        let owner = index
            .lookup(&export.name)
            .expect("an export is in the index");
        if owner.path != prepared.path {
            compiler.diagnostics.push(duplicate_named(
                &prepared.path,
                export.name_span,
                &export.name,
                &owner.path,
                owner.signature.name_span,
            ));
        }
    }
    compiler.compile_functions(prepared.source.parse());
    compiler.compile_defaults(prepared.source.parse());
    compiler.compile_entry(
        &prepared.source,
        prepared.chunks.as_deref(),
        host.bindings_value(&prepared.path).map(Rc::from),
    );
    compiler.resolve_import_operands();
    let Compiler {
        unit,
        mut diagnostics,
        names,
        ..
    } = compiler;
    diagnostics.extend(names.into_diagnostics());
    diagnostics.sort_by_key(|d| (d.span.start, d.span.end));
    Compiled::new(Some(unit), diagnostics)
}

/// Unwraps an accessor that only returns `None` on a tree the parser recovered — which
/// [`compile`] refuses before lowering starts.
fn required<T>(value: Option<T>) -> T {
    value.expect("a parse without diagnostics has every required node")
}

/// The code of the function being compiled, so far. Its names are the matching
/// [`Frame`] in [`Compiler::names`]; the two stacks move together.
struct FnCtx {
    code: Vec<Op>,
    spans: Vec<Span>,
}

impl FnCtx {
    fn new() -> Self {
        Self {
            code: Vec::new(),
            spans: Vec::new(),
        }
    }
}

/// The tag marking a `Call`/`Global`/`MakeClosure` operand as an import while its unit
/// is still being built — [`Compiler::resolve_import_operands`] rewrites it to
/// `functions.len() + import` once the local function count is final.
const IMPORT_TAG: u32 = 1 << 31;

struct Compiler<'a> {
    /// The path of the file being compiled — what every error names.
    path: &'a str,
    /// The unit being built.
    unit: Unit,
    /// Interned string constants, so a name or text run is stored once.
    strings: HashMap<String, u32>,
    /// Every file's signatures — what a call to another file's function resolves
    /// against, and where its consumed signature comes from.
    index: &'a ExportIndex,
    /// What the host declared, as the host declared it. Name resolution reads the
    /// file's own view of it instead ([`Host::for_file`]), since a file compiled as a
    /// record reads the host's document value as that record.
    host: &'a Host,
    /// Whether this file's top-level bindings are an ambient value's fields (D30) —
    /// which makes each of them a surface the project reads, not a local.
    exports_bindings: bool,
    /// Every name the file binds and everything a name may mean; its frames mirror
    /// `ctxs`.
    names: Names<'a>,
    diagnostics: Vec<Diagnostic>,
    /// The function being compiled, then every lambda open inside it, innermost last.
    ctxs: Vec<FnCtx>,
    /// The paths a `lem-if`, `?:`, or `lem-match` has narrowed, innermost last, each
    /// with the type it holds while the condition is known to hold: a binding, or a
    /// field path from one (`article.hero`), which immutability keeps true (D35).
    narrowings: Vec<(narrowing::Path, Type)>,
    /// What the function being compiled reads and calls — open while a function the
    /// loader may run is being lowered, a top-level `lem-fn` or a loader-run one, and
    /// through every lambda inside it (∆iCW, ∆HF0).
    uses: Option<Collecting>,
    /// The top-level bindings of the bindings file lowered so far, by name — the list a
    /// repeat is checked against, since they are fields and not locals in any scope.
    top_bindings: Vec<(Rc<str>, Span)>,
}

/// The reads and calls of a function the loader may run, being collected.
struct Collecting {
    /// What the loader runs this function as, when it runs it itself — which decides
    /// the ambient values it may read ([`refused_at_load`]); `None` for a `lem-fn`,
    /// which runs at render too and may read them all.
    runs: Option<Runs>,
    /// The `self` of a default, by its binder — what tells a read of the record
    /// being built from any other.
    self_binder: Option<Span>,
    /// A `lem-fn`'s parameters, by binder, in declaration order.
    params: Vec<Span>,
    uses: Uses,
}

impl Collecting {
    /// The root the local `binder` is, if it is one.
    fn root(&self, binder: Span) -> Option<Root> {
        if self.self_binder == Some(binder) {
            return Some(Root::Own);
        }
        self.params
            .iter()
            .position(|param| *param == binder)
            .map(|index| Root::Param(index_u32(index)))
    }
}

impl<'a> Compiler<'a> {
    // --- Driver ---------------------------------------------------------------------

    /// Adds an empty function named `name` and returns its unit-local index.
    fn reserve(&mut self, name: &str) -> u32 {
        self.unit.functions.push(Function {
            name: name.to_owned(),
            params: Vec::new(),
            captures: Vec::new(),
            // A unit has exactly one file; `link` rewrites this to the module's index.
            file: 0,
            n_locals: 0,
            code: Vec::new(),
            spans: Vec::new(),
        });
        let index = index_u32(self.unit.functions.len() - 1);
        assert!(index < IMPORT_TAG, "fewer than 2^31 functions");
        index
    }

    fn compile_functions(&mut self, parse: &Parse) {
        for item in parse.root().items() {
            let name = match &item {
                Item::FnBlock(f) => required(f.name()),
                Item::FnVoid(f) => required(f.name()),
                _ => continue,
            };
            let Some(export) = self
                .unit
                .exports
                .iter()
                .find(|e| &*e.name == name.text() && e.name_span == name.span())
            else {
                // Ambient-named or a duplicate: reported when the exports were
                // collected, and nothing to compile into.
                continue;
            };
            let (func, params, ret) = (
                export.func,
                export.params.clone(),
                self.known(export.ret.clone()),
            );
            self.push_frame();
            let tokens: Vec<SyntaxToken> = match &item {
                Item::FnBlock(f) => f.params(),
                Item::FnVoid(f) => f.params(),
                _ => unreachable!("only functions reach here"),
            }
            .map(|params| params.params().map(|p| required(p.name())).collect())
            .unwrap_or_default();
            for (token, param) in tokens.iter().zip(&params) {
                let ty = self.known(param.ty.clone());
                self.bind(token, BindingKind::Parameter, ty);
            }
            let binders = tokens.iter().map(TokenSpan::span).collect();
            // A `lem-fn` runs at render as much as at load, so its body may read the
            // host's document value; what it reads is noted for the link step to
            // close through the calls that reach it from a loader-run function (∆HF0).
            let uses = self.collecting(None, None, binders, |c| match &item {
                Item::FnBlock(f) => {
                    let body = required(f.body());
                    c.emit(Op::Capture, f.span());
                    c.lower_block(body.items().collect(), BodyKind::Value);
                    c.emit(Op::EndCapture, f.span());
                }
                Item::FnVoid(f) => {
                    c.check(&required(f.value()), &ret);
                }
                _ => unreachable!("only functions reach here"),
            });
            self.emit(Op::Return, item.span());
            let (ctx, frame) = self.pop_frame();
            self.finish(
                func,
                ctx,
                &frame,
                params.iter().map(|p| p.name.clone()).collect(),
            );
            self.unit.uses.insert(func, uses);
        }
    }

    /// Compiles the loader's function for every field default the records this file
    /// declares carry (∆iCW), and records every `by` index (∆eag). A default is
    /// checked against its field's type and runs with `self` — the record being
    /// built, as the declaring type — and, on a record that extends the host's
    /// document type, `path` and `body` as well. An index names a field of an
    /// ambient value that is a map from string to the field's record, and is no
    /// function: the loader indexes that map as loaded.
    fn compile_defaults(&mut self, parse: &Parse) {
        for item in parse.root().items() {
            let Item::TypeDecl(decl) = &item else {
                continue;
            };
            let name = required(decl.name());
            // Only a declaration this file kept: a refused one (reserved, repeated, or
            // another file's) was reported where it was collected.
            let Some(declared) = self
                .unit
                .types
                .iter()
                .find(|d| &*d.name == name.text() && d.name_span == name.span())
                .cloned()
            else {
                continue;
            };
            if self
                .index
                .lookup_type(&declared.name)
                .is_some_and(|owner| owner.path != self.path)
            {
                continue;
            }
            let Shape::Record {
                fields: declared_fields,
                ..
            } = &declared.shape
            else {
                unreachable!("a `lem-type` declares a record");
            };
            let page_backed = self.host.document_type().is_some_and(|document| {
                *document == *declared.name || self.index.extends(&declared.name, &document)
            });
            for field in decl.fields() {
                let field_name = required(field.name());
                // A repeated field was refused with the declaration's own diagnostic;
                // whatever default it carries belongs to nothing.
                let Some(field_decl) =
                    declared_fields.iter().find(|f| f.span == field_name.span())
                else {
                    continue;
                };
                let ty = self.known(field_decl.ty.clone());
                let span = field_name.span();
                let name = format!("{}.{}", declared.name, field_decl.name);
                if let Some(default) = field.default() {
                    let func =
                        self.compile_step(&name, &declared.name, page_backed, span, |c| {
                            c.check(&default, &ty);
                        });
                    self.unit.defaults.push(DefaultFn {
                        record: declared.name.clone(),
                        field: field_decl.name.clone(),
                        func,
                        span,
                    });
                }
                if let Some(index) = field.index() {
                    if let Type::Named(record) = ty.strip_optional()
                        && self.decls().kind(record) == Some(DeclKind::Record)
                    {
                        let record = record.clone();
                        if let Some((value, key)) = self.index_field(&index, &record) {
                            self.unit.indexes.push(IndexDecl {
                                record: declared.name.clone(),
                                field: field_decl.name.clone(),
                                value,
                                key,
                                span,
                            });
                        }
                    } else if ty == field_decl.ty {
                        // Declared as written — `any` included, which is spelled and
                        // resolves nothing. An unknown type reads as `any` only after
                        // the annotation reported it, and nothing more is said then.
                        self.error(
                            index.span(),
                            format!(
                                "`by` resolves a record: `{}` is declared `{ty}`",
                                field_decl.name
                            ),
                        );
                    }
                }
            }
        }
    }

    /// The ambient value and field a `by` index names — `by site.photos` — checked
    /// to be declared `{string: R}` for the field's record `R`, or nothing once what
    /// is wrong with the index has been reported: it is some other expression, the
    /// name is undefined or not an ambient value, the value is the host's document
    /// value, which no record may reach while it loads, or the field is not declared
    /// or not that map — the field read typed as `value.field` is typed anywhere.
    /// The value's binding is a fact of this unit, as any read of it is (D25); a
    /// value the host left untyped admits any field.
    fn index_field(
        &mut self,
        index: &Expr,
        record: &Rc<str>,
    ) -> Option<(Rc<str>, Rc<str>)> {
        const SHAPE: &str =
            "`by` names a field of an ambient value, as in `by site.photos`";
        let Expr::Member(member) = index else {
            self.error(index.span(), SHAPE);
            return None;
        };
        let (Expr::Name(name), false) = (required(member.object()), member.is_optional())
        else {
            self.error(index.span(), SHAPE);
            return None;
        };
        let (value, key) = (required(name.name()), required(member.name()));
        let ty = match self.names.peek(value.text()) {
            Some(Resolved::Ambient(ty)) => ty,
            Some(_) => {
                self.error(index.span(), SHAPE);
                return None;
            }
            None => {
                self.error(value.span(), self.undefined(value.text()));
                return None;
            }
        };
        if self.host.document() == Some(value.text()) {
            self.error(
                value.span(),
                format!(
                    "`{}` is bound to a record once it is loaded, so an index cannot \
                     name a field of it",
                    value.text()
                ),
            );
            return None;
        }
        let ty = self.ambient_type(value.text(), ty, value.span());
        let actual = match rules::member(&ty, key.text(), false, self.decls()) {
            Ok(actual) => self.known(actual),
            Err(message) => {
                self.error(key.span(), message);
                return None;
            }
        };
        let expected = Type::map(Type::Named(record.clone()));
        match rules::expected(actual, &expected, self.decls()) {
            Ok(_) => Some((Rc::from(value.text()), Rc::from(key.text()))),
            Err(message) => {
                self.error(index.span(), message);
                None
            }
        }
    }

    /// Compiles one function the loader runs for the record `record`: `self` bound
    /// as that record, and `path`/`body` too when `page_backed`, then whatever `lower`
    /// emits as its return value. Collects what the body reads while it runs, lambdas
    /// inside included. The three implicit names are the language's, so they take
    /// [`BindingKind::Implicit`], and they are told apart by binder at three distinct
    /// spans inside the field's name — nothing is ever reported at one.
    fn compile_step(
        &mut self,
        name: &str,
        record: &Rc<str>,
        page_backed: bool,
        span: Span,
        lower: impl FnOnce(&mut Self),
    ) -> u32 {
        let mut params: Vec<(&str, Type, Span)> =
            vec![("self", Type::Named(record.clone()), span)];
        if page_backed {
            params.push(("path", Type::String, Span::new(span.start, span.start)));
            params.push(("body", Type::Html, Span::new(span.end, span.end)));
        }
        self.function_for_loader(
            name,
            &params,
            Runs::Default { page_backed },
            Some(span),
            span,
            lower,
        )
    }

    /// Compiles a function of `params` the loader calls, returning what `lower` leaves
    /// on the stack, with its reads and calls recorded in the unit — `self_binder`
    /// being the binder of the `self` among the parameters, if one is.
    fn function_for_loader(
        &mut self,
        name: &str,
        params: &[(&str, Type, Span)],
        runs: Runs,
        self_binder: Option<Span>,
        span: Span,
        lower: impl FnOnce(&mut Self),
    ) -> u32 {
        self.push_frame();
        for (param, ty, binder) in params {
            self.names
                .bind_local(param, *binder, BindingKind::Implicit, ty.clone());
        }
        let uses = self.collecting(Some(runs), self_binder, Vec::new(), lower);
        self.emit(Op::Return, span);
        let (ctx, frame) = self.pop_frame();
        let func = self.reserve(name);
        self.finish(
            func,
            ctx,
            &frame,
            params
                .iter()
                .map(|(param, _, _)| Rc::from(*param))
                .collect(),
        );
        self.unit.uses.insert(func, uses);
        func
    }

    /// Runs `lower` with a collector open for the function it lowers — `runs` set when
    /// the loader itself runs that function — and returns what was collected. Nested
    /// under an outer collector, which is restored after: a lambda is its enclosing
    /// function's, so nothing opens one for it.
    fn collecting(
        &mut self,
        runs: Option<Runs>,
        self_binder: Option<Span>,
        params: Vec<Span>,
        lower: impl FnOnce(&mut Self),
    ) -> Uses {
        let outer = self.uses.replace(Collecting {
            runs,
            self_binder,
            params,
            uses: Uses::default(),
        });
        lower(self);
        let collected = self.uses.take().expect("the collector opened above");
        self.uses = outer;
        collected.uses
    }

    /// Records `read` as one the function being compiled makes, once.
    fn note_read(&mut self, read: Access) {
        if let Some(collecting) = &mut self.uses
            && !collecting.uses.reads.contains(&read)
        {
            collecting.uses.reads.push(read);
        }
    }

    /// Records the read `object.field` makes, when a function the loader may run is
    /// being compiled and the object is one whose fields decide order: a root, or an
    /// ambient value, by name.
    fn note_member_read(&mut self, object: &lemmon_syntax::ast::Expr, field: &str) {
        let Some(collecting) = &self.uses else {
            return;
        };
        let read = match self.bare_root(object) {
            Some(narrowing::PathRoot::Ambient(value)) => Access::Ambient {
                value,
                field: Rc::from(field),
            },
            Some(narrowing::PathRoot::Local(binder)) => match collecting.root(binder) {
                Some(Root::Own) => Access::Own(Rc::from(field)),
                Some(Root::Param(index)) => Access::Param {
                    index,
                    field: Rc::from(field),
                },
                Some(Root::Ambient(_)) | None => return,
            },
            None => return,
        };
        self.note_read(read);
    }

    /// Records a naming of the project function `name`, when a function the loader
    /// may run is being compiled, once per distinct `args` — what the call handed
    /// each parameter, in the callee's declaration order.
    fn note_call(&mut self, name: &str, args: Vec<Option<Root>>) {
        if let Some(collecting) = &mut self.uses {
            let call = Call {
                name: Rc::from(name),
                args,
            };
            if !collecting.uses.calls.contains(&call) {
                collecting.uses.calls.push(call);
            }
        }
    }

    /// The root `expr` is, when it is one written bare — `self`, a parameter, or an
    /// ambient value — and the function being compiled is one the loader may run.
    fn root_of(&self, expr: &lemmon_syntax::ast::Expr) -> Option<Root> {
        let collecting = self.uses.as_ref()?;
        match self.bare_root(expr)? {
            narrowing::PathRoot::Local(binder) => collecting.root(binder),
            narrowing::PathRoot::Ambient(value) => Some(Root::Ambient(value)),
        }
    }

    /// Compiles the file's top level into its entry function. `fields_of` is the
    /// ambient value this file's top-level bindings are the fields of, when it is the
    /// bindings file: only here do those bindings answer to their bare names — a
    /// `lem-fn` in the file, compiled before this, reads them as `site.x` like every
    /// other file, and a default there is refused as anywhere.
    fn compile_entry(
        &mut self,
        source: &ParsedFile,
        chunks: Option<&[String]>,
        fields_of: Option<Rc<str>>,
    ) {
        if let Some(value) = fields_of {
            self.names.expose(value);
        }
        self.push_frame();
        let parse = source.parse();
        let items = parse.root().items().collect();
        match (source, chunks) {
            (ParsedFile::Document(doc), Some(chunks)) => {
                self.lower_document(items, &doc.prose, chunks);
            }
            (ParsedFile::Template(_), None) => {
                self.lower_block(items, BodyKind::File);
            }
            _ => unreachable!("a document has its chunks and a template has none"),
        }
        let span = parse.root().span();
        let null = self.constant(Value::Null);
        self.emit(Op::Const { index: null }, span);
        self.emit(Op::Return, span);
        let (ctx, frame) = self.pop_frame();
        self.finish(0, ctx, &frame, Vec::new());
    }

    /// Opens a function: its code and its names, together.
    fn push_frame(&mut self) {
        self.ctxs.push(FnCtx::new());
        self.names.push_frame();
    }

    /// Closes the innermost function, handing back its code and its names.
    fn pop_frame(&mut self) -> (FnCtx, Frame) {
        let ctx = self.ctxs.pop().expect("a function being compiled");
        let frame = self.names.pop_frame();
        (ctx, frame)
    }

    /// Writes a finished function into its reserved slot.
    fn finish(&mut self, func: u32, ctx: FnCtx, frame: &Frame, params: Vec<Rc<str>>) {
        debug_assert_eq!(ctx.code.len(), ctx.spans.len());
        let function = &mut self.unit.functions[func as usize];
        function.params = params;
        function.captures = frame.captures().iter().map(|c| c.slot).collect();
        function.n_locals = frame.n_locals();
        function.code = ctx.code;
        function.spans = ctx.spans;
    }

    /// The project's definition of `name` — only ever asked about a name that
    /// resolved to a function, which is what put it in the index.
    fn export(&self, name: &str) -> &'a Export {
        self.index
            .lookup(name)
            .expect("a name resolved as a function is in the index")
    }

    /// The declaration table the rules consult.
    fn decls(&self) -> &'a dyn Decls {
        self.index
    }

    /// The `func` operand for the project function `export`: the local index when this
    /// file defines it, an import-tagged one otherwise — recorded symbolically, with
    /// the signature consumed as a fact, the first time the name is used.
    fn func_operand(&mut self, export: &'a Export, span: Span) -> u32 {
        if export.path == self.path {
            return export.signature.func;
        }
        let name = &export.signature.name;
        let index = match self.unit.imports.iter().position(|i| &i.name == name) {
            Some(index) => index,
            None => {
                self.unit.imports.push(ImportRef {
                    name: name.clone(),
                    span,
                });
                self.unit.facts.push(ImportedSignature {
                    name: name.clone(),
                    params: export.signature.params.clone(),
                    ret: export.signature.ret.clone(),
                });
                // The types the signature names were consumed with it.
                let mut named = Vec::new();
                export.signature.ty().named_types(&mut named);
                for name in named {
                    self.note_type(&name, span);
                }
                self.unit.imports.len() - 1
            }
        };
        let index = index_u32(index);
        assert!(index < IMPORT_TAG, "fewer than 2^31 imports");
        IMPORT_TAG | index
    }

    /// Rewrites every import-tagged operand to its final index past the local
    /// functions, now that how many of those there are is known.
    fn resolve_import_operands(&mut self) {
        let locals = index_u32(self.unit.functions.len());
        for function in &mut self.unit.functions {
            for op in &mut function.code {
                match op {
                    Op::Global { func }
                    | Op::Call { func, .. }
                    | Op::MakeClosure { func, .. }
                        if *func & IMPORT_TAG != 0 =>
                    {
                        *func = locals + (*func & !IMPORT_TAG);
                    }
                    _ => {}
                }
            }
        }
    }

    // --- Emission -------------------------------------------------------------------

    fn ctx(&mut self) -> &mut FnCtx {
        self.ctxs.last_mut().expect("a function being compiled")
    }

    fn emit(&mut self, op: Op, span: Span) {
        let ctx = self.ctx();
        ctx.code.push(op);
        ctx.spans.push(span);
    }

    /// Emits a jump whose target is patched later; returns its position.
    fn emit_jump(&mut self, op: Op, span: Span) -> usize {
        self.emit(op, span);
        self.ctx().code.len() - 1
    }

    /// Points the jump at `at` to the next instruction.
    fn patch(&mut self, at: usize) {
        let to = index_u32(self.ctx().code.len());
        match &mut self.ctx().code[at] {
            Op::Jump { to: target }
            | Op::JumpIfFalse { to: target }
            | Op::JumpUnlessNull { to: target }
            | Op::JumpIfFalseKeep { to: target }
            | Op::JumpIfTrueKeep { to: target }
            | Op::IterInit { exit: target }
            | Op::IterNext { exit: target, .. } => *target = to,
            other => unreachable!("{other:?} has no jump target"),
        }
    }

    fn here(&mut self) -> u32 {
        index_u32(self.ctx().code.len())
    }

    fn constant(&mut self, value: Value) -> u32 {
        self.unit.constants.push(value);
        index_u32(self.unit.constants.len() - 1)
    }

    /// The constant index of `text` as a string, shared by every use.
    fn intern(&mut self, text: &str) -> u32 {
        if let Some(&index) = self.strings.get(text) {
            return index;
        }
        let index = self.constant(Value::str(text));
        self.strings.insert(text.to_owned(), index);
        index
    }

    fn error(&mut self, span: Span, message: impl Into<String>) {
        self.diagnostics
            .push(Diagnostic::error(self.path, span, message));
    }

    fn warn(&mut self, span: Span, message: impl Into<String>) {
        self.diagnostics
            .push(Diagnostic::warning(self.path, span, message));
    }

    // --- Names ----------------------------------------------------------------------

    fn push_scope(&mut self) {
        self.names.push_scope();
    }

    fn pop_scope(&mut self) {
        self.names.pop_scope();
    }

    /// Binds `name` as `kind` with type `ty` ([`Names::bind_local`]) and returns the
    /// slot its binder stores into — a refused binding's included, so the store still
    /// compiles.
    fn bind(&mut self, name: &SyntaxToken, kind: BindingKind, ty: Type) -> u16 {
        self.names.bind_local(name.text(), name.span(), kind, ty)
    }

    /// What `name` means where it stands ([`Names::resolve`]).
    fn resolve(&mut self, name: &str) -> Option<Resolved> {
        self.names.resolve(name)
    }
}

/// Extends the syntax crate's token with the span every diagnostic needs.
trait TokenSpan {
    fn span(&self) -> Span;
}

impl TokenSpan for SyntaxToken {
    fn span(&self) -> Span {
        self.text_range().into()
    }
}

pub(crate) fn index_u32(index: usize) -> u32 {
    u32::try_from(index).expect("fewer than 2^32 items")
}

#[cfg(test)]
mod tests {
    use lemmon_syntax::Severity;

    use super::*;
    use crate::host::{Ambient, Claim};
    use crate::render;
    use crate::testing::at;
    use crate::unit::Param;

    /// A `lem-fn` taking the name of a stdlib function, which two tests below compile
    /// as one file and as two.
    const LIB_RAW: &str = "<lem-fn raw(s: string): string = s />";

    /// The host every test below compiles against unless it needs another: the three
    /// names the standard shell binds, none of them typed. Its handlers claim the three
    /// names the shell's own bundler answers for, and `code` as a highlighter does —
    /// two languages, a `Code` payload, keyed by its `language` field (D42).
    fn untyped_host() -> Host {
        Host::untyped(["site", "page", "build"]).with_claims([
            ("styles", Claim::new()),
            ("head", Claim::new()),
            ("scripts", Claim::new()),
            (
                "code",
                Claim::new()
                    .with_keys(["bash", "css"])
                    .with_payload("Code", Some("language")),
            ),
        ])
    }

    /// The record that host's `code` claim names, as a site's `ambient.lem` declares it.
    const CODE: &str = "<lem-type Code>\n  language: string?\n  info: string\n  \
                        code: string\n</lem-type>\n";

    fn run(src: &str) -> Result<String, String> {
        run_files(&[SourceFile::new("t.lem", src)], "t.lem")
    }

    fn run_files(files: &[SourceFile], entry: &str) -> Result<String, String> {
        let compiled = compile(files, &untyped_host());
        let Some(module) = compiled.output else {
            return Err(shown(&compiled.diagnostics).join("\n"));
        };
        render(&module, entry, &[("page", Value::str("P"))])
            .map(|rendered| rendered.html)
            .map_err(|e| e.to_string())
    }

    fn shown(diagnostics: &[Diagnostic]) -> Vec<String> {
        diagnostics.iter().map(ToString::to_string).collect()
    }

    /// Compiles `files`, asserting there is no module and returning what was reported.
    fn compile_errors(files: &[SourceFile]) -> Vec<Diagnostic> {
        let compiled = compile(files, &untyped_host());
        assert!(
            compiled.output.is_none(),
            "compiled despite {:?}",
            compiled.diagnostics
        );
        compiled.diagnostics
    }

    // --- Defaults and `by` indexes, compiled for the loader (∆iCW) ----------------

    /// A host shaped like the standard shell's, so a record extending `Page` is
    /// page-backed.
    fn shell_host() -> Host {
        Host::new()
            .bind("page", Ambient::named("Page"))
            .bind("site", Ambient::bindings("site.lem"))
            .documents("page")
            .rendered_through("layout")
    }

    const PAGE: &str = "<lem-type Page>\n  url: url\n</lem-type>";

    fn shown_with(host: &Host, files: &[SourceFile]) -> Vec<String> {
        shown(&compile(files, host).diagnostics)
    }

    /// The unit of `path` compiled against `host` and the other files.
    fn unit_with(host: &Host, path: &str, files: &[SourceFile]) -> Unit {
        let prepared: Vec<_> = files
            .iter()
            .map(|file| prepare(file, host).expect("the fixture parses"))
            .collect();
        let mut index = ExportIndex::new();
        for p in &prepared {
            index.offer(p.path(), p.exports());
            index.offer_types(p.path(), p.types());
        }
        let this = prepared
            .iter()
            .find(|p| p.path() == path)
            .expect("the file is given");
        compile_unit(this, &index, host).expect_output()
    }

    #[test]
    fn checks_a_fields_default_against_the_fields_type() {
        const SRC: &str =
            "<lem-type A>\n  title: string = \"T\"\n  n: int = \"no\"\n</lem-type>";
        assert_eq!(
            shown_with(&untyped_host(), &[SourceFile::new("types.lem", SRC)]),
            [format!(
                "types.lem:{}: expected `int`, got `string`",
                at(SRC, "\"no\"")
            )]
        );
    }

    #[test]
    fn gives_a_default_self_and_records_the_fields_it_reads() {
        let unit = unit_with(
            &untyped_host(),
            "types.lem",
            &[SourceFile::new(
                "types.lem",
                "<lem-type A>\n  first: string\n  words: [string]\n  \
                 full: string = self.first + words(self)\n  \
                 tags: [string] = self.words.filter(|w| w != self.first + join(self, w))\n\
                 </lem-type>\n\
                 <lem-fn words(a: A): string = a.first />\n\
                 <lem-fn join(a: A, w: string): string = w />",
            )],
        );
        let rows: Vec<(String, &Uses)> = unit
            .defaults
            .iter()
            .map(|d| (format!("{}.{}", d.record, d.field), &unit.uses[&d.func]))
            .collect();
        let own = |f: &str| Access::Own(Rc::from(f));
        let call = |name: &str, args: &[Option<Root>]| Call {
            name: Rc::from(name),
            args: args.to_vec(),
        };
        assert_eq!(
            rows,
            [
                (
                    "A.full".to_owned(),
                    // `words(self)` hands the record over whole: a call, not a read.
                    &Uses {
                        reads: vec![own("first")],
                        calls: vec![call("words", &[Some(Root::Own)])],
                    }
                ),
                // Read inside a lambda, and through `self` captured into it — and
                // called there too, `w` being the lambda's own and no root.
                (
                    "A.tags".to_owned(),
                    &Uses {
                        reads: vec![own("words"), own("first")],
                        calls: vec![call("join", &[Some(Root::Own), None])],
                    }
                ),
            ]
        );
        let full = &unit.functions[unit.defaults[0].func as usize];
        assert_eq!(full.name, "A.full");
        assert_eq!(full.params, [Rc::from("self")]);
        // `words(self)` passes the record to a function: no field read, no fact.
        assert_eq!(unit.facts.len(), 0);
    }

    /// Functions whose uses the tests below read: what the link step closes a
    /// default's calls through (∆HF0).
    const FNS: &str = "<lem-type Photo>\n  alt: string\n</lem-type>\n\
                       <lem-type A>\n  first: string\n  hero: Photo?\n  words: [string]\n\
                       </lem-type>\n\
                       <lem-fn tail(a: A, sep: string): string = sep + a.first />\n\
                       <lem-fn describe(p: Photo?): string = p?.alt ?? \"\" />\n\
                       <lem-fn tally(s: any): string = s.name />\n\
                       <lem-fn full(a: A, sep: string): string = \
                       a.first + tail(sep: sep, a: a) + site.name + describe(a.hero) + \
                       a.words.map(|w| tail(a, w))[0] + tally(site) />";

    /// The uses of the `lem-fn` `name` in `unit`.
    fn uses_of<'a>(unit: &'a Unit, name: &str) -> &'a Uses {
        let export = unit.exports.iter().find(|e| &*e.name == name).unwrap();
        &unit.uses[&export.func]
    }

    fn param(index: u32, field: &str) -> Access {
        Access::Param {
            index,
            field: Rc::from(field),
        }
    }

    #[test]
    fn records_the_fields_a_function_reads_off_its_parameters_and_ambient_values() {
        let unit = unit_with(
            &untyped_host(),
            "fns.lem",
            &[SourceFile::new("fns.lem", FNS)],
        );
        assert_eq!(
            uses_of(&unit, "full").reads,
            [
                param(0, "first"),
                Access::Ambient {
                    value: Rc::from("site"),
                    field: Rc::from("name"),
                },
                // `describe(a.hero)` reads `hero` and hands a field over, not a root:
                // `describe`'s reads off its parameter are the photo's.
                param(0, "hero"),
                param(0, "words"),
                Access::Whole(Rc::from("site")),
            ]
        );
        assert_eq!(uses_of(&unit, "tail").reads, [param(0, "first")]);
        assert_eq!(uses_of(&unit, "tally").reads, [param(0, "name")]);
    }

    #[test]
    fn records_the_calls_a_function_makes_with_the_root_handed_each_parameter() {
        // In the callee's parameter order, whatever order the arguments were written
        // in; a root is `self`, a parameter, or an ambient value, handed whole.
        let unit = unit_with(
            &untyped_host(),
            "fns.lem",
            &[SourceFile::new("fns.lem", FNS)],
        );
        assert_eq!(
            uses_of(&unit, "full").calls,
            [
                Call {
                    name: Rc::from("tail"),
                    args: vec![Some(Root::Param(0)), Some(Root::Param(1))],
                },
                Call {
                    name: Rc::from("describe"),
                    args: vec![None],
                },
                // Inside the lambda, `a` is still the function's parameter and `w`
                // is the lambda's own.
                Call {
                    name: Rc::from("tail"),
                    args: vec![Some(Root::Param(0)), None],
                },
                Call {
                    name: Rc::from("tally"),
                    args: vec![Some(Root::Ambient(Rc::from("site")))],
                },
            ]
        );
    }

    #[test]
    fn records_a_repeated_call_once() {
        let unit = unit_with(
            &untyped_host(),
            "fns.lem",
            &[SourceFile::new(
                "fns.lem",
                "<lem-fn twice(s: string): string = once(s) + once(s) + once(\"\") />\n\
                 <lem-fn once(s: string): string = s />",
            )],
        );
        assert_eq!(
            uses_of(&unit, "twice").calls,
            [
                Call {
                    name: Rc::from("once"),
                    args: vec![Some(Root::Param(0))],
                },
                Call {
                    name: Rc::from("once"),
                    args: vec![None],
                },
            ]
        );
    }

    #[test]
    fn records_no_call_for_a_function_named_as_a_value() {
        // `layout = card` stores `card` for the render to call, and `map(loud)` hands
        // `loud` to a builtin: neither is a call written here, so neither is followed
        // — what `card` reads of `page` is the render's business, and what `loud`
        // reads at load is an error at the read, never a guess (invariant 10).
        let unit = unit_with(
            &untyped_host(),
            "types.lem",
            &[SourceFile::new(
                "types.lem",
                "<lem-type A>\n  words: [string]\n  \
                 shouted: [string] = self.words.map(loud)\n  \
                 layout: (A, html): html = card\n</lem-type>\n\
                 <lem-fn loud(w: string): string = w + site.name />\n\
                 <lem-fn card(a: A, body: html)><lem.page.url><lem.body></lem-fn>",
            )],
        );
        for row in &unit.defaults {
            assert!(unit.uses[&row.func].calls.is_empty(), "{}", row.field);
        }
    }

    #[test]
    fn records_a_block_functions_reads_but_not_those_through_a_local() {
        // A block body's reads count like a value's; a local bound to a parameter is
        // not a root, so a field read through it is not counted — the loader's error
        // at the read is what an author who aliases `self` or a parameter gets.
        let unit = unit_with(
            &untyped_host(),
            "fns.lem",
            &[SourceFile::new(
                "fns.lem",
                "<lem-type A>\n  first: string\n  last: string\n</lem-type>\n\
                 <lem-fn card(a: A)><lem-let b = a /><lem.a.first><lem.b.last></lem-fn>",
            )],
        );
        assert_eq!(uses_of(&unit, "card").reads, [param(0, "first")]);
    }

    #[test]
    fn lets_a_function_read_the_document_value_and_records_it() {
        // A `lem-fn` runs at render, where `page` is bound; the read is recorded, a
        // field or the value whole, so the link step can refuse a default that
        // reaches it through the call (∆HF0).
        const FNS: &str = "<lem-fn where(): url = page.url />\n\
                           <lem-fn here(): Page = page />";
        let files = [
            SourceFile::new("ambient.lem", PAGE),
            SourceFile::new("fns.lem", FNS),
        ];
        assert_eq!(shown_with(&shell_host(), &files), Vec::<String>::new());
        let unit = unit_with(&shell_host(), "fns.lem", &files);
        assert_eq!(
            uses_of(&unit, "where").reads,
            [Access::Ambient {
                value: Rc::from("page"),
                field: Rc::from("url")
            }]
        );
        assert_eq!(
            uses_of(&unit, "here").reads,
            [Access::Whole(Rc::from("page"))]
        );
    }

    #[test]
    fn gives_a_page_backed_default_path_and_body() {
        let post = SourceFile::new(
            "schema.lem",
            "<lem-type Post extends Page>\n  slug: string = path\n  \
             excerpt: html = body\n  layout: (Post, html): html\n</lem-type>",
        );
        let files = [SourceFile::new("ambient.lem", PAGE), post];
        assert_eq!(shown_with(&shell_host(), &files), Vec::<String>::new());
        let unit = unit_with(&shell_host(), "schema.lem", &files);
        let slug = &unit.functions[unit.defaults[0].func as usize];
        assert_eq!(
            slug.params,
            [Rc::from("self"), Rc::from("path"), Rc::from("body")]
        );
    }

    #[test]
    fn gives_a_data_records_default_neither_path_nor_body() {
        // A record loaded from data has no document behind it.
        const SRC: &str = "<lem-type Author>\n  bio: html = body\n</lem-type>";
        let author = SourceFile::new("author.lem", SRC);
        assert_eq!(
            shown_with(
                &shell_host(),
                &[SourceFile::new("ambient.lem", PAGE), author]
            ),
            [format!(
                "author.lem:{}: undefined name `body`",
                at(SRC, "body")
            )]
        );
    }

    #[test]
    fn records_a_field_read_by_calling_it() {
        // `self.fmt(…)` reads the field before calling what it holds, and the loader
        // must have it computed first like any other read.
        let unit = unit_with(
            &untyped_host(),
            "types.lem",
            &[SourceFile::new(
                "types.lem",
                "<lem-type A>\n  fmt: (string): string\n  \
                 full: string = self.fmt(\"x\")\n</lem-type>",
            )],
        );
        let uses = &unit.uses[&unit.defaults[0].func];
        assert_eq!(uses.reads, [Access::Own(Rc::from("fmt"))]);
        // A call through a value names no function the link step could follow.
        assert!(uses.calls.is_empty());
    }

    #[test]
    fn refuses_every_ambient_value_inside_a_default() {
        // A record's plan depends on its own declarations alone (∆eag): every ambient
        // value is out of scope, whole or a field of it.
        for (ty, default, name) in [
            ("url", "page.url", "page"),
            ("string", "site.name", "site"),
            ("url", "show(page)", "page"),
        ] {
            let schema = format!(
                "<lem-type Post extends Page>\n  title: {ty} = {default}\n  \
                 layout: (Post, html): html\n</lem-type>\n\
                 <lem-fn show(p: Page): url = p.url />"
            );
            let files = [
                SourceFile::new("ambient.lem", PAGE),
                SourceFile::new("site.lem", "<lem-let name: string = \"n\" />"),
                SourceFile::new("schema.lem", schema.clone()),
            ];
            assert_eq!(
                shown_with(&shell_host(), &files),
                [format!(
                    "schema.lem:{}: `{name}` is out of scope in a default: a default \
                     reads only the record it builds — `self`, and `path` and `body` \
                     on a page",
                    at(&schema, &format!("«{name}»"))
                )],
                "{default}"
            );
        }
    }

    #[test]
    fn tells_a_data_records_default_only_of_self() {
        // A record above the document type has no `path` or `body` to be told of.
        const SRC: &str = "<lem-type Photo>\n  caption: string = site.name\n</lem-type>";
        let files = [
            SourceFile::new("site.lem", "<lem-let name: string = \"n\" />"),
            SourceFile::new("photo.lem", SRC),
        ];
        assert_eq!(
            shown_with(&shell_host(), &files),
            [format!(
                "photo.lem:{}: `site` is out of scope in a default: a default reads \
                 only the record it builds — `self`",
                at(SRC, "«site».name")
            )]
        );
    }

    #[test]
    fn refuses_the_bindings_value_inside_a_default_declared_in_the_bindings_file() {
        // The bindings file's own records get no special reach: a default there reads
        // `site.x` as any file would, and is refused as any file would be.
        const SITE: &str = "<lem-type Tag>\n  label: string = site.name\n  \
                            bare: string = name\n</lem-type>\n\
                            <lem-let name: string = \"n\" />";
        assert_eq!(
            shown_with(&shell_host(), &[SourceFile::new("site.lem", SITE)]),
            [
                format!(
                    "site.lem:{}: `site` is out of scope in a default: a default reads \
                     only the record it builds — `self`",
                    at(SITE, "«site».name")
                ),
                format!("site.lem:{}: undefined name `name`", at(SITE, "= «name»")),
            ]
        );
    }

    #[test]
    fn records_a_by_index_as_the_ambient_field_it_names() {
        // No function: the pair is the whole of what the loader needs, and `site`'s
        // binding is a fact of the unit as any read of it would make it (∆eag).
        let site = SourceFile::new(
            "site.lem",
            "<lem-let photos: {string: Photo} from \"p.json\" />\n\
             <lem-let name: string = \"n\" />",
        );
        let photo = "<lem-type Photo>\n  alt: string\n</lem-type>\n";
        let schema =
            format!("{photo}<lem-type Post>\n  hero: Photo? by site.photos\n</lem-type>");
        let files = [site.clone(), SourceFile::new("schema.lem", schema.clone())];
        assert!(shown_with(&shell_host(), &files).is_empty());
        let unit = unit_with(&shell_host(), "schema.lem", &files);
        assert!(unit.defaults.is_empty());
        assert_eq!(
            unit.indexes,
            [IndexDecl {
                record: Rc::from("Post"),
                field: Rc::from("hero"),
                value: Rc::from("site"),
                key: Rc::from("photos"),
                span: at(&schema, "«hero»: Photo?"),
            }]
        );
        assert_eq!(unit.functions.len(), 1, "the entry alone");
        assert_eq!(
            unit.ambient_refs
                .iter()
                .map(|r| &*r.name)
                .collect::<Vec<_>>(),
            ["site"]
        );
        for (field, message) in [
            (
                "hero: Photo? by site.name",
                "expected `{string: Photo}`, got `string`",
            ),
            (
                "hero: Photo? by site.nope",
                "`site.lem` has no field `nope`; did you mean `name`?",
            ),
            (
                "hero: Photo? by lookup()",
                "`by` names a field of an ambient value, as in `by site.photos`",
            ),
            (
                "hero: Photo? by photos",
                "`by` names a field of an ambient value, as in `by site.photos`",
            ),
            (
                "hero: Photo? by page.photos",
                "`page` is bound to a record once it is loaded, so an index cannot name a \
                 field of it",
            ),
            ("hero: Photo? by ste.photos", "undefined name `ste`"),
            (
                "title: string by site.photos",
                "`by` resolves a record: `title` is declared `string`",
            ),
            // `any` is spelled, and resolves nothing.
            (
                "hero: any by site.photos",
                "`by` resolves a record: `hero` is declared `any`",
            ),
            // The unknown type was reported at the annotation; nothing more is said.
            ("x: Nope by site.photos", "unknown type `Nope`"),
            ("x: Nope? by site.photos", "unknown type `Nope`"),
        ] {
            let files = [
                site.clone(),
                SourceFile::new(
                    "schema.lem",
                    format!("{photo}<lem-type Post>\n  {field}\n</lem-type>"),
                ),
            ];
            let reported = shown_with(&shell_host(), &files);
            assert_eq!(reported.len(), 1, "{field}: {reported:?}");
            assert!(reported[0].ends_with(message), "{field}: {reported:?}");
        }
    }

    #[test]
    fn takes_any_field_of_an_untyped_value_as_a_by_index() {
        let unit = unit_with(
            &untyped_host(),
            "schema.lem",
            &[SourceFile::new(
                "schema.lem",
                "<lem-type Photo>\n  alt: string\n</lem-type>\n\
                 <lem-type Post>\n  hero: Photo? by site.whatever\n</lem-type>",
            )],
        );
        assert_eq!(unit.indexes.len(), 1);
        assert_eq!(&*unit.indexes[0].key, "whatever");
    }

    #[test]
    fn resolves_a_default_that_names_a_project_function() {
        const SCHEMA: &str = "<lem-type Post extends Page>\n  \
                              layout: (Post, html): html = other\n</lem-type>\n\
                              <lem-fn other(s: string): html><lem.s></lem-fn>";
        let files = [
            SourceFile::new("ambient.lem", PAGE),
            SourceFile::new(
                "schema.lem",
                "<lem-type Post extends Page>\n  layout: (Post, html): html = post\n\
                 </lem-type>\n<lem-fn post(p: Post, body: html)><lem.p.url><lem.body></lem-fn>",
            ),
        ];
        assert_eq!(shown_with(&shell_host(), &files), Vec::<String>::new());
        let files = [
            SourceFile::new("ambient.lem", PAGE),
            SourceFile::new("schema.lem", SCHEMA),
        ];
        assert_eq!(
            shown_with(&shell_host(), &files),
            [format!(
                "schema.lem:{}: expected `(Post, html): html`, got `(string): html`",
                at(SCHEMA, "= «other»")
            )]
        );
    }

    #[test]
    fn compiles_no_default_for_a_declaration_or_field_the_file_did_not_keep() {
        // `b.lem`'s `A` lost to `a.lem`'s, and `B`'s second `t` is a repeat: neither
        // default belongs to anything, so neither is compiled — were they, each would
        // add a diagnostic of its own to the duplicate's, since `nope` is no field.
        const A: &str = "<lem-type A>\n  t: string = \"a\"\n</lem-type>";
        const B: &str = "<lem-type A>\n  t: string = self.nope\n</lem-type>\n\
                         <lem-type B>\n  t: string = \"1\"\n  t: string = self.nope\n\
                         </lem-type>";
        let files = [SourceFile::new("a.lem", A), SourceFile::new("b.lem", B)];
        assert_eq!(
            shown_with(&untyped_host(), &files),
            [
                format!(
                    "b.lem:{}: `A` is already defined\n  a.lem:{}: first defined here",
                    at(B, "A"),
                    at(A, "A")
                ),
                format!(
                    "b.lem:{}: `t` is already defined\n  b.lem:{}: first defined here",
                    at(B, "\"1\"\n  «t»: string"),
                    at(B, "B>\n  «t»: string")
                ),
            ]
        );
    }

    #[test]
    fn refuses_a_file_that_did_not_parse_clean_with_its_parse_diagnostics() {
        const A: &str = "<lem-if x>";
        const B: &str = "<lem-bogus />";
        let errors =
            compile_errors(&[SourceFile::new("a.lem", A), SourceFile::new("b.lem", B)]);
        let shown = shown(&errors);
        assert_eq!(shown.len(), 2, "{shown:?}");
        let opens = format!("a.lem:{}: ", at(A, "<lem-if"));
        let bogus = format!("b.lem:{}: ", at(B, "<lem-bogus"));
        assert!(shown[0].starts_with(&opens), "{shown:?}");
        assert!(shown[1].starts_with(&bogus), "{shown:?}");
    }

    #[test]
    fn shares_functions_across_files_but_not_bindings() {
        const PAGE_SECRET: &str = "<lem.secret>";
        let files = [
            SourceFile::new(
                "lib.lem",
                "<lem-fn greet(who: string): string = \"hi \" + who />\n<lem-let secret = 1 />",
            ),
            SourceFile::new("page.lem", "<lem.greet(\"you\")> <lem.page>"),
        ];
        assert_eq!(run_files(&files, "page.lem").unwrap(), "hi you P");
        assert_eq!(run_files(&files, "lib.lem").unwrap(), "");
        let files = [
            SourceFile::new("lib.lem", "<lem-let secret = 1 /><lem.secret>"),
            SourceFile::new("page.lem", PAGE_SECRET),
        ];
        assert_eq!(
            run_files(&files, "page.lem").unwrap_err(),
            format!(
                "page.lem:{}: undefined name `secret`",
                at(PAGE_SECRET, "secret")
            )
        );
    }

    #[test]
    fn warns_at_a_mark_name_no_handler_claims() {
        const SRC: &str = "<lem.mark(\"stylez\")>";
        let compiled = compile(&[SourceFile::new("t.lem", SRC)], &untyped_host());
        assert_eq!(
            shown(&compiled.diagnostics),
            [format!(
                "t.lem:{}: warning: no handler claims `stylez`; did you mean `styles`?",
                at(SRC, "\"stylez\"")
            )]
        );
        // The page is still correct without a handler, so the unit is still built.
        assert!(compiled.output.is_some());
    }

    #[test]
    fn says_nothing_about_a_mark_where_the_host_declares_no_handlers() {
        // A playground or an editor runs no plugins (D42): every mark there is meant to
        // keep its default content, and warning at each would be noise.
        const SRC: &str = "<lem.mark(\"stylez\")>";
        let bare = Host::untyped(["site", "page", "build"]);
        let compiled = compile(&[SourceFile::new("t.lem", SRC)], &bare);
        assert_eq!(shown(&compiled.diagnostics), Vec::<String>::new());
    }

    #[test]
    fn warns_at_every_mark_of_a_name_no_handler_claims() {
        // Each `mark()` is its own written mistake, and the warning stands at the word
        // the author would fix. What is recorded once is the fact behind them.
        const SRC: &str = "<lem.mark(\"stylez\")><lem.mark(\"stylez\")>";
        let compiled = compile(&[SourceFile::new("t.lem", SRC)], &untyped_host());
        let spans: Vec<Span> = compiled.diagnostics.iter().map(|d| d.span).collect();
        assert_eq!(
            spans,
            [
                at(SRC, "«\"stylez\"»)><"),
                at(SRC, "><lem.mark(«\"stylez\"»)")
            ]
        );
    }

    #[test]
    fn refuses_a_mark_name_a_render_would_compute() {
        const SRC: &str = "<lem-let name = \"styles\" />\n<lem.mark(name)>";
        assert_eq!(
            shown(&compile_errors(&[SourceFile::new("t.lem", SRC)])),
            [format!(
                "t.lem:{}: `mark()` needs a name written as a literal: the host's claims \
                 are read while this file compiles, and a name a render computes cannot \
                 be checked then",
                at(SRC, "mark(«name»)")
            )]
        );
    }

    #[test]
    fn takes_a_marks_default_as_one_named_argument() {
        // The default is `html`, and named: `mark("toc", default: …)`, never a second
        // positional, which ∆7wK's payload takes.
        assert_eq!(
            run("<lem-let fallback>none yet</lem-let>\n<lem.mark(\"styles\", default: fallback)>")
                .unwrap(),
            "none yet"
        );
        const STRING: &str = "<lem.mark(\"styles\", default: \"x\")>";
        assert_eq!(
            run(STRING).unwrap_err(),
            format!(
                "t.lem:{}: expected `html`, got `string`",
                at(STRING, "\"x\"")
            ),
        );
        const NAMED: &str = "<lem.mark(\"styles\", fallback: 1)>";
        assert_eq!(
            run(NAMED).unwrap_err(),
            format!(
                "t.lem:{}: `mark()` takes one named argument, `default`, and takes it \
                 once, not `fallback`",
                at(NAMED, "fallback")
            )
        );
    }

    #[test]
    fn keeps_equal_fragments_from_two_files_once_under_one_name() {
        // Each file interns `styles` among its own constants, so after link the name
        // stands at a different index in each: a render that kept fragments by index
        // would give this CSS twice. No single-file case can show it.
        const LIB: &str = "<lem-fn card()><lem-style>.card {}</lem-style>card</lem-fn>";
        const PAGE: &str = "<lem-style>.card {}</lem-style><lem.card()>";
        let files = [
            SourceFile::new("lib.lem", LIB),
            SourceFile::new("page.lem", PAGE),
        ];
        let module = compile(&files, &untyped_host()).output.expect("a module");
        let rendered = render(&module, "page.lem", &[]).unwrap();
        assert_eq!(rendered.html, "card");
        let [fragment] = rendered.fragments.get("styles") else {
            panic!("one fragment: {:?}", rendered.fragments);
        };
        // The page's own block ran first, so the fragment points there.
        assert_eq!(
            (module.files[fragment.file as usize].as_str(), fragment.span),
            ("page.lem", at(PAGE, "<lem-style>.card {}</lem-style>"))
        );
    }

    #[test]
    fn types_a_marks_payload_by_the_record_the_claim_names() {
        // The claim names `Code`, so the record the call builds is checked against that
        // declaration like any other record literal (D42).
        let src = format!(
            "{CODE}<lem.mark(\"code\", {{language: \"bash\", info: \"\", code: \"x\"}})>"
        );
        let compiled = compile(&[SourceFile::new("t.lem", &src)], &untyped_host());
        assert_eq!(shown(&compiled.diagnostics), Vec::<String>::new());
        // A field the record does not declare is the mistake a claimed payload catches.
        let wrong =
            format!("{CODE}<lem.mark(\"code\", {{language: \"bash\", nope: \"\"}})>");
        let compiled = compile(&[SourceFile::new("t.lem", &wrong)], &untyped_host());
        let messages = shown(&compiled.diagnostics);
        assert!(
            messages
                .iter()
                .any(|m| m.contains("`Code` has no field `nope`")),
            "{messages:?}"
        );
    }

    #[test]
    fn refuses_a_payload_where_the_claim_takes_none_and_wants_one_where_it_does() {
        // The two halves of the same rule: the claim says whether a mark of the name
        // carries anything, so a call that disagrees is refused at the call.
        const EXTRA: &str = "<lem.mark(\"styles\", {a: 1})>";
        assert_eq!(
            shown(&compile_errors(&[SourceFile::new("t.lem", EXTRA)])),
            [format!(
                "t.lem:{}: the handler for `styles` takes no payload",
                at(EXTRA, "{a: 1}")
            )]
        );
        const MISSING: &str = "<lem.mark(\"code\")>";
        let src = format!("{CODE}{MISSING}");
        assert_eq!(
            shown(&compile_errors(&[SourceFile::new("t.lem", &src)])),
            [format!(
                "t.lem:{}: the producer for `code` takes a `Code` payload",
                at(&src, "mark(\"code\")")
            )]
        );
    }

    #[test]
    fn warns_at_a_key_no_producer_claims_where_the_key_is_written() {
        // A producer lists the keys it covers and no wildcard (D42), so a key none of
        // them lists is the author's mistake — and it is said at the key, not at the
        // name, which is what the editor puts a squiggle under.
        let src = format!(
            "{CODE}<lem.mark(\"code\", {{language: \"bsah\", info: \"\", code: \"x\"}})>"
        );
        let compiled = compile(&[SourceFile::new("t.lem", &src)], &untyped_host());
        assert_eq!(
            shown(&compiled.diagnostics),
            [format!(
                "t.lem:{}: warning: no producer claims `bsah` for `code`; did you mean \
                 `bash`?",
                at(&src, "\"bsah\"")
            )]
        );
        // The page is correct without a producer, so the unit is still built.
        assert!(compiled.output.is_some());
    }

    #[test]
    fn says_nothing_about_a_key_the_host_declares_no_handlers_for() {
        // The other half: a playground declares no handlers, so neither the name nor
        // the key is anything it can be wrong about.
        let src = format!(
            "{CODE}<lem.mark(\"code\", {{language: \"bsah\", info: \"\", code: \"x\"}})>"
        );
        let host = Host::untyped(["site", "page", "build"]);
        let compiled = compile(&[SourceFile::new("t.lem", &src)], &host);
        assert_eq!(shown(&compiled.diagnostics), Vec::<String>::new());
    }

    #[test]
    fn keys_a_mark_by_the_payload_field_the_claim_names() {
        // Written plainly, the key is taken at compile; computed, it is read from the
        // record at render and left to the handler to check (D42).
        let written = format!(
            "{CODE}<lem.mark(\"code\", {{language: \"bash\", info: \"\", code: \"x\"}})>"
        );
        let computed = format!(
            "{CODE}<lem-let lang = \"ba\" + \"sh\" />\\n\
             <lem.mark(\"code\", {{language: lang, info: \"\", code: \"x\"}})>"
        );
        for src in [written, computed] {
            let module =
                compile(&[SourceFile::new("t.lem", &src)], &untyped_host()).expect_output();
            let rendered = render(&module, "t.lem", &[]).expect("a page");
            let [mark] = rendered.marks.as_slice() else {
                panic!("one mark: {:?}", rendered.marks);
            };
            assert_eq!(
                (&*mark.name, mark.key.as_deref()),
                ("code", Some("bash")),
                "{src}"
            );
            let Some(Value::Record(payload)) = &mark.payload else {
                panic!("a record payload: {:?}", mark.payload);
            };
            assert_eq!(
                (&*payload.name, payload.fields.get("code")),
                ("Code", Some(&Value::str("x")))
            );
        }
    }

    #[test]
    fn leaves_a_mark_unkeyed_where_the_claim_keys_by_nothing() {
        // `styles` takes no payload and no key, so its marks reach every handler for the
        // name — which is what a bundler wants.
        let module = compile(
            &[SourceFile::new("t.lem", "<lem.mark(\"styles\")>")],
            &untyped_host(),
        )
        .expect_output();
        let rendered = render(&module, "t.lem", &[]).expect("a page");
        assert_eq!(
            rendered.marks[0].key.as_deref(),
            None,
            "{:?}",
            rendered.marks
        );
        assert_eq!(rendered.marks[0].payload, None);
    }

    #[test]
    fn refuses_a_payload_that_is_no_record_and_one_that_holds_a_function() {
        // Nothing is claimed here, so nothing types the payload — it only has to be
        // something a handler could be given as data.
        const LOOSE: &str = "<lem.mark(\"styles\", \"x\")>";
        let host = Host::untyped(["site", "page", "build"]);
        assert_eq!(
            shown(&compile(&[SourceFile::new("t.lem", LOOSE)], &host).diagnostics),
            [format!(
                "t.lem:{}: a mark's payload is a record, got string",
                at(LOOSE, "\"x\"")
            )]
        );
        // And a claimed record whose fields reach a function cannot cross to a handler
        // at all, whatever the call wrote.
        const HELD: &str = "<lem-type Held>\n  render: (string): string\n</lem-type>\n\
                            <lem-type Wrap>\n  held: Held\n</lem-type>\n";
        let src = format!(
            "{HELD}<lem.mark(\"code\", {{held: {{render: up}}}})>\n\
                           <lem-fn up(s: string): string = s />"
        );
        let claiming = Host::untyped(["site", "page", "build"])
            .with_claims([("code", Claim::new().with_payload("Wrap", None))]);
        let messages =
            shown(&compile(&[SourceFile::new("t.lem", &src)], &claiming).diagnostics);
        assert!(
            messages.iter().any(|m| m.contains(
                "a mark's payload crosses to the handler as data, and `Wrap.held.render` \
                 is a function, so `code` cannot carry it"
            )),
            "{messages:?}"
        );
    }

    #[test]
    fn refuses_a_claim_naming_a_payload_record_nothing_declares() {
        // The record is the program's to declare, so a claim naming one it has not is a
        // mistake in the host's declaration, said at the call that consumed it.
        const SRC: &str = "<lem.mark(\"code\", {language: \"bash\"})>";
        assert_eq!(
            shown(&compile_errors(&[SourceFile::new("t.lem", SRC)])),
            [format!(
                "t.lem:{}: the host's claim for `code` names a payload `Code`, which \
                 nothing declares",
                at(SRC, "mark(\"code\", {language: \"bash\"})")
            )]
        );
    }

    #[test]
    fn keys_a_mark_only_where_the_field_holds_a_string() {
        // A written key can only be a string literal, so a computed one that is anything
        // else keys nothing either: the handler answers for an unkeyed mark rather than
        // for one keyed by a run of markup, and the two paths key alike.
        const SRC: &str = "<lem-type Spec>\n  kind: any\n</lem-type>\n\
                           <lem-let markup>a <b>diagram</b></lem-let>\n\
                           <lem-let computed = \"pik\" + \"chr\" />\n\
                           <lem.mark(\"diagram\", {kind: markup})>\
                           <lem.mark(\"diagram\", {kind: computed})>\
                           <lem.mark(\"diagram\", {kind: \"mermaid\"})>";
        let host = Host::untyped(["site", "page", "build"])
            .with_claims([("diagram", Claim::new().with_payload("Spec", Some("kind")))]);
        let module = compile(&[SourceFile::new("t.lem", SRC)], &host).expect_output();
        let rendered = render(&module, "t.lem", &[]).expect("a page");
        assert_eq!(
            rendered
                .marks
                .iter()
                .map(|mark| mark.key.as_deref())
                .collect::<Vec<_>>(),
            [None, Some("pikchr"), Some("mermaid")]
        );
    }

    #[test]
    fn refuses_a_claim_whose_payload_names_an_enum() {
        // No record literal could satisfy an enum, so the mistake is the host's and is
        // said as the host's — telling the author their literal was the wrong shape
        // would send them to fix the one thing here that is right.
        const SRC: &str = "<lem-enum Placement = hero | left />\n\
                           <lem.mark(\"code\", {language: \"bash\"})>";
        let host = Host::untyped(["site", "page", "build"])
            .with_claims([("code", Claim::new().with_payload("Placement", None))]);
        assert_eq!(
            shown(&compile(&[SourceFile::new("t.lem", SRC)], &host).diagnostics),
            [format!(
                "t.lem:{}: the host's claim for `code` names a payload `Placement`, \
                 which is an enum, not a record",
                at(SRC, "mark(\"code\", {language: \"bash\"})")
            )]
        );
    }

    #[test]
    fn takes_one_or_two_positional_arguments() {
        for (src, expect) in [
            ("<lem.mark()>", "`mark` takes one or two arguments, got 0"),
            (
                "<lem.mark(\"code\", {a: 1}, 3)>",
                "`mark` takes one or two arguments, got 3",
            ),
        ] {
            let messages = shown(&compile_errors(&[SourceFile::new("t.lem", src)]));
            assert!(messages.iter().any(|m| m.contains(expect)), "{messages:?}");
        }
    }

    #[test]
    fn refuses_a_payload_carrying_a_mark_however_deeply_it_is_held() {
        // The third of the three places a mark cannot stand (D42): a payload is never
        // printed, so a mark inside one would stand for a range of nothing. Held one
        // level down, where a check of the value's own type would miss it.
        const SRC: &str = "<lem-let inner><lem.mark(\"styles\")></lem-let>\n\
                           <lem.mark(\"styles\", {held: [inner]})>";
        let host = Host::untyped(["site", "page", "build"]);
        let module = compile(&[SourceFile::new("t.lem", SRC)], &host).expect_output();
        let error = render(&module, "t.lem", &[]).expect_err("a render error");
        assert_eq!(error.message, "a mark cannot stand inside a payload");
        assert_eq!(error.span, at(SRC, "mark(\"styles\", {held: [inner]})"));
    }

    #[test]
    fn places_a_mark_from_the_file_the_call_stands_in() {
        // The name is a constant of the file that marks, and link offsets each file's
        // pool: an `Op::Mark` left out of that rebasing would read the page's constants
        // instead, under a name nothing claims. Only a module of two files shows it —
        // and the same case pins the file a handler's diagnostic would point at.
        const LIB: &str = "<lem-fn styles(): html = mark(\"styles\") />";
        const PAGE: &str = "<p>a</p><lem.styles()>";
        let lib = SourceFile::new("lib.lem", LIB);
        let page = SourceFile::new("page.lem", PAGE);
        // Both orders, since a file linked first is offset by nothing and would hide a
        // missing rebase.
        for files in [[lib.clone(), page.clone()], [page, lib]] {
            let module = compile(&files, &untyped_host()).output.expect("a module");
            let rendered = render(&module, "page.lem", &[]).unwrap();
            let [mark] = rendered.marks.as_slice() else {
                panic!("one mark: {:?}", rendered.marks);
            };
            assert_eq!(
                (
                    &*mark.name,
                    module.files[mark.file as usize].as_str(),
                    mark.span,
                    (mark.start, mark.end)
                ),
                ("styles", "lib.lem", at(LIB, "mark(\"styles\")"), (8, 8))
            );
        }
    }

    #[test]
    fn calls_a_function_across_files_whichever_order_the_files_come_in() {
        // Both orders, so a link that forgot to offset a unit's local indices — under
        // which `two` here collides with `one` or the other file's entry — misrenders.
        let a = SourceFile::new("a.lem", "<lem-fn one(): int = 1 />");
        let b = SourceFile::new("b.lem", "<lem-fn two(): int = 2 /><lem.one()><lem.two()>");
        assert_eq!(run_files(&[a.clone(), b.clone()], "b.lem").unwrap(), "12");
        assert_eq!(run_files(&[b, a], "b.lem").unwrap(), "12");
    }

    #[test]
    fn compiles_a_unit_from_the_callee_signature_never_its_body() {
        // Invariant 11: an edit to the callee's body leaves the caller's unit
        // byte-identical, because all it consumed is the signature — its one fact.
        let unit_against = |lib_body: &str| {
            let lib =
                prepare(&SourceFile::new("lib.lem", lib_body), &untyped_host()).unwrap();
            let page = prepare(
                &SourceFile::new("page.lem", "<lem.greet(\"you\")>"),
                &untyped_host(),
            )
            .unwrap();
            let mut index = ExportIndex::new();
            index.offer("lib.lem", lib.exports());
            index.offer_types("lib.lem", lib.types());
            index.offer("page.lem", page.exports());
            index.offer_types("page.lem", page.types());
            compile_unit(&page, &index, &untyped_host()).expect_output()
        };
        let unit = unit_against("<lem-fn greet(who: string): string = \"hi \" + who />");
        let edited = unit_against("<lem-fn greet(who: string): string = \"HI \" + who />");
        assert_eq!(unit, edited);
        assert_eq!(
            unit.facts,
            vec![ImportedSignature {
                name: Rc::from("greet"),
                params: vec![Param {
                    name: Rc::from("who"),
                    ty: Type::String,
                }],
                ret: Type::String,
            }]
        );
        assert_eq!(unit.imports.len(), 1);
    }

    #[test]
    fn records_every_type_a_consumed_signature_names_wherever_it_stands() {
        // The signature is the fact; the types it names — inside a map, a function
        // type, an optional — were consumed with it, each at the call that consumed it.
        let types = "<lem-type Article>\n  title: string\n</lem-type>\n\
                     <lem-type Photo>\n  alt: string\n</lem-type>\n\
                     <lem-type Vod>\n  poster: url\n</lem-type>";
        let lib = "<lem-fn f(m: {string: Article}, g: (Photo): Vod?): [Article] = [] />";
        const PAGE: &str = "<lem-let xs = f({}, |p| null) />";
        let prepared: Vec<_> = [("types.lem", types), ("lib.lem", lib), ("page.lem", PAGE)]
            .iter()
            .map(|(path, text)| {
                prepare(&SourceFile::new(*path, *text), &untyped_host()).unwrap()
            })
            .collect();
        let mut index = ExportIndex::new();
        for p in &prepared {
            index.offer(p.path(), p.exports());
            index.offer_types(p.path(), p.types());
        }
        let unit = compile_unit(&prepared[2], &index, &untyped_host()).expect_output();
        let names: Vec<&str> = unit.type_refs.iter().map(|r| &*r.name).collect();
        assert_eq!(names, ["Article", "Photo", "Vod"]);
        let call = unit.imports[0].span;
        assert_eq!(call, at(PAGE, "f({}, |p| null)"));
        assert!(unit.type_refs.iter().all(|r| r.span == call));
        assert!(unit.type_refs.iter().all(|r| r.fact.name == r.name));
    }

    #[test]
    fn tells_a_consumer_when_a_foreign_types_base_is_not_declared() {
        // types.lem reports the unknown base at its `extends`; page.lem, which cannot
        // check `l.title` without the chain, says why at its own use rather than
        // claiming `LinkPost` has no such field.
        const TYPES: &str = "<lem-type LinkPost extends Article>\n  link: url\n\
                             </lem-type>";
        const PAGE: &str = "<lem-fn t(l: LinkPost): string = l.link + l.title />";
        let errors = compile_errors(&[
            SourceFile::new("types.lem", TYPES),
            SourceFile::new("page.lem", PAGE),
        ]);
        assert_eq!(
            shown(&errors),
            [
                format!("types.lem:{}: unknown type `Article`", at(TYPES, "Article")),
                format!(
                    "page.lem:{}: `LinkPost` extends `Article`, which is not declared",
                    at(PAGE, "LinkPost")
                ),
                format!(
                    "page.lem:{}: `LinkPost` has no field `title`",
                    at(PAGE, "l.title")
                ),
            ]
        );
    }

    #[test]
    fn reports_a_broken_file_alongside_the_other_files_problems() {
        // One file's parse errors no longer suppress what is wrong elsewhere.
        const B: &str = "<lem.missing>";
        let errors = compile_errors(&[
            SourceFile::new("a.lem", "<lem-if x>"),
            SourceFile::new("b.lem", B),
        ]);
        let shown = shown(&errors);
        assert_eq!(shown.len(), 2, "{shown:?}");
        assert!(shown[0].starts_with("a.lem:"), "{shown:?}");
        assert_eq!(
            shown[1],
            format!("b.lem:{}: undefined name `missing`", at(B, "missing"))
        );
    }

    #[test]
    #[should_panic(expected = "`compile` was given `a.lem` twice")]
    fn refuses_a_call_that_names_one_file_twice() {
        // Two texts for one path have no single answer for what `a.lem` exports: the
        // second offer withdraws the first's names, and the first file then compiles
        // against an index that has lost them.
        compile(
            &[
                SourceFile::new("a.lem", "<lem-fn f(): int = 1 />"),
                SourceFile::new("a.lem", "<lem-fn g(): int = 2 />"),
            ],
            &untyped_host(),
        );
    }

    #[test]
    fn reads_a_path_repeated_anywhere_in_a_call() {
        assert_eq!(repeated_path(["a.lem", "b.lem"]), None);
        assert_eq!(repeated_path(["a.lem", "b.lem", "a.lem"]), Some("a.lem"));
    }

    #[test]
    fn cites_the_first_definition_when_a_function_is_redefined_in_another_file() {
        const A: &str = "<lem-fn f(): int = 1 />";
        const B: &str = "x\n<lem-fn f(): int = 2 />";
        let files = [SourceFile::new("a.lem", A), SourceFile::new("b.lem", B)];
        assert_eq!(
            run_files(&files, "a.lem").unwrap_err(),
            format!(
                "b.lem:{}: `f` is already defined\n  a.lem:{}: first defined here",
                at(B, "«f»(): int"),
                at(A, "«f»(): int")
            )
        );
    }

    #[test]
    fn cites_the_first_definition_when_a_function_is_redefined_in_the_same_file() {
        const SRC: &str = "<lem-fn f(): int = 1 />\n<lem-fn f(): int = 2 />";
        let errors = compile_errors(&[SourceFile::new("t.lem", SRC)]);
        assert_eq!(
            errors[0].to_string(),
            format!(
                "t.lem:{}: `f` is already defined\n  t.lem:{}: first defined here",
                at(SRC, "«f»(): int = 2"),
                at(SRC, "«f»(): int = 1")
            )
        );
    }

    #[test]
    fn reports_every_problem_in_a_file_rather_than_the_first() {
        const SRC: &str = "<lem.a><lem.b>";
        let errors = compile_errors(&[SourceFile::new("t.lem", SRC)]);
        assert_eq!(errors.len(), 2);
        assert_eq!(
            errors[1].to_string(),
            format!("t.lem:{}: undefined name `b`", at(SRC, "b"))
        );
    }

    #[test]
    fn reports_problems_in_source_order_whichever_pass_found_them() {
        // The undefined name is found compiling the entry, the bad definition while
        // declaring — the later pass, but the earlier bytes.
        let errors = compile_errors(&[SourceFile::new(
            "t.lem",
            "<lem.missing>\n<lem-fn page(): int = 1 />",
        )]);
        let spans: Vec<u32> = errors.iter().map(|e| e.span.start).collect();
        assert_eq!(spans, [5, 22]);
    }

    #[test]
    fn reports_problems_in_the_order_the_files_were_given() {
        let errors = compile_errors(&[
            SourceFile::new("b.lem", "<lem.x>"),
            SourceFile::new("a.lem", "<lem.y>"),
        ]);
        let paths: Vec<&str> = errors.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, ["b.lem", "a.lem"]);
    }

    #[test]
    fn binds_a_let_after_its_value_so_the_value_sees_the_outer_name() {
        assert_eq!(
            run(
                "<lem-let x = 1 /><lem-if true><lem-let x = x + 1 /><lem.x></lem-if><lem.x>"
            )
            .unwrap(),
            "21"
        );
    }

    #[test]
    fn interns_a_string_used_twice_as_one_constant() {
        let module = compile(
            &[SourceFile::new("t.lem", "<lem.\"a\" + \"a\">")],
            &untyped_host(),
        )
        .expect_output();
        let strings = module
            .constants
            .iter()
            .filter(|c| matches!(c, Value::Str(s) if &**s == "a"))
            .count();
        assert_eq!(strings, 1);
    }

    #[test]
    fn compiles_a_lambda_to_a_function_named_for_what_it_is() {
        let module = compile(
            &[SourceFile::new("t.lem", "<lem-let f = |x| x />")],
            &untyped_host(),
        )
        .expect_output();
        assert!(module.functions.iter().any(|f| f.name == "<lambda>"));
    }

    #[test]
    fn refuses_a_parameter_name_given_twice() {
        const PARAMS: &str = "<lem-fn f(a: int, a: int): int = a />";
        assert_eq!(
            run(PARAMS).unwrap_err(),
            format!(
                "t.lem:{}: `a` is already bound here\n  t.lem:{}: first bound here",
                at(PARAMS, ", «a»: int"),
                at(PARAMS, "f(«a»: int")
            )
        );
        const LAMBDA: &str = "<lem-let f = |a, a| a /><lem.f(1, 2)>";
        assert_eq!(
            run(LAMBDA).unwrap_err(),
            format!(
                "t.lem:{}: `a` is already bound here\n  t.lem:{}: first bound here",
                at(LAMBDA, "|a, «a»|"),
                at(LAMBDA, "|«a», a|")
            )
        );
    }

    #[test]
    fn refuses_to_rebind_a_name_the_shell_provides() {
        for src in [
            "<lem-let page = 1 />",
            "<lem-for site in [1]>x</lem-for>",
            "<lem-fn build(): int = 1 />",
            "<lem-fn f(page: int): int = page />",
            "<lem-let f = |build| build />",
        ] {
            let error = run(src).unwrap_err();
            assert!(error.contains("cannot be redefined"), "{src} gave {error}");
        }
        // Reading one is what they are for.
        assert_eq!(run("<lem.page>").unwrap(), "P");
    }

    #[test]
    fn refuses_a_map_key_given_twice() {
        const SRC: &str = "<lem-let m = {a: 1, \"a\": 2} /><lem.m[\"a\"]>";
        assert_eq!(
            run(SRC).unwrap_err(),
            format!("t.lem:{}: `a` is already given", at(SRC, "«\"a\"»: 2"))
        );
    }

    #[test]
    fn warns_when_a_definition_shadows_the_stdlib_and_still_compiles() {
        // D23: site definitions may shadow the stdlib, with a warning — and the
        // shadowing definition is what the name then means.
        for (src, definition, out) in [
            (
                "<lem-fn raw(s: string): string = s + \"2\" />\n<lem.raw(\"x\")>",
                "«raw»(s: string)",
                "x2",
            ),
            ("<lem-let raw = 2 />\n<lem.raw>", "«raw» = 2", "2"),
        ] {
            let compiled = compile(&[SourceFile::new("t.lem", src)], &untyped_host());
            assert_eq!(
                shown(&compiled.diagnostics),
                [format!(
                    "t.lem:{}: warning: `raw` shadows the stdlib function of the same \
                     name",
                    at(src, definition)
                )],
                "{src}"
            );
            let module = compiled.output.expect("a module still comes with warnings");
            assert_eq!(render(&module, "t.lem", &[]).unwrap().html, out, "{src}");
        }
        // A name that is not the stdlib's does not warn.
        assert_eq!(
            compile(
                &[SourceFile::new(
                    "t.lem",
                    "<lem-let cooked = 2 /><lem.cooked>"
                )],
                &untyped_host(),
            )
            .diagnostics,
            Vec::new()
        );
    }

    #[test]
    fn leaves_parameters_and_loop_bindings_named_after_the_stdlib_alone() {
        // Only definitions warn (`docs/language.md` §Names): a parameter or a loop
        // variable is not a name an author chose against the stdlib.
        for src in [
            "<lem-fn f(raw: string): string = raw />",
            "<lem-let f = |raw| raw /><lem.f(1)>",
            "<lem-for raw in [1]><lem.raw></lem-for>",
            "<lem-for k, raw in {a: 1}><lem.raw></lem-for>",
        ] {
            let compiled = compile(&[SourceFile::new("t.lem", src)], &untyped_host());
            assert_eq!(compiled.diagnostics, Vec::new(), "{src}");
        }
    }

    #[test]
    fn does_not_blame_the_stdlib_for_a_binding_that_shadows_a_project_function() {
        // The `lem-fn raw` displaced the stdlib and warned for it; the `lem-let raw`
        // displaces that function, which D23 allows silently.
        let compiled = compile(
            &[
                SourceFile::new("lib.lem", LIB_RAW),
                SourceFile::new(
                    "t.lem",
                    "<lem-fn card(title: string)><lem-let raw = raw(title) /><lem.raw></lem-fn>",
                ),
            ],
            &untyped_host(),
        );
        assert_eq!(
            shown(&compiled.diagnostics),
            [format!(
                "lib.lem:{}: warning: `raw` shadows the stdlib function of the same name",
                at(LIB_RAW, "raw")
            )]
        );
        assert!(compiled.output.is_some());
    }

    #[test]
    fn warns_only_for_the_copy_of_a_function_the_project_kept() {
        // The repeat is an error and nothing more: the name it collides with is the
        // project's, not the stdlib's.
        let errors = compile_errors(&[
            SourceFile::new("lib.lem", LIB_RAW),
            SourceFile::new("t.lem", LIB_RAW),
        ]);
        assert_eq!(
            shown(&errors),
            [
                format!(
                    "lib.lem:{}: warning: `raw` shadows the stdlib function of the same \
                     name",
                    at(LIB_RAW, "raw")
                ),
                format!(
                    "t.lem:{}: `raw` is already defined\n  lib.lem:{}: first defined \
                     here",
                    at(LIB_RAW, "raw"),
                    at(LIB_RAW, "raw")
                ),
            ]
        );
    }

    #[test]
    fn reports_warnings_in_source_order_with_the_errors() {
        let errors = compile_errors(&[SourceFile::new(
            "t.lem",
            "<lem.missing>\n<lem-let raw = 1 />\n<lem.also><lem.raw>",
        )]);
        let severities: Vec<Severity> = errors.iter().map(|d| d.severity).collect();
        assert_eq!(
            severities,
            [Severity::Error, Severity::Warning, Severity::Error]
        );
    }

    #[test]
    fn panics_with_every_diagnostic_when_output_is_expected_of_a_failed_compile() {
        let result = std::panic::catch_unwind(|| {
            compile(
                &[SourceFile::new("t.lem", "<lem.a><lem.b>")],
                &untyped_host(),
            )
            .expect_output()
        });
        let message = *result.unwrap_err().downcast::<String>().unwrap();
        assert!(message.contains("undefined name `a`"), "{message}");
        assert!(message.contains("undefined name `b`"), "{message}");
    }

    #[test]
    fn renders_the_spike_subset_end_to_end() {
        assert_eq!(run("a<lem.1 + 2>b").unwrap(), "a3b");
        assert_eq!(
            run("<lem-fn f(a: int, b: int)>[<lem.a>|<lem.b>]</lem-fn><lem.f(b: 2, a: 1)>")
                .unwrap(),
            "[1|2]"
        );
        assert_eq!(
            run("<lem-for x, i in [10, 20]><lem.i>:<lem.x> </lem-for>").unwrap(),
            "0:10 1:20 "
        );
        assert_eq!(
            run("<lem-if null>a<lem-else-if 1 lt 2 />b<lem-else />c</lem-if>").unwrap(),
            "b"
        );
        assert_eq!(
            run("<lem-let n = 5 /><lem-let f = |x| x + n /><lem.f(1)>").unwrap(),
            "6"
        );
        assert_eq!(
            run("<lem-fn g(x: int): int = x * 2 /><lem.g(3)>").unwrap(),
            "6"
        );
        assert_eq!(
            run("<lem.\"Hello There\".slugify()>").unwrap(),
            "hello-there"
        );
    }
}

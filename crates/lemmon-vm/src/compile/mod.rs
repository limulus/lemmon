//! Lowering from the typed AST to bytecode, checking types on the way (D31).
//!
//! The compiler reads only the `lemmon_syntax::ast` views — never `SyntaxKind` — and
//! runs only on files that parsed clean, so every `Option` an accessor returns is `Some`
//! here; [`required`] says so in one place. The driver lives in this module; [`items`]
//! lowers what stands in a body, [`expr`] lowers heads and types them as it goes,
//! [`rules`] holds the typing rules the walk asks — pure functions from types to a type
//! or a message — [`annotations`] reads types as written, and [`names`] owns every rule
//! about names — what may be bound where, what a name means and what type it has, in
//! one stated order. There is no separate checking pass: the one walk that emits an
//! expression's code is the walk that knows its type, so the checker's rules are
//! applied where the lowering they guard happens (`docs/language.md` §Checking). What
//! is typed `any` is checked by the VM at run time instead.

mod annotations;
mod expr;
mod items;
mod names;
mod rules;
mod whitespace;

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::rc::Rc;

use lemmon_syntax::ast::{Item, Spanned};
use lemmon_syntax::{DocumentParse, Parse, Severity, Span, SyntaxToken, parse_document};

use crate::bytecode::{Function, Module, Op};
use crate::document;
use crate::exports::{Export, ExportIndex};
use crate::types::Type;
use crate::unit::{ExportedSignature, ImportRef, ImportedSignature, Param, Unit, link};
use crate::{SourceFile, Value};

pub(crate) use names::duplicate_named;
use names::{BindingKind, Frame, Names, Resolved};

/// Something a compile has to say about a file, pointed at precisely: an error the file
/// cannot compile past, or a warning it compiles with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// The file the problem is in.
    pub path: String,
    /// The bytes the problem is about.
    pub span: Span,
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

    /// Adds a secondary span with its note.
    #[must_use]
    pub fn with_label(
        mut self,
        path: impl Into<String>,
        span: Span,
        message: impl Into<String>,
    ) -> Self {
        self.labels.push(Label {
            path: path.into(),
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

    /// The parser's diagnostic about the file at `path`, whose labels are in the same file.
    fn from_syntax(path: &str, diagnostic: &lemmon_syntax::Diagnostic) -> Self {
        Self {
            path: path.to_owned(),
            span: diagnostic.span,
            severity: diagnostic.severity,
            message: diagnostic.message.clone(),
            labels: diagnostic
                .labels
                .iter()
                .map(|label| Label {
                    path: path.to_owned(),
                    span: label.span,
                    message: label.message.clone(),
                })
                .collect(),
        }
    }
}

/// The corpus trailer shape: `path:start..end: message`, `warning: ` before a warning's
/// message, and each label on its own indented line below.
impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let severity = match self.severity {
            Severity::Error => "",
            Severity::Warning => "warning: ",
        };
        write!(
            f,
            "{}:{}..{}: {severity}{}",
            self.path, self.span.start, self.span.end, self.message
        )?;
        for label in &self.labels {
            write!(
                f,
                "\n  {}:{}..{}: {}",
                label.path, label.span.start, label.span.end, label.message
            )?;
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
pub fn compile(files: &[SourceFile]) -> Compiled<Module> {
    if let Some(path) = repeated_path(files.iter().map(|file| file.path.as_str())) {
        panic!("`compile` was given `{path}` twice");
    }
    let mut diagnostics = Vec::new();
    let mut prepared = Vec::new();
    for file in files {
        match prepare(file) {
            Ok(p) => prepared.push(p),
            Err(errs) => diagnostics.extend(errs),
        }
    }
    // Offered in file order, so the first definition of a name is the one the project
    // keeps and every later one is reported where its own file compiles.
    let mut index = ExportIndex::new();
    for p in &prepared {
        index.offer(&p.path, &p.exports);
    }
    let mut units = Vec::new();
    for p in &prepared {
        let compiled = compile_unit(p, &index);
        diagnostics.extend(compiled.diagnostics);
        units.extend(compiled.output);
    }
    let output = if diagnostics.iter().any(Diagnostic::is_error) {
        None
    } else {
        // Every unit was compiled against this same index, so nothing can be
        // unresolved at link time.
        let linked = link(&units, &index);
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

/// A file parsed, its prose rendered, and its exports collected — everything
/// [`compile_unit`] needs that comes from this file alone.
pub struct PreparedFile {
    path: String,
    source: Source,
    /// A document's prose, rendered by comrak here so markdown never runs later.
    chunks: Option<Vec<String>>,
    exports: Vec<ExportedSignature>,
    /// What parsing warned about and what collecting the exports found (ambient names,
    /// same-file duplicates), reported by [`compile_unit`] with the rest of the file's.
    diagnostics: Vec<Diagnostic>,
}

impl PreparedFile {
    /// The path the file is known by.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The top-level `lem-fn`s the file defines — an [`ExportIndex`]'s input.
    #[must_use]
    pub fn exports(&self) -> &[ExportedSignature] {
        &self.exports
    }
}

/// Parses `file`, renders a document's prose, and collects its exported signatures.
///
/// # Errors
///
/// The file's parse or prose diagnostics — a file that does not parse clean yields
/// nothing to compile. Parser warnings are kept in the prepared file and reported with
/// the unit's.
pub fn prepare(file: &SourceFile) -> Result<PreparedFile, Vec<Diagnostic>> {
    let source = Source::of(file);
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
        Source::Template(_) => None,
        Source::Document(doc) => {
            match document::render_prose(&file.path, &file.text, doc) {
                Ok(chunks) => Some(chunks),
                Err(errors) => {
                    diagnostics.extend(errors);
                    return Err(diagnostics);
                }
            }
        }
    };
    let (exports, export_diagnostics) = collect_exports(&file.path, source.parse());
    diagnostics.extend(export_diagnostics);
    Ok(PreparedFile {
        path: file.path.clone(),
        source,
        chunks,
        exports,
        diagnostics,
    })
}

/// Collects the signatures of a parsed file's top-level `lem-fn`s, assigning each its
/// unit-local function index — the entry is 0, so they count from 1. A name the rules
/// refuse ([`Names::bind_function`]: ambient, or repeated in this file) exports nothing.
///
/// A signature is read from the head alone, never the body (invariant 11): a block body
/// returns `html`, and an expression body must declare what it returns.
fn collect_exports(path: &str, parse: &Parse) -> (Vec<ExportedSignature>, Vec<Diagnostic>) {
    let mut names = Names::new(path, None);
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
            (Some(annotation), false) => {
                annotations::resolve(path, &required(annotation.ty()), &mut diagnostics)
            }
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

/// Compiles one prepared file into its [`Unit`], resolving names against `index`. A
/// call to a function another file defines stays a symbolic import, and the signature
/// it was compiled against is recorded as one of the unit's facts.
///
/// A definition the index refused — another file already owns the name — is reported
/// here, so a collision is reported once, by the compile that introduced it, and the
/// refused file yields no unit.
///
/// Every problem in the file comes back in source order; the unit only when none of them
/// is an error.
pub fn compile_unit(prepared: &PreparedFile, index: &ExportIndex) -> Compiled<Unit> {
    let mut compiler = Compiler {
        path: &prepared.path,
        unit: Unit {
            path: prepared.path.clone(),
            constants: Vec::new(),
            functions: Vec::new(),
            imports: Vec::new(),
            exports: prepared.exports.clone(),
            facts: Vec::new(),
        },
        strings: HashMap::new(),
        index,
        names: Names::new(&prepared.path, Some(index)),
        diagnostics: prepared.diagnostics.clone(),
        ctxs: Vec::new(),
        narrowings: Vec::new(),
    };
    let entry = compiler.reserve(&prepared.path);
    debug_assert_eq!(entry, 0, "the entry is a unit's first function");
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
    compiler.compile_entry(&prepared.source, prepared.chunks.as_deref());
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

/// A file as parsed: a template, or a document in document mode. Decided by the path,
/// as it is in a site — `.md` is a document, anything else a template.
enum Source {
    Template(Parse),
    Document(DocumentParse),
}

impl Source {
    fn of(file: &SourceFile) -> Self {
        if file.path.ends_with(".md") {
            Self::Document(parse_document(&file.text))
        } else {
            Self::Template(lemmon_syntax::parse(&file.text))
        }
    }

    /// The tree either way: a document's has its prose as text runs and its blocks as
    /// items, so every pass but the entry's reads it exactly like a template's.
    fn parse(&self) -> &Parse {
        match self {
            Self::Template(parse) => parse,
            Self::Document(doc) => &doc.parse,
        }
    }
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
    /// Every name the file binds and everything a name may mean; its frames mirror
    /// `ctxs`.
    names: Names<'a>,
    diagnostics: Vec<Diagnostic>,
    /// The function being compiled, then every lambda open inside it, innermost last.
    ctxs: Vec<FnCtx>,
    /// The bindings a `lem-if` or `?:` has narrowed, innermost last, each by its
    /// binder's span and the type it holds while the condition is known to hold.
    narrowings: Vec<(Span, Type)>,
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
            let (func, params, ret) =
                (export.func, export.params.clone(), export.ret.clone());
            self.push_frame();
            let tokens: Vec<SyntaxToken> = match &item {
                Item::FnBlock(f) => f.params(),
                Item::FnVoid(f) => f.params(),
                _ => unreachable!("only functions reach here"),
            }
            .map(|params| params.params().map(|p| required(p.name())).collect())
            .unwrap_or_default();
            for (token, param) in tokens.iter().zip(&params) {
                self.bind(token, BindingKind::Parameter, param.ty.clone());
            }
            match &item {
                Item::FnBlock(f) => {
                    let body = required(f.body());
                    self.emit(Op::Capture, f.span());
                    self.lower_block(body.items().collect(), whitespace::Kind::Value);
                    self.emit(Op::EndCapture, f.span());
                }
                Item::FnVoid(f) => {
                    self.check(&required(f.value()), &ret);
                }
                _ => unreachable!("only functions reach here"),
            }
            self.emit(Op::Return, item.span());
            let (ctx, frame) = self.pop_frame();
            self.finish(
                func,
                ctx,
                &frame,
                params.iter().map(|p| p.name.clone()).collect(),
            );
        }
    }

    fn compile_entry(&mut self, source: &Source, chunks: Option<&[String]>) {
        self.push_frame();
        let parse = source.parse();
        let items = parse.root().items().collect();
        match (source, chunks) {
            (Source::Document(doc), Some(chunks)) => {
                self.lower_document(items, &doc.prose, chunks);
            }
            (Source::Template(_), None) => {
                self.lower_block(items, whitespace::Kind::File);
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
    use super::*;
    use crate::render;

    fn run(src: &str) -> Result<String, String> {
        run_files(&[SourceFile::new("t.lem", src)], "t.lem")
    }

    fn run_files(files: &[SourceFile], entry: &str) -> Result<String, String> {
        let compiled = compile(files);
        let Some(module) = compiled.output else {
            return Err(shown(&compiled.diagnostics).join("\n"));
        };
        render(&module, entry, &[("page", Value::str("P"))]).map_err(|e| e.to_string())
    }

    fn shown(diagnostics: &[Diagnostic]) -> Vec<String> {
        diagnostics.iter().map(ToString::to_string).collect()
    }

    /// Compiles `files`, asserting there is no module and returning what was reported.
    fn compile_errors(files: &[SourceFile]) -> Vec<Diagnostic> {
        let compiled = compile(files);
        assert!(
            compiled.output.is_none(),
            "compiled despite {:?}",
            compiled.diagnostics
        );
        compiled.diagnostics
    }

    #[test]
    fn refuses_a_file_that_did_not_parse_clean_with_its_parse_diagnostics() {
        let errors = compile_errors(&[
            SourceFile::new("a.lem", "<lem-if x>"),
            SourceFile::new("b.lem", "<lem-bogus />"),
        ]);
        let shown = shown(&errors);
        assert_eq!(shown.len(), 2, "{shown:?}");
        assert!(shown[0].starts_with("a.lem:0..7: "), "{shown:?}");
        assert!(shown[1].starts_with("b.lem:0..10: "), "{shown:?}");
    }

    #[test]
    fn shares_functions_across_files_but_not_bindings() {
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
            SourceFile::new("page.lem", "<lem.secret>"),
        ];
        assert_eq!(
            run_files(&files, "page.lem").unwrap_err(),
            "page.lem:5..11: undefined name `secret`"
        );
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
            let lib = prepare(&SourceFile::new("lib.lem", lib_body)).unwrap();
            let page =
                prepare(&SourceFile::new("page.lem", "<lem.greet(\"you\")>")).unwrap();
            let mut index = ExportIndex::new();
            index.offer("lib.lem", lib.exports());
            index.offer("page.lem", page.exports());
            compile_unit(&page, &index).expect_output()
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
    fn reports_a_broken_file_alongside_the_other_files_problems() {
        // One file's parse errors no longer suppress what is wrong elsewhere.
        let errors = compile_errors(&[
            SourceFile::new("a.lem", "<lem-if x>"),
            SourceFile::new("b.lem", "<lem.missing>"),
        ]);
        let shown = shown(&errors);
        assert_eq!(shown.len(), 2, "{shown:?}");
        assert!(shown[0].starts_with("a.lem:"), "{shown:?}");
        assert_eq!(shown[1], "b.lem:5..12: undefined name `missing`");
    }

    #[test]
    #[should_panic(expected = "`compile` was given `a.lem` twice")]
    fn refuses_a_call_that_names_one_file_twice() {
        // Two texts for one path have no single answer for what `a.lem` exports: the
        // second offer withdraws the first's names, and the first file then compiles
        // against an index that has lost them.
        compile(&[
            SourceFile::new("a.lem", "<lem-fn f(): int = 1 />"),
            SourceFile::new("a.lem", "<lem-fn g(): int = 2 />"),
        ]);
    }

    #[test]
    fn reads_a_path_repeated_anywhere_in_a_call() {
        assert_eq!(repeated_path(["a.lem", "b.lem"]), None);
        assert_eq!(repeated_path(["a.lem", "b.lem", "a.lem"]), Some("a.lem"));
    }

    #[test]
    fn cites_the_first_definition_when_a_function_is_redefined_in_another_file() {
        let files = [
            SourceFile::new("a.lem", "<lem-fn f(): int = 1 />"),
            SourceFile::new("b.lem", "x\n<lem-fn f(): int = 2 />"),
        ];
        assert_eq!(
            run_files(&files, "a.lem").unwrap_err(),
            "b.lem:10..11: `f` is already defined\n  a.lem:8..9: first defined here"
        );
    }

    #[test]
    fn cites_the_first_definition_when_a_function_is_redefined_in_the_same_file() {
        let errors = compile_errors(&[SourceFile::new(
            "t.lem",
            "<lem-fn f(): int = 1 />\n<lem-fn f(): int = 2 />",
        )]);
        assert_eq!(
            errors[0].to_string(),
            "t.lem:32..33: `f` is already defined\n  t.lem:8..9: first defined here"
        );
    }

    #[test]
    fn reports_every_problem_in_a_file_rather_than_the_first() {
        let errors = compile_errors(&[SourceFile::new("t.lem", "<lem.a><lem.b>")]);
        assert_eq!(errors.len(), 2);
        assert_eq!(errors[1].to_string(), "t.lem:12..13: undefined name `b`");
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
        let module =
            compile(&[SourceFile::new("t.lem", "<lem.\"a\" + \"a\">")]).expect_output();
        let strings = module
            .constants
            .iter()
            .filter(|c| matches!(c, Value::Str(s) if &**s == "a"))
            .count();
        assert_eq!(strings, 1);
    }

    #[test]
    fn compiles_a_lambda_to_a_function_named_for_what_it_is() {
        let module =
            compile(&[SourceFile::new("t.lem", "<lem-let f = |x| x />")]).expect_output();
        assert!(module.functions.iter().any(|f| f.name == "<lambda>"));
    }

    #[test]
    fn refuses_a_parameter_name_given_twice() {
        assert_eq!(
            run("<lem-fn f(a: int, a: int): int = a />").unwrap_err(),
            "t.lem:18..19: `a` is already bound here\n  t.lem:10..11: first bound here"
        );
        assert_eq!(
            run("<lem-let f = |a, a| a /><lem.f(1, 2)>").unwrap_err(),
            "t.lem:17..18: `a` is already bound here\n  t.lem:14..15: first bound here"
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
        assert_eq!(
            run("<lem-let m = {a: 1, \"a\": 2} /><lem.m[\"a\"]>").unwrap_err(),
            "t.lem:20..23: `a` is already given"
        );
    }

    #[test]
    fn leaves_no_capture_behind_when_a_ufcs_probe_finds_a_local() {
        let module = compile(&[SourceFile::new(
            "t.lem",
            "<lem-let name = 1 /><lem-let f = |o| o.name() />",
        )])
        .expect_output();
        let lambda = module
            .functions
            .iter()
            .find(|f| f.name == "<lambda>")
            .expect("the lambda");
        assert_eq!(lambda.captures, Vec::<u16>::new());
    }

    #[test]
    fn warns_when_a_definition_shadows_the_stdlib_and_still_compiles() {
        // D23: site definitions may shadow the stdlib, with a warning — and the
        // shadowing definition is what the name then means.
        for (src, at, out) in [
            (
                "<lem-fn raw(s: string): string = s + \"2\" />\n<lem.raw(\"x\")>",
                "8..11",
                "x2",
            ),
            ("<lem-let raw = 2 />\n<lem.raw>", "9..12", "2"),
        ] {
            let compiled = compile(&[SourceFile::new("t.lem", src)]);
            assert_eq!(
                shown(&compiled.diagnostics),
                [format!(
                    "t.lem:{at}: warning: `raw` shadows the stdlib function of the same name"
                )],
                "{src}"
            );
            let module = compiled.output.expect("a module still comes with warnings");
            assert_eq!(render(&module, "t.lem", &[]).unwrap(), out, "{src}");
        }
        // A name that is not the stdlib's does not warn.
        assert_eq!(
            compile(&[SourceFile::new(
                "t.lem",
                "<lem-let cooked = 2 /><lem.cooked>"
            )])
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
            let compiled = compile(&[SourceFile::new("t.lem", src)]);
            assert_eq!(compiled.diagnostics, Vec::new(), "{src}");
        }
    }

    #[test]
    fn does_not_blame_the_stdlib_for_a_binding_that_shadows_a_project_function() {
        // The `lem-fn raw` displaced the stdlib and warned for it; the `lem-let raw`
        // displaces that function, which D23 allows silently.
        let compiled = compile(&[
            SourceFile::new("lib.lem", "<lem-fn raw(s: string): string = s />"),
            SourceFile::new(
                "t.lem",
                "<lem-fn card(title: string)><lem-let raw = raw(title) /><lem.raw></lem-fn>",
            ),
        ]);
        assert_eq!(
            shown(&compiled.diagnostics),
            ["lib.lem:8..11: warning: `raw` shadows the stdlib function of the same name"]
        );
        assert!(compiled.output.is_some());
    }

    #[test]
    fn warns_only_for_the_copy_of_a_function_the_project_kept() {
        // The repeat is an error and nothing more: the name it collides with is the
        // project's, not the stdlib's.
        let errors = compile_errors(&[
            SourceFile::new("lib.lem", "<lem-fn raw(s: string): string = s />"),
            SourceFile::new("t.lem", "<lem-fn raw(s: string): string = s />"),
        ]);
        assert_eq!(
            shown(&errors),
            [
                "lib.lem:8..11: warning: `raw` shadows the stdlib function of the same name",
                "t.lem:8..11: `raw` is already defined\n  lib.lem:8..11: first defined here",
            ]
        );
    }

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
    fn shows_a_warning_and_its_labels_in_the_trailer_shape() {
        let diagnostic = Diagnostic::warning("a.lem", Span::new(1, 2), "unused")
            .with_label("b.lem", Span::new(3, 4), "bound here");
        assert_eq!(
            diagnostic.to_string(),
            "a.lem:1..2: warning: unused\n  b.lem:3..4: bound here"
        );
        assert!(!diagnostic.is_error());
    }

    #[test]
    fn panics_with_every_diagnostic_when_output_is_expected_of_a_failed_compile() {
        let result = std::panic::catch_unwind(|| {
            compile(&[SourceFile::new("t.lem", "<lem.a><lem.b>")]).expect_output()
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
            run("<lem-fn g(x: int): int = x * 2 /><lem.3.g()>").unwrap(),
            "6"
        );
    }
}

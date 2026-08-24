//! Lowering from the typed AST to bytecode.
//!
//! The compiler reads only the `lemmon_syntax::ast` views — never `SyntaxKind` — and
//! runs only on files that parsed clean, so every `Option` an accessor returns is `Some`
//! here; [`required`] says so in one place. The driver lives in this module with the
//! lexical resolver; [`items`] lowers what stands in a body and [`expr`] lowers heads.
//!
//! Names resolve lexically, innermost first (D23): locals and parameters (through any
//! enclosing lambdas, which capture what they reach), then the project-flat functions
//! every file's `lem-fn`s define, then the ambient builtins and the render-supplied
//! `site`/`page`/`build`. The type checker (`∆C8d`) is not here yet, so the checks that
//! need it — argument types, field existence, `is` — are the VM's at runtime.

mod expr;
mod items;

use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use lemmon_syntax::ast::{Item, Spanned};
use lemmon_syntax::{DocumentParse, Parse, Span, SyntaxToken, parse_document};

use crate::builtins;
use crate::bytecode::{Function, Module, Op};
use crate::document;
use crate::unit::{ExportedSignature, ImportRef, ImportedSignature, Unit, link};
use crate::{SourceFile, Value};

/// The ambient record names every template may read. Bound at render time and never
/// redefinable — a binding or definition that takes one of these names is an error, not
/// a shadow (D23, `docs/language.md` §Names). Stdlib names *may* be shadowed, which D23
/// says should warn; there is no warning channel yet, so that stays silent for now.
const AMBIENT: &[&str] = &["site", "page", "build"];

/// Something in a file that cannot be compiled, pointed at precisely.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompileError {
    /// The file the problem is in.
    pub path: String,
    /// The bytes the problem is about.
    pub span: Span,
    /// Human-readable description of the problem.
    pub message: String,
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}..{}: {}",
            self.path, self.span.start, self.span.end, self.message
        )
    }
}

impl std::error::Error for CompileError {}

/// Compiles every file of a site into one module: each file [`prepare`]d and compiled
/// into its own [`Unit`] against the table of every file's signatures, then [`link`]ed.
///
/// A file that does not parse clean yields no unit — nothing is lowered from a tree the
/// parser had to recover — but the other files still compile, and every problem found
/// in every file is reported, not just the first.
///
/// # Errors
///
/// Every diagnostic, each with the path and span it is about.
pub fn compile(files: &[SourceFile]) -> Result<Module, Vec<CompileError>> {
    let mut errors = Vec::new();
    let mut prepared = Vec::new();
    for file in files {
        match prepare(file) {
            Ok(p) => prepared.push(Some(p)),
            Err(errs) => {
                errors.extend(errs);
                prepared.push(None);
            }
        }
    }
    let (table, table_errors) = SignatureTable::build(
        prepared
            .iter()
            .flatten()
            .map(|p| (p.path.as_str(), p.exports.as_slice())),
    );
    errors.extend(table_errors);
    let mut units = Vec::new();
    for p in prepared.iter().flatten() {
        match compile_unit(p, &table) {
            Ok(unit) => units.push(unit),
            Err(errs) => errors.extend(errs),
        }
    }
    if errors.is_empty() {
        // Every unit was compiled against this same table, so nothing can be
        // unresolved or doubled at link time.
        return link(&units);
    }
    // In source order, like the parser's diagnostics — the order the passes above happen
    // to find things in is an implementation detail nobody reading a file wants to see.
    let order = |error: &CompileError| {
        let file = files.iter().position(|f| f.path == error.path);
        (file, error.span.start, error.span.end)
    };
    errors.sort_by_key(order);
    Err(errors)
}

/// A file parsed, its prose rendered, and its exports collected — everything
/// [`compile_unit`] needs that comes from this file alone.
pub struct PreparedFile {
    path: String,
    source: Source,
    /// A document's prose, rendered by comrak here so markdown never runs later.
    chunks: Option<Vec<String>>,
    exports: Vec<ExportedSignature>,
    /// Problems found collecting the exports (ambient names, same-file duplicates),
    /// reported by [`compile_unit`] alongside the rest of the file's errors.
    errors: Vec<CompileError>,
}

impl PreparedFile {
    /// The path the file is known by.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The top-level `lem-fn`s the file defines — a [`SignatureTable`]'s input.
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
/// nothing to compile.
pub fn prepare(file: &SourceFile) -> Result<PreparedFile, Vec<CompileError>> {
    let source = Source::of(file);
    let parse_errors: Vec<CompileError> = source
        .parse()
        .diagnostics
        .iter()
        .map(|d| CompileError {
            path: file.path.clone(),
            span: d.span,
            message: d.message.clone(),
        })
        .collect();
    if !parse_errors.is_empty() {
        return Err(parse_errors);
    }
    // Prose is rendered before anything is lowered, and a document markdown rejected
    // fails the file the way a parse error does: its blocks may be fine, but where
    // their output would go is not known.
    let chunks = match &source {
        Source::Template(_) => None,
        Source::Document(doc) => Some(document::render_prose(&file.path, &file.text, doc)?),
    };
    let (exports, errors) = collect_exports(&file.path, source.parse());
    Ok(PreparedFile {
        path: file.path.clone(),
        source,
        chunks,
        exports,
        errors,
    })
}

/// Collects the signatures of a parsed file's top-level `lem-fn`s, assigning each its
/// unit-local function index — the entry is 0, so they count from 1. An ambient or
/// repeated name is an error and exports nothing.
fn collect_exports(
    path: &str,
    parse: &Parse,
) -> (Vec<ExportedSignature>, Vec<CompileError>) {
    let mut exports: Vec<ExportedSignature> = Vec::new();
    let mut errors = Vec::new();
    for item in parse.root().items() {
        let (name, params) = match &item {
            Item::FnBlock(f) => (required(f.name()), f.params()),
            Item::FnVoid(f) => (required(f.name()), f.params()),
            _ => continue,
        };
        if AMBIENT.contains(&name.text()) {
            errors.push(CompileError {
                path: path.to_owned(),
                span: name.span(),
                message: ambient_message(name.text()),
            });
            continue;
        }
        if let Some(existing) = exports.iter().find(|e| &*e.name == name.text()) {
            errors.push(CompileError {
                path: path.to_owned(),
                span: name.span(),
                message: duplicate_message(name.text(), path, existing.span),
            });
            continue;
        }
        let params: Vec<Rc<str>> = params
            .map(|params| {
                params
                    .params()
                    .map(|p| Rc::from(required(p.name()).text()))
                    .collect()
            })
            .unwrap_or_default();
        exports.push(ExportedSignature {
            name: Rc::from(name.text()),
            params,
            func: index_u32(exports.len() + 1),
            name_span: name.span(),
            span: item.span(),
        });
    }
    (exports, errors)
}

/// Every project-flat function signature, whichever file defines it — what a file's
/// calls compile against (D25: signatures and where they live, never bodies).
pub struct SignatureTable {
    entries: Vec<TableEntry>,
}

struct TableEntry {
    path: String,
    export: ExportedSignature,
}

impl SignatureTable {
    /// Builds the table from every file's exports, in file order. A name two files
    /// export is an error at the later definition, citing the first — which the table
    /// keeps.
    pub fn build<'a>(
        files: impl IntoIterator<Item = (&'a str, &'a [ExportedSignature])>,
    ) -> (Self, Vec<CompileError>) {
        let mut table = Self {
            entries: Vec::new(),
        };
        let mut errors = Vec::new();
        for (path, exports) in files {
            for export in exports {
                match table.entries.iter().find(|e| e.export.name == export.name) {
                    Some(existing) => errors.push(CompileError {
                        path: path.to_owned(),
                        span: export.name_span,
                        message: duplicate_message(
                            &export.name,
                            &existing.path,
                            existing.export.span,
                        ),
                    }),
                    None => table.entries.push(TableEntry {
                        path: path.to_owned(),
                        export: export.clone(),
                    }),
                }
            }
        }
        (table, errors)
    }

    fn position(&self, name: &str) -> Option<usize> {
        self.entries.iter().position(|e| &*e.export.name == name)
    }
}

/// Compiles one prepared file into its [`Unit`], resolving names against `table`. A
/// call to a function another file defines stays a symbolic import, and the signature
/// it was compiled against is recorded as one of the unit's facts.
///
/// # Errors
///
/// Every problem in the file, in source order.
pub fn compile_unit(
    prepared: &PreparedFile,
    table: &SignatureTable,
) -> Result<Unit, Vec<CompileError>> {
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
        table,
        errors: prepared.errors.clone(),
        ctxs: Vec::new(),
    };
    let entry = compiler.reserve(&prepared.path);
    debug_assert_eq!(entry, 0, "the entry is a unit's first function");
    for export in &prepared.exports {
        let func = compiler.reserve(&export.name);
        debug_assert_eq!(
            func, export.func,
            "exports are reserved in collection order"
        );
    }
    compiler.compile_functions(prepared.source.parse());
    compiler.compile_entry(&prepared.source, prepared.chunks.as_deref());
    compiler.resolve_import_operands();
    if compiler.errors.is_empty() {
        return Ok(compiler.unit);
    }
    let mut errors = compiler.errors;
    errors.sort_by_key(|error| (error.span.start, error.span.end));
    Err(errors)
}

/// The message a duplicate definition gets wherever it is found — the same words when
/// two files compiled together collide as when two units disagree at link time.
pub(crate) fn duplicate_message(name: &str, path: &str, span: Span) -> String {
    format!(
        "`{name}` is already defined in {path} at {}..{}",
        span.start, span.end
    )
}

fn ambient_message(name: &str) -> String {
    format!("`{name}` is provided by the shell and cannot be redefined")
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

/// How a name in an expression resolved.
enum Resolved {
    Local(u16),
    /// Index into the [`SignatureTable`]'s entries.
    Function(usize),
    Builtin(u32),
    Ambient,
}

/// The function being compiled: its code so far and its lexical scopes.
struct FnCtx {
    code: Vec<Op>,
    spans: Vec<Span>,
    n_locals: u16,
    /// Innermost last; each maps a name to its slot.
    scopes: Vec<Vec<(Rc<str>, u16)>>,
    /// `(slot in the enclosing function, slot here)` for every name closed over.
    captures: Vec<(u16, u16)>,
}

impl FnCtx {
    fn new() -> Self {
        Self {
            code: Vec::new(),
            spans: Vec::new(),
            n_locals: 0,
            scopes: vec![Vec::new()],
            captures: Vec::new(),
        }
    }

    fn lookup(&self, name: &str) -> Option<u16> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.iter().rev().find(|(n, _)| &**n == name))
            .map(|(_, slot)| *slot)
    }

    fn alloc(&mut self) -> u16 {
        let slot = self.n_locals;
        self.n_locals = slot.checked_add(1).expect("fewer than 65536 locals");
        slot
    }

    /// Binds `name` in the function-level scope to a slot that a capture fills.
    fn declare_capture(&mut self, name: Rc<str>, parent_slot: u16) -> u16 {
        let slot = self.alloc();
        self.scopes[0].push((name, slot));
        self.captures.push((parent_slot, slot));
        slot
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
    table: &'a SignatureTable,
    errors: Vec<CompileError>,
    /// The function being compiled, then every lambda open inside it, innermost last.
    ctxs: Vec<FnCtx>,
}

impl Compiler<'_> {
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
                .find(|e| &*e.name == name.text() && e.span == item.span())
            else {
                // Ambient-named or a duplicate: reported when the exports were
                // collected, and nothing to compile into.
                continue;
            };
            let (func, params) = (export.func, export.params.clone());
            self.ctxs.push(FnCtx::new());
            // Through `declare`, so that a repeated or ambient parameter name is
            // the same error in a signature as it is in a lambda.
            let tokens: Vec<SyntaxToken> = match &item {
                Item::FnBlock(f) => f.params(),
                Item::FnVoid(f) => f.params(),
                _ => unreachable!("only functions reach here"),
            }
            .map(|params| params.params().map(|p| required(p.name())).collect())
            .unwrap_or_default();
            for token in &tokens {
                self.declare(token);
            }
            match &item {
                Item::FnBlock(f) => {
                    let body = required(f.body());
                    self.emit(Op::Capture, f.span());
                    self.lower_block(body.items().collect(), false);
                    self.emit(Op::EndCapture, f.span());
                }
                Item::FnVoid(f) => self.expr(&required(f.value())),
                _ => unreachable!("only functions reach here"),
            }
            self.emit(Op::Return, item.span());
            let ctx = self.ctxs.pop().expect("the function's context");
            self.finish(func, ctx, params);
        }
    }

    fn compile_entry(&mut self, source: &Source, chunks: Option<&[String]>) {
        self.ctxs.push(FnCtx::new());
        let parse = source.parse();
        let items = parse.root().items().collect();
        match (source, chunks) {
            (Source::Document(doc), Some(chunks)) => {
                self.lower_document(items, &doc.prose, chunks);
            }
            (Source::Template(_), None) => self.lower_block(items, true),
            _ => unreachable!("a document has its chunks and a template has none"),
        }
        let span = parse.root().span();
        let null = self.constant(Value::Null);
        self.emit(Op::Const { index: null }, span);
        self.emit(Op::Return, span);
        let ctx = self.ctxs.pop().expect("the entry's context");
        self.finish(0, ctx, Vec::new());
    }

    /// Writes a finished context into the reserved function slot.
    fn finish(&mut self, func: u32, ctx: FnCtx, params: Vec<Rc<str>>) {
        debug_assert_eq!(ctx.code.len(), ctx.spans.len());
        let function = &mut self.unit.functions[func as usize];
        function.params = params;
        function.captures = ctx.captures.iter().map(|&(_, slot)| slot).collect();
        function.n_locals = ctx.n_locals;
        function.code = ctx.code;
        function.spans = ctx.spans;
    }

    /// The `func` operand for the table entry at `sig`: the local index when this file
    /// defines the function, an import-tagged one otherwise — recorded symbolically,
    /// with the signature consumed as a fact, the first time the name is used.
    fn func_operand(&mut self, sig: usize, span: Span) -> u32 {
        let table = self.table;
        let entry = &table.entries[sig];
        if entry.path == self.path {
            return entry.export.func;
        }
        let name = &entry.export.name;
        let index = match self.unit.imports.iter().position(|i| &i.name == name) {
            Some(index) => index,
            None => {
                self.unit.imports.push(ImportRef {
                    name: name.clone(),
                    span,
                });
                self.unit.facts.push(ImportedSignature {
                    name: name.clone(),
                    params: entry.export.params.clone(),
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
        self.errors.push(CompileError {
            path: self.path.to_owned(),
            span,
            message: message.into(),
        });
    }

    // --- Scopes ---------------------------------------------------------------------

    fn push_scope(&mut self) {
        self.ctx().scopes.push(Vec::new());
    }

    fn pop_scope(&mut self) {
        self.ctx().scopes.pop();
    }

    /// Reports `name` if it is one of the ambient records, which no binding may take.
    fn reject_ambient(&mut self, name: &SyntaxToken) -> bool {
        if AMBIENT.contains(&name.text()) {
            self.error(name.span(), ambient_message(name.text()));
            return true;
        }
        false
    }

    /// Binds `name` in the innermost scope. A name already bound in that same scope is
    /// an error — bindings are immutable, so a second `lem-let` cannot be an update.
    fn declare(&mut self, name: &SyntaxToken) -> u16 {
        self.reject_ambient(name);
        let scope = self.ctx().scopes.last().expect("a scope");
        if scope.iter().any(|(n, _)| &**n == name.text()) {
            self.error(
                name.span(),
                format!("`{}` is already bound here", name.text()),
            );
        }
        let slot = self.ctx().alloc();
        self.ctx()
            .scopes
            .last_mut()
            .expect("a scope")
            .push((Rc::from(name.text()), slot));
        slot
    }

    /// The function or builtin `name` names, or `None` if it names anything else — a
    /// local, an ambient record, or nothing at all.
    ///
    /// Unlike [`resolve`](Self::resolve) this touches no compiler state. The UFCS probe
    /// asks about a name it may not end up compiling as a call, and a lookup that
    /// allocated a capture slot on the way past an enclosing function would leave the
    /// slot behind whatever the answer was.
    fn peek_callable(&self, name: &str) -> Option<Resolved> {
        if self.ctxs.iter().any(|ctx| ctx.lookup(name).is_some()) {
            return None;
        }
        if let Some(sig) = self.table.position(name) {
            return Some(Resolved::Function(sig));
        }
        builtins::lookup(name).map(Resolved::Builtin)
    }

    fn resolve(&mut self, name: &str) -> Option<Resolved> {
        let level = self.ctxs.len() - 1;
        if let Some(slot) = self.resolve_local(level, name) {
            return Some(Resolved::Local(slot));
        }
        if let Some(sig) = self.table.position(name) {
            return Some(Resolved::Function(sig));
        }
        if let Some(id) = builtins::lookup(name) {
            return Some(Resolved::Builtin(id));
        }
        AMBIENT.contains(&name).then_some(Resolved::Ambient)
    }

    /// Finds `name` among the locals of the context at `level`, reaching into the
    /// enclosing contexts and recording a capture at each level crossed.
    fn resolve_local(&mut self, level: usize, name: &str) -> Option<u16> {
        if let Some(slot) = self.ctxs[level].lookup(name) {
            return Some(slot);
        }
        if level == 0 {
            return None;
        }
        let parent_slot = self.resolve_local(level - 1, name)?;
        Some(self.ctxs[level].declare_capture(Rc::from(name), parent_slot))
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
        let module = compile(files).map_err(|errors| {
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        })?;
        render(&module, entry, &[("page", Value::str("P"))]).map_err(|e| e.to_string())
    }

    #[test]
    fn refuses_a_file_that_did_not_parse_clean_with_its_parse_diagnostics() {
        let errors = compile(&[
            SourceFile::new("a.lem", "<lem-if x>"),
            SourceFile::new("b.lem", "<lem-bogus />"),
        ])
        .unwrap_err();
        let shown: Vec<String> = errors.iter().map(ToString::to_string).collect();
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
            SourceFile::new("lib.lem", "<lem-let secret = 1 />"),
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
            let (table, errors) = SignatureTable::build([
                ("lib.lem", lib.exports()),
                ("page.lem", page.exports()),
            ]);
            assert_eq!(errors, Vec::new());
            compile_unit(&page, &table).unwrap()
        };
        let unit = unit_against("<lem-fn greet(who: string): string = \"hi \" + who />");
        let edited = unit_against("<lem-fn greet(who: string): string = \"HI \" + who />");
        assert_eq!(unit, edited);
        assert_eq!(
            unit.facts,
            vec![ImportedSignature {
                name: Rc::from("greet"),
                params: vec![Rc::from("who")],
            }]
        );
        assert_eq!(unit.imports.len(), 1);
    }

    #[test]
    fn reports_a_broken_file_alongside_the_other_files_problems() {
        // One file's parse errors no longer suppress what is wrong elsewhere.
        let errors = compile(&[
            SourceFile::new("a.lem", "<lem-if x>"),
            SourceFile::new("b.lem", "<lem.missing>"),
        ])
        .unwrap_err();
        let shown: Vec<String> = errors.iter().map(ToString::to_string).collect();
        assert_eq!(shown.len(), 2, "{shown:?}");
        assert!(shown[0].starts_with("a.lem:"), "{shown:?}");
        assert_eq!(shown[1], "b.lem:5..12: undefined name `missing`");
    }

    #[test]
    fn cites_the_first_definition_when_a_function_is_redefined_in_another_file() {
        let files = [
            SourceFile::new("a.lem", "<lem-fn f(): int = 1 />"),
            SourceFile::new("b.lem", "x\n<lem-fn f(): int = 2 />"),
        ];
        assert_eq!(
            run_files(&files, "a.lem").unwrap_err(),
            "b.lem:10..11: `f` is already defined in a.lem at 0..23"
        );
    }

    #[test]
    fn cites_the_first_definition_when_a_function_is_redefined_in_the_same_file() {
        let errors = compile(&[SourceFile::new(
            "t.lem",
            "<lem-fn f(): int = 1 />\n<lem-fn f(): int = 2 />",
        )])
        .unwrap_err();
        assert_eq!(
            errors[0].to_string(),
            "t.lem:32..33: `f` is already defined in t.lem at 0..23"
        );
    }

    #[test]
    fn reports_every_problem_in_a_file_rather_than_the_first() {
        let errors = compile(&[SourceFile::new("t.lem", "<lem.a><lem.b>")]).unwrap_err();
        assert_eq!(errors.len(), 2);
        assert_eq!(errors[1].to_string(), "t.lem:12..13: undefined name `b`");
    }

    #[test]
    fn reports_problems_in_source_order_whichever_pass_found_them() {
        // The undefined name is found compiling the entry, the bad definition while
        // declaring — the later pass, but the earlier bytes.
        let errors = compile(&[SourceFile::new(
            "t.lem",
            "<lem.missing>\n<lem-fn page(): int = 1 />",
        )])
        .unwrap_err();
        let spans: Vec<u32> = errors.iter().map(|e| e.span.start).collect();
        assert_eq!(spans, [5, 22]);
    }

    #[test]
    fn reports_problems_in_the_order_the_files_were_given() {
        let errors = compile(&[
            SourceFile::new("b.lem", "<lem.x>"),
            SourceFile::new("a.lem", "<lem.y>"),
        ])
        .unwrap_err();
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
        let module = compile(&[SourceFile::new("t.lem", "<lem.\"a\" + \"a\">")]).unwrap();
        let strings = module
            .constants
            .iter()
            .filter(|c| matches!(c, Value::Str(s) if &**s == "a"))
            .count();
        assert_eq!(strings, 1);
    }

    #[test]
    fn compiles_a_lambda_to_a_function_named_for_what_it_is() {
        let module = compile(&[SourceFile::new("t.lem", "<lem-let f = |x| x />")]).unwrap();
        assert!(module.functions.iter().any(|f| f.name == "<lambda>"));
    }

    #[test]
    fn refuses_a_parameter_name_given_twice() {
        assert_eq!(
            run("<lem-fn f(a: int, a: int): int = a />").unwrap_err(),
            "t.lem:18..19: `a` is already bound here"
        );
        assert_eq!(
            run("<lem-let f = |a, a| a />").unwrap_err(),
            "t.lem:17..18: `a` is already bound here"
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
            run("<lem-let m = {a: 1, \"a\": 2} />").unwrap_err(),
            "t.lem:20..23: `a` is already given"
        );
    }

    #[test]
    fn leaves_no_capture_behind_when_a_ufcs_probe_finds_a_local() {
        let module = compile(&[SourceFile::new(
            "t.lem",
            "<lem-let name = 1 /><lem-let f = |o| o.name() />",
        )])
        .unwrap();
        let lambda = module
            .functions
            .iter()
            .find(|f| f.name == "<lambda>")
            .expect("the lambda");
        assert_eq!(lambda.captures, Vec::<u16>::new());
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

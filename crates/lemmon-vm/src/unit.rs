//! Per-file compiled units and the link step that joins them (D25, invariant 11).
//!
//! A [`Unit`] is one file's compiled output: its own constant pool (document prose
//! included) and its own functions, with every reference to another file's function left
//! symbolic in [`Unit::imports`]. [`link`] is a pass over plain data that concatenates
//! the pools, rewrites the indices, and resolves the symbols — producing the one
//! [`Module`] the VM runs, unchanged in shape. That split is what makes a unit a
//! function of its file's text plus the signatures it consumed ([`Unit::facts`]) and
//! nothing else: no file's bytecode depends on another file's body.

use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;

use lemmon_syntax::Span;

use crate::Value;
use crate::bytecode::{
    Declared, Document, Exported, Function, MarkKey, MarkPayload, Module, Op,
};
use crate::closure::{self, Closed, ReadClosure};
use crate::compile::{Compiled, Diagnostic, Runs, index_u32, refused_at_load};
use crate::decls::{DeclKind, Declaration, Decls, Shape, TypeFact};
use crate::document::CodePayload;
use crate::exports::ExportIndex;
use crate::host::{ClaimAnswer, Host};
use crate::plan::{
    BindingStep, Bindings, Fulfilled, Index, RecordPlan, SourceKind, Step, order,
};
use crate::types::Type;

/// One file's compiled output: self-contained except for the functions it names in
/// other files, which stay symbolic until [`link`] resolves them.
#[derive(Clone, Debug, PartialEq)]
pub struct Unit {
    /// The file this unit was compiled from.
    pub path: String,
    /// The file-local constant pool: literals, text runs, names, and rendered prose.
    pub constants: Vec<Value>,
    /// The file's functions; `functions[0]` is its entry. A `Call`, `Global`, or
    /// `MakeClosure` operand `>= functions.len()` names
    /// `imports[operand - functions.len()]` instead of a function here.
    pub functions: Vec<Function>,
    /// The foreign functions the bytecode references, symbolically.
    pub imports: Vec<ImportRef>,
    /// The top-level `lem-fn`s this file defines, as other files may call them.
    pub exports: Vec<ExportedSignature>,
    /// The top-level `lem-type`s and `lem-enum`s this file declares, as other files
    /// may name them.
    pub types: Vec<Declaration>,
    /// The compile-time facts this unit consumed: the foreign signatures its calls were
    /// compiled against. With the file's text and the other facts below — `type_refs`,
    /// `ambient_refs`, `asset_refs`, `claim_refs`, and `record` — these are the whole
    /// cache key (D25): an edit that changes none of them leaves this unit
    /// byte-identical.
    pub facts: Vec<ImportedSignature>,
    /// The second kind of fact: the foreign types this file's checking depended on,
    /// each with the declaration it was compiled against, at the first use that reached
    /// it — an annotation, an `extends`, or a consumed signature.
    pub type_refs: Vec<TypeRef>,
    /// The third: what the host bound each ambient value this file read to, where it
    /// first read it. The declaration behind such a type is a `TypeRef` like any
    /// other; this is the binding itself — that the host called `page` a `Page` (D30).
    pub ambient_refs: Vec<AmbientRef>,
    /// The fourth: every `asset("…")` this file resolved, with the URL the manifest
    /// gave it. `asset()` is answered while compiling (D10) and the URL folded into the
    /// bytecode, so a build that re-hashes that asset leaves this unit stale and every
    /// unit that never named it untouched — which is invariant 11 stated as a test.
    pub asset_refs: Vec<AssetRef>,
    /// The fifth: what the host's claims said about each mark name this file placed and
    /// each key it placed it under — a `mark()` call's, or a code block's language word
    /// (D42) — at the first place that asked.
    pub claim_refs: Vec<ClaimRef>,
    /// The record the host compiled this file as, which types its document value: a
    /// fact of the unit like the rest, and the reason two documents of different
    /// schemas cannot share one.
    pub record: Option<String>,
    /// The functions the loader runs for the records this file declares: each field's
    /// default (∆iCW). Part of this unit and no consumer's: a default is not part of
    /// a type's shape fact.
    pub defaults: Vec<DefaultFn>,
    /// The `by` indexes of the records this file declares: each the ambient field a
    /// string in data resolves through (∆eag). A unit's own, like `defaults`.
    pub indexes: Vec<IndexDecl>,
    /// The top-level bindings of this file when it is the one the host reads an
    /// ambient value from — what supplies each, in binding order. Empty elsewhere.
    pub bindings: Vec<BindingFn>,
    /// What each function whose body the loader may run reads and calls, by its index
    /// here: every top-level `lem-fn`, and the functions behind `defaults` and
    /// `bindings`. The link step closes these through calls to order the loader's
    /// work (∆HF0). Part of this unit alone, like `defaults`: a body is never a
    /// consumer's fact.
    pub uses: BTreeMap<u32, Uses>,
    /// Where the record was named — the `type:` key's value when the host said, else
    /// the front matter — for what the loader has to say about the record.
    pub record_span: Span,
    /// A document's prose as rendered, its blocks' output absent: what `body` is when
    /// the loader runs the document's defaults (D37). `None` for a template.
    pub body: Option<Rc<str>>,
    /// What this document's code blocks hand their producers, one per distinct block
    /// (D42): reported when the file compiles, and named by id in every render that marks
    /// one. Part of this unit and no consumer's, like `body`: the code is this file's text.
    pub payloads: Vec<CodePayload>,
}

/// One loader-run function a unit compiled for a field's default of a record it
/// declares.
#[derive(Clone, Debug, PartialEq)]
pub struct DefaultFn {
    pub record: Rc<str>,
    pub field: Rc<str>,
    /// The function's index in its unit; its reads and calls are in [`Unit::uses`].
    pub func: u32,
    /// The field's name in the declaration.
    pub span: Span,
}

/// One field's `by value.key` index, as declared: the map a string in data resolves
/// through is the field `key` of the ambient value `value`.
#[derive(Clone, Debug, PartialEq)]
pub struct IndexDecl {
    pub record: Rc<str>,
    pub field: Rc<str>,
    pub value: Rc<str>,
    pub key: Rc<str>,
    /// The field's name in the declaration.
    pub span: Span,
}

/// One top-level binding of the bindings file, as compiled.
#[derive(Clone, Debug, PartialEq)]
pub struct BindingFn {
    pub name: Rc<str>,
    pub supplied: Supplied,
    /// The binding's name in the file.
    pub span: Span,
}

/// How one binding of the bindings file gets its value, as compiled — what
/// [`Fulfilled`] is once the link step has closed the function's reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Supplied {
    /// A `from` binding: the host loads it.
    ByHost(SourceKind),
    /// An `= expr` or capture binding: the loader runs this function of the unit.
    ByFunction(u32),
}

/// What one function of a unit reads and calls — the notes the link step closes
/// through calls, so that a default calling a `lem-fn` that reads a sibling is ordered
/// after that sibling instead of faulting at load (∆HF0).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Uses {
    /// What is read, inside a lambda of the body as much as in the body itself.
    pub reads: Vec<Access>,
    /// The project functions the body names, with what was handed to each.
    pub calls: Vec<Call>,
}

/// One thing a function reads, as its own body has it — before the link step has
/// closed it through the calls that reach it, into what a plan carries: the fields
/// of `self` a step reads, the ambient values a computed binding reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Access {
    /// A field of `self`, the record a default is building.
    Own(Rc<str>),
    /// A field of the function's own parameter, by position: what a `lem-fn` reads
    /// off a record handed to it. Closed into the caller's own read of that field
    /// when the caller handed a root, dropped when it handed anything else.
    Param { index: u32, field: Rc<str> },
    /// A field of an ambient value: `site.photos`.
    Ambient { value: Rc<str>, field: Rc<str> },
    /// An ambient value read whole: `page`, handed on or returned. Orders a binding
    /// after the value like a field read does, and is how a default that reaches an
    /// ambient value through a call is found (D37, ∆eag).
    Whole(Rc<str>),
}

/// One call of a project function by name, written `f(…)`. A function named as a
/// value is not one: stored in a field or handed to a builtin, it is a value like any
/// other, and what it reads is not followed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Call {
    /// The function's project-flat name.
    pub name: Rc<str>,
    /// Per callee parameter, in declaration order, the caller's root passed whole —
    /// `None` for anything else: a field of one, a literal, a call's result.
    pub args: Vec<Option<Root>>,
}

/// A value whose fields a function's reads are counted off: the record a step is
/// building, one of a `lem-fn`'s parameters, or an ambient value. Read off directly,
/// by name — a local bound to one is not a root, so a field read through it is not
/// counted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Root {
    /// `self` of a default.
    Own,
    /// A parameter of a `lem-fn`, by position.
    Param(u32),
    /// A value the host binds, by name.
    Ambient(Rc<str>),
}

/// A foreign type a unit was compiled against: where this file first reached it, and
/// the declaration it consumed there.
#[derive(Clone, Debug, PartialEq)]
pub struct TypeRef {
    pub name: Rc<str>,
    pub span: Span,
    pub fact: TypeFact,
}

/// A host binding a unit was compiled against: the ambient name, where this file first
/// read it, and the type it had there.
#[derive(Clone, Debug, PartialEq)]
pub struct AmbientRef {
    pub name: Rc<str>,
    pub span: Span,
    pub ty: Type,
}

/// A manifest entry a file was compiled against: the logical path it asked for, the URL
/// it was given, and the literal it asked at.
#[derive(Clone, Debug, PartialEq)]
pub struct AssetRef {
    pub path: Rc<str>,
    pub url: Rc<str>,
    pub span: Span,
}

/// A mark name a file placed — and the key it placed it under, where it had one while
/// compiling — with what the host's claims said about the pair.
///
/// The answer was consumed then: the warning at an unclaimed name, the warning at a key
/// no producer covers, and the record the payload was checked against were all settled
/// while the file compiled. So a declaration that changes any of them for this name and
/// key leaves this unit stale — and one that changes them for another key leaves it
/// alone, which is what lets a highlighter add a language without recompiling every
/// document that holds a fence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaimRef {
    pub name: Rc<str>,
    /// Where the name is written: a `mark()` call's literal, or a code block's language
    /// word. What a diagnostic about the name or the payload points at.
    pub span: Span,
    /// The key this file asked about and where it is written, when it had one while
    /// compiling. A key a render computes has none, and is the handler's to check.
    pub key: Option<(Rc<str>, Span)>,
    pub answer: ClaimAnswer,
}

/// A symbolic reference to a function or type another file defines.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportRef {
    /// The project-flat name.
    pub name: Rc<str>,
    /// Where this file first uses the name — where an unresolved import points.
    pub span: Span,
}

/// One parameter of a signature: its name, which named arguments reorder against, and
/// its declared type, which arguments are checked against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Param {
    pub name: Rc<str>,
    pub ty: Type,
}

/// A top-level `lem-fn` a unit defines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportedSignature {
    /// The function's project-flat name.
    pub name: Rc<str>,
    /// The parameters, in declaration order.
    pub params: Vec<Param>,
    /// The declared return type: `html` for a block body.
    pub ret: Type,
    /// The function's index in its unit.
    pub func: u32,
    /// The name token in the definition — where a duplicate of it is reported, and what
    /// a duplicate's label points at.
    pub name_span: Span,
}

impl ExportedSignature {
    /// The function's type, as a value of it has.
    #[must_use]
    pub fn ty(&self) -> Type {
        Type::function(
            self.params.iter().map(|p| p.ty.clone()).collect(),
            self.ret.clone(),
        )
    }
}

/// A foreign signature a unit's calls were compiled against: the callable shape only,
/// never the body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportedSignature {
    /// The function's project-flat name.
    pub name: Rc<str>,
    /// The parameters, in declaration order.
    pub params: Vec<Param>,
    /// The declared return type.
    pub ret: Type,
}

/// Joins units into the [`Module`] the VM runs: constant pools concatenated, function
/// indices offset, imports resolved to the functions `index` says define them.
///
/// The index is the project's one answer to what a name means (`∆e5i`), so linking
/// derives nothing about names itself: it decides only where each unit's functions land.
/// An import the index has no definition for is an error pointing at the source that
/// names it, in the order the units were given; with any error there is no module. A
/// type a unit was compiled against is checked the same way: one the index no longer
/// declares, or declares differently, leaves the unit stale — its field reads and
/// assignability checks were made against the old shape — and the error names the
/// file to recompile.
///
/// # Panics
///
/// If the index names a file that is not among `units` — the caller is expected to link
/// every unit the index was offered.
pub fn link(units: &[Unit], index: &ExportIndex, host: &Host) -> Compiled<Module> {
    let mut diagnostics = Vec::new();
    let mut constant_offsets = Vec::with_capacity(units.len());
    let mut function_offsets = Vec::with_capacity(units.len());
    let (mut constants, mut functions) = (0u32, 0u32);
    for unit in units {
        constant_offsets.push(constants);
        function_offsets.push(functions);
        constants = add_u32(constants, index_u32(unit.constants.len()));
        functions = add_u32(functions, index_u32(unit.functions.len()));
    }

    let positions: BTreeMap<&str, usize> = units
        .iter()
        .enumerate()
        .map(|(position, unit)| (unit.path.as_str(), position))
        .collect();

    let resolved: Vec<Vec<Option<u32>>> = units
        .iter()
        .map(|unit| {
            // What the host binds each ambient name to is a fact like a signature: a
            // unit that read `page` as a `Page` has its field reads and assignability
            // checks made against that type, so a host that rebinds the name — or drops
            // it — leaves the unit stale rather than merely relinkable. Compared
            // against the host as *this file* saw it, since a file compiled as a record
            // reads the document value as that record.
            let seen = host.for_file(&unit.path, unit.record.as_deref());
            for reference in &unit.ambient_refs {
                let bound = seen.binding(&reference.name).map(|a| a.ty());
                match bound {
                    Some(ty) if ty == reference.ty => {}
                    Some(ty) => diagnostics.push(Diagnostic::error(
                        &unit.path,
                        reference.span,
                        format!(
                            "the host binds `{}` as a `{ty}` now, and this file was \
                             compiled against `{}`",
                            reference.name, reference.ty
                        ),
                    )),
                    None => diagnostics.push(Diagnostic::error(
                        &unit.path,
                        reference.span,
                        format!("the host no longer binds `{}`", reference.name),
                    )),
                }
            }
            // A manifest entry is the same kind of fact, and the one that moves most
            // often: every rebuild of an asset re-hashes its URL, and the URL is inside
            // this unit's bytecode. Compared against the declaration rather than the
            // file's view, since an asset is the host's and no file retypes one.
            for reference in &unit.asset_refs {
                match host.asset(&reference.path) {
                    Some(url) if url == &*reference.url => {}
                    Some(url) => diagnostics.push(Diagnostic::error(
                        &unit.path,
                        reference.span,
                        format!(
                            "the manifest maps `{}` to `{url}` now, and this file was \
                             compiled against `{}`",
                            reference.path, reference.url
                        ),
                    )),
                    None => diagnostics.push(Diagnostic::error(
                        &unit.path,
                        reference.span,
                        format!("`{}` is no longer in the manifest", reference.path),
                    )),
                }
            }
            // A claim is the host's as a manifest entry is, and what it said was
            // answered while the file compiled: the warning at the word, the warning at
            // a key, and the record a payload was checked against.
            for reference in &unit.claim_refs {
                let now = host.answer(
                    &reference.name,
                    reference.key.as_ref().map(|(key, _)| &**key),
                );
                if now == reference.answer {
                    continue;
                }
                if let Some((message, span)) = stale_claim(reference, &now) {
                    diagnostics.push(Diagnostic::error(&unit.path, span, message));
                }
            }
            for reference in &unit.type_refs {
                match index.lookup_type(&reference.name) {
                    Some(export) if export.decl.fact() == reference.fact => {}
                    Some(export) => diagnostics.push(
                        Diagnostic::error(
                            &unit.path,
                            reference.span,
                            stale_type_message(&export.decl.fact(), &reference.fact),
                        )
                        .with_label(
                            &export.path,
                            export.decl.name_span,
                            "now declared here",
                        ),
                    ),
                    None => diagnostics.push(Diagnostic::error(
                        &unit.path,
                        reference.span,
                        format!("unknown type `{}`", reference.name),
                    )),
                }
            }
            unit.imports
                .iter()
                .map(|import| {
                    match index.lookup(&import.name) {
                        Some(export) => {
                            let owner = positions[export.path.as_str()];
                            let export = &export.signature;
                            // The signature this unit was compiled against is one of its
                            // facts (D25). If the definition no longer matches it, the
                            // caller's bytecode is stale — named arguments were already
                            // reordered against the old parameter list, and every
                            // argument and result was checked against the old types —
                            // so this unit has to be recompiled, not relinked. Reporting
                            // it here is what tells the shell which file that is.
                            let fact = unit
                                .facts
                                .iter()
                                .find(|fact| fact.name == import.name)
                                .expect("an import records the signature it consumed");
                            if fact.params != export.params || fact.ret != export.ret {
                                diagnostics.push(
                                    Diagnostic::error(
                                        &unit.path,
                                        import.span,
                                        stale_signature_message(
                                            &import.name,
                                            (&fact.params, &fact.ret),
                                            (&export.params, &export.ret),
                                        ),
                                    )
                                    .with_label(
                                        &units[owner].path,
                                        export.name_span,
                                        "now defined here",
                                    ),
                                );
                            }
                            Some(add_u32(function_offsets[owner], export.func))
                        }
                        None => {
                            diagnostics.push(Diagnostic::error(
                                &unit.path,
                                import.span,
                                format!("undefined name `{}`", import.name),
                            ));
                            None
                        }
                    }
                })
                .collect()
        })
        .collect();
    // An unresolved import is always an error, so past this point every entry of
    // `resolved` is `Some`; a warning alone builds the module like a clean link does.
    if diagnostics.iter().any(Diagnostic::is_error) {
        return Compiled::new(None, diagnostics);
    }

    // Every type the project declares, whichever unit it came from: a type test asks
    // the table at run time and the loader validates against it, so the table is the
    // module's and not any unit's.
    let declarations = index
        .types()
        .map(|export| Declared {
            file: index_u32(positions[export.path.as_str()]),
            decl: export.decl.clone(),
        })
        .collect();
    let closure = ReadClosure::new(units, index, &positions);
    let places = Places {
        units,
        index,
        host,
        positions: &positions,
        function_offsets: &function_offsets,
        closure: &closure,
    };
    let records = places.record_plans(&mut diagnostics);
    let bindings = places.bindings_plans(&records, &mut diagnostics);
    // A cycle among what the loader runs is as fatal as an unresolved import: the
    // program could never load.
    if diagnostics.iter().any(Diagnostic::is_error) {
        return Compiled::new(None, diagnostics);
    }
    let exports = index
        .exports()
        .map(|export| {
            let owner = positions[export.path.as_str()];
            Exported {
                name: export.signature.name.clone(),
                func: add_u32(function_offsets[owner], export.signature.func),
                ty: export.signature.ty(),
            }
        })
        .collect();
    let mut module = Module {
        declarations,
        exports,
        records,
        bindings,
        ..Module::default()
    };
    let mut payload_ids: HashSet<Rc<str>> = HashSet::new();
    for (position, unit) in units.iter().enumerate() {
        let (constant_offset, function_offset) =
            (constant_offsets[position], function_offsets[position]);
        let locals = index_u32(unit.functions.len());
        let func = |func: u32| {
            if func < locals {
                add_u32(function_offset, func)
            } else {
                resolved[position][(func - locals) as usize].expect("resolved above")
            }
        };
        module.files.push(unit.path.clone());
        module.entries.push(function_offset);
        module
            .documents
            .push(unit.body.as_ref().map(|body| Document {
                record: unit.record.as_deref().map(Rc::from),
                record_span: unit.record_span,
                body: body.clone(),
            }));
        // Two documents writing the same block report one payload between them: the id
        // is the content, and the host keys what its producer made on it. Kept by a set,
        // since a relink walks every unit's payloads and must stay cheaper than a compile.
        for payload in &unit.payloads {
            if payload_ids.insert(payload.id.clone()) {
                module.payloads.push(payload.clone());
            }
        }
        module.constants.extend(unit.constants.iter().cloned());
        for function in &unit.functions {
            let mut function = function.clone();
            function.file = index_u32(position);
            for op in &mut function.code {
                match op {
                    Op::Const { index } | Op::EmitText { index } => {
                        *index = add_u32(constant_offset, *index);
                    }
                    Op::Ambient { name }
                    | Op::GetField { name }
                    | Op::OptField { name }
                    | Op::CallDynamic { name, .. }
                    | Op::MakeRecord { name, .. }
                    | Op::TypeTest { name }
                    | Op::Collect { name } => {
                        *name = add_u32(constant_offset, *name);
                    }
                    // Three operands rather than one: a mark's key and its payload name
                    // this file's constants as its name does.
                    Op::Mark { name, key, payload } => {
                        *name = add_u32(constant_offset, *name);
                        match key {
                            MarkKey::Const { index } | MarkKey::Field { index } => {
                                *index = add_u32(constant_offset, *index);
                            }
                            MarkKey::None => {}
                        }
                        match payload {
                            MarkPayload::Reported { index } => {
                                *index = add_u32(constant_offset, *index);
                            }
                            MarkPayload::None | MarkPayload::Popped => {}
                        }
                    }
                    Op::Global { func: target }
                    | Op::Call { func: target, .. }
                    | Op::MakeClosure { func: target, .. } => *target = func(*target),
                    // Spelled out rather than `_`: an op that names a unit-local table
                    // and is missed here reads another file's constants or functions,
                    // and only in a module with more than one file — which no
                    // single-file corpus case ever builds. A new op must be placed.
                    Op::Local { .. }
                    | Op::StoreLocal { .. }
                    | Op::Index
                    | Op::CallValue { .. }
                    | Op::CallBuiltin { .. }
                    | Op::MakeList { .. }
                    | Op::MakeMap { .. }
                    | Op::Jump { .. }
                    | Op::JumpIfFalse { .. }
                    | Op::JumpUnlessNull { .. }
                    | Op::JumpIfFalseKeep { .. }
                    | Op::JumpIfTrueKeep { .. }
                    | Op::IterInit { .. }
                    | Op::IterNext { .. }
                    | Op::Neg
                    | Op::Not
                    | Op::Add
                    | Op::Sub
                    | Op::Mul
                    | Op::Div
                    | Op::Rem
                    | Op::Eq
                    | Op::Ne
                    | Op::Lt
                    | Op::Le
                    | Op::Gt
                    | Op::Ge
                    | Op::In
                    | Op::Emit
                    | Op::Capture
                    | Op::EndCapture
                    | Op::Return => {}
                }
            }
            module.functions.push(function);
        }
    }
    Compiled::new(Some(module), diagnostics)
}

fn add_u32(a: u32, b: u32) -> u32 {
    a.checked_add(b).expect("fewer than 2^32 items")
}

/// Where each unit's functions land in the module — what turns a unit's loader-run
/// functions into the module's plans.
struct Places<'a> {
    units: &'a [Unit],
    index: &'a ExportIndex,
    host: &'a Host,
    positions: &'a BTreeMap<&'a str, usize>,
    function_offsets: &'a [u32],
    /// Every project function's reads closed through its calls, for the steps' own
    /// closing (∆HF0).
    closure: &'a ReadClosure,
}

/// A step with its reads closed through its calls — kept together until the plan is
/// ordered, so that a cycle can name the call a read came through.
#[derive(Clone)]
struct Planned<T> {
    step: T,
    closed: Vec<Closed>,
}

/// One dependency of a binding on a sibling: for a computed binding, a read of its
/// own, `via` the call it came through if it did; for a `from` binding, a lookup a
/// record its value may hold makes while it loads, `through` that record's index
/// naming the sibling. One or the other, never both.
struct Edge<'a> {
    to: usize,
    via: Option<Rc<str>>,
    through: Option<(&'a Rc<str>, &'a Index)>,
}

impl Places<'_> {
    /// A unit-local function index, in the module.
    fn func(&self, position: usize, func: u32) -> u32 {
        add_u32(self.function_offsets[position], func)
    }

    /// The loader's plan for every declared record — the bindings record included,
    /// whose plan is empty since its fields are bindings. Each field's default and
    /// index come from the nearest declaration in the chain that gives one: a
    /// redeclaration changes the default (D32), and one that gives none leaves the
    /// ancestor's in force. The steps are relocated into the module and ordered by
    /// what each reads through `self`, the calls it makes followed (∆HF0); a field
    /// that reads itself through others is a link error at the field, since no order
    /// could run it, and so is one that reaches an ambient value through a call,
    /// since a default reads only the record it builds (∆eag).
    fn record_plans(&self, diagnostics: &mut Vec<Diagnostic>) -> Vec<RecordPlan> {
        let mut plans = Vec::new();
        for export in self.index.types() {
            let name = &export.decl.name;
            if export.decl.kind() != DeclKind::Record {
                continue;
            }
            let mut steps: Vec<Planned<Step>> = Vec::new();
            let mut indexes: Vec<Index> = Vec::new();
            for decl in self.index.chain(name) {
                let Shape::Record { fields, .. } = &decl.shape else {
                    continue;
                };
                let owner = self
                    .index
                    .lookup_type(&decl.name)
                    .expect("a chain walks declared types");
                let position = self.positions[owner.path.as_str()];
                let unit = &self.units[position];
                // A default is refused once, where it is declared; a subtype inherits
                // a default already refused.
                let refuse = decl.name == *name;
                let page_backed = self.host.document_type().is_some_and(|document| {
                    document == decl.name || self.index.extends(&decl.name, &document)
                });
                for field in fields {
                    let of_field = |record: &Rc<str>, name: &Rc<str>| {
                        *record == decl.name && *name == field.name
                    };
                    if !steps.iter().any(|planned| planned.step.field == field.name)
                        && let Some(row) = unit
                            .defaults
                            .iter()
                            .find(|row| of_field(&row.record, &row.field))
                    {
                        let closed = self.closure.step(&unit.uses[&row.func]);
                        if refuse {
                            diagnostics.extend(self.refuse_reach(
                                &closed,
                                &unit.path,
                                row.span,
                                &row.field,
                                Runs::Default { page_backed },
                            ));
                        }
                        steps.push(Planned {
                            step: Step {
                                field: row.field.clone(),
                                func: self.func(position, row.func),
                                reads: closure::own_reads(&closed),
                                file: index_u32(position),
                                span: row.span,
                            },
                            closed,
                        });
                    }
                    if !indexes.iter().any(|index| index.field == field.name)
                        && let Some(row) = unit
                            .indexes
                            .iter()
                            .find(|row| of_field(&row.record, &row.field))
                    {
                        indexes.push(Index {
                            field: row.field.clone(),
                            value: row.value.clone(),
                            key: row.key.clone(),
                            file: index_u32(position),
                            span: row.span,
                        });
                    }
                }
            }
            let deps = |i: usize| -> Vec<usize> {
                steps[i]
                    .step
                    .reads
                    .iter()
                    .filter_map(|field| steps.iter().position(|p| p.step.field == *field))
                    .collect()
            };
            let steps = ordered(&steps, deps, |cycle| {
                let first = &steps[cycle[0]].step;
                diagnostics.push(Diagnostic::error(
                    &self.units[first.file as usize].path,
                    first.span,
                    format!(
                        "`{}` depends on itself: {}",
                        first.field,
                        chain(cycle, |a, b| {
                            let (a, b) = (&steps[a], &steps[b].step.field);
                            let via = closure::via(
                                &a.closed,
                                |access| matches!(access, Access::Own(field) if field == b),
                            );
                            edge(&a.step.field, "reads", b, via)
                        })
                    ),
                ));
            });
            plans.push(RecordPlan {
                name: name.clone(),
                steps: steps.into_iter().map(|planned| planned.step).collect(),
                indexes,
            });
        }
        plans
    }

    /// The refusals of a function the loader runs as `runs` that reaches an ambient
    /// value it may not read ([`refused_at_load`]) — a field of it, or the value whole
    /// — one per value, at `span` in `path`, for the field or binding `subject`. Read
    /// directly, the compiler refused it where it stood, in the same words, and no
    /// program with that error links; through a call, only the link step can see it,
    /// and names the call (∆HF0).
    fn refuse_reach(
        &self,
        closed: &[Closed],
        path: &str,
        span: Span,
        subject: &str,
        runs: Runs,
    ) -> Vec<Diagnostic> {
        let mut refused: Vec<&Rc<str>> = Vec::new();
        closed
            .iter()
            .filter_map(|closed| {
                let (Access::Ambient { value, .. } | Access::Whole(value)) = &closed.access
                else {
                    return None;
                };
                let why = refused_at_load(runs, self.host.document(), value)?;
                if refused.contains(&value) {
                    return None;
                }
                refused.push(value);
                let how = match &closed.via {
                    Some(via) => format!("reaches `{value}` through `{via}`"),
                    None => format!("reads `{value}`"),
                };
                Some(Diagnostic::error(
                    path,
                    span,
                    format!("`{subject}` {how}, but {why}"),
                ))
            })
            .collect()
    }

    /// A plan per ambient value the host reads from a file the program has, in the
    /// host's order.
    fn bindings_plans(
        &self,
        records: &[RecordPlan],
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Vec<Bindings> {
        let plans: Vec<Bindings> = self
            .host
            .bindings_files()
            .filter_map(|(value, file)| {
                self.bindings_plan(value, file, records, diagnostics)
            })
            .collect();
        // The host fulfils a `from` binding by its name alone (D37), so one name may
        // stand for one binding across every file the host reads bindings from.
        let mut loaded: Vec<(&Bindings, &BindingStep)> = Vec::new();
        for plan in &plans {
            for step in &plan.steps {
                if !matches!(step.fulfilled, Fulfilled::ByHost(_)) {
                    continue;
                }
                match loaded.iter().find(|(_, first)| first.name == step.name) {
                    Some((first_plan, first)) => diagnostics.push(
                        Diagnostic::error(
                            &self.units[plan.file as usize].path,
                            step.span,
                            format!(
                                "`{}` is already loaded, for `{}`: the host fulfils a \
                                 `from` binding by name, so no two files it reads bindings \
                                 from may load one called `{}`",
                                step.name, first_plan.value, step.name
                            ),
                        )
                        .with_label(
                            &self.units[first_plan.file as usize].path,
                            first.span,
                            "first loaded here",
                        ),
                    ),
                    None => loaded.push((plan, step)),
                }
            }
        }
        plans
    }

    /// The plan for the bindings file `file`, read as `value`: its bindings in load
    /// order. A computed binding follows the siblings it reads, the calls it makes
    /// followed (∆HF0); a `from` binding follows every sibling the records its value
    /// may hold look a string up in while they load — through `extends` and through
    /// field types, since a subtype's or a nested record's index resolves too. A
    /// binding that needs itself is a link error at the binding, and so is a
    /// computed one that reaches the host's document value through a call. `None`
    /// when the file is not in the program.
    fn bindings_plan(
        &self,
        value: &str,
        file: &str,
        records: &[RecordPlan],
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<Bindings> {
        let &position = self.positions.get(file)?;
        let unit = &self.units[position];
        let steps: Vec<Planned<BindingStep>> = unit
            .bindings
            .iter()
            .map(|binding| {
                let (fulfilled, closed) = match &binding.supplied {
                    Supplied::ByFunction(func) => {
                        let closed = self.closure.step(&unit.uses[func]);
                        diagnostics.extend(self.refuse_reach(
                            &closed,
                            &unit.path,
                            binding.span,
                            &binding.name,
                            Runs::Binding,
                        ));
                        let fulfilled = Fulfilled::Computed {
                            func: self.func(position, *func),
                            reads: closure::ambient_values(&closed),
                        };
                        (fulfilled, closed)
                    }
                    Supplied::ByHost(kind) => (Fulfilled::ByHost(kind.clone()), Vec::new()),
                };
                Planned {
                    step: BindingStep {
                        name: binding.name.clone(),
                        fulfilled,
                        span: binding.span,
                    },
                    closed,
                }
            })
            .collect();
        let sibling = |field: &str| {
            steps
                .iter()
                .position(|planned| &*planned.step.name == field)
        };
        // Only a read of this value's own field orders its bindings; the value whole
        // is the record so far, and another value's field is the loader's order.
        let of_value = |closed: &Closed| match &closed.access {
            Access::Ambient { value: read, field } if **read == *value => sibling(field),
            Access::Ambient { .. } | Access::Whole(_) => None,
            Access::Own(_) | Access::Param { .. } => {
                unreachable!("a binding reads off ambient values alone")
            }
        };
        let edges: Vec<Vec<Edge<'_>>> = steps
            .iter()
            .map(|planned| match &planned.step.fulfilled {
                Fulfilled::Computed { .. } => planned
                    .closed
                    .iter()
                    .filter_map(|closed| {
                        Some(Edge {
                            to: of_value(closed)?,
                            via: closed.via.clone(),
                            through: None,
                        })
                    })
                    .collect(),
                Fulfilled::ByHost(_) => {
                    let Some(ty) = self.index.field(file, &planned.step.name) else {
                        return Vec::new();
                    };
                    self.records_reachable(&ty)
                        .iter()
                        .filter_map(|record| records.iter().find(|r| r.name == *record))
                        .flat_map(|record| {
                            record.indexes.iter().filter_map(move |index| {
                                if *index.value != *value {
                                    return None;
                                }
                                Some(Edge {
                                    to: sibling(&index.key)?,
                                    via: None,
                                    through: Some((&record.name, index)),
                                })
                            })
                        })
                        .collect()
                }
            })
            .collect();
        let deps =
            |i: usize| -> Vec<usize> { edges[i].iter().map(|edge| edge.to).collect() };
        let steps = ordered(&steps, deps, |cycle| {
            let first = &steps[cycle[0]].step;
            let mut diagnostic = Diagnostic::error(
                &unit.path,
                first.span,
                format!(
                    "`{}` cannot be loaded before itself: {}",
                    first.name,
                    chain(cycle, |a, b| {
                        let edge = edges[a]
                            .iter()
                            .find(|edge| edge.to == b)
                            .expect("a cycle runs along dependencies");
                        needs(&steps[a].step.name, &steps[b].step.name, edge)
                    })
                ),
            );
            // The first lookup a record makes on the cycle is where the author looks.
            let through =
                cycle
                    .iter()
                    .zip(cycle.iter().cycle().skip(1))
                    .find_map(|(&a, &b)| {
                        edges[a]
                            .iter()
                            .find(|edge| edge.to == b)
                            .and_then(|edge| edge.through.as_ref())
                    });
            if let Some((_, index)) = through {
                diagnostic = diagnostic.with_label(
                    &self.units[index.file as usize].path,
                    index.span,
                    "looked up here",
                );
            }
            diagnostics.push(diagnostic);
        });
        Some(Bindings {
            value: Rc::from(value),
            record: Rc::from(file),
            file: index_u32(position),
            steps: steps.into_iter().map(|planned| planned.step).collect(),
        })
    }

    /// Every declared record a value of type `ty` may hold: the records it names, the
    /// records extending those (a document may be any subtype of its collection's
    /// element type), and the records their fields name, however deep.
    fn records_reachable(&self, ty: &Type) -> Vec<Rc<str>> {
        let mut pending = Vec::new();
        ty.named_types(&mut pending);
        let mut seen: Vec<Rc<str>> = Vec::new();
        while let Some(name) = pending.pop() {
            if seen.contains(&name) || self.index.kind(&name) != Some(DeclKind::Record) {
                continue;
            }
            seen.push(name.clone());
            for export in self.index.types() {
                if self.index.extends(&export.decl.name, &name) {
                    pending.push(export.decl.name.clone());
                }
            }
            for (_, field_ty) in self.index.fields(&name) {
                field_ty.named_types(&mut pending);
            }
        }
        seen
    }
}

/// `items` in the order `deps` admits, or nothing once `report` has been told each
/// cycle — the diagnostics it pushes are what stop the link.
fn ordered<T: Clone>(
    items: &[T],
    deps: impl Fn(usize) -> Vec<usize>,
    mut report: impl FnMut(&[usize]),
) -> Vec<T> {
    match order(items.len(), deps) {
        Ok(ordered) => ordered.into_iter().map(|i| items[i].clone()).collect(),
        Err(cycles) => {
            for cycle in cycles {
                report(&cycle);
            }
            Vec::new()
        }
    }
}

/// A cycle as a sentence, one link per member: "`a` reads `b`, `b` reads `a`".
fn chain(cycle: &[usize], link: impl Fn(usize, usize) -> String) -> String {
    let links: Vec<String> = cycle
        .iter()
        .zip(cycle.iter().cycle().skip(1))
        .map(|(&a, &b)| link(a, b))
        .collect();
    links.join(", ")
}

/// One link of a cycle: "`a` reads `b`", naming the call the read came through when
/// it came through one, since that is the line the author has to look at.
fn edge(a: &str, verb: &str, b: &str, via: Option<&Rc<str>>) -> String {
    match via {
        Some(via) => format!("`{a}` {verb} `{b}` through `{via}`"),
        None => format!("`{a}` {verb} `{b}`"),
    }
}

/// One link of a cycle among bindings: what `a` needs `b` for — a read of its own,
/// through a call or not, or a lookup a record its value holds makes while `a`
/// loads, which names that record's field, since the binding itself says nothing of
/// `b`.
fn needs(a: &str, b: &str, edge: &Edge<'_>) -> String {
    match (&edge.through, &edge.via) {
        (Some((record, index)), _) => format!(
            "`{a}` needs `{b}`, which `{record}.{}` looks up while `{a}` loads",
            index.field
        ),
        (None, Some(via)) => format!("`{a}` needs `{b}` through `{via}`"),
        (None, None) => format!("`{a}` needs `{b}`"),
    }
}

/// What a caller is told when the function it was compiled against has changed shape
/// under it: both signatures, so the difference is visible without opening the callee.
fn stale_signature_message(
    name: &str,
    compiled: (&[Param], &Type),
    defined: (&[Param], &Type),
) -> String {
    format!(
        "`{name}` is now `{}`, but this file was compiled against `{}` — recompile it",
        signature(name, defined.0, defined.1),
        signature(name, compiled.0, compiled.1)
    )
}

/// Why `reference` is stale under the answer the declaration gives `now`, and where to
/// say so: at the key where the key's coverage moved, at the name otherwise.
///
/// The comparison is ordered as a reader would ask it — whether the name is handled at
/// all, then whether this key is covered, then what the payload is — and the first
/// difference is the one reported, so one changed claim costs one diagnostic. `None`
/// where the two answers are equal, which the caller has already ruled out.
fn stale_claim(reference: &ClaimRef, now: &ClaimAnswer) -> Option<(String, Span)> {
    let name = &reference.name;
    let at_name = |message: String| Some((message, reference.span));
    let was = &reference.answer;
    match (was, now) {
        (ClaimAnswer::Claimed { .. }, ClaimAnswer::Claimed { .. }) => {}
        (ClaimAnswer::Claimed { .. }, _) => {
            return at_name(format!(
                "no handler claims `{name}` now, and this file was compiled where one did"
            ));
        }
        (_, ClaimAnswer::Claimed { .. }) => {
            return at_name(format!(
                "a handler claims `{name}` now, and this file was compiled where none did"
            ));
        }
        (ClaimAnswer::Undeclared, _) => {
            return at_name(format!(
                "the host declares handlers now, and this file was compiled against \
                 none, so nothing was said about `{name}`"
            ));
        }
        _ => {
            return at_name(format!(
                "the host declares no handlers now, and this file was compiled against \
                 the ones it did, which left `{name}` unclaimed"
            ));
        }
    }
    let (
        ClaimAnswer::Claimed {
            key: was_key,
            payload: was_payload,
            keyed_by: was_keyed_by,
        },
        ClaimAnswer::Claimed {
            key: now_key,
            payload: now_payload,
            keyed_by: now_keyed_by,
        },
    ) = (was, now)
    else {
        unreachable!("both answers are claims here");
    };
    if was_key != now_key {
        // A key is only asked about where the file wrote one, so there is a span for it.
        let (key, span) = reference
            .key
            .as_ref()
            .expect("a key answer comes of asking about a key");
        let message = match (was_key, now_key) {
            (_, Some(true)) => format!(
                "a producer claims `{key}` for `{name}` now, and this file was compiled \
                 where none did"
            ),
            (_, Some(false)) => format!(
                "no producer claims `{key}` for `{name}` now, and this file was compiled \
                 where one did"
            ),
            _ => format!(
                "the handlers for `{name}` claim no keys now, and this file was compiled \
                 against the keys they did"
            ),
        };
        return Some((message, *span));
    }
    if was_payload != now_payload {
        return at_name(match (was_payload, now_payload) {
            (Some(was), Some(now)) => format!(
                "the handler for `{name}` takes a `{now}` payload now, and this file was \
                 compiled against `{was}`"
            ),
            (None, Some(now)) => format!(
                "the handler for `{name}` takes a `{now}` payload now, and this file was \
                 compiled where it took none"
            ),
            (Some(was), None) => format!(
                "the handler for `{name}` takes no payload now, and this file was \
                 compiled against a `{was}`"
            ),
            (None, None) => unreachable!("the payloads differ"),
        });
    }
    at_name(match (was_keyed_by, now_keyed_by) {
        (Some(was), Some(now)) => format!(
            "the handler for `{name}` keys its payload by `{now}` now, and this file was \
             compiled against `{was}`"
        ),
        (None, Some(now)) => format!(
            "the handler for `{name}` keys its payload by `{now}` now, and this file was \
             compiled where it keyed by nothing"
        ),
        (Some(was), None) => format!(
            "the handler for `{name}` keys its payload by nothing now, and this file was \
             compiled against `{was}`"
        ),
        (None, None) => unreachable!("an answer that differs in nothing is not stale"),
    })
}

/// What a consumer is told when the type it was compiled against has changed shape
/// under it: both shapes, so the difference is visible without opening the declaration.
fn stale_type_message(declared: &TypeFact, compiled: &TypeFact) -> String {
    format!(
        "`{}` is now `{}`, but this file was compiled against `{}` — recompile it",
        declared.name,
        declared.text(),
        compiled.text()
    )
}

/// A signature the way it is declared: `name(a: string, b: int): html`.
fn signature(name: &str, params: &[Param], ret: &Type) -> String {
    let params: Vec<String> = params
        .iter()
        .map(|p| format!("{}: {}", p.name, p.ty))
        .collect();
    format!("{name}({}): {ret}", params.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::{compile_unit, prepare};
    use crate::host::{Ambient, Claim};
    use crate::testing::at;
    use crate::{SourceFile, Value, render};

    /// The host every test below compiles against unless it needs another: the three
    /// names the standard shell binds, none of them typed.
    fn untyped_host() -> Host {
        Host::untyped(["site", "page", "build"])
    }

    /// Compiles the file at `path` into a unit against an index of every given file's
    /// exports.
    fn unit_of(path: &str, index_of: &[(&str, &str)]) -> Unit {
        let prepared: Vec<_> = index_of
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
        let this = prepared
            .iter()
            .find(|p| p.path() == path)
            .expect("the file compiles against an index that includes it");
        compile_unit(this, &index, &untyped_host()).expect_output()
    }

    /// Compiles the file at `path` against `host`, and against an index of every given
    /// file's exports.
    fn unit_against(host: &Host, path: &str, files: &[(&str, &str)]) -> Unit {
        let prepared: Vec<_> = files
            .iter()
            .map(|(path, text)| prepare(&SourceFile::new(*path, *text), host).unwrap())
            .collect();
        let mut index = ExportIndex::new();
        for p in &prepared {
            index.offer(p.path(), p.exports());
            index.offer_types(p.path(), p.types());
        }
        let this = prepared
            .iter()
            .find(|p| p.path() == path)
            .expect("the file compiles against an index that includes it");
        compile_unit(this, &index, host).expect_output()
    }

    /// Links `units` against the index they make, asserting there is no module and
    /// returning what was reported.
    fn link_errors(units: &[Unit]) -> Vec<Diagnostic> {
        let linked = link(units, &ExportIndex::of(units), &untyped_host());
        assert!(
            linked.output.is_none(),
            "linked despite {:?}",
            linked.diagnostics
        );
        linked.diagnostics
    }

    #[test]
    fn refuses_to_link_an_import_no_unit_exports() {
        // The unit compiled against a table that knows lib.lem's function, then linked
        // without lib.lem — the shape of a stale unit after a file disappears.
        let page = unit_of(
            "page.lem",
            &[
                ("lib.lem", "<lem-fn f(): int = 1 />"),
                ("page.lem", "<lem.f()>"),
            ],
        );
        let errors = link_errors(&[page]);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "page.lem");
        assert!(
            errors[0].to_string().contains("undefined name `f`"),
            "{errors:?}"
        );
    }

    #[test]
    fn refuses_to_link_a_unit_whose_callee_changed_shape_under_it() {
        // page.lem compiled against `pair(a, b)` — its named arguments are already in
        // that order — while lib.lem now defines `pair(b, a)`. Resolving by name alone
        // would link cleanly and silently swap the arguments.
        const PAGE: &str = "<lem.pair(b: \"B\", a: \"A\")>";
        const LIB: &str = "<lem-fn pair(b: string, a: string): string = a + b />";
        let page = unit_of(
            "page.lem",
            &[
                (
                    "lib.lem",
                    "<lem-fn pair(a: string, b: string): string = a + b />",
                ),
                ("page.lem", PAGE),
            ],
        );
        let lib = unit_of("lib.lem", &[("lib.lem", LIB)]);
        let errors = link_errors(&[lib, page]);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "page.lem");
        assert_eq!(
            errors[0].to_string(),
            format!(
                "page.lem:{}: `pair` is now `pair(b: string, a: string): string`, but \
                 this file was compiled against `pair(a: string, b: string): string` — \
                 recompile it\n  lib.lem:{}: now defined here",
                at(PAGE, "pair(b: \"B\", a: \"A\")"),
                at(LIB, "pair")
            )
        );
    }

    #[test]
    fn refuses_to_link_a_unit_whose_callee_changed_a_type_under_it() {
        // The caller checked its argument against `int` and printed the result as a
        // `string`; both were facts, and either changing makes the unit stale.
        let against = |lib: &str| {
            unit_of("page.lem", &[("lib.lem", lib), ("page.lem", "<lem.f(1)>")])
        };
        let page = against("<lem-fn f(a: int): string = \"s\" />");
        let lib = unit_of("lib.lem", &[("lib.lem", "<lem-fn f(a: int): int = a />")]);
        let errors = link_errors(&[lib, page.clone()]);
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0].message.contains("`f` is now `f(a: int): int`"),
            "{errors:?}"
        );
        let lib = unit_of(
            "lib.lem",
            &[("lib.lem", "<lem-fn f(a: any): string = \"s\" />")],
        );
        let errors = link_errors(&[lib, page]);
        assert!(
            errors[0].message.contains("`f` is now `f(a: any): string`"),
            "{errors:?}"
        );
    }

    #[test]
    fn refuses_to_link_a_unit_whose_callee_changed_arity_under_it() {
        let page = unit_of(
            "page.lem",
            &[
                ("lib.lem", "<lem-fn greet(who: string): string = who />"),
                ("page.lem", "<lem.greet(\"you\")>"),
            ],
        );
        let lib = unit_of(
            "lib.lem",
            &[(
                "lib.lem",
                "<lem-fn greet(who: string, loudly: bool): string = who />",
            )],
        );
        let errors = link_errors(&[lib, page]);
        assert_eq!(errors[0].path, "page.lem");
        assert!(
            errors[0]
                .message
                .contains("`greet(who: string, loudly: bool): string`"),
            "{errors:?}"
        );
    }

    #[test]
    fn links_a_unit_whose_callee_only_changed_its_body() {
        // The other half of the rule: a body edit is exactly what units exist to
        // survive, so relinking against it must stay clean (invariant 11).
        let page = unit_of(
            "page.lem",
            &[
                (
                    "lib.lem",
                    "<lem-fn greet(who: string): string = \"hi \" + who />",
                ),
                ("page.lem", "<lem.greet(\"you\")>"),
            ],
        );
        let lib = unit_of(
            "lib.lem",
            &[(
                "lib.lem",
                "<lem-fn greet(who: string): string = \"HELLO \" + who />",
            )],
        );
        let units = [lib, page];
        let module =
            link(&units, &ExportIndex::of(&units), &untyped_host()).expect_output();
        assert_eq!(
            render(&module, "page.lem", &[("page", Value::Null)])
                .unwrap()
                .html,
            "HELLO you"
        );
    }

    #[test]
    fn relocates_a_dynamic_calls_name_into_the_linked_constant_pool() {
        // `x.f(…)` on an `any` carries its method name as a constant index, which only
        // a second unit's offset can show is relocated: every single-file module links
        // at offset zero.
        let files = [
            ("lib.lem", "<lem-fn f(): string = \"first\" + \"second\" />"),
            (
                "page.lem",
                "<lem-let u: any = \"Hello There\" /><lem.u.slugify()> <lem.f()>",
            ),
        ];
        let lib = unit_of("lib.lem", &files);
        let page = unit_of("page.lem", &files);
        let units = [lib, page];
        let module =
            link(&units, &ExportIndex::of(&units), &untyped_host()).expect_output();
        assert_eq!(
            render(&module, "page.lem", &[("page", Value::Null)])
                .unwrap()
                .html,
            "hello-there firstsecond"
        );
    }

    #[test]
    fn relocates_a_record_literals_and_a_type_tests_name_into_the_linked_pool() {
        // Both ops carry a constant index, which only a second unit's offset can show
        // is relocated: every single-file module — every corpus case — links at offset
        // zero. Without the relocation the record is tagged with, and the test asks
        // about, whatever string sits at that index in the first unit's pool.
        let files = [
            (
                "types.lem",
                "<lem-type Photo>\n  url: string\n</lem-type>\
                 <lem-type Framed extends Photo>\n  frame: string\n</lem-type>\
                 <lem-fn pad(): string = \"first\" + \"second\" />",
            ),
            (
                "page.lem",
                "<lem-let framed: Framed = {url: \"/p.jpg\", frame: \"oak\"} />\
                 <lem-let photos: [Photo] = [framed] />\
                 <lem-for p in photos>\
                 <lem-if p is Framed><lem.p.frame><lem-else />plain</lem-if>\
                 </lem-for>",
            ),
        ];
        let units = [unit_of("types.lem", &files), unit_of("page.lem", &files)];
        let module =
            link(&units, &ExportIndex::of(&units), &untyped_host()).expect_output();
        assert_eq!(
            render(&module, "page.lem", &[("page", Value::Null)])
                .unwrap()
                .html,
            "oak"
        );
    }

    #[test]
    fn records_a_type_that_only_a_type_test_names() {
        // `x is T` is a place a foreign type is written with no annotation behind it to
        // have recorded it. Without the fact, dropping the declaration would leave this
        // unit fresh and its `Op::TypeTest` asking about a type the project no longer
        // has.
        fn files(types: &str) -> [(&str, &str); 2] {
            [
                ("types.lem", types),
                // `LinkPost` is named nowhere else here: `a` is annotated `Article`.
                (
                    "page.lem",
                    "<lem-let a: Article = {title: \"T\"} />\
                     <lem-if a is LinkPost>l</lem-if>",
                ),
            ]
        }
        let both = "<lem-type Article>\n  title: string\n</lem-type>\
                    <lem-type LinkPost extends Article>\n  link: string\n</lem-type>";
        let page = unit_of("page.lem", &files(both));
        let names: Vec<&str> = page.type_refs.iter().map(|r| &*r.name).collect();
        assert!(names.contains(&"LinkPost"), "{names:?}");
        let changed = "<lem-type Article>\n  title: string\n</lem-type>\
                       <lem-type LinkPost extends Article>\n  href: string\n</lem-type>";
        let types = unit_of("types.lem", &[("types.lem", changed)]);
        let errors = link_errors(&[types, page]);
        assert!(
            errors[0].message.starts_with("`LinkPost` is now"),
            "{errors:?}"
        );
    }

    #[test]
    fn tables_every_records_base_from_whichever_unit_declares_it() {
        let files = [
            (
                "types.lem",
                "<lem-type Page>\n  url: string\n</lem-type>\
                 <lem-type Article extends Page>\n  title: string\n</lem-type>\
                 <lem-enum Placement = hero | left />",
            ),
            (
                "page.lem",
                "<lem-type LinkPost extends Article>\n  link: string\n</lem-type>x",
            ),
        ];
        let units = [unit_of("types.lem", &files), unit_of("page.lem", &files)];
        let module =
            link(&units, &ExportIndex::of(&units), &untyped_host()).expect_output();
        let mut types: Vec<String> = module
            .declarations
            .iter()
            .map(|declared| {
                let name = &declared.decl.name;
                let chain: Vec<&str> =
                    module.chain(name).iter().map(|d| &*d.name).collect();
                format!("{name}: {}", chain.join(", "))
            })
            .collect();
        types.sort();
        assert_eq!(
            types,
            [
                "Article: Article, Page",
                "LinkPost: LinkPost, Article, Page",
                "Page: Page",
                "Placement: Placement",
            ]
        );
        assert!(module.is_a("LinkPost", "Page"));
    }

    #[test]
    fn relinks_the_same_units_into_the_same_module() {
        // Linking is a pure pass over plain data: the boundary relinks resident units
        // after every compile, so the result must not depend on when it runs.
        let files = [
            ("lib.lem", "<lem-fn f(): int = 40 + 2 />"),
            ("page.lem", "a<lem.f()>b"),
        ];
        let lib = unit_of("lib.lem", &files);
        let page = unit_of("page.lem", &files);
        let units = [lib, page];
        let index = ExportIndex::of(&units);
        let module = link(&units, &index, &untyped_host()).expect_output();
        assert_eq!(
            module,
            link(&units, &index, &untyped_host()).expect_output()
        );
        assert_eq!(
            render(&module, "page.lem", &[("page", Value::Null)])
                .unwrap()
                .html,
            "a42b"
        );
    }

    const ARTICLE: &str =
        "<lem-type Article>\n  title: string\n  date: datetime\n</lem-type>";

    #[test]
    fn refuses_to_link_a_unit_whose_type_changed_shape_under_it() {
        // page.lem read `a.title` as a `string` and printed it; with `title` now an
        // `int`, or gone, that check no longer holds and the unit must be recompiled.
        const PAGE: &str = "<lem-fn card(a: Article): string = a.title />";
        const NARROWED: &str = "<lem-type Article>\n  title: int\n</lem-type>";
        let page = unit_of("page.lem", &[("types.lem", ARTICLE), ("page.lem", PAGE)]);
        for (edit, expect) in [
            (
                NARROWED,
                format!(
                    "page.lem:{}: `Article` is now `Article {{title: int}}`, but this \
                     file was compiled against `Article {{title: string, date: \
                     datetime}}` — recompile it\n  types.lem:{}: now declared here",
                    at(PAGE, "Article"),
                    at(NARROWED, "Article")
                ),
            ),
            (
                "<lem-type Photo>\n  alt: string\n</lem-type>",
                format!("page.lem:{}: unknown type `Article`", at(PAGE, "Article")),
            ),
        ] {
            let types = unit_of("types.lem", &[("types.lem", edit)]);
            let errors = link_errors(&[types, page.clone()]);
            assert_eq!(errors.len(), 1, "{errors:?}");
            assert_eq!(errors[0].to_string(), expect);
        }
    }

    #[test]
    fn links_a_unit_whose_type_only_changed_its_defaults() {
        // A default runs at load, never at compile, so it is not part of the shape a
        // consumer depends on — the other half of the rule, and invariant 11 for
        // declarations: the consumer's unit is byte-identical either way, and only
        // the declaring unit carries the new function.
        let against = |types: &str| {
            unit_of(
                "page.lem",
                &[
                    ("types.lem", types),
                    ("page.lem", "<lem-fn card(a: Article): string = a.title />"),
                ],
            )
        };
        let page = against(ARTICLE);
        let defaulted =
            "<lem-type Article>\n  title: string = \"T\"\n  date: datetime\n</lem-type>";
        let edited = against(defaulted);
        assert_eq!(page, edited);
        let plain = unit_of("types.lem", &[("types.lem", ARTICLE)]);
        let types = unit_of("types.lem", &[("types.lem", defaulted)]);
        assert!(plain.defaults.is_empty());
        assert_eq!(types.defaults.len(), 1);
        let units = [types, page];
        let module =
            link(&units, &ExportIndex::of(&units), &untyped_host()).expect_output();
        let article = module
            .records
            .iter()
            .find(|plan| &*plan.name == "Article")
            .expect("a plan per record");
        assert_eq!(article.steps.len(), 1);
        assert_eq!(&*article.steps[0].field, "title");
        assert_eq!(
            module.functions[article.steps[0].func as usize].name,
            "Article.title"
        );
        assert_eq!(&module.files[article.steps[0].file as usize], "types.lem");
    }

    /// A host shaped like the standard shell's, reading `site` from `site.lem`.
    /// The host's document type, as every test against `shell_host` declares it.
    const PAGE: &str = "<lem-type Page>\n  url: url\n</lem-type>";

    fn shell_host() -> Host {
        Host::new()
            .bind("page", Ambient::named("Page"))
            .bind("site", Ambient::bindings("site.lem"))
            .documents("page")
            .rendered_through("layout")
    }

    /// Every file compiled against `host` and linked: the module, or what stopped it.
    fn linked(host: &Host, files: &[(&str, &str)]) -> Compiled<Module> {
        let units: Vec<Unit> = files
            .iter()
            .map(|(path, _)| unit_against(host, path, files))
            .collect();
        link(&units, &ExportIndex::of(&units), host)
    }

    fn steps_of<'m>(module: &'m Module, record: &str) -> Vec<&'m str> {
        module
            .records
            .iter()
            .find(|plan| &*plan.name == record)
            .expect("a plan per record")
            .steps
            .iter()
            .map(|step| &*step.field)
            .collect()
    }

    #[test]
    fn plans_a_records_steps_in_the_order_their_reads_dictate() {
        let module = linked(
            &untyped_host(),
            &[(
                "types.lem",
                "<lem-type A>\n  a: string = self.b + \"!\"\n  b: string = self.c\n  \
                 c: string = \"C\"\n  d: string = \"D\"\n</lem-type>",
            )],
        )
        .expect_output();
        assert_eq!(steps_of(&module, "A"), ["c", "b", "a", "d"]);
    }

    #[test]
    fn plans_inherited_steps_from_the_nearest_declaration_of_each_field() {
        // `LinkPost` redeclares `title` to change its default (D32): its plan runs its
        // own `title` and `Page`'s `draft`; `Article`'s runs its own `title`.
        let module = linked(
            &shell_host(),
            &[
                (
                    "ambient.lem",
                    "<lem-type Page>\n  url: url\n  draft: bool = false\n</lem-type>",
                ),
                (
                    "schema.lem",
                    "<lem-type Article extends Page>\n  title: string = \"A\"\n  \
                     layout: (Article, html): html\n</lem-type>\n\
                     <lem-type LinkPost extends Article>\n  title: string = \"L\"\n\
                     </lem-type>",
                ),
            ],
        )
        .expect_output();
        assert_eq!(steps_of(&module, "LinkPost"), ["title", "draft"]);
        assert_eq!(steps_of(&module, "Article"), ["title", "draft"]);
        assert_eq!(steps_of(&module, "Page"), ["draft"]);
        let name = |record: &str| {
            let plan = module.records.iter().find(|p| &*p.name == record).unwrap();
            module.functions[plan.steps[0].func as usize].name.clone()
        };
        assert_eq!(name("LinkPost"), "LinkPost.title");
        assert_eq!(name("Article"), "Article.title");
    }

    #[test]
    fn keeps_an_inherited_default_under_a_redeclaration_that_gives_none() {
        // A redeclaration changes the default (D32); one that gives none changes
        // nothing, so `Page`'s step still runs for a `Note`.
        let module = linked(
            &shell_host(),
            &[
                (
                    "ambient.lem",
                    "<lem-type Page>\n  url: url\n  draft: bool = false\n</lem-type>",
                ),
                (
                    "schema.lem",
                    "<lem-type Note extends Page>\n  draft: bool\n  \
                     layout: (Note, html): html\n</lem-type>",
                ),
            ],
        )
        .expect_output();
        assert_eq!(steps_of(&module, "Note"), ["draft"]);
        let plan = module.records.iter().find(|p| &*p.name == "Note").unwrap();
        assert_eq!(
            module.functions[plan.steps[0].func as usize].name,
            "Page.draft"
        );
    }

    #[test]
    fn refuses_to_link_a_default_that_reads_itself_through_another() {
        const CYCLE: &str = "<lem-type A>\n  ok: string = \"x\"\n  a: string = self.b\n  \
                             b: string = self.a\n  c: string = self.c\n</lem-type>";
        let linked = linked(&untyped_host(), &[("types.lem", CYCLE)]);
        assert!(linked.output.is_none());
        assert_eq!(
            shown_all(&linked.diagnostics),
            [
                format!(
                    "types.lem:{}: `a` depends on itself: `a` reads `b`, `b` reads `a`",
                    at(CYCLE, "«a»: string = self.b")
                ),
                format!(
                    "types.lem:{}: `c` depends on itself: `c` reads `c`",
                    at(CYCLE, "«c»: string")
                ),
            ]
        );
    }

    fn shown_all(diagnostics: &[Diagnostic]) -> Vec<String> {
        diagnostics.iter().map(ToString::to_string).collect()
    }

    fn binding_names(module: &Module) -> Vec<&str> {
        assert_eq!(module.bindings.len(), 1, "one bindings plan");
        module.bindings[0]
            .steps
            .iter()
            .map(|step| &*step.name)
            .collect()
    }

    #[test]
    fn plans_computed_bindings_after_the_siblings_they_read() {
        let module = linked(
            &shell_host(),
            &[(
                "site.lem",
                "<lem-let tagline: string = \"Notes from \" + name />\n\
                 <lem-let name: string = \"limulus.net\" />",
            )],
        )
        .expect_output();
        assert_eq!(binding_names(&module), ["name", "tagline"]);
        // The bindings record is a record like any other, with nothing to run: its
        // fields are the bindings planned here.
        assert_eq!(steps_of(&module, "site.lem"), Vec::<&str>::new());
        let bindings = &module.bindings[0];
        assert_eq!(&*bindings.value, "site");
        assert_eq!(&*bindings.record, "site.lem");
        assert_eq!(&module.files[bindings.file as usize], "site.lem");
        let Fulfilled::Computed { func, .. } = &bindings.steps[1].fulfilled else {
            panic!("computed");
        };
        assert_eq!(module.functions[*func as usize].name, "tagline");
    }

    #[test]
    fn plans_a_loaded_binding_after_the_map_its_records_index() {
        // `Article.hero` names `site.photos`: `posts` waits for `photos`, declared
        // after it, on the strength of the pair alone (∆eag).
        let module = linked(
            &shell_host(),
            &[
                ("ambient.lem", PAGE),
                (
                    "site.lem",
                    "<lem-let posts: [Article] from \"feed/*.md\" />\n\
                     <lem-let photos: {string: Photo} from \"photos.json\" />",
                ),
                (
                    "schema.lem",
                    "<lem-type Photo>\n  alt: string\n</lem-type>\n\
                     <lem-type Article extends Page>\n  hero: Photo? by site.photos\n  \
                     layout: (Article, html): html\n</lem-type>",
                ),
            ],
        )
        .expect_output();
        assert_eq!(binding_names(&module), ["photos", "posts"]);
        let article = module
            .records
            .iter()
            .find(|p| &*p.name == "Article")
            .unwrap();
        assert_eq!(article.indexes.len(), 1);
        assert_eq!(&*article.indexes[0].key, "photos");
        assert_eq!(
            &module.files[article.indexes[0].file as usize],
            "schema.lem"
        );
    }

    #[test]
    fn refuses_to_link_a_binding_its_records_index_while_it_loads() {
        const SITE: &str = "<lem-let posts: {string: Article} from \"feed/*.md\" />";
        const SCHEMA: &str = "<lem-type Article extends Page>\n  \
                              related: Article? by site.posts\n  \
                              layout: (Article, html): html\n</lem-type>";
        let linked = linked(
            &shell_host(),
            &[
                ("ambient.lem", PAGE),
                ("site.lem", SITE),
                ("schema.lem", SCHEMA),
            ],
        );
        assert!(linked.output.is_none());
        assert_eq!(
            shown_all(&linked.diagnostics),
            [format!(
                "site.lem:{}: `posts` cannot be loaded before itself: `posts` needs \
                 `posts`, which `Article.related` looks up while `posts` loads\n  \
                 schema.lem:{}: looked up here",
                at(SITE, "«posts»: {string: Article}"),
                at(SCHEMA, "related")
            )]
        );
    }

    #[test]
    fn takes_a_fields_default_and_index_each_from_the_nearest_declaration_giving_one() {
        // `LinkPost` redeclares `hero` to give it a default and says nothing of the
        // index: the default is its own, the index is `Article`'s.
        let module = linked(
            &shell_host(),
            &[
                ("ambient.lem", PAGE),
                (
                    "site.lem",
                    "<lem-let photos: {string: Photo} from \"photos.json\" />\n\
                     <lem-let posts: [Article] from \"feed/*.md\" />",
                ),
                (
                    "schema.lem",
                    "<lem-type Photo>\n  alt: string\n</lem-type>\n\
                     <lem-type Article extends Page>\n  hero: Photo? by site.photos\n  \
                     layout: (Article, html): html\n</lem-type>\n\
                     <lem-type LinkPost extends Article>\n  hero: Photo? = null\n\
                     </lem-type>",
                ),
            ],
        )
        .expect_output();
        let link_post = module
            .records
            .iter()
            .find(|p| &*p.name == "LinkPost")
            .unwrap();
        assert_eq!(steps_of(&module, "LinkPost"), ["hero"]);
        assert_eq!(
            &module.files[link_post.steps[0].file as usize],
            "schema.lem"
        );
        assert_eq!(
            module.functions[link_post.steps[0].func as usize].name,
            "LinkPost.hero"
        );
        assert_eq!(link_post.indexes.len(), 1);
        assert_eq!(&*link_post.indexes[0].key, "photos");
    }

    #[test]
    fn plans_no_bindings_without_a_bindings_file_in_the_program() {
        let module = linked(&untyped_host(), &[("a.lem", "x")]).expect_output();
        assert!(module.bindings.is_empty());
        // The host reads `site.lem`, which has not been compiled.
        let module = linked(&shell_host(), &[("a.lem", "x")]).expect_output();
        assert!(module.bindings.is_empty());
    }

    #[test]
    fn plans_every_bindings_file_a_host_reads() {
        let host = Host::new()
            .bind("site", Ambient::bindings("site.lem"))
            .bind("data", Ambient::bindings("data.lem"));
        let module = linked(
            &host,
            &[
                ("site.lem", "<lem-let name: string = \"n\" />"),
                (
                    "data.lem",
                    "<lem-let n: int = 1 />\n<lem-let m: int = n + 1 />",
                ),
            ],
        )
        .expect_output();
        let plans: Vec<(&str, &str, Vec<&str>)> = module
            .bindings
            .iter()
            .map(|b| {
                (
                    &*b.value,
                    &*b.record,
                    b.steps.iter().map(|s| &*s.name).collect(),
                )
            })
            .collect();
        assert_eq!(
            plans,
            [
                ("site", "site.lem", vec!["name"]),
                ("data", "data.lem", vec!["n", "m"]),
            ]
        );
    }

    #[test]
    fn tables_every_project_function_by_name() {
        let module = linked(
            &untyped_host(),
            &[
                ("a.lem", "<lem-fn f(): int = 1 />"),
                ("b.lem", "<lem-fn g(): int = 2 /><lem-fn h(): int = 3 />"),
            ],
        )
        .expect_output();
        let names: Vec<(&str, &str, String)> = module
            .exports
            .iter()
            .map(|export| {
                (
                    &*export.name,
                    module.functions[export.func as usize].name.as_str(),
                    export.ty.to_string(),
                )
            })
            .collect();
        assert_eq!(
            names,
            [
                ("f", "f", "(): int".to_owned()),
                ("g", "g", "(): int".to_owned()),
                ("h", "h", "(): int".to_owned()),
            ]
        );
    }

    #[test]
    fn records_an_enum_a_qualified_variant_named() {
        // `Placement.hero` is the one place an enum's name is written with no
        // annotation behind it to have recorded it (∆EN6). Without the fact, dropping
        // the variant would leave this unit fresh and its `Op::Const` a variant the
        // enum no longer has.
        fn files(types: &str) -> [(&str, &str); 2] {
            [
                ("types.lem", types),
                // No annotation anywhere: the qualified variant is the only thing in
                // this file that names the enum, so it is the only thing that can
                // have recorded it.
                ("page.lem", "<lem.Placement.hero>"),
            ]
        }
        let both = "<lem-enum Placement = hero | left />";
        let page = unit_of("page.lem", &files(both));
        let names: Vec<&str> = page.type_refs.iter().map(|r| &*r.name).collect();
        assert_eq!(names, ["Placement"]);
        let dropped = "<lem-enum Placement = left />";
        let types = unit_of("types.lem", &[("types.lem", dropped)]);
        let errors = link_errors(&[types, page]);
        assert!(
            errors[0]
                .message
                .starts_with("`Placement` is now `Placement = left`"),
            "{errors:?}"
        );
    }

    #[test]
    fn records_a_type_reached_only_through_another_types_chain() {
        // page.lem names `LinkPost` alone; checking it against `Article` walked the
        // chain, so `Article`'s declaration is a fact too, and a change to it — say a
        // `LinkPost` that no longer extends it — is caught at link time.
        fn files(types: &str) -> [(&str, &str); 2] {
            [
                ("types.lem", types),
                (
                    "page.lem",
                    "<lem-fn card(a: Article): string = a.title />\n\
                     <lem-fn link(l: LinkPost): string = card(l) />",
                ),
            ]
        }
        let chained = "<lem-type Article>\n  title: string\n</lem-type>\n\
                       <lem-type LinkPost extends Article>\n  link: url\n</lem-type>";
        let page = unit_of("page.lem", &files(chained));
        let names: Vec<&str> = page.type_refs.iter().map(|r| &*r.name).collect();
        assert_eq!(names, ["Article", "LinkPost"]);
        let severed = "<lem-type Article>\n  title: string\n</lem-type>\n\
                       <lem-type LinkPost>\n  link: url\n</lem-type>";
        let types = unit_of("types.lem", &[("types.lem", severed)]);
        let errors = link_errors(&[types, page]);
        assert!(
            errors[0]
                .message
                .starts_with("`LinkPost` is now `LinkPost {link: url}`"),
            "{errors:?}"
        );
    }

    #[test]
    fn reports_a_units_stale_types_beside_its_stale_imports() {
        const PAGE: &str = "<lem-fn card(a: Article): string = f(a.title) />\n\
                            <lem-fn other(): string = f(\"x\") />";
        // One pass per unit, so a file's diagnostics stay together and in span order
        // whatever kind they are.
        let page = unit_of(
            "page.lem",
            &[
                (
                    "lib.lem",
                    "<lem-type Article>\n  title: string\n</lem-type>\n\
                     <lem-fn f(s: string): string = s />",
                ),
                ("page.lem", PAGE),
            ],
        );
        let lib = unit_of(
            "lib.lem",
            &[(
                "lib.lem",
                "<lem-type Article>\n  title: int\n</lem-type>\n\
                 <lem-fn f(s: int): string = \"s\" />",
            )],
        );
        let errors = link_errors(&[lib, page]);
        let starts: Vec<u32> = errors.iter().map(|e| e.span.start).collect();
        assert_eq!(
            starts,
            [at(PAGE, "Article").start, at(PAGE, "f(a.title)").start]
        );
    }

    /// A host with one asset built, which is what the asset tests compile against.
    fn built() -> Host {
        Host::new().with_assets([("styles/site.css", "/site.abc123.css")])
    }

    #[test]
    fn compiles_a_unit_that_names_an_asset_to_the_same_bytes_under_the_same_manifest() {
        // Invariant 11's first half: the fact is only worth recording if consuming it
        // is stable. Two compiles against one manifest are one unit.
        const TEMPLATE: &str = "<lem.asset(\"styles/site.css\")>";
        let files = [("page.lem", TEMPLATE)];
        let once = unit_against(&built(), "page.lem", &files);
        let twice = unit_against(&built(), "page.lem", &files);
        assert_eq!(once.constants, twice.constants);
        assert_eq!(once.functions, twice.functions);
        assert_eq!(
            once.asset_refs
                .iter()
                .map(|reference| (&*reference.path, &*reference.url))
                .collect::<Vec<_>>(),
            [("styles/site.css", "/site.abc123.css")]
        );
    }

    #[test]
    fn refuses_to_link_a_unit_the_manifest_has_moved_the_asset_under() {
        // The other half, and the one that earns the fact: the URL is inside this
        // unit's bytecode, so a rebuild that re-hashes the asset leaves it stale. A
        // file that named no asset is untouched by the same rebuild.
        const TEMPLATE: &str = "<lem.asset(\"styles/site.css\")>";
        let files = [("page.lem", TEMPLATE)];
        let unit = unit_against(&built(), "page.lem", &files);
        for (rebuilt, expect) in [
            (
                Host::new().with_assets([("styles/site.css", "/site.def456.css")]),
                format!(
                    "page.lem:{}: the manifest maps `styles/site.css` to \
                     `/site.def456.css` now, and this file was compiled against \
                     `/site.abc123.css`",
                    at(TEMPLATE, "\"styles/site.css\"")
                ),
            ),
            (
                Host::new(),
                format!(
                    "page.lem:{}: `styles/site.css` is no longer in the manifest",
                    at(TEMPLATE, "\"styles/site.css\"")
                ),
            ),
        ] {
            let units = [unit.clone()];
            let linked = link(&units, &ExportIndex::of(&units), &rebuilt);
            assert_eq!(
                linked
                    .diagnostics
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
                [expect]
            );
            assert!(linked.output.is_none());
        }
    }

    #[test]
    fn refuses_to_link_a_file_whose_marks_the_host_claims_differently_now() {
        // What the claims said was answered while the file compiled — the warning at the
        // word, and with ∆7wK a payload's type — so any change to what they say about a
        // name this file marks leaves it stale, at that name.
        const SRC: &str = "<lem.mark(\"styles\")>";
        let files = [("page.lem", SRC)];
        let claimed = Host::untyped(["page"]).with_marks(["styles"]);
        let other = Host::untyped(["page"]).with_marks(["head"]);
        let none = Host::untyped(["page"]);
        for (compiled, now, message) in [
            (
                &claimed,
                &other,
                "no handler claims `styles` now, and this file was compiled where one did",
            ),
            (
                &other,
                &claimed,
                "a handler claims `styles` now, and this file was compiled where none did",
            ),
            (
                &none,
                &other,
                "the host declares handlers now, and this file was compiled against none, \
                 so nothing was said about `styles`",
            ),
            (
                &other,
                &none,
                "the host declares no handlers now, and this file was compiled against \
                 the ones it did, which left `styles` unclaimed",
            ),
        ] {
            let units = [unit_against(compiled, "page.lem", &files)];
            let linked = link(&units, &ExportIndex::of(&units), now);
            assert_eq!(
                linked
                    .diagnostics
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
                [format!("page.lem:{}: {message}", at(SRC, "\"styles\""))]
            );
        }
        // Unchanged claims link, and one name marked twice is one fact.
        const TWICE: &str = "<lem.mark(\"styles\")><lem.mark(\"styles\")>";
        let unit = unit_against(&claimed, "page.lem", &[("page.lem", TWICE)]);
        assert_eq!(unit.claim_refs.len(), 1);
        let units = [unit];
        assert!(
            link(&units, &ExportIndex::of(&units), &claimed)
                .output
                .is_some()
        );
    }

    /// A host whose handlers claim `code` for `keys`, with the payload record a mark of
    /// that name carries and the field it is keyed by.
    fn claiming(keys: &[&str], payload: &str) -> Host {
        Host::untyped(["site", "page", "build"]).with_claims([(
            "code",
            Claim::new()
                .with_keys(keys.iter().copied())
                .with_payload(payload, Some("language")),
        )])
    }

    /// A file declaring the payload record and marking `code` under one written key.
    const KEYED: &str = "<lem-type Code>\n  language: string?\n</lem-type>\n\
                         <lem.mark(\"code\", {language: \"bash\"})>";

    #[test]
    fn refuses_to_link_a_file_whose_key_the_host_claims_differently_now() {
        // What a per-key fact buys: the answer at `bash` was consumed while the file
        // compiled — it decided the warning — so a producer arriving for it, or dropping
        // it, leaves this file stale, and the message names the key.
        let host = claiming(&["css"], "Code");
        let unit = unit_against(&host, "page.lem", &[("page.lem", KEYED)]);
        for (now, expect) in [
            (
                claiming(&["bash", "css"], "Code"),
                "a producer claims `bash` for `code` now, and this file was compiled \
                 where none did",
            ),
            (
                claiming(&["css"], "Snippet"),
                "the handler for `code` takes a `Snippet` payload now, and this file was \
                 compiled against `Code`",
            ),
        ] {
            let units = [unit.clone()];
            let linked = link(&units, &ExportIndex::of(&units), &now);
            assert_eq!(
                linked
                    .diagnostics
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
                [format!(
                    "page.lem:{}: {expect}",
                    if expect.starts_with("a producer") {
                        at(KEYED, "\"bash\"")
                    } else {
                        at(KEYED, "mark(«\"code\"»")
                    }
                )],
                "{expect}"
            );
        }
        // The converse, and the half a digest could not give: a producer arriving for
        // another language leaves this file alone.
        let units = [unit];
        assert!(
            link(
                &units,
                &ExportIndex::of(&units),
                &claiming(&["css", "rust"], "Code")
            )
            .diagnostics
            .is_empty()
        );
    }

    #[test]
    fn records_one_claim_fact_for_each_name_and_key_a_file_asks_about() {
        // One fact per pair, however often the file asks: two marks of one name and key
        // are one question, and a second key is a second one.
        const TWICE: &str = "<lem-type Code>\n  language: string?\n</lem-type>\n\
                             <lem.mark(\"code\", {language: \"bash\"})>\
                             <lem.mark(\"code\", {language: \"bash\"})>\
                             <lem.mark(\"code\", {language: \"css\"})>";
        let unit = unit_against(
            &claiming(&["bash", "css"], "Code"),
            "page.lem",
            &[("page.lem", TWICE)],
        );
        assert_eq!(
            unit.claim_refs
                .iter()
                .map(|reference| (
                    &*reference.name,
                    reference.key.as_ref().map(|(key, _)| &**key)
                ))
                .collect::<Vec<_>>(),
            [("code", Some("bash")), ("code", Some("css"))]
        );
    }

    #[test]
    fn refuses_to_link_a_document_whose_language_a_producer_claims_now() {
        // A fence asks what a `mark()` with a written key asks, so a highlighter that
        // starts covering its language leaves exactly that document stale, the message
        // at the word — as does a host that starts declaring handlers at all. A producer
        // arriving for another language leaves it alone.
        const DOC: &str = "```sh\necho\n```\n";
        for (compiled, message) in [
            (
                claiming(&["css"], "Code"),
                "a producer claims `sh` for `code` now, and this file was compiled where \
                 none did",
            ),
            (
                Host::new(),
                "a handler claims `code` now, and this file was compiled where none did",
            ),
        ] {
            let units = [unit_against(&compiled, "post.md", &[("post.md", DOC)])];
            let linked = link(
                &units,
                &ExportIndex::of(&units),
                &claiming(&["css", "sh"], "Code"),
            );
            assert_eq!(
                linked
                    .diagnostics
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
                [format!("post.md:{}: {message}", at(DOC, "sh"))]
            );
        }
        let units = [unit_against(
            &claiming(&["css"], "Code"),
            "post.md",
            &[("post.md", DOC)],
        )];
        assert!(
            link(
                &units,
                &ExportIndex::of(&units),
                &claiming(&["css", "rust"], "Code")
            )
            .diagnostics
            .is_empty()
        );
    }

    #[test]
    fn compiles_a_document_whose_code_names_no_language_the_same_under_any_claims() {
        // Invariant 11 for claims: a block with no language asks no producer anything,
        // so no change to the claims touches its unit or its link — and it is still a
        // mark, with its payload reported, either way.
        const DOC: &str = "Plain:\n\n```\necho\n```\n\n    indented\n";
        let files = [("post.md", DOC)];
        let without = unit_against(&Host::new(), "post.md", &files);
        let with = unit_against(&claiming(&["sh"], "Code"), "post.md", &files);
        assert_eq!(without, with);
        assert!(with.claim_refs.is_empty());
        assert_eq!(with.payloads.len(), 2);
        let units = [with];
        assert!(
            link(&units, &ExportIndex::of(&units), &Host::new())
                .output
                .is_some()
        );
    }

    #[test]
    fn reports_one_payload_for_the_same_block_in_two_documents() {
        // The id is the content, so the module keeps one payload for a block two
        // documents both write, and each document's unit keeps its own.
        const DOC: &str = "```sh\necho\n```\n";
        let host = claiming(&["sh"], "Code");
        let units = [
            unit_against(&host, "a.md", &[("a.md", DOC)]),
            unit_against(&host, "b.md", &[("b.md", DOC)]),
        ];
        assert_eq!(units[0].payloads, units[1].payloads);
        let module = link(&units, &ExportIndex::of(&units), &host).expect_output();
        assert_eq!(module.payloads, units[0].payloads);
    }

    #[test]
    fn records_one_fact_however_many_times_a_file_names_the_same_asset() {
        const TEMPLATE: &str =
            "<lem.asset(\"styles/site.css\")><lem.asset(\"styles/site.css\")>";
        let unit = unit_against(&built(), "page.lem", &[("page.lem", TEMPLATE)]);
        assert_eq!(unit.asset_refs.len(), 1);
    }

    #[test]
    fn refuses_to_link_a_unit_the_host_rebound_an_ambient_name_under() {
        const TEMPLATE: &str = "<lem.page.url>";
        // page.lem read `page.url` as a `Page`'s field; a host that now calls `page`
        // something else checked that read against a declaration this unit never saw.
        let files = [("ambient.lem", PAGE), ("page.lem", TEMPLATE)];
        let host = Host::new().bind("page", Ambient::named("Page"));
        let unit = unit_against(&host, "page.lem", &files);
        let ambient = unit_against(&host, "ambient.lem", &files);
        for (rebound, expect) in [
            (
                Host::new().bind("page", Ambient::named("Photo")),
                format!(
                    "page.lem:{}: the host binds `page` as a `Photo` now, and this file \
                     was compiled against `Page`",
                    at(TEMPLATE, "«page».url")
                ),
            ),
            (
                Host::new(),
                format!(
                    "page.lem:{}: the host no longer binds `page`",
                    at(TEMPLATE, "«page».url")
                ),
            ),
        ] {
            let units = [ambient.clone(), unit.clone()];
            let linked = link(&units, &ExportIndex::of(&units), &rebound);
            assert_eq!(
                linked
                    .diagnostics
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
                [expect]
            );
            assert!(linked.output.is_none());
        }
    }

    #[test]
    fn links_a_document_against_the_host_as_that_document_saw_it() {
        // The document value is typed by the file's own record, so the binding the host
        // declares for it is not what this unit was compiled against — and comparing
        // the two would call every document stale.
        let files = [
            ("ambient.lem", PAGE),
            (
                "schema.lem",
                "<lem-type Post extends Page>\n  title: string\n  \
                 layout: (Post, html): html\n</lem-type>",
            ),
            ("post.md", "<lem.page.title>"),
        ];
        let host = Host::new()
            .bind("page", Ambient::named("Page"))
            .documents("page")
            .rendered_through("layout");
        let prepared: Vec<_> = files
            .iter()
            .map(|(path, text)| {
                let mut file = SourceFile::new(*path, *text);
                if *path == "post.md" {
                    file.record = Some("Post".to_owned());
                }
                prepare(&file, &host).unwrap()
            })
            .collect();
        let mut index = ExportIndex::new();
        for p in &prepared {
            index.offer(p.path(), p.exports());
            index.offer_types(p.path(), p.types());
        }
        let units: Vec<Unit> = prepared
            .iter()
            .map(|p| compile_unit(p, &index, &host).expect_output())
            .collect();
        assert!(link(&units, &index, &host).output.is_some());
    }

    #[test]
    fn refuses_one_from_binding_name_in_two_bindings_files() {
        let host = Host::new()
            .bind("site", Ambient::bindings("site.lem"))
            .bind("data", Ambient::bindings("data.lem"));
        const SITE: &str = "<lem-let motto: string from env(\"MOTTO\") />";
        const DATA: &str = "<lem-let motto: string from env(\"OTHER\") />";
        let files = [("site.lem", SITE), ("data.lem", DATA)];
        let errors: Vec<String> = linked(&host, &files)
            .diagnostics
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            errors,
            [format!(
                "data.lem:{}: `motto` is already loaded, for `site`: the host fulfils a \
                 `from` binding by name, so no two files it reads bindings from may \
                 load one called `motto`\n  site.lem:{}: first loaded here",
                at(DATA, "«motto»: string"),
                at(SITE, "«motto»: string")
            )]
        );
    }

    // --- Reads through calls (∆HF0) --------------------------------------------------

    #[test]
    fn orders_a_default_after_the_fields_a_called_function_reads() {
        // `a` hands `self` to `f`, which reads `b`: the loader computes `b` first.
        let module = linked(
            &untyped_host(),
            &[(
                "types.lem",
                "<lem-type A>\n  a: string = f(self)\n  b: string = \"B\"\n</lem-type>\n\
                 <lem-fn f(x: A): string = x.b />",
            )],
        )
        .expect_output();
        assert_eq!(steps_of(&module, "A"), ["b", "a"]);
    }

    #[test]
    fn closes_reads_through_a_chain_of_calls_across_files_and_through_recursion() {
        // `a` → `f` → `g` → `h`, where `h` reads `c` off what `g` passed on, and `g`
        // calls itself.
        let module = linked(
            &untyped_host(),
            &[
                (
                    "types.lem",
                    "<lem-type A>\n  a: string = f(self)\n  b: string = \"B\"\n  \
                     c: string = \"C\"\n</lem-type>",
                ),
                (
                    "lib.lem",
                    "<lem-fn f(x: A): string = g(x, \"\") />\n\
                     <lem-fn g(y: A, s: string): string = s + h(y) + g(y, s) />\n\
                     <lem-fn h(z: A): string = z.c />",
                ),
            ],
        )
        .expect_output();
        assert_eq!(steps_of(&module, "A"), ["b", "c", "a"]);
        let a = &module.records[0].steps[2];
        assert_eq!(a.reads, [Rc::from("c")]);
    }

    #[test]
    fn drops_a_callees_read_off_a_parameter_handed_something_other_than_a_root() {
        // `f(self.hero)` hands a field, so `f`'s reads off it are the photo's own
        // business; `a` waits for `hero` and not for the `alt` of its own.
        let module = linked(
            &untyped_host(),
            &[(
                "types.lem",
                "<lem-type Photo>\n  alt: string = \"x\"\n</lem-type>\n\
                 <lem-type A>\n  a: string = f(self.hero)\n  \
                 hero: Photo = {alt: \"h\"}\n  alt: string = \"A\"\n</lem-type>\n\
                 <lem-fn f(p: Photo): string = p.alt />",
            )],
        )
        .expect_output();
        assert_eq!(steps_of(&module, "A"), ["hero", "a", "alt"]);
    }

    #[test]
    fn refuses_a_cycle_through_a_call_and_names_the_call() {
        const CYCLE: &str = "<lem-type A>\n  a: string = f(self)\n  b: string = self.a\n\
                             </lem-type>\n\
                             <lem-fn f(x: A): string = x.b />";
        let linked = linked(&untyped_host(), &[("types.lem", CYCLE)]);
        assert!(linked.output.is_none());
        assert_eq!(
            shown_all(&linked.diagnostics),
            [format!(
                "types.lem:{}: `a` depends on itself: `a` reads `b` through `f`, `b` \
                 reads `a`",
                at(CYCLE, "«a»: string")
            )]
        );
    }

    #[test]
    fn orders_a_computed_binding_after_the_siblings_a_called_function_reads() {
        let module = linked(
            &shell_host(),
            &[
                ("ambient.lem", PAGE),
                (
                    "site.lem",
                    "<lem-fn all(): [string] = site.posts />\n\
                     <lem-let articles: [string] = all() />\n\
                     <lem-let posts: [string] from \"feed/*.md\" />",
                ),
            ],
        )
        .expect_output();
        assert_eq!(binding_names(&module), ["posts", "articles"]);
    }

    #[test]
    fn refuses_a_binding_that_needs_itself_through_a_call_and_names_the_call() {
        const SITE: &str = "<lem-fn all(): [string] = site.articles />\n\
                            <lem-let articles: [string] = all() />";
        let linked = linked(&shell_host(), &[("ambient.lem", PAGE), ("site.lem", SITE)]);
        assert!(linked.output.is_none());
        assert_eq!(
            shown_all(&linked.diagnostics),
            [format!(
                "site.lem:{}: `articles` cannot be loaded before itself: `articles` \
                 needs `articles` through `all`",
                at(SITE, "«articles»: [string] =")
            )]
        );
    }

    #[test]
    fn refuses_a_default_that_reaches_an_ambient_value_through_a_call() {
        // Read directly, `site` is refused where it stands; inside `caption()`, only
        // the link step can see it, and it says so at the field, naming the call.
        const SCHEMA: &str = "<lem-type Article extends Page>\n  \
                              caption: string = caption()\n  \
                              layout: (Article, html): html\n</lem-type>\n\
                              <lem-fn caption(): string = site.captions[\"x\"] />";
        let linked = linked(
            &shell_host(),
            &[
                ("ambient.lem", PAGE),
                (
                    "site.lem",
                    "<lem-let posts: [Article] from \"feed/*.md\" />\n\
                     <lem-let captions: {string: string} from \"captions.json\" />",
                ),
                ("schema.lem", SCHEMA),
            ],
        );
        assert!(linked.output.is_none());
        assert_eq!(
            shown_all(&linked.diagnostics),
            [format!(
                "schema.lem:{}: `caption` reaches `site` through `caption`, but `site` \
                 is out of scope in a default: a default reads only the record it \
                 builds — `self`, and `path` and `body` on a page",
                at(SCHEMA, "«caption»: string")
            )]
        );
    }

    #[test]
    fn refuses_every_ambient_value_a_default_reaches_and_once_per_declaration() {
        // `banner` reaches `site` and `build` through two calls: both are said, in
        // one link. `LinkPost` inherits the default and repeats nothing.
        const SCHEMA: &str = "<lem-type Article extends Page>\n  \
                              banner: string = brand() + year()\n  \
                              layout: (Article, html): html\n</lem-type>\n\
                              <lem-type LinkPost extends Article>\n  via: string\n\
                              </lem-type>\n\
                              <lem-fn brand(): string = site.name />\n\
                              <lem-fn year(): string = build.generator />";
        let host = shell_host().bind("build", Ambient::named("Build"));
        let linked = linked(
            &host,
            &[
                (
                    "ambient.lem",
                    "<lem-type Page>\n  url: url\n</lem-type>\n\
                     <lem-type Build>\n  generator: string\n</lem-type>",
                ),
                ("site.lem", "<lem-let name: string = \"n\" />"),
                ("schema.lem", SCHEMA),
            ],
        );
        assert!(linked.output.is_none());
        let refused = |value: &str, via: &str| {
            format!(
                "schema.lem:{}: `banner` reaches `{value}` through `{via}`, but `{value}` \
                 is out of scope in a default: a default reads only the record it \
                 builds — `self`, and `path` and `body` on a page",
                at(SCHEMA, "«banner»:")
            )
        };
        assert_eq!(
            shown_all(&linked.diagnostics),
            [refused("site", "brand"), refused("build", "year")]
        );
    }

    #[test]
    fn refuses_a_default_that_reaches_the_document_value_through_a_call() {
        // Read directly, `page` is refused where it stands (∆iCW); reached through
        // `where` or `here` — a field of it, or the value whole — only the link step
        // can see it, and it says so at the field.
        const SCHEMA: &str = "<lem-type Post extends Page>\n  at: url = where()\n  \
                              origin: Page = here()\n  layout: (Post, html): html\n\
                              </lem-type>";
        const FNS: &str = "<lem-fn where(): url = page.url />\n\
                           <lem-fn here(): Page = page />";
        let linked = linked(
            &shell_host(),
            &[
                ("ambient.lem", PAGE),
                ("schema.lem", SCHEMA),
                ("fns.lem", FNS),
            ],
        );
        assert!(linked.output.is_none());
        let refused = |field: &str, via: &str| {
            format!(
                "schema.lem:{}: `{field}` reaches `page` through `{via}`, but `page` is \
                 out of scope in a default: a default reads only the record it builds \
                 — `self`, and `path` and `body` on a page",
                at(SCHEMA, &format!("«{field}»:"))
            )
        };
        assert_eq!(
            shown_all(&linked.diagnostics),
            [refused("at", "where"), refused("origin", "here")]
        );
    }

    #[test]
    fn refuses_a_computed_binding_that_reaches_the_document_value_through_a_call() {
        const SITE: &str = "<lem-let home: url = where() />";
        let linked = linked(
            &shell_host(),
            &[
                ("ambient.lem", PAGE),
                ("site.lem", SITE),
                ("fns.lem", "<lem-fn where(): url = page.url />"),
            ],
        );
        assert!(linked.output.is_none());
        assert_eq!(
            shown_all(&linked.diagnostics),
            [format!(
                "site.lem:{}: `home` reaches `page` through `where`, but `page` is bound \
                 per page, and a binding here is computed once, at load",
                at(SITE, "home")
            )]
        );
    }

    #[test]
    fn leaves_a_function_named_as_a_value_out_of_the_order() {
        // `layout = post` stores `post` for the render to call, when `page` is bound
        // and `site` is whole: what it reads is no part of the load order, however
        // much of `page` and `site` it reads, and `posts` does not need itself.
        const SCHEMA: &str = "<lem-type Post extends Page>\n  title: string\n  \
                              layout: (Post, html): html = post\n</lem-type>\n\
                              <lem-fn post(p: Post, body: html)>\
                              <lem.page.url><lem.site.posts[0].title><lem.p.title>\
                              <lem.body></lem-fn>";
        let module = linked(
            &shell_host(),
            &[
                ("ambient.lem", PAGE),
                ("site.lem", "<lem-let posts: [Post] from \"feed/*.md\" />"),
                ("schema.lem", SCHEMA),
            ],
        )
        .expect_output();
        assert_eq!(steps_of(&module, "Post"), ["layout"]);
        let post = module.records.iter().find(|p| &*p.name == "Post").unwrap();
        assert!(post.steps[0].reads.is_empty());
    }

    #[test]
    fn reorders_a_plan_when_a_callee_body_changes_leaving_the_caller_unit_as_it_was() {
        // Invariant 11 with the closure in play: `f`'s body decides the order, and it
        // is the link step, not the schema's unit, that changes when the body does.
        let types = (
            "types.lem",
            "<lem-type A>\n  a: string = f(self)\n  b: string = \"B\"\n</lem-type>",
        );
        let plan_against = |lib: &str| {
            let files = [types, ("lib.lem", lib)];
            let schema = unit_of("types.lem", &files);
            let lib = unit_of("lib.lem", &files);
            let units = [schema.clone(), lib];
            let module =
                link(&units, &ExportIndex::of(&units), &untyped_host()).expect_output();
            (schema, steps_of(&module, "A").join(","))
        };
        let (unit, plan) = plan_against("<lem-fn f(x: A): string = \"f\" />");
        let (edited, replanned) = plan_against("<lem-fn f(x: A): string = x.b />");
        assert_eq!(unit, edited);
        assert_eq!((plan.as_str(), replanned.as_str()), ("a,b", "b,a"));
    }
}

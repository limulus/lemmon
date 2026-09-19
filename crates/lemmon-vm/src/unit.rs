//! One file's compiled output: its own constant pool and functions, the surfaces it
//! offers other files, and the compile-time facts it consumed. A unit is a function of
//! its file's text plus those facts, and nothing else (D25, invariant 11).

use std::collections::BTreeMap;
use std::rc::Rc;

use lemmon_syntax::Span;

use crate::Value;
use crate::bytecode::Function;
use crate::decls::{Declaration, TypeFact};
use crate::document::CodePayload;
use crate::host::ClaimAnswer;
use crate::plan::SourceKind;
use crate::types::Type;

/// One file's compiled output: self-contained except for the functions it names in
/// other files, which stay symbolic until [`link`](crate::link()) resolves them.
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
    /// Whether the file is shaped like a page: a document, or a template with front
    /// matter (D41). A fact of the file's text, kept here because the link step,
    /// which has no text, builds a page source for such a file and none for any
    /// other, and reads the host as the file saw it
    /// ([`Host::for_file`](crate::Host::for_file)).
    pub page_shaped: bool,
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
    /// Where the default first reads `body`, if it does — a page template has no
    /// body until it renders, so the link step refuses one typed by a record whose
    /// plan runs such a default, and points here (D43).
    pub reads_body: Option<Span>,
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
/// [`Fulfilled`](crate::Fulfilled) is once the link step has closed the function's
/// reads.
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

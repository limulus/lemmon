//! What a module tells the loader to run: the plan behind `loadData` (`∆iCW`,
//! `docs/language.md` §Schemas, §Ambient types).
//!
//! A schema's computed defaults and the bindings file's computed bindings are
//! expressions the *loader* evaluates rather than a render — a default sees the record
//! being built, a binding sees the site so far. The compiler lowers each into a
//! function of its own unit and records what it reads; [`link`](crate::link()) joins
//! those into the tables here, in the order the reads dictate, and refuses a cycle as
//! it refuses an unresolved import. A field's `by` index is no function: it names one
//! field of one ambient value, and the loader indexes that map as loaded (`∆eag`). The
//! loader then runs each table front to back and never has to reason about order
//! itself.
//!
//! Everything here is plain data on the [`Module`](crate::Module), so it serialises
//! with the rest of the program.

use std::rc::Rc;

use lemmon_syntax::Span;

/// What the host loads to fulfil a `from` binding: the literal after `from`, by its
/// form (`docs/language.md` §Ambient types).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceKind {
    /// `from "…"`: a glob when the binding's type is a list of the host's document
    /// type, a data file otherwise — the host decides by the type.
    File(Rc<str>),
    /// `from plugin("…")`: a plugin's data hook.
    Plugin(Rc<str>),
    /// `from env("…")`: an environment value, resolved by the binding's type.
    Env(Rc<str>),
}

/// One field's `= default`: the function the loader runs when the loaded data omits
/// the field. Its parameters are `self` and, on a record that extends the host's
/// document type, `path` and `body`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub field: Rc<str>,
    /// The function, in the module.
    pub func: u32,
    /// The fields of `self` the function reads, its calls followed, which the loader
    /// must have computed first.
    pub reads: Vec<Rc<str>>,
    /// The file the field is declared in, as a module file index — what an error in
    /// the step names.
    pub file: u32,
    /// The field's name in that file.
    pub span: Span,
}

/// One field's `by value.field` index: the map a string the loaded data gives for the
/// field resolves through, named as a field of an ambient value (`∆eag`). Resolved
/// before any default runs, since a default may read the field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Index {
    pub field: Rc<str>,
    /// The ambient value the map is a field of: `site`.
    pub value: Rc<str>,
    /// The map's name in that value: `photos`.
    pub key: Rc<str>,
    /// The file the field is declared in, as a module file index — what an error in
    /// the lookup names.
    pub file: u32,
    /// The field's name in that file.
    pub span: Span,
}

/// Every step of one declared record, its ancestors' included, in an order that has
/// each field's reads computed before it, and every index of one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordPlan {
    pub name: Rc<str>,
    pub steps: Vec<Step>,
    pub indexes: Vec<Index>,
}

/// How one binding of the bindings file gets its value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fulfilled {
    /// A `from` binding: the host loads it.
    ByHost(SourceKind),
    /// An `= expr` or capture binding: the loader runs the function, once every
    /// ambient value it reads — a field of one, or the value whole, its calls
    /// followed — is loaded. A default makes no such read: it reads only the record
    /// it builds (`∆eag`), so a step's reads are field names of `self`.
    Computed { func: u32, reads: Vec<Rc<str>> },
}

/// One top-level binding of the bindings file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindingStep {
    pub name: Rc<str>,
    pub fulfilled: Fulfilled,
    /// The binding's name in the bindings file.
    pub span: Span,
}

/// The bindings file's plan: the record the ambient value is, and its bindings in an
/// order that has everything a computed one reads — and every sibling the records a
/// loaded one holds look a string up in while they load — ready first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bindings {
    /// The ambient name the host reads the bindings as: `site`.
    pub value: Rc<str>,
    /// The synthesized record's name — the bindings file's path.
    pub record: Rc<str>,
    /// The bindings file, as a module file index.
    pub file: u32,
    pub steps: Vec<BindingStep>,
}

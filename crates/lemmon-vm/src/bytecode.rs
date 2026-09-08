//! The bytecode a module is compiled to and the VM runs.
//!
//! A [`Module`] is plain data — constants, functions, a file table, and the record
//! types' ancestry — so that it can be serialised for `exportModule` later (`∆TNT`)
//! without the VM changing. Every instruction in a function has a [`Span`] beside it at
//! the same index: that parallel table is how a runtime error points at source
//! (invariant 8).

use std::rc::Rc;

use lemmon_syntax::Span;

use crate::Value;
use crate::plan::{Bindings, RecordPlan};

/// Defines [`Op`] and, from the same list, the table of variant names — so that the
/// corpus gate asserting every op is reached by some case cannot fall out of date.
macro_rules! ops {
    ($($(#[$attr:meta])* $name:ident $({ $($field:ident : $ty:ty),* $(,)? })?),* $(,)?) => {
        /// One VM instruction. Jump targets are absolute indices into the function's code.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Op {
            $($(#[$attr])* $name $({ $($field: $ty),* })?,)*
        }

        impl Op {
            /// Every variant's name, in declaration order.
            pub const NAMES: &'static [&'static str] = &[$(stringify!($name)),*];

            /// This instruction's variant name.
            #[must_use]
            pub const fn name(&self) -> &'static str {
                match self {
                    $(Self::$name { .. } => stringify!($name),)*
                }
            }
        }
    };
}

ops! {
    /// Push `constants[index]`.
    Const { index: u32 },
    /// Push local `slot`.
    Local { slot: u16 },
    /// Pop into local `slot`.
    StoreLocal { slot: u16 },
    /// Push `functions[func]` as a function value with no captures.
    Global { func: u32 },
    /// Push the render-supplied value bound to the ambient name at `constants[name]`.
    Ambient { name: u32 },
    /// Pop a map; push its field named by `constants[name]`. Missing is an error.
    GetField { name: u32 },
    /// As `GetField`, but a `null` object yields `null` (`a?.b`).
    OptField { name: u32 },
    /// Pop an index then a list or map; push the element.
    Index,
    /// Pop `argc` arguments; call `functions[func]`; push its return value.
    Call { func: u32, argc: u16 },
    /// Pop `argc` arguments then the function value under them; call it.
    CallValue { argc: u16 },
    /// Pop `argc` arguments; call the builtin; push its result.
    CallBuiltin { id: u32, argc: u16 },
    /// Pop `argc` arguments then the receiver under them; call what the receiver's
    /// field named by `constants[name]` holds, or that name's method for the receiver's
    /// runtime type. What `x.f(…)` compiles to when `x` is `any` (D34).
    CallDynamic { name: u32, argc: u16 },
    /// Pop `captures` values; push a closure over `functions[func]`.
    MakeClosure { func: u32, captures: u16 },
    /// Pop `len` values; push them as a list.
    MakeList { len: u32 },
    /// Pop `len` key/value pairs; push them as a map.
    MakeMap { len: u32 },
    /// Pop `len` key/value pairs; push them as a record of the declared type named by
    /// `constants[name]`.
    MakeRecord { name: u32, len: u32 },
    /// Pop a value; push whether it is a record of the type named by `constants[name]`
    /// or of one that extends it. `null` is not; a value that is no record cannot be
    /// asked, and is an error.
    TypeTest { name: u32 },
    /// Continue at `to`.
    Jump { to: u32 },
    /// Pop a condition (read as a `lem-if` does); continue at `to` when it is false.
    JumpIfFalse { to: u32 },
    /// Leave a non-null top in place and jump; pop a `null` and fall through (`??`).
    JumpUnlessNull { to: u32 },
    /// Leave a `false` top in place and jump; otherwise pop and fall through (`&&`).
    JumpIfFalseKeep { to: u32 },
    /// Leave a `true` top in place and jump; otherwise pop and fall through (`||`).
    JumpIfTrueKeep { to: u32 },
    /// Pop an iterable and open an iterator over it; jump to `exit` if it is empty.
    IterInit { exit: u32 },
    /// Push the next element (two values when `pair`), or close the iterator and jump.
    IterNext { exit: u32, pair: bool },
    /// Arithmetic negation.
    Neg,
    /// Boolean negation.
    Not,
    /// `+`, on numbers or strings.
    Add,
    /// `-`
    Sub,
    /// `*`
    Mul,
    /// `/`
    Div,
    /// `%`
    Rem,
    /// `==`
    Eq,
    /// `!=`
    Ne,
    /// `lt`
    Lt,
    /// `lte`
    Le,
    /// `gt`
    Gt,
    /// `gte`
    Ge,
    /// `in`: list membership or map key.
    In,
    /// Pop a value and append it to the output, escaped by type.
    Emit,
    /// Append `constants[index]`, a text run, verbatim.
    EmitText { index: u32 },
    /// Start capturing output into a fresh buffer.
    Capture,
    /// Stop capturing; push the buffer as `html`.
    EndCapture,
    /// Pop the return value and leave the frame.
    Return,
}

/// A compiled function: a `lem-fn`, a lambda, or a file's top-level items.
#[derive(Clone, Debug, PartialEq)]
pub struct Function {
    /// The name reported in messages: the `lem-fn`'s, `<lambda>`, or the file's path.
    pub name: String,
    /// Parameter names, in declaration order — the slots arguments land in.
    pub params: Vec<Rc<str>>,
    /// Which slot each captured value lands in, in capture order.
    pub captures: Vec<u16>,
    /// Index into the module's file table, for error paths.
    pub file: u32,
    /// How many local slots a frame needs, parameters and captures included.
    pub n_locals: u16,
    /// The instructions.
    pub code: Vec<Op>,
    /// The source each instruction was compiled from, parallel to `code`.
    pub spans: Vec<Span>,
}

/// A compiled program: every file of a site, sharing one constant pool.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Module {
    /// Source paths, indexed by `Function::file`.
    pub files: Vec<String>,
    /// The entry function of each file, parallel to `files`.
    pub entries: Vec<u32>,
    /// Literals, text runs, and names.
    pub constants: Vec<Value>,
    /// Every function; indices are what `Call`, `Global`, and `MakeClosure` name.
    pub functions: Vec<Function>,
    /// Every declared record with the types it is: itself, then the records it extends,
    /// nearest first. Closed at link, so `TypeTest` answers with one lookup and no walk
    /// — and neither a cycle nor a base the project lost can be spelled here.
    pub types: Vec<(Rc<str>, Vec<Rc<str>>)>,
    /// Every project-flat function by name — what a string in loaded data resolves
    /// through where a function type is expected (`layout: article`).
    pub exports: Vec<(Rc<str>, u32)>,
    /// What the loader runs for each declared record: its defaults and `by` indexes,
    /// in dependency order (∆iCW).
    pub records: Vec<RecordPlan>,
    /// One plan per ambient value the host reads from a file the program has, in the
    /// host's declaration order.
    pub bindings: Vec<Bindings>,
}

impl Module {
    /// Whether a record of the declared type `ty` is a `name`: itself, or a record it
    /// extends. The table is the whole answer — every record a module can build has a
    /// row — so a tag with no row is no type at all rather than a type that is itself.
    #[must_use]
    pub fn is_a(&self, ty: &str, name: &str) -> bool {
        self.types
            .iter()
            .find(|(declared, _)| &**declared == ty)
            .is_some_and(|(_, chain)| chain.iter().any(|ancestor| &**ancestor == name))
    }

    /// The entry function of the file at `path`, if the module has that file.
    #[must_use]
    pub fn entry(&self, path: &str) -> Option<u32> {
        self.files
            .iter()
            .position(|p| p == path)
            .map(|i| self.entries[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_every_op_from_one_list() {
        assert_eq!(Op::Emit.name(), "Emit");
        assert_eq!(Op::Const { index: 3 }.name(), "Const");
        assert!(Op::NAMES.contains(&"IterNext"));
        assert_eq!(Op::NAMES.len(), 43);
    }

    #[test]
    fn answers_a_type_test_from_the_closed_ancestry_table() {
        let module = Module {
            types: vec![
                (Rc::from("Page"), vec![Rc::from("Page")]),
                (
                    Rc::from("Article"),
                    vec![Rc::from("Article"), Rc::from("Page")],
                ),
                (
                    Rc::from("LinkPost"),
                    vec![Rc::from("LinkPost"), Rc::from("Article"), Rc::from("Page")],
                ),
            ],
            ..Module::default()
        };
        assert!(module.is_a("LinkPost", "LinkPost"));
        assert!(module.is_a("LinkPost", "Article"));
        assert!(module.is_a("LinkPost", "Page"));
        assert!(!module.is_a("Page", "Article"));
        // A tag the table does not cover is no type: answering "itself" would let a
        // module whose constant pool went wrong pass a test against its own bad tag.
        assert!(!module.is_a("Unknown", "Page"));
        assert!(!module.is_a("Unknown", "Unknown"));
    }

    #[test]
    fn finds_a_file_entry_by_path() {
        let module = Module {
            files: vec!["a.lem".to_owned(), "b.lem".to_owned()],
            entries: vec![0, 1],
            ..Module::default()
        };
        assert_eq!(module.entry("b.lem"), Some(1));
        assert_eq!(module.entry("c.lem"), None);
    }
}

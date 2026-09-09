//! The bytecode a module is compiled to and the VM runs.
//!
//! A [`Module`] is plain data — constants, functions, a file table, the project's type
//! declarations, and the loader's plans — so that it can be serialised for
//! `exportModule` later (`∆TNT`) without the VM changing. Every instruction in a
//! function has a [`Span`] beside it at the same index: that parallel table is how a
//! runtime error points at source (invariant 8).

use std::rc::Rc;

use lemmon_syntax::Span;

use crate::Value;
use crate::decls::{Declaration, Decls, Shape};
use crate::plan::{Bindings, RecordPlan};
use crate::types::Type;

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

/// A project-flat function as loaded data may name it: `layout: article` resolves
/// through the module's table of these, and the type is what the string is checked
/// against where a function type is expected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exported {
    pub name: Rc<str>,
    /// The function, in the module.
    pub func: u32,
    pub ty: Type,
}

/// A type declaration with the file that made it — the project's declaration table,
/// carried by the module so the loader can validate against it and point at it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declared {
    /// Index into the module's file table.
    pub file: u32,
    pub decl: Declaration,
}

/// What a document is to the loader: the record the host compiled it as, where that
/// was named, and its prose as rendered — what `body` is in a default (D37).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    /// The record this document's content is, if the host named one (D30).
    pub record: Option<Rc<str>>,
    /// Where the record was named: the `type:` key's value when the host said, else
    /// the front matter.
    pub record_span: Span,
    /// The prose, rendered, with what the document's blocks would emit absent.
    pub body: Rc<str>,
}

/// A compiled program: every file of a site, sharing one constant pool.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Module {
    /// Source paths, indexed by `Function::file`.
    pub files: Vec<String>,
    /// The entry function of each file, parallel to `files`.
    pub entries: Vec<u32>,
    /// What each file is to the loader, parallel to `files`: a document's record and
    /// rendered prose, or `None` for a template.
    pub documents: Vec<Option<Document>>,
    /// Literals, text runs, and names.
    pub constants: Vec<Value>,
    /// Every function; indices are what `Call`, `Global`, and `MakeClosure` name.
    pub functions: Vec<Function>,
    /// Every type the project declares, records and enums, with the file that declares
    /// it. The one table the VM's type tests and the loader's validation both read, so
    /// they cannot disagree about what a name is.
    pub declarations: Vec<Declared>,
    /// Every project-flat function by name — what a string in loaded data resolves
    /// through where a function type is expected (`layout: article`).
    pub exports: Vec<Exported>,
    /// What the loader runs for each declared record: its defaults and `by` indexes,
    /// in dependency order (∆iCW).
    pub records: Vec<RecordPlan>,
    /// One plan per ambient value the host reads from a file the program has, in the
    /// host's declaration order.
    pub bindings: Vec<Bindings>,
}

impl Module {
    /// Whether a record of the declared type `ty` is a `name`: itself, or a record it
    /// extends. The declaration table is the whole answer — every record a module can
    /// build is declared in it — so a tag it has no row for is no type at all rather
    /// than a type that is itself.
    #[must_use]
    pub fn is_a(&self, ty: &str, name: &str) -> bool {
        let mut next = Some(ty);
        // A walk up `extends` links, collecting nothing: `TypeTest` asks this on every
        // test at render. A chain visits no more declarations than the table holds, so
        // a longer walk is a cycle a failing file introduced, and ends as a missing
        // base does — with no answer but `false`.
        for _ in 0..=self.declarations.len() {
            let Some(decl) = next.and_then(|current| self.declaration(current)) else {
                return false;
            };
            if &*decl.name == name {
                return true;
            }
            next = match &decl.shape {
                Shape::Record {
                    base: Some((base, _)),
                    ..
                } => Some(base),
                _ => None,
            };
        }
        false
    }

    /// The entry function of the file at `path`, if the module has that file.
    #[must_use]
    pub fn entry(&self, path: &str) -> Option<u32> {
        self.file(path).map(|i| self.entries[i])
    }

    /// The index of the file at `path` in the file table, if the module has it.
    #[must_use]
    pub fn file(&self, path: &str) -> Option<usize> {
        self.files.iter().position(|p| p == path)
    }

    /// What the file at `path` is to the loader, if it is a document the module has.
    #[must_use]
    pub fn document(&self, path: &str) -> Option<&Document> {
        self.file(path).and_then(|i| self.documents[i].as_ref())
    }

    /// The project-flat function called `name`, if there is one.
    #[must_use]
    pub fn export(&self, name: &str) -> Option<&Exported> {
        self.exports.iter().find(|export| &*export.name == name)
    }

    /// The declaration of `name` with the file that made it, if the project declares it.
    #[must_use]
    pub fn declared(&self, name: &str) -> Option<&Declared> {
        self.declarations
            .iter()
            .find(|declared| &*declared.decl.name == name)
    }

    /// The file that declared `name`, if the project declares it.
    #[must_use]
    pub fn declared_in(&self, name: &str) -> Option<u32> {
        self.declared(name).map(|declared| declared.file)
    }
}

impl Decls for Module {
    fn declaration(&self, name: &str) -> Option<&Declaration> {
        self.declared(name).map(|declared| &declared.decl)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decls::tests::record;

    #[test]
    fn names_every_op_from_one_list() {
        assert_eq!(Op::Emit.name(), "Emit");
        assert_eq!(Op::Const { index: 3 }.name(), "Const");
        assert!(Op::NAMES.contains(&"IterNext"));
        assert_eq!(Op::NAMES.len(), 43);
    }

    #[test]
    fn answers_a_type_test_from_the_declaration_table() {
        let module = Module {
            files: vec!["types.lem".to_owned()],
            declarations: [
                record("Page", None, &[]),
                record("Article", Some("Page"), &[]),
                record("LinkPost", Some("Article"), &[]),
            ]
            .into_iter()
            .map(|decl| Declared { file: 0, decl })
            .collect(),
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
        assert_eq!(module.declared_in("Article"), Some(0));
        assert_eq!(module.declared_in("Unknown"), None);
    }

    #[test]
    fn ends_a_type_test_at_a_missing_base_or_a_cycle() {
        // Neither can come out of a clean link; a resident subtype outlives its base's
        // file failing to recompile, and the test must still answer.
        let module = Module {
            files: vec!["types.lem".to_owned()],
            declarations: [
                record("Orphan", Some("Gone"), &[]),
                record("A", Some("B"), &[]),
                record("B", Some("A"), &[]),
            ]
            .into_iter()
            .map(|decl| Declared { file: 0, decl })
            .collect(),
            ..Module::default()
        };
        assert!(module.is_a("Orphan", "Orphan"));
        assert!(!module.is_a("Orphan", "Gone"));
        assert!(module.is_a("A", "B"));
        assert!(!module.is_a("A", "C"));
    }

    #[test]
    fn finds_a_document_and_an_export_by_name() {
        let module = Module {
            files: vec!["a.lem".to_owned(), "b.md".to_owned()],
            documents: vec![
                None,
                Some(Document {
                    record: Some(Rc::from("Article")),
                    record_span: Span::new(4, 11),
                    body: Rc::from("<p>b</p>\n"),
                }),
            ],
            exports: vec![Exported {
                name: Rc::from("f"),
                func: 3,
                ty: Type::function(vec![], Type::Html),
            }],
            ..Module::default()
        };
        assert_eq!(module.document("a.lem"), None);
        assert_eq!(
            module.document("b.md").and_then(|d| d.record.as_deref()),
            Some("Article")
        );
        assert_eq!(module.document("c.md"), None);
        assert_eq!(module.export("f").map(|e| e.func), Some(3));
        assert_eq!(module.export("g"), None);
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

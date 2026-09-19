//! The project's one export index: which file defines each project-flat function and
//! each project-flat type, and the signature or declaration the rest compile against.
//!
//! One index serves every stage that needs to know what a name means across files —
//! [`compile_unit`](crate::compile_unit) resolving calls and annotations,
//! [`link`](crate::link()) resolving imports — so the answer cannot differ between them.
//! It is built by offering each file's exports in turn, and that order is the whole
//! duplicate rule: a name another file already owns stays that file's, and the file
//! offering it is the one blamed. A resident core withdraws the files it is handed,
//! then offers them after the ones it already holds, so a collision is the edit's, not
//! whichever path happens to sort first — and a name that moved between two files in
//! one edit lands in its new home. Types are a namespace of their own (D23): a
//! function and a type may share a name.

use std::collections::BTreeMap;
use std::rc::Rc;

use crate::decls::{Declaration, Decls};
use crate::unit::{ExportedSignature, Unit};

/// A project-flat function: the file that defines it, and its signature there.
#[derive(Clone, Debug, PartialEq)]
pub struct Export {
    /// The file whose definition the project kept.
    pub path: String,
    /// The signature callers compile against — never the body (D25).
    pub signature: ExportedSignature,
}

/// A project-flat type: the file that declares it, and its declaration there.
#[derive(Clone, Debug, PartialEq)]
pub struct TypeExport {
    /// The file whose declaration the project kept.
    pub path: String,
    /// The declaration every file compiles against.
    pub decl: Declaration,
}

/// Every project-flat function signature and type declaration, by name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExportIndex {
    entries: BTreeMap<Rc<str>, Export>,
    types: BTreeMap<Rc<str>, TypeExport>,
}

impl ExportIndex {
    /// An index of nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The index `units` make. Order-independent: a definition a unit was refused never
    /// lands as a unit, so no two resident units export the same name.
    #[must_use]
    pub fn of<'a>(units: impl IntoIterator<Item = &'a Unit>) -> Self {
        let mut index = Self::new();
        for unit in units {
            index.offer(&unit.path, &unit.exports);
            index.offer_types(&unit.path, &unit.types);
        }
        index
    }

    /// Withdraws everything `path` defines and declares. A batch of files is withdrawn
    /// before any of it is offered, so that a name one file of the batch gave up and
    /// another took is nobody's when the taker offers it.
    pub fn withdraw(&mut self, path: &str) {
        self.entries.retain(|_, export| export.path != path);
        self.types.retain(|_, export| export.path != path);
    }

    /// Withdraws every function `path` defines, then offers its `exports`: each name no
    /// other file owns becomes this file's.
    ///
    /// Withdrawing first is what lets a file be recompiled without colliding with
    /// itself; offering after is what makes the offering file the loser of a collision.
    /// Refusals are silent here — the file's own compile reports them, so that a
    /// collision is reported once, by the call that introduced it.
    pub fn offer(&mut self, path: &str, exports: &[ExportedSignature]) {
        self.entries.retain(|_, export| export.path != path);
        for signature in exports {
            self.entries
                .entry(signature.name.clone())
                .or_insert_with(|| Export {
                    path: path.to_owned(),
                    signature: signature.clone(),
                });
        }
    }

    /// Withdraws every type `path` declares, then offers its `decls`: each name no
    /// other file owns becomes this file's. The same rule as [`offer`](Self::offer), in
    /// the type namespace.
    pub fn offer_types(&mut self, path: &str, decls: &[Declaration]) {
        self.types.retain(|_, export| export.path != path);
        for decl in decls {
            self.types
                .entry(decl.name.clone())
                .or_insert_with(|| TypeExport {
                    path: path.to_owned(),
                    decl: decl.clone(),
                });
        }
    }

    /// The function the project knows as `name`.
    #[must_use]
    pub fn lookup(&self, name: &str) -> Option<&Export> {
        self.entries.get(name)
    }

    /// Whether the function the project knows as `name` is the one `path` defines.
    #[must_use]
    pub fn defined_in(&self, name: &str, path: &str) -> bool {
        self.lookup(name).is_some_and(|export| export.path == path)
    }

    /// The type the project knows as `name`.
    #[must_use]
    pub fn lookup_type(&self, name: &str) -> Option<&TypeExport> {
        self.types.get(name)
    }

    /// Every type the project knows, by name.
    pub fn types(&self) -> impl Iterator<Item = &TypeExport> {
        self.types.values()
    }

    /// Every function the project knows, by name.
    pub fn exports(&self) -> impl Iterator<Item = &Export> {
        self.entries.values()
    }
}

impl Decls for ExportIndex {
    fn declaration(&self, name: &str) -> Option<&Declaration> {
        self.lookup_type(name).map(|export| &export.decl)
    }
}

#[cfg(test)]
mod tests {
    use lemmon_syntax::Span;

    use super::*;
    use crate::decls::tests::record;
    use crate::types::Type;

    /// A signature for `name` at a span that tells the definitions apart.
    fn signature(name: &str, start: u32) -> ExportedSignature {
        ExportedSignature {
            name: Rc::from(name),
            params: Vec::new(),
            ret: Type::Html,
            func: 1,
            name_span: Span::new(start, start + 1),
        }
    }

    #[test]
    fn keeps_the_definition_of_the_file_that_offered_first() {
        let mut index = ExportIndex::new();
        index.offer("a.lem", &[signature("f", 1)]);
        index.offer("b.lem", &[signature("f", 2)]);
        let kept = index.lookup("f").expect("f is defined");
        assert_eq!(kept.path, "a.lem");
        assert_eq!(kept.signature.name_span, Span::new(1, 2));
    }

    #[test]
    fn lets_a_file_offer_a_name_it_already_owns() {
        // Recompiling a file offers its exports again; without the withdrawal it would
        // collide with itself and lose its own name.
        let mut index = ExportIndex::new();
        index.offer("a.lem", &[signature("f", 1)]);
        index.offer("a.lem", &[signature("f", 9)]);
        let kept = index.lookup("f").expect("f is defined");
        assert_eq!(kept.path, "a.lem");
        assert_eq!(kept.signature.name_span, Span::new(9, 10));
    }

    #[test]
    fn frees_a_name_the_file_that_owned_it_no_longer_defines() {
        let mut index = ExportIndex::new();
        index.offer("a.lem", &[signature("f", 1)]);
        index.offer("a.lem", &[signature("g", 2)]);
        assert_eq!(index.lookup("f"), None);
        assert!(index.defined_in("g", "a.lem"));
    }

    #[test]
    fn reports_a_name_as_defined_only_in_the_file_that_owns_it() {
        let mut index = ExportIndex::new();
        index.offer("a.lem", &[signature("f", 1)]);
        index.offer("b.lem", &[signature("f", 2)]);
        assert!(index.defined_in("f", "a.lem"));
        assert!(!index.defined_in("f", "b.lem"));
        assert!(!index.defined_in("nope", "a.lem"));
        let names: Vec<&str> = index.exports().map(|e| &*e.signature.name).collect();
        assert_eq!(names, ["f"]);
    }

    #[test]
    fn keeps_types_in_a_namespace_of_their_own_under_the_same_rule() {
        let mut index = ExportIndex::new();
        index.offer("a.lem", &[signature("Article", 1)]);
        index.offer_types("a.lem", &[record("Article", None, &[])]);
        index.offer_types("b.lem", &[record("Article", Some("Page"), &[])]);
        assert!(index.defined_in("Article", "a.lem"));
        assert_eq!(index.lookup_type("Article").unwrap().path, "a.lem");
        assert_eq!(index.kind("Article"), Some(crate::decls::DeclKind::Record));
        assert!(!index.extends("Article", "Page"));
        assert_eq!(index.lookup_type("Nope"), None);
        let names: Vec<&str> = index.types().map(|t| &*t.decl.name).collect();
        assert_eq!(names, ["Article"]);
    }

    #[test]
    fn frees_a_type_the_file_that_owned_it_no_longer_declares() {
        // Offering again withdraws first, so a file's types never collide with their
        // own previous offer, and a type it no longer declares is freed — for the next
        // offer of it, not for the one that was refused.
        let mut index = ExportIndex::new();
        index.offer_types("a.lem", &[record("Article", None, &[])]);
        index.offer_types("b.lem", &[record("Article", Some("Page"), &[])]);
        index.offer_types("a.lem", &[record("Page", None, &[])]);
        assert_eq!(index.lookup_type("Article"), None);
        index.offer_types("b.lem", &[record("Article", Some("Page"), &[])]);
        assert_eq!(index.lookup_type("Article").unwrap().path, "b.lem");
        assert!(index.extends("Article", "Page"));
    }

    #[test]
    fn lets_a_name_move_between_two_files_withdrawn_together() {
        // The new home is offered first, while the old one still owns the name: with
        // both withdrawn up front the name is nobody's, and the new home takes it.
        let mut index = ExportIndex::new();
        index.offer("a.lem", &[signature("f", 1)]);
        index.offer_types("a.lem", &[record("Article", None, &[])]);
        for path in ["b.lem", "a.lem"] {
            index.withdraw(path);
        }
        index.offer("b.lem", &[signature("f", 2)]);
        index.offer_types("b.lem", &[record("Article", None, &[])]);
        index.offer("a.lem", &[]);
        index.offer_types("a.lem", &[]);
        assert!(index.defined_in("f", "b.lem"));
        assert_eq!(index.lookup_type("Article").unwrap().path, "b.lem");
    }
}

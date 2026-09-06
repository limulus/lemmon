//! The project's one export index: which file defines each project-flat function, and
//! the signature its callers compile against.
//!
//! One index serves every stage that needs to know what a name means across files —
//! [`compile_unit`](crate::compile_unit) resolving calls, [`link`](crate::link)
//! resolving imports — so the answer cannot differ between them. It is built by
//! offering each file's exports in turn, and that order is the whole duplicate rule: a
//! name another file already owns stays that file's, and the file offering it is the
//! one blamed. A resident core offers the files it is handed after the ones it already
//! holds, so a collision is the edit's, not whichever path happens to sort first.

use std::collections::BTreeMap;
use std::rc::Rc;

use crate::unit::{ExportedSignature, Unit};

/// A project-flat function: the file that defines it, and its signature there.
#[derive(Clone, Debug, PartialEq)]
pub struct Export {
    /// The file whose definition the project kept.
    pub path: String,
    /// The signature callers compile against — never the body (D25).
    pub signature: ExportedSignature,
}

/// Every project-flat function signature, by name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExportIndex {
    entries: BTreeMap<Rc<str>, Export>,
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
        }
        index
    }

    /// Withdraws everything `path` defines, then offers its `exports`: each name no
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
}

#[cfg(test)]
mod tests {
    use lemmon_syntax::Span;

    use super::*;

    /// A signature for `name` at a span that tells the definitions apart.
    fn signature(name: &str, start: u32) -> ExportedSignature {
        ExportedSignature {
            name: Rc::from(name),
            params: Vec::new(),
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
    }
}

//! Name binding and resolution — the one place every rule about names lives (D23,
//! `docs/language.md` §Names).
//!
//! [`Names::bind_function`] admits or rejects a file's top-level `lem-fn`s and
//! [`Names::bind_local`] everything a function binds — parameters, `lem-for` loop
//! bindings, `lem-let` definitions — applying the rules in one fixed order: an ambient
//! name cannot be redefined; a name bound twice in one scope is an error; a definition
//! that displaces a stdlib function warns. [`Names::resolve`] and [`Names::peek`] look a
//! name up in one fixed order: locals and parameters, innermost first, through any
//! enclosing lambdas (which capture what they reach); then the project-flat functions
//! every file's `lem-fn`s define; then the stdlib; then the ambient values the host binds
//! at render.
//!
//! Nothing here depends on bytecode or on the syntax tree: codegen is one caller, and the
//! type checker (∆C8d) binds and resolves through the same entry points.

use std::rc::Rc;

use lemmon_syntax::Span;

use super::{Diagnostic, SignatureTable};
use crate::builtins;

/// The ambient record names every template may read. Bound at render time and never
/// redefinable — a binding or definition that takes one of these names is an error, not
/// a shadow (D23). They are the standard shell's until the host declares them (∆Iau).
const AMBIENT: &[&str] = &["site", "page", "build"];

/// What a local binding is, which decides whether the stdlib-shadowing rule applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BindingKind {
    /// A `lem-let`.
    Definition,
    /// A `lem-fn`'s or lambda's parameter.
    Parameter,
    /// A `lem-for`'s value, key, or index.
    Loop,
}

/// How a name resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Resolved {
    Local(u16),
    /// Index into the [`SignatureTable`]'s entries.
    Function(usize),
    Builtin(u32),
    Ambient,
}

/// A local bound to a slot of its frame, with its name's own bytes — where a repeat's
/// label points.
struct Binding {
    name: Rc<str>,
    span: Span,
    slot: u16,
}

/// One lexical scope's bindings, in binding order.
#[derive(Default)]
struct Scope(Vec<Binding>);

impl Scope {
    fn find(&self, name: &str) -> Option<&Binding> {
        self.0.iter().rev().find(|b| &*b.name == name)
    }
}

/// A name a lambda closes over: bound in its own frame to a slot the enclosing frame's
/// fills at the closure's creation.
pub(crate) struct Capture {
    name: Rc<str>,
    /// The slot in the enclosing frame.
    pub(crate) parent_slot: u16,
    /// The slot here.
    pub(crate) slot: u16,
}

/// One of this file's top-level `lem-fn`s, with its name's own bytes — where a
/// repeat's label points.
struct Export {
    name: Rc<str>,
    span: Span,
}

/// The name half of a function being compiled: its lexical scopes, how many slots it
/// has handed out, and what it closes over.
pub(crate) struct Frame {
    /// Innermost last. The first holds the parameters.
    scopes: Vec<Scope>,
    n_locals: u16,
    /// Every name closed over, in the order they were reached. Visible throughout the
    /// function, after its scopes: a name is captured only where no scope had it.
    captures: Vec<Capture>,
}

impl Frame {
    fn new() -> Self {
        Self {
            scopes: vec![Scope::default()],
            n_locals: 0,
            captures: Vec::new(),
        }
    }

    fn lookup(&self, name: &str) -> Option<u16> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.find(name))
            .map(|binding| binding.slot)
            .or_else(|| {
                self.captures
                    .iter()
                    .find(|c| &*c.name == name)
                    .map(|c| c.slot)
            })
    }

    fn innermost(&self) -> &Scope {
        self.scopes.last().expect("a scope")
    }

    fn alloc(&mut self) -> u16 {
        let slot = self.n_locals;
        self.n_locals = slot.checked_add(1).expect("fewer than 65536 locals");
        slot
    }

    /// Binds `name` to a slot that a capture fills from `parent_slot` in the enclosing
    /// frame.
    fn capture(&mut self, name: &str, parent_slot: u16) -> u16 {
        let slot = self.alloc();
        self.captures.push(Capture {
            name: Rc::from(name),
            parent_slot,
            slot,
        });
        slot
    }

    /// How many slots the function needs.
    pub(crate) fn n_locals(&self) -> u16 {
        self.n_locals
    }

    /// Every name closed over, in the order they were reached.
    pub(crate) fn captures(&self) -> &[Capture] {
        &self.captures
    }
}

/// The names visible while one file compiles, and everything the file binds.
pub(crate) struct Names<'a> {
    /// The path of the file — what every diagnostic names.
    path: &'a str,
    /// Every file's signatures; `None` while this file's exports are being collected,
    /// before the table exists.
    table: Option<&'a SignatureTable>,
    /// This file's top-level `lem-fn`s, in order — the list a repeated one is checked
    /// against. Never consulted by lookup: the table covers them once it exists.
    file: Vec<Export>,
    /// The function being compiled, then every lambda open inside it, innermost last.
    frames: Vec<Frame>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Names<'a> {
    pub(crate) fn new(path: &'a str, table: Option<&'a SignatureTable>) -> Self {
        Self {
            path,
            table,
            file: Vec::new(),
            frames: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    pub(crate) fn push_frame(&mut self) {
        self.frames.push(Frame::new());
    }

    pub(crate) fn pop_frame(&mut self) -> Frame {
        self.frames.pop().expect("a frame")
    }

    fn frame(&mut self) -> &mut Frame {
        self.frames.last_mut().expect("a frame")
    }

    pub(crate) fn push_scope(&mut self) {
        self.frame().scopes.push(Scope::default());
    }

    pub(crate) fn pop_scope(&mut self) {
        self.frame().scopes.pop();
    }

    /// Everything the rules reported, in the order they were applied.
    pub(crate) fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }

    /// Binds a top-level `lem-fn` named `name`, whose name's own bytes are `span`, and
    /// says whether it was admitted. The rules in order, the first one broken reported:
    /// an ambient name cannot be redefined; a name this file already defines cannot be
    /// defined again.
    ///
    /// Whether the function displaces a stdlib function is known only once every file's
    /// functions are: the project keeps one function per name, and the warning belongs
    /// to the file whose definition it kept. So the same call made again with the table
    /// in hand raises it — a file's exports are bound once to be collected and once to
    /// be compiled.
    pub(crate) fn bind_function(&mut self, name: &str, span: Span) -> bool {
        let first = self
            .file
            .iter()
            .rev()
            .find(|e| &*e.name == name)
            .map(|e| e.span);
        let repeat =
            first.map(|first| duplicate_named(self.path, span, name, self.path, first));
        if let Some(refusal) = self.refusal(name, span, repeat) {
            self.diagnostics.push(refusal);
            return false;
        }
        if self.function_shadows_builtin(name) {
            self.diagnostics.push(self.shadow_warning(name, span));
        }
        self.file.push(Export {
            name: Rc::from(name),
            span,
        });
        true
    }

    /// Binds `name`, whose own bytes are `span`, as `kind` in the innermost scope of the
    /// innermost frame, and returns its slot. The rules in order, the first one broken
    /// reported: an ambient name cannot be redefined; a name already bound in the same
    /// scope cannot be bound again; a definition that displaces a stdlib function may,
    /// with a warning. A parameter or loop binding named after the stdlib is not a
    /// shadow, and neither is a definition displacing a project function, a parameter,
    /// or an outer local.
    ///
    /// A refused binding still takes its slot, so its binder can compile a store into it
    /// and the parameters after it stay where a call lays its arguments — but it is not
    /// recorded, so no reference reaches it.
    pub(crate) fn bind_local(&mut self, name: &str, span: Span, kind: BindingKind) -> u16 {
        let first = self.frame().innermost().find(name).map(|b| b.span);
        let slot = self.frame().alloc();
        let repeat = first.map(|first| {
            Diagnostic::error(self.path, span, format!("`{name}` is already bound here"))
                .with_label(self.path, first, "first bound here")
        });
        if let Some(refusal) = self.refusal(name, span, repeat) {
            self.diagnostics.push(refusal);
            return slot;
        }
        if kind == BindingKind::Definition && self.local_shadows_builtin(name) {
            self.diagnostics.push(self.shadow_warning(name, span));
        }
        self.frame()
            .scopes
            .last_mut()
            .expect("a scope")
            .0
            .push(Binding {
                name: Rc::from(name),
                span,
                slot,
            });
        slot
    }

    /// The rules that refuse a binding, in order: the ambient rule, then whatever
    /// `repeat` the caller found in the target scope. The first one broken.
    fn refusal(
        &self,
        name: &str,
        span: Span,
        repeat: Option<Diagnostic>,
    ) -> Option<Diagnostic> {
        if AMBIENT.contains(&name) {
            return Some(Diagnostic::error(
                self.path,
                span,
                format!("`{name}` is provided by the shell and cannot be redefined"),
            ));
        }
        repeat
    }

    fn shadow_warning(&self, name: &str, span: Span) -> Diagnostic {
        Diagnostic::warning(
            self.path,
            span,
            format!("`{name}` shadows the stdlib function of the same name"),
        )
    }

    /// Whether a definition of `name` here displaces a stdlib function: one exists, and
    /// nothing nearer — a local in any open frame, a project function — already means
    /// the name. The stdlib is asked first because almost no name is in it.
    fn local_shadows_builtin(&self, name: &str) -> bool {
        builtins::lookup(name).is_some()
            && matches!(self.peek(name), Some(Resolved::Builtin(_)))
    }

    /// Whether this file's `lem-fn` `name` displaces a stdlib function: one exists, and
    /// the table kept this file's definition of the name. Before the table exists
    /// nothing is known to be displaced.
    fn function_shadows_builtin(&self, name: &str) -> bool {
        builtins::lookup(name).is_some()
            && self
                .table
                .is_some_and(|table| table.defined_in(name, self.path))
    }

    /// What `name` means here, touching nothing: a local in any open frame (whose slot
    /// is that frame's, not necessarily usable here), else what [`outer`](Self::outer)
    /// finds.
    ///
    /// The UFCS probe asks about a name it may not end up compiling as a call, and a
    /// lookup that recorded a capture on the way past an enclosing frame would leave the
    /// slot behind whatever the answer was.
    pub(crate) fn peek(&self, name: &str) -> Option<Resolved> {
        self.frames
            .iter()
            .rev()
            .find_map(|frame| frame.lookup(name))
            .map_or_else(|| self.outer(name), |slot| Some(Resolved::Local(slot)))
    }

    /// [`peek`](Self::peek) narrowed to what a call may name: a function or a builtin. A
    /// local shadows either, and an ambient value is not callable.
    pub(crate) fn peek_callable(&self, name: &str) -> Option<Resolved> {
        self.peek(name)
            .filter(|found| matches!(found, Resolved::Function(_) | Resolved::Builtin(_)))
    }

    /// What `name` means in the innermost frame: a local's slot usable there, capturing
    /// through every frame crossed to reach a binding in an enclosing one; else what
    /// [`outer`](Self::outer) finds.
    pub(crate) fn resolve(&mut self, name: &str) -> Option<Resolved> {
        let local = self
            .frames
            .iter()
            .enumerate()
            .rev()
            .find_map(|(level, frame)| frame.lookup(name).map(|slot| (level, slot)));
        let Some((hit, mut slot)) = local else {
            return self.outer(name);
        };
        for level in hit + 1..self.frames.len() {
            slot = self.frames[level].capture(name, slot);
        }
        Some(Resolved::Local(slot))
    }

    /// The names beyond the frames, in the order they are consulted: the project-flat
    /// functions, the stdlib, the ambient values.
    fn outer(&self, name: &str) -> Option<Resolved> {
        if let Some(sig) = self.table.and_then(|table| table.position(name)) {
            return Some(Resolved::Function(sig));
        }
        if let Some(id) = builtins::lookup(name) {
            return Some(Resolved::Builtin(id));
        }
        AMBIENT.contains(&name).then_some(Resolved::Ambient)
    }
}

/// The diagnostic a duplicate definition gets wherever it is found — the same words when
/// two files compiled together collide as when two units disagree at link time — with
/// the first definition's name as its label. Labels point at what the primary span
/// points at: the name, never the whole definition.
pub(crate) fn duplicate_named(
    path: &str,
    name_span: Span,
    name: &str,
    first_path: &str,
    first_span: Span,
) -> Diagnostic {
    Diagnostic::error(path, name_span, format!("`{name}` is already defined")).with_label(
        first_path,
        first_span,
        "first defined here",
    )
}

#[cfg(test)]
mod tests {
    use lemmon_syntax::Severity;

    use super::*;
    use crate::compile::index_u32;
    use crate::unit::ExportedSignature;

    /// A signature table exporting `names` from `lib.lem`.
    fn table(names: &[&str]) -> SignatureTable {
        let exports: Vec<ExportedSignature> = names
            .iter()
            .enumerate()
            .map(|(i, name)| ExportedSignature {
                name: Rc::from(*name),
                params: Vec::new(),
                func: index_u32(i + 1),
                name_span: at(0),
            })
            .collect();
        SignatureTable::build([("lib.lem", exports.as_slice())]).0
    }

    fn at(start: u32) -> Span {
        Span::new(start, start + 1)
    }

    /// Names for `t.lem` with one frame open, resolving against `table`.
    fn in_frame(table: Option<&SignatureTable>) -> Names<'_> {
        let mut names = Names::new("t.lem", table);
        names.push_frame();
        names
    }

    fn shown(diagnostics: &[Diagnostic]) -> Vec<String> {
        diagnostics.iter().map(ToString::to_string).collect()
    }

    /// A frame's captures as `(slot in the enclosing frame, slot here)` pairs.
    fn pairs(frame: &Frame) -> Vec<(u16, u16)> {
        frame
            .captures()
            .iter()
            .map(|c| (c.parent_slot, c.slot))
            .collect()
    }

    const SHADOWS: &str =
        "t.lem:0..1: warning: `raw` shadows the stdlib function of the same name";

    #[test]
    fn binds_parameters_to_consecutive_slots_in_declaration_order() {
        let mut names = in_frame(None);
        assert_eq!(names.bind_local("a", at(0), BindingKind::Parameter), 0);
        assert_eq!(names.bind_local("b", at(2), BindingKind::Parameter), 1);
        assert_eq!(names.bind_local("c", at(4), BindingKind::Parameter), 2);
        assert_eq!(names.pop_frame().n_locals(), 3);
    }

    #[test]
    fn keeps_a_refused_parameters_slot_so_later_parameters_stay_aligned() {
        // The VM lays a call's arguments out at consecutive slots, so the parameter after
        // a refused one must still be the second slot.
        let mut names = in_frame(None);
        assert_eq!(names.bind_local("page", at(0), BindingKind::Parameter), 0);
        assert_eq!(names.bind_local("x", at(6), BindingKind::Parameter), 1);
    }

    #[test]
    fn binds_a_loop_pair_to_two_distinct_slots() {
        let mut names = in_frame(None);
        names.push_scope();
        let key = names.bind_local("k", at(0), BindingKind::Loop);
        let value = names.bind_local("v", at(3), BindingKind::Loop);
        assert_ne!(key, value);
        assert_eq!(names.resolve("k"), Some(Resolved::Local(key)));
        assert_eq!(names.resolve("v"), Some(Resolved::Local(value)));
    }

    #[test]
    fn binds_a_definition_in_the_innermost_scope_only_until_it_closes() {
        let mut names = in_frame(None);
        names.push_scope();
        let slot = names.bind_local("x", at(0), BindingKind::Definition);
        assert_eq!(names.resolve("x"), Some(Resolved::Local(slot)));
        names.pop_scope();
        assert_eq!(names.resolve("x"), None);
    }

    #[test]
    fn admits_a_function_once_per_file() {
        let mut names = Names::new("t.lem", None);
        assert!(names.bind_function("f", at(0)));
        assert!(names.bind_function("g", at(2)));
        assert!(!names.bind_function("f", at(4)));
        assert!(!names.bind_function("page", at(6)));
        assert!(names.bind_function("h", at(9)));
    }

    #[test]
    fn does_not_let_a_reference_reach_a_refused_binding() {
        let mut names = in_frame(None);
        names.bind_local("page", at(0), BindingKind::Definition);
        assert_eq!(names.resolve("page"), Some(Resolved::Ambient));
        let first = names.bind_local("a", at(6), BindingKind::Definition);
        names.bind_local("a", at(8), BindingKind::Definition);
        assert_eq!(names.resolve("a"), Some(Resolved::Local(first)));
    }

    #[test]
    fn refuses_an_ambient_name_whatever_the_kind() {
        let expected = [
            "t.lem:0..1: `site` is provided by the shell and cannot be redefined",
            "t.lem:1..2: `page` is provided by the shell and cannot be redefined",
            "t.lem:2..3: `build` is provided by the shell and cannot be redefined",
        ];
        for kind in [
            BindingKind::Definition,
            BindingKind::Parameter,
            BindingKind::Loop,
        ] {
            let mut names = in_frame(None);
            for (i, name) in ["site", "page", "build"].iter().enumerate() {
                names.bind_local(name, at(index_u32(i)), kind);
            }
            assert_eq!(shown(&names.diagnostics), expected, "{kind:?}");
            assert!(
                names
                    .diagnostics
                    .iter()
                    .all(|d| d.severity == Severity::Error)
            );
        }
        let mut names = Names::new("t.lem", None);
        for (i, name) in ["site", "page", "build"].iter().enumerate() {
            assert!(
                !names.bind_function(name, at(index_u32(i))),
                "exported `{name}`"
            );
        }
        assert_eq!(shown(&names.diagnostics), expected, "function");
    }

    #[test]
    fn refuses_a_name_bound_twice_in_one_scope() {
        let mut names = in_frame(None);
        assert_eq!(names.bind_local("a", at(0), BindingKind::Parameter), 0);
        assert_eq!(names.bind_local("a", at(3), BindingKind::Parameter), 1);
        assert_eq!(
            shown(&names.diagnostics),
            ["t.lem:3..4: `a` is already bound here\n  t.lem:0..1: first bound here"]
        );
    }

    #[test]
    fn allows_a_name_rebound_in_an_inner_scope() {
        let mut names = in_frame(None);
        let outer = names.bind_local("x", at(0), BindingKind::Definition);
        names.push_scope();
        let inner = names.bind_local("x", at(4), BindingKind::Definition);
        assert_ne!(outer, inner);
        assert_eq!(names.resolve("x"), Some(Resolved::Local(inner)));
        names.pop_scope();
        assert_eq!(names.resolve("x"), Some(Resolved::Local(outer)));
        assert!(
            names.diagnostics.is_empty(),
            "{:?}",
            shown(&names.diagnostics)
        );
    }

    #[test]
    fn allows_a_parameter_rebound_by_a_definition_in_the_body_scope() {
        let mut names = in_frame(None);
        names.bind_local("a", at(0), BindingKind::Parameter);
        names.push_scope();
        assert_eq!(names.bind_local("a", at(4), BindingKind::Definition), 1);
        assert!(
            names.diagnostics.is_empty(),
            "{:?}",
            shown(&names.diagnostics)
        );
    }

    #[test]
    fn refuses_a_function_defined_twice_citing_the_first() {
        let mut names = Names::new("t.lem", None);
        names.bind_function("f", at(1));
        assert!(!names.bind_function("f", at(5)));
        assert_eq!(
            shown(&names.diagnostics),
            ["t.lem:5..6: `f` is already defined\n  t.lem:1..2: first defined here"]
        );
    }

    #[test]
    fn reports_only_the_first_rule_a_binding_breaks() {
        // An ambient name bound twice is two ambient errors, never also a repeat; a
        // stdlib name bound twice warns once and then errors, never warns again.
        let mut names = in_frame(None);
        names.bind_local("page", at(0), BindingKind::Parameter);
        names.bind_local("page", at(2), BindingKind::Parameter);
        names.bind_local("raw", at(4), BindingKind::Definition);
        names.bind_local("raw", at(6), BindingKind::Definition);
        assert_eq!(
            shown(&names.diagnostics),
            [
                "t.lem:0..1: `page` is provided by the shell and cannot be redefined",
                "t.lem:2..3: `page` is provided by the shell and cannot be redefined",
                "t.lem:4..5: warning: `raw` shadows the stdlib function of the same name",
                "t.lem:6..7: `raw` is already bound here\n  t.lem:4..5: first bound here",
            ]
        );
    }

    #[test]
    fn warns_when_a_definition_displaces_the_stdlib() {
        let mut names = in_frame(None);
        assert_eq!(names.bind_local("raw", at(0), BindingKind::Definition), 0);
        assert_eq!(shown(&names.diagnostics), [SHADOWS]);
        assert!(
            names
                .diagnostics
                .iter()
                .all(|d| d.severity == Severity::Warning)
        );
    }

    #[test]
    fn warns_for_a_function_the_project_kept_that_displaces_the_stdlib() {
        let table = table(&["raw"]);
        let mut names = Names::new("lib.lem", Some(&table));
        assert!(names.bind_function("raw", at(0)));
        assert_eq!(
            shown(&names.diagnostics),
            ["lib.lem:0..1: warning: `raw` shadows the stdlib function of the same name"]
        );
    }

    #[test]
    fn does_not_warn_for_a_function_the_project_kept_from_another_file() {
        // The repeat is an error where the table was built; this file's copy displaces
        // nothing.
        let table = table(&["raw"]);
        let mut names = Names::new("t.lem", Some(&table));
        assert!(names.bind_function("raw", at(0)));
        assert!(
            names.diagnostics.is_empty(),
            "{:?}",
            shown(&names.diagnostics)
        );
    }

    #[test]
    fn does_not_warn_for_a_function_before_the_table_exists() {
        let mut names = Names::new("t.lem", None);
        assert!(names.bind_function("raw", at(0)));
        assert!(
            names.diagnostics.is_empty(),
            "{:?}",
            shown(&names.diagnostics)
        );
    }

    #[test]
    fn does_not_warn_when_a_definition_displaces_a_project_function() {
        let table = table(&["raw"]);
        let mut names = in_frame(Some(&table));
        assert_eq!(names.bind_local("raw", at(0), BindingKind::Definition), 0);
        assert!(
            names.diagnostics.is_empty(),
            "{:?}",
            shown(&names.diagnostics)
        );
    }

    #[test]
    fn does_not_warn_when_a_definition_displaces_an_outer_local() {
        let mut names = in_frame(None);
        names.bind_local("raw", at(0), BindingKind::Parameter);
        names.push_scope();
        names.bind_local("raw", at(4), BindingKind::Definition);
        assert!(
            names.diagnostics.is_empty(),
            "{:?}",
            shown(&names.diagnostics)
        );

        // Only the outer definition warns, whatever it displaced itself.
        let mut names = in_frame(None);
        names.bind_local("raw", at(0), BindingKind::Definition);
        names.push_scope();
        names.bind_local("raw", at(4), BindingKind::Definition);
        assert_eq!(shown(&names.diagnostics), [SHADOWS]);
    }

    #[test]
    fn does_not_warn_for_a_parameter_or_loop_name_from_the_stdlib() {
        let mut names = in_frame(None);
        names.bind_local("raw", at(0), BindingKind::Parameter);
        names.push_scope();
        names.bind_local("raw", at(4), BindingKind::Loop);
        assert!(
            names.diagnostics.is_empty(),
            "{:?}",
            shown(&names.diagnostics)
        );
    }

    #[test]
    fn resolves_a_local_before_a_project_function_before_the_stdlib_before_ambient() {
        let table = table(&["raw", "f"]);
        let mut names = in_frame(Some(&table));
        names.push_scope();
        let slot = names.bind_local("raw", at(0), BindingKind::Definition);
        assert_eq!(names.resolve("raw"), Some(Resolved::Local(slot)));
        names.pop_scope();
        assert_eq!(names.resolve("raw"), Some(Resolved::Function(0)));
        assert_eq!(names.resolve("f"), Some(Resolved::Function(1)));

        let mut names = in_frame(None);
        assert_eq!(names.resolve("raw"), Some(Resolved::Builtin(0)));
        assert_eq!(names.resolve("site"), Some(Resolved::Ambient));
        assert_eq!(names.resolve("zzz"), None);
    }

    #[test]
    fn resolves_the_innermost_scope_first() {
        let mut names = in_frame(None);
        names.bind_local("x", at(0), BindingKind::Definition);
        names.push_scope();
        names.push_scope();
        let innermost = names.bind_local("x", at(4), BindingKind::Definition);
        assert_eq!(names.resolve("x"), Some(Resolved::Local(innermost)));
    }

    #[test]
    fn records_a_capture_when_a_name_comes_from_an_enclosing_frame() {
        let mut names = in_frame(None);
        names.bind_local("m", at(0), BindingKind::Definition);
        let parent = names.bind_local("n", at(2), BindingKind::Definition);
        names.push_frame();
        assert_eq!(names.resolve("n"), Some(Resolved::Local(0)));
        let inner = names.pop_frame();
        assert_eq!(pairs(&inner), [(parent, 0)]);
        assert_eq!(inner.n_locals(), 1);
        assert!(names.pop_frame().captures().is_empty());
    }

    #[test]
    fn records_a_capture_in_every_frame_crossed() {
        let mut names = in_frame(None);
        names.bind_local("outer", at(0), BindingKind::Definition);
        names.push_frame();
        names.bind_local("a", at(2), BindingKind::Parameter);
        names.push_frame();
        names.bind_local("b", at(4), BindingKind::Parameter);
        assert_eq!(names.resolve("outer"), Some(Resolved::Local(1)));
        let innermost = names.pop_frame();
        let middle = names.pop_frame();
        assert_eq!(pairs(&middle), [(0, 1)]);
        assert_eq!(pairs(&innermost), [(1, 1)]);
    }

    #[test]
    fn reuses_a_capture_already_recorded() {
        let mut names = in_frame(None);
        names.bind_local("n", at(0), BindingKind::Definition);
        names.push_frame();
        assert_eq!(names.resolve("n"), Some(Resolved::Local(0)));
        assert_eq!(names.resolve("n"), Some(Resolved::Local(0)));
        assert_eq!(pairs(&names.pop_frame()), [(0, 0)]);
    }

    #[test]
    fn peeks_without_recording_a_capture() {
        let mut names = in_frame(None);
        names.bind_local("n", at(0), BindingKind::Definition);
        names.push_frame();
        assert_eq!(names.peek("n"), Some(Resolved::Local(0)));
        let inner = names.pop_frame();
        assert!(inner.captures().is_empty());
        assert_eq!(inner.n_locals(), 0);
    }

    #[test]
    fn offers_a_call_only_a_function_or_builtin_never_a_local_or_an_ambient_value() {
        let table = table(&["f"]);
        let mut names = in_frame(Some(&table));
        assert_eq!(names.peek_callable("f"), Some(Resolved::Function(0)));
        assert_eq!(names.peek_callable("raw"), Some(Resolved::Builtin(0)));
        assert_eq!(names.peek_callable("page"), None);
        names.bind_local("raw", at(0), BindingKind::Definition);
        names.push_frame();
        assert_eq!(names.peek_callable("raw"), None);
    }

    #[test]
    fn forgets_a_frames_names_when_it_pops() {
        let mut names = in_frame(None);
        names.push_frame();
        names.bind_local("inner", at(0), BindingKind::Parameter);
        names.pop_frame();
        assert_eq!(names.resolve("inner"), None);
    }

    #[test]
    fn hands_its_diagnostics_back_in_the_order_the_rules_raised_them() {
        let mut names = in_frame(None);
        names.bind_local("raw", at(0), BindingKind::Definition);
        names.bind_local("page", at(2), BindingKind::Definition);
        assert_eq!(
            shown(&names.into_diagnostics()),
            [
                SHADOWS,
                "t.lem:2..3: `page` is provided by the shell and cannot be redefined",
            ]
        );
    }
}

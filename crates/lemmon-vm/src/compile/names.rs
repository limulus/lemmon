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
//! Every local carries its type, which is what the checker reads back when the name is
//! used, and whether it has been read: a definition or parameter that never is warns when
//! its scope closes (§Checking). Nothing here depends on bytecode or on the syntax tree.

use std::rc::Rc;

use lemmon_syntax::Span;

use super::Diagnostic;
use crate::builtins;
use crate::exports::ExportIndex;
use crate::host::Host;
use crate::types::Type;

/// What a local binding is, which decides whether the stdlib-shadowing rule and the
/// unused rule apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BindingKind {
    /// A `lem-let`.
    Definition,
    /// A `lem-let` at the top level of the file an ambient value reads its bindings
    /// from: a field of that record (D30), so it is read wherever the project reads
    /// that value and never unused here.
    Exported,
    /// A `lem-fn`'s or lambda's parameter.
    Parameter,
    /// A `lem-for`'s value, key, or index — an unused index or key is the common case,
    /// so these never warn.
    Loop,
}

/// How a name resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Resolved {
    Local {
        slot: u16,
        ty: Type,
        /// The name token of the binding this reaches — one binding, one span, so it
        /// identifies the binding wherever the name is later read (a narrowing is keyed
        /// on it, and an inner shadow has a span of its own).
        binder: Span,
    },
    /// A project-flat function, whichever file defines it — the [`ExportIndex`]
    /// answers which, by the same name that resolved here.
    Function,
    Builtin(u32),
    /// A value the host binds at render, with the type the host gave it — `any` where
    /// the host named it without typing it (D30).
    Ambient(Type),
}

/// A local bound to a slot of its frame, with its name's own bytes — where a repeat's
/// label points, and what identifies the binding.
struct Binding {
    name: Rc<str>,
    span: Span,
    slot: u16,
    kind: BindingKind,
    ty: Type,
    /// Whether any reference has reached it.
    used: bool,
}

/// One lexical scope's bindings, in binding order.
#[derive(Default)]
struct Scope(Vec<Binding>);

impl Scope {
    fn find(&self, name: &str) -> Option<&Binding> {
        self.0.iter().rev().find(|b| &*b.name == name)
    }

    fn find_mut(&mut self, name: &str) -> Option<&mut Binding> {
        self.0.iter_mut().rev().find(|b| &*b.name == name)
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
    ty: Type,
    /// The binding captured, by its name token — see [`Resolved::Local`].
    binder: Span,
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

    /// What this frame knows `name` as, touching nothing.
    fn lookup(&self, name: &str) -> Option<Resolved> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.find(name))
            .map(|binding| Resolved::Local {
                slot: binding.slot,
                ty: binding.ty.clone(),
                binder: binding.span,
            })
            .or_else(|| self.captured(name))
    }

    /// [`lookup`](Self::lookup), marking a binding of this frame's own as read. A
    /// capture needs no mark: it exists because a reference already reached the
    /// binding it copies.
    fn lookup_mut(&mut self, name: &str) -> Option<Resolved> {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(binding) = scope.find_mut(name) {
                binding.used = true;
                return Some(Resolved::Local {
                    slot: binding.slot,
                    ty: binding.ty.clone(),
                    binder: binding.span,
                });
            }
        }
        self.captured(name)
    }

    /// What this frame closed over as `name`, if anything.
    fn captured(&self, name: &str) -> Option<Resolved> {
        self.captures
            .iter()
            .find(|c| &*c.name == name)
            .map(|c| Resolved::Local {
                slot: c.slot,
                ty: c.ty.clone(),
                binder: c.binder,
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
    fn capture(&mut self, name: &str, parent_slot: u16, ty: Type, binder: Span) -> u16 {
        let slot = self.alloc();
        self.captures.push(Capture {
            name: Rc::from(name),
            parent_slot,
            slot,
            ty,
            binder,
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
    /// before the index exists.
    index: Option<&'a ExportIndex>,
    /// The names the host binds at render, and their types. Never redefinable — a
    /// binding or definition that takes one of them is an error, not a shadow (D23).
    host: &'a Host,
    /// This file's top-level `lem-fn`s, in order — the list a repeated one is checked
    /// against. Never consulted by lookup: the index covers them once it exists.
    file: Vec<Export>,
    /// The function being compiled, then every lambda open inside it, innermost last.
    frames: Vec<Frame>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Names<'a> {
    pub(crate) fn new(
        path: &'a str,
        index: Option<&'a ExportIndex>,
        host: &'a Host,
    ) -> Self {
        Self {
            path,
            index,
            host,
            file: Vec::new(),
            frames: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    pub(crate) fn push_frame(&mut self) {
        self.frames.push(Frame::new());
    }

    /// Closes the innermost function, warning for what its parameters' scope never read.
    pub(crate) fn pop_frame(&mut self) -> Frame {
        let mut frame = self.frames.pop().expect("a frame");
        for scope in frame.scopes.drain(..).rev() {
            self.warn_unused(&scope);
        }
        frame
    }

    fn frame(&mut self) -> &mut Frame {
        self.frames.last_mut().expect("a frame")
    }

    pub(crate) fn push_scope(&mut self) {
        self.frame().scopes.push(Scope::default());
    }

    /// A slot of the innermost frame that no name reaches — where `lem-match` keeps
    /// its subject while the cases test it. Unnamed, so nothing captures or reads it.
    pub(crate) fn temp(&mut self) -> u16 {
        self.frame().alloc()
    }

    /// Closes the innermost scope, warning for each definition it never read.
    pub(crate) fn pop_scope(&mut self) {
        let scope = self.frame().scopes.pop().expect("a scope");
        self.warn_unused(&scope);
    }

    /// The unused rule: a definition or a parameter nothing referenced warns at its
    /// name, when the scope that could have referenced it closes. Loop bindings are
    /// exempt, and a refused binding was never recorded.
    fn warn_unused(&mut self, scope: &Scope) {
        for binding in &scope.0 {
            if binding.used
                || matches!(binding.kind, BindingKind::Loop | BindingKind::Exported)
            {
                continue;
            }
            self.diagnostics.push(Diagnostic::warning(
                self.path,
                binding.span,
                format!("`{}` is never used", binding.name),
            ));
        }
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
    /// to the file whose definition it kept. So the same call made again with the index
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

    /// Binds `name`, whose own bytes are `span`, as `kind` with type `ty` in the
    /// innermost scope of the innermost frame, and returns its slot. The rules in order,
    /// the first one broken reported: an ambient name cannot be redefined; a name
    /// already bound in the same scope cannot be bound again; a definition that
    /// displaces a stdlib function may, with a warning. A parameter or loop binding
    /// named after the stdlib is not a shadow, and neither is a definition displacing a
    /// project function, a parameter, or an outer local.
    ///
    /// A refused binding still takes its slot, so its binder can compile a store into it
    /// and the parameters after it stay where a call lays its arguments — but it is not
    /// recorded, so no reference reaches it and nothing warns when none does.
    pub(crate) fn bind_local(
        &mut self,
        name: &str,
        span: Span,
        kind: BindingKind,
        ty: Type,
    ) -> u16 {
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
        if matches!(kind, BindingKind::Definition | BindingKind::Exported)
            && self.local_shadows_builtin(name)
        {
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
                kind,
                ty,
                used: false,
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
        if self.host.binding(name).is_some() {
            return Some(Diagnostic::error(
                self.path,
                span,
                format!("`{name}` is bound by the host and cannot be redefined"),
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
    /// the index kept this file's definition of the name. Before the index exists
    /// nothing is known to be displaced.
    fn function_shadows_builtin(&self, name: &str) -> bool {
        builtins::lookup(name).is_some()
            && self
                .index
                .is_some_and(|index| index.defined_in(name, self.path))
    }

    /// What `name` means here, touching nothing: a local in any open frame (whose slot
    /// is that frame's, not necessarily usable here), else what [`outer`](Self::outer)
    /// finds.
    ///
    /// A probe asks about a name it may not end up compiling at all — whether a
    /// definition displaces a stdlib one — and a lookup that recorded a capture on the
    /// way past an enclosing frame would leave the slot behind whatever the answer was;
    /// nor does a probe count as a read.
    pub(crate) fn peek(&self, name: &str) -> Option<Resolved> {
        self.frames
            .iter()
            .rev()
            .find_map(|frame| frame.lookup(name))
            .or_else(|| self.outer(name))
    }

    /// What `name` means in the innermost frame: a local's slot usable there, capturing
    /// through every frame crossed to reach a binding in an enclosing one, and marking
    /// that binding read; else what [`outer`](Self::outer) finds.
    pub(crate) fn resolve(&mut self, name: &str) -> Option<Resolved> {
        let local = self
            .frames
            .iter_mut()
            .enumerate()
            .rev()
            .find_map(|(level, frame)| frame.lookup_mut(name).map(|found| (level, found)));
        let Some((
            hit,
            Resolved::Local {
                mut slot,
                ty,
                binder,
            },
        )) = local
        else {
            return self.outer(name);
        };
        for level in hit + 1..self.frames.len() {
            slot = self.frames[level].capture(name, slot, ty.clone(), binder);
        }
        Some(Resolved::Local { slot, ty, binder })
    }

    /// The names beyond the frames, in the order they are consulted: the project-flat
    /// functions, the stdlib, the ambient values.
    fn outer(&self, name: &str) -> Option<Resolved> {
        if self.index.is_some_and(|index| index.lookup(name).is_some()) {
            return Some(Resolved::Function);
        }
        if let Some(id) = builtins::lookup(name) {
            return Some(Resolved::Builtin(id));
        }
        self.host
            .binding(name)
            .map(|ambient| Resolved::Ambient(ambient.ty()))
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

    /// An index in which `lib.lem` defines `names`.
    fn index(names: &[&str]) -> ExportIndex {
        let exports: Vec<ExportedSignature> = names
            .iter()
            .enumerate()
            .map(|(i, name)| ExportedSignature {
                name: Rc::from(*name),
                params: Vec::new(),
                ret: Type::Html,
                func: index_u32(i + 1),
                name_span: at(0),
            })
            .collect();
        let mut index = ExportIndex::new();
        index.offer("lib.lem", &exports);
        index
    }

    fn at(start: u32) -> Span {
        Span::new(start, start + 1)
    }

    /// A host shaped like the standard shell's, with nothing typed — the three names
    /// every test below expects to be reserved.
    fn untyped_host() -> Host {
        Host::untyped(["site", "page", "build"])
    }

    /// Names for `t.lem` with one frame open, resolving against `index` and `host`.
    fn in_frame<'a>(host: &'a Host, index: Option<&'a ExportIndex>) -> Names<'a> {
        let mut names = Names::new("t.lem", index, host);
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

    /// The slot a resolution landed on.
    fn slot_of(resolved: Option<Resolved>) -> Option<u16> {
        match resolved {
            Some(Resolved::Local { slot, .. }) => Some(slot),
            _ => None,
        }
    }

    /// Binds `name` as `kind`, typed `any`, and marks it read so the test is about the
    /// rule it names and not the unused rule.
    fn bind(names: &mut Names<'_>, name: &str, span: Span, kind: BindingKind) -> u16 {
        let slot = names.bind_local(name, span, kind, Type::Any);
        names.resolve(name);
        slot
    }

    const SHADOWS: &str =
        "t.lem:0..1: warning: `raw` shadows the stdlib function of the same name";

    #[test]
    fn binds_parameters_to_consecutive_slots_in_declaration_order() {
        let host = untyped_host();
        let mut names = in_frame(&host, None);
        assert_eq!(bind(&mut names, "a", at(0), BindingKind::Parameter), 0);
        assert_eq!(bind(&mut names, "b", at(2), BindingKind::Parameter), 1);
        assert_eq!(bind(&mut names, "c", at(4), BindingKind::Parameter), 2);
        assert_eq!(names.pop_frame().n_locals(), 3);
    }

    #[test]
    fn keeps_a_refused_parameters_slot_so_later_parameters_stay_aligned() {
        // The VM lays a call's arguments out at consecutive slots, so the parameter after
        // a refused one must still be the second slot.
        let host = untyped_host();
        let mut names = in_frame(&host, None);
        assert_eq!(bind(&mut names, "page", at(0), BindingKind::Parameter), 0);
        assert_eq!(bind(&mut names, "x", at(6), BindingKind::Parameter), 1);
    }

    #[test]
    fn binds_a_loop_pair_to_two_distinct_slots() {
        let host = untyped_host();
        let mut names = in_frame(&host, None);
        names.push_scope();
        let key = names.bind_local("k", at(0), BindingKind::Loop, Type::String);
        let value = names.bind_local("v", at(3), BindingKind::Loop, Type::Int);
        assert_ne!(key, value);
        assert_eq!(
            names.resolve("k"),
            Some(Resolved::Local {
                slot: key,
                ty: Type::String,
                binder: at(0)
            })
        );
        assert_eq!(
            names.resolve("v"),
            Some(Resolved::Local {
                slot: value,
                ty: Type::Int,
                binder: at(3)
            })
        );
    }

    #[test]
    fn binds_a_definition_in_the_innermost_scope_only_until_it_closes() {
        let host = untyped_host();
        let mut names = in_frame(&host, None);
        names.push_scope();
        let slot = bind(&mut names, "x", at(0), BindingKind::Definition);
        assert_eq!(slot_of(names.resolve("x")), Some(slot));
        names.pop_scope();
        assert_eq!(names.resolve("x"), None);
    }

    #[test]
    fn admits_a_function_once_per_file() {
        let host = untyped_host();
        let mut names = Names::new("t.lem", None, &host);
        assert!(names.bind_function("f", at(0)));
        assert!(names.bind_function("g", at(2)));
        assert!(!names.bind_function("f", at(4)));
        assert!(!names.bind_function("page", at(6)));
        assert!(names.bind_function("h", at(9)));
    }

    #[test]
    fn does_not_let_a_reference_reach_a_refused_binding() {
        let host = untyped_host();
        let mut names = in_frame(&host, None);
        names.bind_local("page", at(0), BindingKind::Definition, Type::Int);
        assert_eq!(names.resolve("page"), Some(Resolved::Ambient(Type::Any)));
        let first = bind(&mut names, "a", at(6), BindingKind::Definition);
        names.bind_local("a", at(8), BindingKind::Definition, Type::Int);
        assert_eq!(slot_of(names.resolve("a")), Some(first));
    }

    #[test]
    fn refuses_an_ambient_name_whatever_the_kind() {
        let expected = [
            "t.lem:0..1: `site` is bound by the host and cannot be redefined",
            "t.lem:1..2: `page` is bound by the host and cannot be redefined",
            "t.lem:2..3: `build` is bound by the host and cannot be redefined",
        ];
        for kind in [
            BindingKind::Definition,
            BindingKind::Parameter,
            BindingKind::Loop,
        ] {
            let host = untyped_host();
            let mut names = in_frame(&host, None);
            for (i, name) in ["site", "page", "build"].iter().enumerate() {
                names.bind_local(name, at(index_u32(i)), kind, Type::Any);
            }
            assert_eq!(shown(&names.diagnostics), expected, "{kind:?}");
            assert!(
                names
                    .diagnostics
                    .iter()
                    .all(|d| d.severity == Severity::Error)
            );
        }
        let host = untyped_host();
        let mut names = Names::new("t.lem", None, &host);
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
        let host = untyped_host();
        let mut names = in_frame(&host, None);
        assert_eq!(bind(&mut names, "a", at(0), BindingKind::Parameter), 0);
        assert_eq!(bind(&mut names, "a", at(3), BindingKind::Parameter), 1);
        assert_eq!(
            shown(&names.diagnostics),
            ["t.lem:3..4: `a` is already bound here\n  t.lem:0..1: first bound here"]
        );
    }

    #[test]
    fn allows_a_name_rebound_in_an_inner_scope() {
        let host = untyped_host();
        let mut names = in_frame(&host, None);
        let outer = bind(&mut names, "x", at(0), BindingKind::Definition);
        names.push_scope();
        let inner = bind(&mut names, "x", at(4), BindingKind::Definition);
        assert_ne!(outer, inner);
        assert_eq!(slot_of(names.resolve("x")), Some(inner));
        names.pop_scope();
        assert_eq!(slot_of(names.resolve("x")), Some(outer));
        assert!(
            names.diagnostics.is_empty(),
            "{:?}",
            shown(&names.diagnostics)
        );
    }

    #[test]
    fn allows_a_parameter_rebound_by_a_definition_in_the_body_scope() {
        let host = untyped_host();
        let mut names = in_frame(&host, None);
        bind(&mut names, "a", at(0), BindingKind::Parameter);
        names.push_scope();
        assert_eq!(bind(&mut names, "a", at(4), BindingKind::Definition), 1);
        assert!(
            names.diagnostics.is_empty(),
            "{:?}",
            shown(&names.diagnostics)
        );
    }

    #[test]
    fn refuses_a_function_defined_twice_citing_the_first() {
        let host = untyped_host();
        let mut names = Names::new("t.lem", None, &host);
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
        let host = untyped_host();
        let mut names = in_frame(&host, None);
        bind(&mut names, "page", at(0), BindingKind::Parameter);
        bind(&mut names, "page", at(2), BindingKind::Parameter);
        bind(&mut names, "raw", at(4), BindingKind::Definition);
        bind(&mut names, "raw", at(6), BindingKind::Definition);
        assert_eq!(
            shown(&names.diagnostics),
            [
                "t.lem:0..1: `page` is bound by the host and cannot be redefined",
                "t.lem:2..3: `page` is bound by the host and cannot be redefined",
                "t.lem:4..5: warning: `raw` shadows the stdlib function of the same name",
                "t.lem:6..7: `raw` is already bound here\n  t.lem:4..5: first bound here",
            ]
        );
    }

    #[test]
    fn warns_when_a_definition_displaces_the_stdlib() {
        let host = untyped_host();
        let mut names = in_frame(&host, None);
        assert_eq!(bind(&mut names, "raw", at(0), BindingKind::Definition), 0);
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
        let index = index(&["raw"]);
        let host = untyped_host();
        let mut names = Names::new("lib.lem", Some(&index), &host);
        assert!(names.bind_function("raw", at(0)));
        assert_eq!(
            shown(&names.diagnostics),
            ["lib.lem:0..1: warning: `raw` shadows the stdlib function of the same name"]
        );
    }

    #[test]
    fn does_not_warn_for_a_function_the_project_kept_from_another_file() {
        // The repeat is an error where the index was built; this file's copy displaces
        // nothing.
        let index = index(&["raw"]);
        let host = untyped_host();
        let mut names = Names::new("t.lem", Some(&index), &host);
        assert!(names.bind_function("raw", at(0)));
        assert!(
            names.diagnostics.is_empty(),
            "{:?}",
            shown(&names.diagnostics)
        );
    }

    #[test]
    fn does_not_warn_for_a_function_before_the_index_exists() {
        let host = untyped_host();
        let mut names = Names::new("t.lem", None, &host);
        assert!(names.bind_function("raw", at(0)));
        assert!(
            names.diagnostics.is_empty(),
            "{:?}",
            shown(&names.diagnostics)
        );
    }

    #[test]
    fn does_not_warn_when_a_definition_displaces_a_project_function() {
        let index = index(&["raw"]);
        let host = untyped_host();
        let mut names = in_frame(&host, Some(&index));
        assert_eq!(bind(&mut names, "raw", at(0), BindingKind::Definition), 0);
        assert!(
            names.diagnostics.is_empty(),
            "{:?}",
            shown(&names.diagnostics)
        );
    }

    #[test]
    fn does_not_warn_when_a_definition_displaces_an_outer_local() {
        let host = untyped_host();
        let mut names = in_frame(&host, None);
        bind(&mut names, "raw", at(0), BindingKind::Parameter);
        names.push_scope();
        bind(&mut names, "raw", at(4), BindingKind::Definition);
        assert!(
            names.diagnostics.is_empty(),
            "{:?}",
            shown(&names.diagnostics)
        );

        // Only the outer definition warns, whatever it displaced itself.
        let host = untyped_host();
        let mut names = in_frame(&host, None);
        bind(&mut names, "raw", at(0), BindingKind::Definition);
        names.push_scope();
        bind(&mut names, "raw", at(4), BindingKind::Definition);
        assert_eq!(shown(&names.diagnostics), [SHADOWS]);
    }

    #[test]
    fn does_not_warn_for_a_parameter_or_loop_name_from_the_stdlib() {
        let host = untyped_host();
        let mut names = in_frame(&host, None);
        bind(&mut names, "raw", at(0), BindingKind::Parameter);
        names.push_scope();
        bind(&mut names, "raw", at(4), BindingKind::Loop);
        assert!(
            names.diagnostics.is_empty(),
            "{:?}",
            shown(&names.diagnostics)
        );
    }

    #[test]
    fn resolves_a_local_before_a_project_function_before_the_stdlib_before_ambient() {
        let index = index(&["raw", "f"]);
        let host = untyped_host();
        let mut names = in_frame(&host, Some(&index));
        names.push_scope();
        let slot = bind(&mut names, "raw", at(0), BindingKind::Definition);
        assert_eq!(slot_of(names.resolve("raw")), Some(slot));
        names.pop_scope();
        assert_eq!(names.resolve("raw"), Some(Resolved::Function));
        assert_eq!(names.resolve("f"), Some(Resolved::Function));

        let host = untyped_host();
        let mut names = in_frame(&host, None);
        assert_eq!(names.resolve("raw"), Some(Resolved::Builtin(0)));
        assert_eq!(names.resolve("site"), Some(Resolved::Ambient(Type::Any)));
        assert_eq!(names.resolve("zzz"), None);
    }

    #[test]
    fn resolves_the_innermost_scope_first() {
        let host = untyped_host();
        let mut names = in_frame(&host, None);
        bind(&mut names, "x", at(0), BindingKind::Definition);
        names.push_scope();
        names.push_scope();
        let innermost = bind(&mut names, "x", at(4), BindingKind::Definition);
        assert_eq!(slot_of(names.resolve("x")), Some(innermost));
    }

    #[test]
    fn records_a_capture_when_a_name_comes_from_an_enclosing_frame() {
        let host = untyped_host();
        let mut names = in_frame(&host, None);
        bind(&mut names, "m", at(0), BindingKind::Definition);
        let parent = names.bind_local("n", at(2), BindingKind::Definition, Type::Int);
        names.push_frame();
        // The capture carries the binding's type and identity into the inner frame.
        assert_eq!(
            names.resolve("n"),
            Some(Resolved::Local {
                slot: 0,
                ty: Type::Int,
                binder: at(2)
            })
        );
        let inner = names.pop_frame();
        assert_eq!(pairs(&inner), [(parent, 0)]);
        assert_eq!(inner.n_locals(), 1);
        assert!(names.pop_frame().captures().is_empty());
    }

    #[test]
    fn records_a_capture_in_every_frame_crossed() {
        let host = untyped_host();
        let mut names = in_frame(&host, None);
        bind(&mut names, "outer", at(0), BindingKind::Definition);
        names.push_frame();
        bind(&mut names, "a", at(2), BindingKind::Parameter);
        names.push_frame();
        bind(&mut names, "b", at(4), BindingKind::Parameter);
        assert_eq!(slot_of(names.resolve("outer")), Some(1));
        let innermost = names.pop_frame();
        let middle = names.pop_frame();
        assert_eq!(pairs(&middle), [(0, 1)]);
        assert_eq!(pairs(&innermost), [(1, 1)]);
    }

    #[test]
    fn reuses_a_capture_already_recorded() {
        let host = untyped_host();
        let mut names = in_frame(&host, None);
        bind(&mut names, "n", at(0), BindingKind::Definition);
        names.push_frame();
        assert_eq!(slot_of(names.resolve("n")), Some(0));
        assert_eq!(slot_of(names.resolve("n")), Some(0));
        assert_eq!(pairs(&names.pop_frame()), [(0, 0)]);
    }

    #[test]
    fn peeks_without_recording_a_capture() {
        let host = untyped_host();
        let mut names = in_frame(&host, None);
        bind(&mut names, "n", at(0), BindingKind::Definition);
        names.push_frame();
        assert_eq!(slot_of(names.peek("n")), Some(0));
        let inner = names.pop_frame();
        assert!(inner.captures().is_empty());
        assert_eq!(inner.n_locals(), 0);
    }

    #[test]
    fn hands_out_a_temp_slot_that_no_name_reaches() {
        let host = untyped_host();
        let mut names = in_frame(&host, None);
        bind(&mut names, "n", at(0), BindingKind::Definition);
        let temp = names.temp();
        assert_eq!(temp, 1);
        bind(&mut names, "m", at(1), BindingKind::Definition);
        assert_eq!(slot_of(names.peek("m")), Some(2));
        assert_eq!(names.pop_frame().n_locals(), 3);
    }

    #[test]
    fn forgets_a_frames_names_when_it_pops() {
        let host = untyped_host();
        let mut names = in_frame(&host, None);
        names.push_frame();
        bind(&mut names, "inner", at(0), BindingKind::Parameter);
        names.pop_frame();
        assert_eq!(names.resolve("inner"), None);
    }

    #[test]
    fn hands_its_diagnostics_back_in_the_order_the_rules_raised_them() {
        let host = untyped_host();
        let mut names = in_frame(&host, None);
        bind(&mut names, "raw", at(0), BindingKind::Definition);
        bind(&mut names, "page", at(2), BindingKind::Definition);
        assert_eq!(
            shown(&names.into_diagnostics()),
            [
                SHADOWS,
                "t.lem:2..3: `page` is bound by the host and cannot be redefined",
            ]
        );
    }

    #[test]
    fn warns_for_a_definition_or_parameter_nothing_read_when_its_scope_closes() {
        let host = untyped_host();
        let mut names = in_frame(&host, None);
        names.bind_local("p", at(0), BindingKind::Parameter, Type::Int);
        names.push_scope();
        names.bind_local("d", at(2), BindingKind::Definition, Type::Int);
        names.bind_local("read", at(4), BindingKind::Definition, Type::Int);
        names.resolve("read");
        assert!(
            names.diagnostics.is_empty(),
            "nothing warns before the scope closes"
        );
        names.pop_scope();
        assert_eq!(
            shown(&names.diagnostics),
            ["t.lem:2..3: warning: `d` is never used"]
        );
        names.pop_frame();
        assert_eq!(
            shown(&names.diagnostics),
            [
                "t.lem:2..3: warning: `d` is never used",
                "t.lem:0..1: warning: `p` is never used",
            ]
        );
    }

    #[test]
    fn does_not_warn_for_a_loop_binding_a_probe_does_not_count_as_a_read() {
        let host = untyped_host();
        let mut names = in_frame(&host, None);
        names.push_scope();
        names.bind_local("i", at(0), BindingKind::Loop, Type::Int);
        names.bind_local("probed", at(2), BindingKind::Definition, Type::Int);
        names.peek("probed");
        names.pop_scope();
        assert_eq!(
            shown(&names.diagnostics),
            ["t.lem:2..3: warning: `probed` is never used"]
        );
    }

    #[test]
    fn counts_a_read_through_a_capture_as_a_read_of_the_binding() {
        let host = untyped_host();
        let mut names = in_frame(&host, None);
        names.bind_local("n", at(0), BindingKind::Definition, Type::Int);
        names.push_frame();
        names.resolve("n");
        names.pop_frame();
        names.pop_frame();
        assert!(
            names.diagnostics.is_empty(),
            "{:?}",
            shown(&names.diagnostics)
        );
    }

    #[test]
    fn never_warns_for_a_refused_binding() {
        let host = untyped_host();
        let mut names = in_frame(&host, None);
        names.bind_local("page", at(0), BindingKind::Definition, Type::Int);
        names.bind_local("a", at(2), BindingKind::Definition, Type::Int);
        names.bind_local("a", at(4), BindingKind::Definition, Type::Int);
        names.resolve("a");
        names.pop_frame();
        let severities: Vec<Severity> =
            names.diagnostics.iter().map(|d| d.severity).collect();
        assert_eq!(severities, [Severity::Error, Severity::Error]);
    }
}

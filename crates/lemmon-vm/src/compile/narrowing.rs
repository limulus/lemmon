//! Narrowing: what a condition proves about a value for the arm it guards (D35).
//!
//! A narrowing is keyed on a [`Path`] — a root and the fields read off it in order — so
//! that `page.author.name` is a different place from `page.author`, and an inner shadow
//! of a binding is a different place from the binding it shadows. Only a plain `.`
//! extends a path: `?.` yields an optional by construction, an operator or a call reads
//! no stable place, and either ends the path where it stands.
//!
//! Both roots are immutable for the whole render — a local by the span of its name token,
//! an ambient value by its name, which no binding may take (D23) — which is what lets a
//! narrowing hold for a whole arm. What each construct narrows to is the rules' verdict
//! ([`rules::condition`], [`rules::type_test`]); what is here is
//! the walk that reads a path out of an expression, emits the test, and looks an open
//! narrowing back up.

use std::rc::Rc;

use lemmon_syntax::ast::{Expr, Spanned, TypeTest};
use lemmon_syntax::{Span, SyntaxToken};

use crate::bytecode::Op;
use crate::types::Type;

use super::expr::unparen;
use super::names::Resolved;
use super::rules;
use super::{Compiler, TokenSpan, required};

/// What a narrowing is keyed on: a root and the fields read off it in order, none for
/// the root itself. Only a plain `.` extends a path: `?.` yields an optional by
/// construction, and nothing else reads a stable place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Path {
    root: PathRoot,
    fields: Vec<Rc<str>>,
}

/// Where a path starts. Both are immutable for the whole render, which is what lets a
/// narrowing hold for the arm it guards (D35).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PathRoot {
    /// A local binding, by the span of its name token: one binding, one span, so an
    /// inner shadow is a different path.
    Local(Span),
    /// A value the host binds, by its name — which no binding may take (D23), so it
    /// needs no span to tell one from another.
    Ambient(Rc<str>),
}

impl Path {
    /// The path of the binding itself.
    pub(super) const fn local(binder: Span) -> Self {
        Self {
            root: PathRoot::Local(binder),
            fields: Vec::new(),
        }
    }

    /// The path of the ambient value itself.
    pub(super) fn ambient(name: &str) -> Self {
        Self {
            root: PathRoot::Ambient(Rc::from(name)),
            fields: Vec::new(),
        }
    }

    /// The path of one field of an ambient value — what a sibling binding read by its
    /// bare name at the bindings file's top level is (D36).
    pub(super) fn ambient_field(value: &Rc<str>, field: &str) -> Self {
        Self {
            root: PathRoot::Ambient(value.clone()),
            fields: vec![Rc::from(field)],
        }
    }
}

impl Compiler<'_> {
    /// Checks a type test of `subject` against `name` — the name first, then the pair
    /// — reporting what is refused, records the declaration consulted, and emits the
    /// test over the subject on the stack. What the test narrows the subject to, when
    /// it was admitted. `form` decides only what an enum's name is told instead.
    pub(super) fn type_test(
        &mut self,
        subject: &Type,
        name: &SyntaxToken,
        span: Span,
        form: rules::TestForm,
    ) -> Option<Type> {
        let refused = self.require(
            name.span(),
            rules::tested_type(name.text(), form, self.decls()),
        );
        let narrowed = if refused {
            None
        } else {
            self.note_type(name.text(), name.span());
            let verdict = rules::type_test(subject, name.text(), self.decls());
            self.rule_or(span, verdict.map(Some), None)
        };
        let index = self.intern(name.text());
        self.emit(Op::TypeTest { name: index }, span);
        narrowed
    }

    /// Lowers `x is T`, returning its type and what it narrows: the subject's path, to
    /// the type the rule granted. One walk, one verdict — a caller that wants only the
    /// type drops the second half.
    pub(super) fn type_test_expr(
        &mut self,
        test: &TypeTest,
        span: Span,
    ) -> (Type, Option<(Path, Type)>) {
        let subject_expr = required(test.expr());
        let path = self.path_of(&subject_expr).map(|(path, _)| path);
        let subject = self.expr(&subject_expr);
        let name = required(test.type_name());
        let narrowed = self.type_test(&subject, &name, span, rules::TestForm::Is);
        (Type::Bool, path.zip(narrowed))
    }
    /// The root `expr` names bare — a local or an ambient value, parenthesised or not
    /// — the one shape whose fields decide when the loader may run a function, and
    /// the one shape a call hands a root whole (∆HF0). Peeked, as [`Self::path_of`]
    /// peeks, and cheaper: nothing about the value is wanted, only its root.
    pub(super) fn bare_root(&self, expr: &Expr) -> Option<PathRoot> {
        let Expr::Name(name) = unparen(expr.clone()) else {
            return None;
        };
        let name = required(name.name());
        match self.names.peek(name.text())? {
            Resolved::Local { binder, .. } => Some(PathRoot::Local(binder)),
            Resolved::Ambient(_) => Some(PathRoot::Ambient(Rc::from(name.text()))),
            Resolved::Field { .. } | Resolved::Function | Resolved::Builtin(_) => None,
        }
    }

    /// The path `expr` reads, if it reads one — a local, or fields read off one with
    /// plain `.`s, parenthesised or not — with the type its last step was declared
    /// with, whatever a condition has narrowed that step to: the binding's, or the
    /// field's as the declaration of what it is read off says, that being narrowed or
    /// not, an optional read through. Peeked, never resolved: asking is not reading.
    pub(super) fn path_of(&self, expr: &Expr) -> Option<(Path, Type)> {
        match unparen(expr.clone()) {
            Expr::Name(name) => {
                let name = required(name.name());
                match self.names.peek(name.text()) {
                    Some(Resolved::Local { binder, ty, .. }) => {
                        Some((Path::local(binder), ty))
                    }
                    Some(Resolved::Ambient(ty)) => {
                        Some((Path::ambient(name.text()), self.known(ty)))
                    }
                    Some(Resolved::Field { value, ty }) => {
                        Some((Path::ambient_field(&value, name.text()), self.known(ty)))
                    }
                    _ => None,
                }
            }
            Expr::Member(member) if !member.is_optional() => {
                let (mut path, declared) = self.path_of(&required(member.object()))?;
                let object = self.narrowed(&path).unwrap_or(declared);
                let field = required(member.name());
                let ty = self
                    .decls()
                    .field(object.strip_optional().named()?, field.text())?;
                path.fields.push(Rc::from(field.text()));
                Some((path, ty))
            }
            _ => None,
        }
    }
    /// Lowers a `lem-if` or `?:` condition: its type, and what it narrows wherever it
    /// holds. An optional path — a local, or fields read off one, parenthesised or not
    /// — narrows to its inner type; `path is T` narrows to what the type test granted,
    /// which is the one verdict the rules gave. A path an enclosing condition already
    /// narrowed passes as the optional it was declared.
    pub(super) fn lower_condition(&mut self, expr: &Expr) -> (Type, Option<(Path, Type)>) {
        let peeled = unparen(expr.clone());
        if let Expr::TypeTest(test) = &peeled {
            return self.type_test_expr(test, peeled.span());
        }
        let ty = self.expr(expr);
        let narrowing = match rules::condition(&ty) {
            Ok(None) => None,
            Ok(Some(inner)) => self.path_of(&peeled).map(|(path, _)| (path, inner)),
            Err(_) if self.declared_optional(&peeled) => None,
            Err(message) => {
                self.error(peeled.span(), message);
                None
            }
        };
        (ty, narrowing)
    }

    /// Whether `expr` is a path declared optional, whatever a `lem-if` around it has
    /// narrowed it to.
    pub(super) fn declared_optional(&self, expr: &Expr) -> bool {
        self.path_of(expr)
            .is_some_and(|(_, ty)| matches!(ty, Type::Optional(_)))
    }

    /// The type an open condition has narrowed `path` to, if one has.
    pub(super) fn narrowed(&self, path: &Path) -> Option<Type> {
        self.narrowings
            .iter()
            .rev()
            .find(|(p, _)| p == path)
            .map(|(_, ty)| ty.clone())
    }
}

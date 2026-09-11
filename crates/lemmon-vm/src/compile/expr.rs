//! Lowering of expressions: the heads of tags — typed as they are lowered (D31).
//!
//! Every arm emits the code for its expression and returns the expression's type.
//! Checking is bidirectional (`docs/language.md` §Checking): where a type is expected —
//! an annotated binding, a parameter, a list under a typed list — [`Compiler::check`]
//! flows the expectation into literals, lambdas, and conditionals, and everything else
//! is inferred and then compared. The walk emits, asks, and reports: the rules that
//! decide a type from its operands' types live in [`rules`](super::rules), and what a
//! rule refuses is reported here, at the construct's span. The construct then takes
//! whatever type keeps the rest of the file checkable — the type that was expected
//! where one was, `any` where there is nothing better — so that one mistake costs one
//! diagnostic.
//!
//! Two arms of the walk are large enough to have modules of their own, and reach back
//! into this one for the sub-expressions they lower: [`calls`](super::calls) binds an
//! argument list to what it calls, and [`narrowing`](super::narrowing) reads the path a
//! condition narrows.

use std::rc::Rc;

use lemmon_syntax::ast::{
    BinaryExpr, BinaryOp, Expr, IndexExpr, Lambda, ListLit, Literal, LiteralKind, MapEntry,
    MapLit, MemberExpr, Spanned, TernaryExpr, UnaryExpr, UnaryOp,
};
use lemmon_syntax::{Span, SyntaxKind, SyntaxToken};

use super::names::{BindingKind, Resolved};
use super::narrowing::Path;
use super::rules::{self, LambdaShape, Rule};
use super::{Compiler, TokenSpan, required};
use crate::Value;
use crate::bytecode::Op;
use crate::decls::Kind;
use crate::types::{Type, join};
use crate::unit::Access;

impl Compiler<'_> {
    /// Lowers `expr`, leaving its value on the stack, and returns its type.
    pub(super) fn expr(&mut self, expr: &Expr) -> Type {
        self.expr_in(expr, None)
    }

    /// Lowers `expr` where a value of type `expected` is wanted. A mismatch is reported
    /// once, at the expression, and the expression then counts as having the type that
    /// was wanted.
    pub(super) fn check(&mut self, expr: &Expr, expected: &Type) -> Type {
        self.expr_in(expr, Some(expected))
    }

    /// What a rule granted, or `any` once what it refused is reported at `span`.
    pub(super) fn rule(&mut self, span: Span, verdict: Rule) -> Type {
        self.rule_or(span, verdict, Type::Any)
    }

    /// What a rule granted, or `fallback` once what it refused is reported at `span`.
    pub(super) fn rule_or<T>(&mut self, span: Span, verdict: Rule<T>, fallback: T) -> T {
        verdict.unwrap_or_else(|message| {
            self.error(span, message);
            fallback
        })
    }

    /// Reports what a rule refused at `span`, if it refused; whether it did.
    pub(super) fn require(&mut self, span: Span, verdict: Rule<()>) -> bool {
        match verdict {
            Ok(()) => false,
            Err(message) => {
                self.error(span, message);
                true
            }
        }
    }

    fn expr_in(&mut self, expr: &Expr, expected: Option<&Type>) -> Type {
        let actual = self.lower(expr, expected);
        match expected {
            Some(expected) => self.rule_or(
                expr.span(),
                rules::expected(actual, expected, self.decls()),
                expected.clone(),
            ),
            None => actual,
        }
    }

    /// Lowers `expr` under an expectation without holding it to one: the expectation
    /// shapes what it can — a literal's elements, a lambda's parameters, a bare
    /// variant — and the type that comes back is the caller's to judge.
    fn lower(&mut self, expr: &Expr, expected: Option<&Type>) -> Type {
        let span = expr.span();
        match expr {
            Expr::Literal(literal) => self.literal(literal),
            Expr::Name(name) => self.name(&required(name.name())),
            // `.hero` is a variant of whichever enum is expected here (∆EN6); nothing
            // else resolves it, so a bare name is only ever a name.
            Expr::Variant(variant) => {
                let token = required(variant.name());
                let ty = self.rule(
                    variant.span(),
                    rules::dotted_variant(expected, token.text(), self.decls()),
                );
                self.emit_variant(token.text(), variant.span());
                ty
            }
            Expr::Paren(paren) => self.lower(&required(paren.inner()), expected),
            Expr::Member(member) => self.member(member),
            Expr::Index(index) => self.index(index),
            Expr::Unary(unary) => self.unary(unary),
            Expr::Binary(binary) => self.binary(binary),
            Expr::Ternary(ternary) => self.ternary(ternary, expected),
            Expr::List(list) => self.list(list, expected),
            Expr::Map(map) => self.map(map, expected),
            Expr::Call(call) => self.call(call),
            Expr::Lambda(lambda) => self.lambda(lambda, expected),
            // `x is T` read as a value — a `bool`. As a condition it comes through
            // `lower_condition` instead, which keeps the narrowing the rule granted.
            Expr::TypeTest(test) => self.type_test_expr(test, span).0,
        }
    }

    pub(super) fn literal(&mut self, literal: &Literal) -> Type {
        let token = required(literal.token());
        let span = token.span();
        let (value, ty) = match required(literal.kind()) {
            LiteralKind::String => {
                let index = self.intern(&decode_string(token.text()));
                self.emit(Op::Const { index }, span);
                return Type::String;
            }
            LiteralKind::Int => match token.text().parse::<i64>() {
                Ok(i) => (Value::Int(i), Type::Int),
                Err(_) => {
                    self.error(span, "integer literal is out of range");
                    return Type::Any;
                }
            },
            LiteralKind::Float => (
                Value::Float(
                    token
                        .text()
                        .parse()
                        .expect("the scanner only lexes well-formed floats"),
                ),
                Type::Float,
            ),
            LiteralKind::True => (Value::Bool(true), Type::Bool),
            LiteralKind::False => (Value::Bool(false), Type::Bool),
            LiteralKind::Null => (Value::Null, Type::Null),
        };
        let index = self.constant(value);
        self.emit(Op::Const { index }, span);
        ty
    }

    /// Lowers a reference to `token` and returns what it has: a local's type — narrowed,
    /// where a `lem-if` on it is open — a function's signature, or what the host gave an
    /// ambient value.
    pub(super) fn name(&mut self, token: &SyntaxToken) -> Type {
        self.name_read(token, true)
    }

    /// Lowers the object of a member access: as [`Self::expr`] does, except that a
    /// bare ambient value here is read for one of its fields, not whole.
    pub(super) fn object(&mut self, object: &Expr) -> Type {
        match unparen(object.clone()) {
            Expr::Name(name) => self.name_read(&required(name.name()), false),
            _ => self.expr(object),
        }
    }

    /// Lowers a reference to `token`; `whole` says the value itself is what is read,
    /// which for an ambient value is noted for the loader's order (∆HF0).
    fn name_read(&mut self, token: &SyntaxToken, whole: bool) -> Type {
        let span = token.span();
        match self.resolve(token.text()) {
            Some(Resolved::Local { slot, ty, binder }) => {
                self.emit(Op::Local { slot }, span);
                self.narrowed(&Path::local(binder)).unwrap_or(ty)
            }
            // Named as a value, a function is stored or handed on, not called: what
            // it reads is no part of the loader's order (∆HF0).
            Some(Resolved::Function) => {
                let export = self.export(token.text());
                let func = self.func_operand(export, span);
                self.emit(Op::Global { func }, span);
                self.known(export.signature.ty())
            }
            Some(Resolved::Builtin(_)) => {
                self.error(
                    span,
                    format!(
                        "`{}` is a builtin and cannot be used as a value",
                        token.text()
                    ),
                );
                Type::Any
            }
            Some(Resolved::Ambient(ty)) => {
                if let Some(refusal) = self.unavailable_at_load(token.text()) {
                    self.error(span, refusal);
                    return Type::Any;
                }
                if whole {
                    self.note_read(Access::Whole(Rc::from(token.text())));
                }
                let name = self.intern(token.text());
                self.emit(Op::Ambient { name }, span);
                let ty = self.ambient_type(token.text(), ty, span);
                self.narrowed(&Path::ambient(token.text())).unwrap_or(ty)
            }
            // A sibling binding at the bindings file's top level: a field of the
            // ambient value, read exactly as `value.name` is — the value's binding
            // recorded as this unit's fact, the read noted for the loader's order.
            Some(Resolved::Field { value, ty }) => {
                let ambient = self.intern(&value);
                self.emit(Op::Ambient { name: ambient }, span);
                let field = self.intern(token.text());
                self.emit(Op::GetField { name: field }, span);
                self.ambient_type(&value, Type::Named(Rc::from(self.path)), span);
                self.note_read(Access::Ambient {
                    value: value.clone(),
                    field: Rc::from(token.text()),
                });
                let ty = self.known(ty);
                self.narrowed(&Path::ambient_field(&value, token.text()))
                    .unwrap_or(ty)
            }
            None => {
                self.error(span, self.undefined(token.text()));
                Type::Any
            }
        }
    }

    /// Why the ambient value `name` cannot be read by the function being compiled, if
    /// the loader runs it and it cannot ([`refused_at_load`]).
    fn unavailable_at_load(&self, name: &str) -> Option<String> {
        let runs = self.uses.as_ref()?.runs?;
        refused_at_load(runs, self.host.document(), name)
    }
}

/// The function the loader runs a read stands in: a field's default, of a record that
/// extends the host's document type or not, or a computed binding of the bindings
/// file. What decides which ambient values the function may read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Runs {
    Default { page_backed: bool },
    Binding,
}

/// Why the ambient value `name` is not there for a function the loader runs as
/// `runs`, if it is not — `document` being the host's document value: a default reads
/// only the record it builds, since a record's plan depends on its own declarations
/// alone (∆eag), and the document value is bound per page, so a computed binding,
/// which runs before any page is, may not read that one (∆iCW). One rule and one
/// sentence, said by the compiler where the read stands and by the link step when a
/// call reaches it (∆HF0).
pub(crate) fn refused_at_load(
    runs: Runs,
    document: Option<&str>,
    name: &str,
) -> Option<String> {
    match runs {
        Runs::Default { page_backed } => Some(format!(
            "`{name}` is out of scope in a default: a default reads only the record it \
             builds — `self`{}",
            if page_backed {
                ", and `path` and `body` on a page"
            } else {
                ""
            }
        )),
        Runs::Binding => (document == Some(name)).then(|| {
            format!(
                "`{name}` is bound per page, and a binding here is computed once, at load"
            )
        }),
    }
}

impl Compiler<'_> {
    /// Why a name resolves to nothing: a name the project declares as a type is not a
    /// value, and saying so beats "undefined name" for the author who wrote `Placement`
    /// where its variant belonged.
    pub(super) fn undefined(&self, name: &str) -> String {
        match self.decls().kind(name) {
            Some(Kind::Enum) => format!("`{name}` is an enum, not a value"),
            Some(Kind::Record) => format!("`{name}` is a record, not a value"),
            None => format!("undefined name `{name}`"),
        }
    }

    /// Emits a variant: an enum is its variant's name at run time (D32), so the value
    /// on the stack is that name as a string constant.
    pub(super) fn emit_variant(&mut self, name: &str, span: Span) {
        let index = self.intern(name);
        self.emit(Op::Const { index }, span);
    }

    /// The enum `object.…` reads a variant off, when `object` is a bare name the
    /// project declares as one. The type namespace answers `Name.x` before the value
    /// namespace does, so `Placement.hero` means the same thing wherever it stands,
    /// whatever a local of that spelling holds (∆EN6).
    pub(super) fn enum_object(&self, object: &Expr) -> Option<Rc<str>> {
        let Expr::Name(name) = object else {
            return None;
        };
        let token = required(name.name());
        (self.decls().kind(token.text()) == Some(Kind::Enum))
            .then(|| Rc::from(token.text()))
    }

    /// Lowers `Placement.hero`, having established that `Placement` is a declared enum.
    /// The declaration is a fact this unit consumed (D25): the name is written here with
    /// no annotation behind it to have noted it.
    fn qualified_variant(
        &mut self,
        enum_name: &Rc<str>,
        member: &MemberExpr,
        object_span: Span,
    ) -> Type {
        let name = required(member.name());
        self.note_type(enum_name, object_span);
        let ty = if member.is_optional() {
            self.error(
                member.span(),
                format!("`{enum_name}` is an enum: read a variant with `.`, not `?.`"),
            );
            Type::Any
        } else {
            self.rule(
                member.span(),
                rules::qualified_variant(enum_name, name.text(), self.decls()),
            )
        };
        self.emit_variant(name.text(), member.span());
        ty
    }

    fn member(&mut self, member: &MemberExpr) -> Type {
        let span = member.span();
        let object = required(member.object());
        if let Some(enum_name) = self.enum_object(&object) {
            return self.qualified_variant(&enum_name, member, object.span());
        }
        let object_ty = self.object(&object);
        let name = required(member.name());
        self.note_member_read(&object, name.text());
        let index = self.intern(name.text());
        let optional = member.is_optional();
        let op = if optional {
            Op::OptField { name: index }
        } else {
            Op::GetField { name: index }
        };
        self.emit(op, span);
        let ty = self.rule(
            span,
            rules::member(&object_ty, name.text(), optional, self.decls()),
        );
        let ty = self.known(ty);
        // A condition on this very path may have narrowed it for the arm being lowered.
        match self.path_of(&Expr::Member(member.clone())) {
            Some((path, _)) => self.narrowed(&path).unwrap_or(ty),
            None => ty,
        }
    }

    fn index(&mut self, index: &IndexExpr) -> Type {
        let span = index.span();
        let object_ty = self.expr(&required(index.object()));
        let index_ty = self.expr(&required(index.index()));
        self.emit(Op::Index, span);
        self.rule(span, rules::index(&object_ty, &index_ty, self.decls()))
    }

    fn unary(&mut self, unary: &UnaryExpr) -> Type {
        let span = unary.span();
        let ty = self.expr(&required(unary.operand()));
        let (op, _) = required(unary.op());
        self.emit(
            match op {
                UnaryOp::Neg => Op::Neg,
                UnaryOp::Not => Op::Not,
            },
            span,
        );
        self.rule(span, rules::unary(op, &ty))
    }

    fn binary(&mut self, binary: &BinaryExpr) -> Type {
        let span = binary.span();
        let (op, token) = required(binary.op());
        let lhs = required(binary.lhs());
        let rhs = required(binary.rhs());
        let lhs_ty = self.expr(&lhs);
        match op {
            BinaryOp::And | BinaryOp::Or => {
                let jump = if op == BinaryOp::And {
                    Op::JumpIfFalseKeep { to: 0 }
                } else {
                    Op::JumpIfTrueKeep { to: 0 }
                };
                self.require(lhs.span(), rules::logical(token.text(), &lhs_ty));
                let end = self.emit_jump(jump, span);
                let rhs_ty = self.expr(&rhs);
                self.require(rhs.span(), rules::logical(token.text(), &rhs_ty));
                self.patch(end);
                Type::Bool
            }
            BinaryOp::Coalesce => {
                let end = self.emit_jump(Op::JumpUnlessNull { to: 0 }, span);
                let ty = match &lhs_ty {
                    Type::Any => {
                        self.expr(&rhs);
                        Type::Any
                    }
                    // A value known to be null is an optional with nothing inside:
                    // `null ?? x` is `x`.
                    Type::Null => self.expr(&rhs),
                    Type::Optional(inner) => {
                        let rhs_ty = self.check(&rhs, &Type::optional((**inner).clone()));
                        // The one type both sides share — `T` when the default is one,
                        // `T?` when it may itself be null, `any` when it is unknown.
                        join(inner, &rhs_ty, self.decls())
                            .unwrap_or_else(|| (**inner).clone())
                    }
                    // Inside a `lem-if` on it, the binding is already known to hold a
                    // value; a default there is dead, not wrong.
                    other if self.declared_optional(&lhs) => {
                        self.check(&rhs, &Type::optional(other.clone()));
                        other.clone()
                    }
                    other => {
                        self.error(
                            lhs.span(),
                            format!("`??` needs an optional on its left, got `{other}`"),
                        );
                        self.expr(&rhs);
                        other.clone()
                    }
                };
                self.patch(end);
                ty
            }
            _ => {
                // Beside an enum the right operand is lowered expecting it, so a bare
                // variant may stand there; the operator rule still judges both types.
                let want = rules::operand_expectation(op, &lhs_ty, self.decls());
                let rhs_ty = self.lower(&rhs, want.as_ref());
                self.emit(
                    match op {
                        BinaryOp::Add => Op::Add,
                        BinaryOp::Sub => Op::Sub,
                        BinaryOp::Mul => Op::Mul,
                        BinaryOp::Div => Op::Div,
                        BinaryOp::Rem => Op::Rem,
                        BinaryOp::Eq => Op::Eq,
                        BinaryOp::NotEq => Op::Ne,
                        BinaryOp::Lt => Op::Lt,
                        BinaryOp::Lte => Op::Le,
                        BinaryOp::Gt => Op::Gt,
                        BinaryOp::Gte => Op::Ge,
                        BinaryOp::In => Op::In,
                        BinaryOp::And | BinaryOp::Or | BinaryOp::Coalesce => {
                            unreachable!("handled above")
                        }
                    },
                    span,
                );
                self.rule(
                    span,
                    rules::operator(op, token.text(), &lhs_ty, &rhs_ty, self.decls()),
                )
            }
        }
    }

    fn ternary(&mut self, ternary: &TernaryExpr, expected: Option<&Type>) -> Type {
        let span = ternary.span();
        let condition = required(ternary.condition());
        let (_, narrowing) = self.lower_condition(&condition);
        let otherwise = self.emit_jump(Op::JumpIfFalse { to: 0 }, span);
        let depth = self.narrowings.len();
        self.narrowings.extend(narrowing);
        let then_ty = self.expr_in(&required(ternary.then()), expected);
        self.narrowings.truncate(depth);
        let end = self.emit_jump(Op::Jump { to: 0 }, span);
        self.patch(otherwise);
        let else_ty = self.expr_in(&required(ternary.otherwise()), expected);
        self.patch(end);
        match expected {
            Some(expected) => expected.clone(),
            None => self.rule(span, rules::branches(&then_ty, &else_ty, self.decls())),
        }
    }

    fn list(&mut self, list: &ListLit, expected: Option<&Type>) -> Type {
        let want = rules::literal_expectation(expected, |ty| match ty {
            Type::List(element) => Some(element),
            _ => None,
        });
        let mut element = want.clone();
        let mut len = 0;
        for item in list.elements() {
            self.literal_element(&item, want.as_ref(), &mut element, "list's elements");
            len += 1;
        }
        self.emit(Op::MakeList { len }, list.span());
        Type::list(element.unwrap_or(Type::Any))
    }

    /// Lowers `{…}`: a record literal where a record is expected (D35), a map literal
    /// everywhere else.
    fn map(&mut self, map: &MapLit, expected: Option<&Type>) -> Type {
        if let Some(record) = rules::expected_record(expected, self.decls()) {
            let record = Rc::<str>::from(record);
            return self.record_literal(map, &record);
        }
        let want = rules::literal_expectation(expected, |ty| match ty {
            Type::Map(value) => Some(value),
            _ => None,
        });
        let mut value = want.clone();
        let mut len = 0;
        let mut seen: Vec<String> = Vec::new();
        for entry in map.entries() {
            let Some((key, text)) = self.map_key(&entry, &mut seen) else {
                continue;
            };
            let index = self.intern(&text);
            self.emit(Op::Const { index }, key.span());
            let item = required(entry.value());
            self.literal_element(&item, want.as_ref(), &mut value, "map's values");
            len += 1;
        }
        self.emit(Op::MakeMap { len }, map.span());
        Type::map(value.unwrap_or(Type::Any))
    }

    /// The key of a map or record literal's entry, decoded, unless it repeats one in
    /// `seen`: a repeated key in a *literal* is a mistake, not a merge — `Map` keeps the
    /// last value written, so saying nothing would silently drop the first.
    fn map_key(
        &mut self,
        entry: &MapEntry,
        seen: &mut Vec<String>,
    ) -> Option<(SyntaxToken, String)> {
        let key = required(entry.key());
        let text = if key.kind() == SyntaxKind::StringLit {
            decode_string(key.text())
        } else {
            key.text().to_owned()
        };
        if seen.contains(&text) {
            self.error(key.span(), format!("`{text}` is already given"));
            return None;
        }
        seen.push(text.clone());
        Some((key, text))
    }

    /// Lowers `{…}` as a value of the declared record `record`: each entry is checked
    /// against the field it names, an omitted optional field is filled with null, and
    /// an omitted required one is refused — the value is complete, so a field read
    /// never misses on a record (D35). An entry naming no field is still lowered, for
    /// its diagnostics.
    fn record_literal(&mut self, map: &MapLit, record: &Rc<str>) -> Type {
        self.note_type(record, map.span());
        let mut len = 0;
        let mut seen: Vec<String> = Vec::new();
        for entry in map.entries() {
            let Some((key, text)) = self.map_key(&entry, &mut seen) else {
                continue;
            };
            let index = self.intern(&text);
            self.emit(Op::Const { index }, key.span());
            let item = required(entry.value());
            let field =
                self.rule(key.span(), rules::record_entry(record, &text, self.decls()));
            let field = self.known(field);
            self.check(&item, &field);
            len += 1;
        }
        let omitted = self.rule_or(
            map.span(),
            rules::record_omissions(record, &seen, self.decls()),
            Vec::new(),
        );
        for name in omitted {
            let index = self.intern(&name);
            self.emit(Op::Const { index }, map.span());
            let null = self.constant(Value::Null);
            self.emit(Op::Const { index: null }, map.span());
            len += 1;
        }
        let name = self.intern(record);
        self.emit(Op::MakeRecord { name, len }, map.span());
        Type::Named(record.clone())
    }

    /// Lowers one element of a list or map literal: checked against the element type
    /// `want`ed, or — with nothing wanted — inferred and folded into `so_far`, which the
    /// first element sets and an element that fits neither leaves as it was.
    fn literal_element(
        &mut self,
        item: &Expr,
        want: Option<&Type>,
        so_far: &mut Option<Type>,
        what: &str,
    ) {
        if let Some(want) = want {
            self.check(item, want);
            return;
        }
        let ty = self.expr(item);
        *so_far = Some(match so_far.take() {
            None => ty,
            Some(so_far) => self.rule_or(
                item.span(),
                rules::element(&so_far, &ty, what, self.decls()),
                so_far,
            ),
        });
    }

    fn lambda(&mut self, lambda: &Lambda, expected: Option<&Type>) -> Type {
        // Parameter types come from the function type expected here; with none, they
        // are `any` (§Checking). A lambda of the wrong arity is still lowered with `any`
        // parameters, so its body is checked and the names it reads count as read.
        let shape = rules::lambda_shape(expected, lambda_params(lambda).len());
        let mismatched = shape.is_err();
        let want = self.rule_or(lambda.span(), shape, None);
        self.lambda_under(lambda, want, mismatched)
    }

    /// Lowers `lambda` under `want`: its parameter types, and the return its body is
    /// checked against when the shape names one — with none the body is inferred and
    /// its type is the lambda's return. `mismatched` says the shape was refused, so the
    /// lambda's own type is `any`.
    pub(super) fn lambda_under(
        &mut self,
        lambda: &Lambda,
        want: Option<LambdaShape>,
        mismatched: bool,
    ) -> Type {
        let span = lambda.span();
        let params = lambda_params(lambda);
        let param_tys: Rc<[Type]> = match &want {
            Some(shape) => shape.params.clone(),
            None => Rc::from(vec![Type::Any; params.len()]),
        };
        self.push_frame();
        for (param, ty) in params.iter().zip(param_tys.iter()) {
            self.bind(param, BindingKind::Parameter, ty.clone());
        }
        let body = required(lambda.body());
        let ret = match want.as_ref().and_then(|shape| shape.ret.as_ref()) {
            Some(ret) => self.check(&body, ret),
            None => self.expr(&body),
        };
        self.emit(Op::Return, span);
        let (ctx, frame) = self.pop_frame();
        let func = self.reserve("<lambda>");
        self.finish(
            func,
            ctx,
            &frame,
            params.iter().map(|p| Rc::from(p.text())).collect(),
        );
        let captures = frame.captures();
        for capture in captures {
            let slot = capture.parent_slot;
            self.emit(Op::Local { slot }, span);
        }
        self.emit(
            Op::MakeClosure {
                func,
                captures: u16::try_from(captures.len()).expect("fewer than 65536 captures"),
            },
            span,
        );
        if mismatched {
            Type::Any
        } else {
            Type::Fn(param_tys, Rc::new(ret))
        }
    }
}

/// The parameter names a lambda declares, in order.
pub(super) fn lambda_params(lambda: &Lambda) -> Vec<SyntaxToken> {
    lambda
        .params()
        .map(|p| p.names().collect())
        .unwrap_or_default()
}

/// `expr` without the parentheses around it.
pub(super) fn unparen(mut expr: Expr) -> Expr {
    while let Expr::Paren(paren) = &expr {
        expr = required(paren.inner());
    }
    expr
}

/// The text of a string literal token: the quotes dropped and every `\c` replaced by
/// `c`, the scanner's rule. Interpolation is left as written — its delimiter is a
/// deferred question (docs/decisions/README.md).
pub(super) fn decode_string(token: &str) -> String {
    let inner = token
        .strip_prefix('"')
        .expect("a string literal opens with a quote");
    let inner = inner.strip_suffix('"').unwrap_or(inner);
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.extend(chars.next()),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_the_quotes_and_unescapes_backslashes() {
        assert_eq!(decode_string(r#""say \"hi\" \\ {x}""#), r#"say "hi" \ {x}"#);
    }

    #[test]
    fn keeps_a_trailing_backslash_from_panicking() {
        assert_eq!(decode_string("\"a\\"), "a");
    }
}

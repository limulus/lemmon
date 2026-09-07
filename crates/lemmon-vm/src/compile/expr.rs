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

use std::rc::Rc;

use lemmon_syntax::ast::{
    ArgList, BinaryExpr, BinaryOp, CallExpr, Expr, IndexExpr, Lambda, ListLit, Literal,
    LiteralKind, MapLit, MemberExpr, NamedArg, Spanned, TernaryExpr, UnaryExpr, UnaryOp,
};
use lemmon_syntax::{Span, SyntaxKind, SyntaxToken};

use super::names::{BindingKind, Resolved};
use super::rules::{self, Arg, Rule};
use super::{Compiler, TokenSpan, required};
use crate::Value;
use crate::builtins::BUILTINS;
use crate::bytecode::Op;
use crate::decls::Kind;
use crate::types::{Type, join};

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
            Expr::TypeTest(test) => {
                self.expr(&required(test.expr()));
                self.error(span, "`is` needs narrowing, which is not here yet (∆PCK)");
                Type::Any
            }
        }
    }

    fn literal(&mut self, literal: &Literal) -> Type {
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
    /// where a `lem-if` on it is open — a function's signature, `any` for an ambient
    /// value.
    pub(super) fn name(&mut self, token: &SyntaxToken) -> Type {
        let span = token.span();
        match self.resolve(token.text()) {
            Some(Resolved::Local { slot, ty, binder }) => {
                self.emit(Op::Local { slot }, span);
                self.narrowed(binder).unwrap_or(ty)
            }
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
            Some(Resolved::Ambient) => {
                let name = self.intern(token.text());
                self.emit(Op::Ambient { name }, span);
                Type::Any
            }
            None => {
                self.error(span, self.undefined(token.text()));
                Type::Any
            }
        }
    }

    /// Why a name resolves to nothing: a name the project declares as a type is not a
    /// value, and saying so beats "undefined name" for the author who wrote `Placement`
    /// where its variant belonged.
    fn undefined(&self, name: &str) -> String {
        match self.decls().kind(name) {
            Some(Kind::Enum) => format!("`{name}` is an enum, not a value"),
            Some(Kind::Record) => format!("`{name}` is a record, not a value"),
            None => format!("undefined name `{name}`"),
        }
    }

    /// Emits a variant: an enum is its variant's name at run time (D32), so the value
    /// on the stack is that name as a string constant.
    fn emit_variant(&mut self, name: &str, span: Span) {
        let index = self.intern(name);
        self.emit(Op::Const { index }, span);
    }

    /// The enum `object.…` reads a variant off, when `object` is a bare name the
    /// project declares as one. The type namespace answers `Name.x` before the value
    /// namespace does, so `Placement.hero` means the same thing wherever it stands,
    /// whatever a local of that spelling holds (∆EN6).
    fn enum_object(&self, object: &Expr) -> Option<Rc<str>> {
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
        let object_ty = self.expr(&object);
        let name = required(member.name());
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
        self.known(ty)
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
        let condition_ty = self.expr(&condition);
        let narrowing = self.condition(&condition, &condition_ty);
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

    /// Applies the condition rule to a `lem-if` or `?:` condition: an optional that is
    /// a plain local name — parenthesised or not — narrows that binding to its inner
    /// type wherever the condition holds, returned for the caller to open. A binding an
    /// enclosing condition already narrowed passes as the optional it was declared.
    pub(super) fn condition(&mut self, expr: &Expr, ty: &Type) -> Option<(Span, Type)> {
        let expr = unparen(expr.clone());
        match rules::condition(ty) {
            Ok(None) => None,
            Ok(Some(inner)) => {
                let Expr::Name(name) = &expr else {
                    return None;
                };
                match self.names.peek(required(name.name()).text()) {
                    Some(Resolved::Local { binder, .. }) => Some((binder, inner)),
                    _ => None,
                }
            }
            Err(_) if self.declared_optional(&expr) => None,
            Err(message) => {
                self.error(expr.span(), message);
                None
            }
        }
    }

    /// Whether `expr` is a plain local name bound as an optional, whatever a `lem-if`
    /// around it has narrowed it to.
    fn declared_optional(&self, expr: &Expr) -> bool {
        let Expr::Name(name) = unparen(expr.clone()) else {
            return false;
        };
        matches!(
            self.names.peek(required(name.name()).text()),
            Some(Resolved::Local {
                ty: Type::Optional(_),
                ..
            })
        )
    }

    /// The type a `lem-if` on the binding at `binder` has narrowed it to, if one is open.
    fn narrowed(&self, binder: Span) -> Option<Type> {
        self.narrowings
            .iter()
            .rev()
            .find(|(b, _)| *b == binder)
            .map(|(_, ty)| ty.clone())
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

    fn map(&mut self, map: &MapLit, expected: Option<&Type>) -> Type {
        let want = rules::literal_expectation(expected, |ty| match ty {
            Type::Map(value) => Some(value),
            _ => None,
        });
        let mut value = want.clone();
        let mut len = 0;
        let mut seen: Vec<String> = Vec::new();
        for entry in map.entries() {
            let key = required(entry.key());
            let text = if key.kind() == SyntaxKind::StringLit {
                decode_string(key.text())
            } else {
                key.text().to_owned()
            };
            // A repeated key in a *literal* is a mistake, not a merge: `Map` keeps the
            // last value written, so saying nothing would silently drop the first.
            if seen.contains(&text) {
                self.error(key.span(), format!("`{text}` is already given"));
                continue;
            }
            seen.push(text.clone());
            let index = self.intern(&text);
            self.emit(Op::Const { index }, key.span());
            let item = required(entry.value());
            self.literal_element(&item, want.as_ref(), &mut value, "map's values");
            len += 1;
        }
        self.emit(Op::MakeMap { len }, map.span());
        Type::map(value.unwrap_or(Type::Any))
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
        let span = lambda.span();
        let params: Vec<SyntaxToken> = lambda
            .params()
            .map(|p| p.names().collect())
            .unwrap_or_default();
        // Parameter types come from the function type expected here; with none, they
        // are `any` (§Checking). A lambda of the wrong arity is still lowered with `any`
        // parameters, so its body is checked and the names it reads count as read.
        let shape = rules::lambda_shape(expected, params.len());
        let mismatched = shape.is_err();
        let want = self.rule_or(span, shape, None);
        let param_tys: Rc<[Type]> = match &want {
            Some((param_tys, _)) => param_tys.clone(),
            None => Rc::from(vec![Type::Any; params.len()]),
        };
        self.push_frame();
        for (param, ty) in params.iter().zip(param_tys.iter()) {
            self.bind(param, BindingKind::Parameter, ty.clone());
        }
        let body = required(lambda.body());
        let ret = match &want {
            Some((_, ret)) => self.check(&body, ret),
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

    fn call(&mut self, call: &CallExpr) -> Type {
        let span = call.span();
        let callee = required(call.callee());
        let args = call.args();
        let positional: Vec<Expr> = args.iter().flat_map(ArgList::positional).collect();
        let named: Vec<NamedArg> = args.iter().flat_map(ArgList::named).collect();

        // `.` asks the receiver's type and nothing else (D34): `x.f(…)` calls a builtin
        // type's method `f`, or a record's field `f` as the value it holds. What the
        // enclosing scope names `f` never enters into it.
        let (resolved, name) = match &callee {
            // A qualified variant is not a receiver: the enum claims `Name.x` before
            // anything else (∆EN6), so `Placement.hero(…)` falls through to the last arm
            // below, which lowers it as the variant it is and reports the call of a
            // `Placement`.
            Expr::Member(member)
                if self.enum_object(&required(member.object())).is_none() =>
            {
                return self.call_member(member, &positional, &named, span);
            }
            Expr::Name(name) => {
                let name = required(name.name());
                match self.resolve(name.text()) {
                    Some(resolved) => (Some(resolved), Some(name)),
                    None => {
                        self.error(name.span(), self.undefined(name.text()));
                        self.discard(&positional, &named);
                        return Type::Any;
                    }
                }
            }
            _ => (None, None),
        };

        match resolved {
            Some(Resolved::Function) => {
                let name = name.expect("a project function has a name");
                self.call_function(name.text(), &positional, &named, span)
            }
            Some(Resolved::Builtin(id)) => {
                let name = name.expect("a builtin has a name");
                if self.reject_named(&named, &name) {
                    self.discard(&positional, &named);
                    return Type::Any;
                }
                self.call_builtin(id, None, &positional, span)
            }
            Some(Resolved::Local { .. } | Resolved::Ambient) => {
                let name = name.expect("a resolved name");
                if self.reject_named(&named, &name) {
                    self.discard(&positional, &named);
                    return Type::Any;
                }
                let ty = self.name(&name);
                self.call_value(&ty, &positional, span, Some(name.text()))
            }
            None => {
                if let Some(first) = named.first() {
                    self.error(
                        first.span(),
                        "named arguments need a callee that is a `lem-fn` by name",
                    );
                    self.expr(&callee);
                    self.discard(&positional, &named);
                    return Type::Any;
                }
                let ty = self.expr(&callee);
                self.call_value(&ty, &positional, span, None)
            }
        }
    }

    /// Lowers `object.name(…)`, the one call form that reads the dot (D34). The receiver
    /// is lowered first and its type decides everything after: a builtin type's method,
    /// a record's field called as the value it holds, or — on `any` — a name the VM
    /// resolves against the value it finds there.
    ///
    /// Through `?.` the receiver stays on the stack and the whole call is jumped around
    /// when it is null, so neither the field nor the method is reached and the result is
    /// the call's made optional.
    fn call_member(
        &mut self,
        member: &MemberExpr,
        positional: &[Expr],
        named: &[NamedArg],
        span: Span,
    ) -> Type {
        let object = required(member.object());
        let name = required(member.name());
        let member_span = member.span();
        let optional = member.is_optional();
        let object_ty = self.expr(&object);
        let verdict =
            rules::callable_member(&object_ty, name.text(), optional, self.decls());
        let callable = match verdict {
            Ok(callable) => callable,
            Err(message) => {
                self.error(member_span, message);
                self.discard(positional, named);
                return Type::Any;
            }
        };
        if let Some(first) = named.first() {
            // A method is a builtin, and no builtin names its parameters — saying so
            // beats sending the author to look for a `lem-fn` that could not help.
            let message = match &callable {
                rules::Callable::Method(_) => {
                    format!("`.{}()` does not take named arguments", name.text())
                }
                _ => "named arguments need a callee that is a `lem-fn` by name".to_owned(),
            };
            self.error(first.span(), message);
            self.discard(positional, named);
            return Type::Any;
        }
        let skip = optional.then(|| self.skip_when_null(member_span));
        // What carries the method or the field is the receiver's own type, which is what
        // `?.` has an optional around.
        let receiver = object_ty.strip_optional().clone();
        let ty = match callable {
            rules::Callable::Method(id) => {
                self.call_builtin(id, Some(receiver), positional, span)
            }
            rules::Callable::Field(field) => {
                let index = self.intern(name.text());
                self.emit(Op::GetField { name: index }, member_span);
                let field = self.known(field);
                self.call_value(&field, positional, span, Some(name.text()))
            }
            rules::Callable::Dynamic => {
                for arg in positional {
                    self.expr(arg);
                }
                let index = self.intern(name.text());
                let argc =
                    u16::try_from(positional.len()).expect("fewer than 65536 arguments");
                self.emit(Op::CallDynamic { name: index, argc }, span);
                Type::Any
            }
        };
        match skip {
            Some(end) => {
                self.patch(end);
                Type::optional(ty)
            }
            None => ty,
        }
    }

    /// Emits the jump that leaves a null receiver where it stands and steps over the
    /// call about to be emitted; returns the jump to patch once that call is in.
    fn skip_when_null(&mut self, span: Span) -> usize {
        let call = self.emit_jump(Op::JumpUnlessNull { to: 0 }, span);
        let index = self.constant(Value::Null);
        self.emit(Op::Const { index }, span);
        let end = self.emit_jump(Op::Jump { to: 0 }, span);
        self.patch(call);
        end
    }

    /// Lowers a call of `BUILTINS[id]`, reached by its bare name or — with `receiver` —
    /// as a method, whose value is already the first on the stack.
    fn call_builtin(
        &mut self,
        id: u32,
        receiver: Option<Type>,
        positional: &[Expr],
        span: Span,
    ) -> Type {
        let builtin = &BUILTINS[id as usize];
        let arity = positional.len() + usize::from(receiver.is_some());
        if self.require(span, rules::builtin(builtin, arity)) {
            self.discard(positional, &[]);
            return Type::Any;
        }
        let mut types = Vec::with_capacity(arity);
        types.extend(receiver);
        for arg in positional {
            types.push(self.expr(arg));
        }
        let argc = u16::try_from(arity).expect("fewer than 65536 arguments");
        self.emit(Op::CallBuiltin { id, argc }, span);
        self.rule(span, (BUILTINS[id as usize].ty)(&types))
    }

    /// Lowers a call of a value already on the stack: a local, an ambient value, or any
    /// other expression. A function value's type checks the arguments; `any` leaves them
    /// to the VM.
    fn call_value(
        &mut self,
        callee: &Type,
        positional: &[Expr],
        span: Span,
        name: Option<&str>,
    ) -> Type {
        let (params, ret) = match rules::callee(callee, name, positional.len()) {
            Ok(granted) => granted,
            Err(message) => {
                self.error(span, message);
                self.discard(positional, &[]);
                return Type::Any;
            }
        };
        match params {
            Some(params) => {
                for (arg, param) in positional.iter().zip(params.iter()) {
                    self.check(arg, param);
                }
            }
            None => {
                for arg in positional {
                    self.expr(arg);
                }
            }
        }
        let argc = u16::try_from(positional.len()).expect("fewer than 65536 arguments");
        self.emit(Op::CallValue { argc }, span);
        ret
    }

    /// Reports named arguments passed to something that has no parameter names.
    fn reject_named(&mut self, named: &[NamedArg], callee: &SyntaxToken) -> bool {
        match named.first() {
            Some(first) => {
                self.error(
                    first.span(),
                    format!("`{}` does not take named arguments", callee.text()),
                );
                true
            }
            None => false,
        }
    }

    /// Lowers a call's arguments after an error that stops the call from compiling: the
    /// names they read still count as read, and the mistakes inside them are still
    /// reported. A receiver was lowered before the call was decided, so it is never
    /// among them. The code emitted is unbalanced, and never runs — an error yields no
    /// unit.
    fn discard(&mut self, positional: &[Expr], named: &[NamedArg]) {
        for expr in positional.iter().chain(
            named
                .iter()
                .map(|arg| required(arg.value()))
                .collect::<Vec<_>>()
                .iter(),
        ) {
            self.expr(expr);
        }
    }

    /// Lowers a call to a project function, reordering named arguments into parameter
    /// order at compile time so the VM only ever sees positional calls, and checking
    /// each against its parameter's type.
    fn call_function(
        &mut self,
        fn_name: &str,
        positional: &[Expr],
        named: &[NamedArg],
        span: Span,
    ) -> Type {
        let export = self.export(fn_name);
        let names: Vec<SyntaxToken> =
            named.iter().map(|arg| required(arg.name())).collect();
        let name_texts: Vec<&str> = names.iter().map(SyntaxToken::text).collect();
        let order = rules::arguments(
            fn_name,
            &export.signature.params,
            positional.len(),
            &name_texts,
        );
        let order = match order {
            Ok(order) => order,
            Err((blame, message)) => {
                let at = match blame {
                    Some(Arg::Positional(i)) => positional[i].span(),
                    Some(Arg::Named(n)) => names[n].span(),
                    None => span,
                };
                self.error(at, message);
                self.discard(positional, named);
                return Type::Any;
            }
        };
        for (param, arg) in export.signature.params.iter().zip(order) {
            let param_ty = self.known(param.ty.clone());
            match arg {
                Arg::Positional(i) => {
                    self.check(&positional[i], &param_ty);
                }
                Arg::Named(n) => {
                    self.check(&required(named[n].value()), &param_ty);
                }
            }
        }
        let argc = u16::try_from(export.signature.params.len())
            .expect("fewer than 65536 parameters");
        let func = self.func_operand(export, span);
        self.emit(Op::Call { func, argc }, span);
        self.known(export.signature.ret.clone())
    }
}

/// `expr` without the parentheses around it.
fn unparen(mut expr: Expr) -> Expr {
    while let Expr::Paren(paren) = &expr {
        expr = required(paren.inner());
    }
    expr
}

/// The text of a string literal token: the quotes dropped and every `\c` replaced by
/// `c`, the scanner's rule. Interpolation is left as written — its delimiter is a
/// deferred question (docs/decisions/README.md).
fn decode_string(token: &str) -> String {
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

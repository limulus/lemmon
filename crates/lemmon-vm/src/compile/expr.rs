//! Lowering of expressions: the heads of tags — typed as they are lowered (D31).
//!
//! Every arm emits the code for its expression and returns the expression's type.
//! Checking is bidirectional (`docs/language.md` §Checking): where a type is expected —
//! an annotated binding, a parameter, a list under a typed list — [`Compiler::check`]
//! flows the expectation into literals, lambdas, and conditionals, and everything else
//! is inferred and then compared. An expression that fails types as `any`, so that one
//! mistake costs one diagnostic and the rest of the file is still checked.

use std::rc::Rc;

use lemmon_syntax::ast::{
    ArgList, BinaryExpr, BinaryOp, CallExpr, Expr, IndexExpr, Lambda, ListLit, Literal,
    LiteralKind, MapLit, MemberExpr, NamedArg, Spanned, TernaryExpr, UnaryExpr, UnaryOp,
};
use lemmon_syntax::{Span, SyntaxKind, SyntaxToken};

use super::names::{BindingKind, Resolved};
use super::{Compiler, TokenSpan, required};
use crate::Value;
use crate::builtins::BUILTINS;
use crate::bytecode::Op;
use crate::types::{Type, assignable, join};
use crate::unit::Param;

/// The type an expectation asks its literal for: a `T?` wanted is a `T` written.
fn expectation(expected: Option<&Type>) -> Option<&Type> {
    expected.map(Type::strip_optional)
}

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

    fn expr_in(&mut self, expr: &Expr, expected: Option<&Type>) -> Type {
        let span = expr.span();
        let actual = match expr {
            Expr::Literal(literal) => self.literal(literal),
            Expr::Name(name) => {
                let token = required(name.name());
                self.name(&token)
            }
            Expr::Paren(paren) => self.expr_in(&required(paren.inner()), expected),
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
        };
        match expected {
            Some(expected) if !assignable(&actual, expected) => {
                self.error(span, format!("expected `{expected}`, got `{actual}`"));
                expected.clone()
            }
            _ => actual,
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
                export.signature.ty()
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
                self.error(span, format!("undefined name `{}`", token.text()));
                Type::Any
            }
        }
    }

    fn member(&mut self, member: &MemberExpr) -> Type {
        let span = member.span();
        let object_ty = self.expr(&required(member.object()));
        let name = required(member.name());
        let index = self.intern(name.text());
        let optional = member.is_optional();
        let op = if optional {
            Op::OptField { name: index }
        } else {
            Op::GetField { name: index }
        };
        self.emit(op, span);
        match (&object_ty, optional) {
            (Type::Any, _) => Type::Any,
            // A value known to be null is an optional with nothing inside: `?.` on it
            // is null again.
            (Type::Null, true) => Type::Null,
            (Type::Optional(inner), true) => self
                .field_type(inner, name.text(), span)
                .map_or(Type::Any, Type::optional),
            (Type::Optional(_), false) => {
                self.may_be_null(&object_ty, span, ", or use `?.`");
                Type::Any
            }
            (other, true) => {
                self.error(span, format!("`?.` needs an optional, got `{other}`"));
                Type::Any
            }
            (other, false) => self
                .field_type(other, name.text(), span)
                .unwrap_or(Type::Any),
        }
    }

    /// The type of the field `name` of a value of type `ty`, or the error that it has
    /// none. Records arrive with ∆cea; today only `any` has fields.
    fn field_type(&mut self, ty: &Type, name: &str, span: Span) -> Option<Type> {
        match ty {
            Type::Any => Some(Type::Any),
            Type::Map(_) => {
                self.error(
                    span,
                    format!("a `{ty}` has no fields: index a map as `[\"{name}\"]`"),
                );
                None
            }
            Type::Null => {
                self.error(span, format!("cannot read field `{name}` of null"));
                None
            }
            other => {
                self.error(span, format!("a `{other}` has no field `{name}`"));
                None
            }
        }
    }

    /// The error for reading through a `T?` as if it were a `T`.
    fn may_be_null(&mut self, ty: &Type, span: Span, alternative: &str) {
        self.error(
            span,
            format!("a `{ty}` may be null: narrow it with `lem-if`{alternative}"),
        );
    }

    fn index(&mut self, index: &IndexExpr) -> Type {
        let span = index.span();
        let object_ty = self.expr(&required(index.object()));
        let index_ty = self.expr(&required(index.index()));
        self.emit(Op::Index, span);
        let (element, key) = match &object_ty {
            Type::Any => return Type::Any,
            Type::Optional(_) => {
                self.may_be_null(&object_ty, span, "");
                return Type::Any;
            }
            Type::List(element) => ((**element).clone(), Type::Int),
            Type::Map(value) => ((**value).clone(), Type::String),
            other => {
                self.error(span, format!("cannot index a `{other}`"));
                return Type::Any;
            }
        };
        if !assignable(&index_ty, &key) {
            let what = if key == Type::Int { "list" } else { "map" };
            self.error(
                span,
                format!("a {what} is indexed by `{key}`, got `{index_ty}`"),
            );
        }
        element
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
        match (op, &ty) {
            (UnaryOp::Neg, Type::Int | Type::Float | Type::Any) => ty,
            (UnaryOp::Neg, other) => {
                self.error(span, format!("cannot negate a `{other}`"));
                Type::Any
            }
            (UnaryOp::Not, Type::Bool | Type::Any) => Type::Bool,
            (UnaryOp::Not, other) => {
                self.error(span, format!("`!` needs a `bool`, got `{other}`"));
                Type::Bool
            }
        }
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
                self.logical(&lhs_ty, token.text(), lhs.span());
                let end = self.emit_jump(jump, span);
                let rhs_ty = self.expr(&rhs);
                self.logical(&rhs_ty, token.text(), rhs.span());
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
                        join(inner, &rhs_ty).unwrap_or_else(|| (**inner).clone())
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
                let rhs_ty = self.expr(&rhs);
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
                self.operator(op, token.text(), &lhs_ty, &rhs_ty, span)
            }
        }
    }

    /// The operand rule of `&&` and `||`.
    fn logical(&mut self, ty: &Type, symbol: &str, span: Span) {
        if !matches!(ty, Type::Bool | Type::Any) {
            self.error(span, format!("`{symbol}` needs a `bool`, got `{ty}`"));
        }
    }

    /// The type a binary operator's result has, or the error that its operands do not
    /// fit it (`docs/language.md` §Expressions). An `any` on either side is the VM's to
    /// check: the result is `any` where the operator yields its operands' type, and
    /// `bool` where it yields one.
    fn operator(
        &mut self,
        op: BinaryOp,
        symbol: &str,
        lhs: &Type,
        rhs: &Type,
        span: Span,
    ) -> Type {
        let mismatch = |this: &mut Self| {
            this.error(
                span,
                format!("cannot apply `{symbol}` to `{lhs}` and `{rhs}`"),
            );
        };
        let any = *lhs == Type::Any || *rhs == Type::Any;
        match op {
            BinaryOp::Add
            | BinaryOp::Sub
            | BinaryOp::Mul
            | BinaryOp::Div
            | BinaryOp::Rem => match (lhs, rhs) {
                _ if any => Type::Any,
                (Type::Int, Type::Int) => Type::Int,
                (a, b) if a.is_number() && b.is_number() => Type::Float,
                (Type::String, Type::String) if op == BinaryOp::Add => Type::String,
                _ => {
                    mismatch(self);
                    Type::Any
                }
            },
            BinaryOp::Lt | BinaryOp::Lte | BinaryOp::Gt | BinaryOp::Gte => {
                let ordered = any
                    || (lhs.is_number() && rhs.is_number())
                    || matches!(
                        (lhs, rhs),
                        (Type::String, Type::String) | (Type::Datetime, Type::Datetime)
                    );
                if !ordered {
                    mismatch(self);
                }
                Type::Bool
            }
            BinaryOp::Eq | BinaryOp::NotEq => {
                // Numbers compare numerically whatever their type, as the VM does.
                let numbers = lhs.is_number() && rhs.is_number();
                if matches!(lhs, Type::Fn(..)) || matches!(rhs, Type::Fn(..)) {
                    self.error(span, "functions cannot be compared");
                } else if !(numbers || assignable(lhs, rhs) || assignable(rhs, lhs)) {
                    self.error(span, format!("cannot compare `{lhs}` with `{rhs}`"));
                }
                Type::Bool
            }
            BinaryOp::In => {
                match rhs {
                    Type::Any => {}
                    Type::List(element) => {
                        if !assignable(lhs, element) {
                            self.error(
                                span,
                                format!(
                                    "`in` on a `{rhs}` needs `{element}` on its left, got `{lhs}`"
                                ),
                            );
                        }
                    }
                    Type::Map(_) => {
                        if !assignable(lhs, &Type::String) {
                            self.error(
                                span,
                                format!("`in` on a map needs a `string` key on its left, got `{lhs}`"),
                            );
                        }
                    }
                    _ => mismatch(self),
                }
                Type::Bool
            }
            BinaryOp::And | BinaryOp::Or | BinaryOp::Coalesce => {
                unreachable!("short-circuit operators are lowered by `binary`")
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
            None => join(&then_ty, &else_ty).unwrap_or_else(|| {
                self.error(
                    span,
                    format!(
                        "the branches have different types: `{then_ty}` and `{else_ty}`"
                    ),
                );
                Type::Any
            }),
        }
    }

    /// The condition rule: a `lem-if` or `?:` tests a `bool` or an optional (`any` is
    /// the VM's to test, and `null` is an optional with nothing inside). An optional
    /// that is a plain local name — parenthesised or not — narrows that binding to its
    /// inner type wherever the condition holds, returned for the caller to open. A
    /// binding an enclosing condition already narrowed passes as the optional it was
    /// declared.
    pub(super) fn condition(&mut self, expr: &Expr, ty: &Type) -> Option<(Span, Type)> {
        let expr = unparen(expr.clone());
        match ty {
            Type::Bool | Type::Any | Type::Null => None,
            Type::Optional(inner) => {
                let Expr::Name(name) = &expr else {
                    return None;
                };
                match self.names.peek(required(name.name()).text()) {
                    Some(Resolved::Local { binder, .. }) => {
                        Some((binder, (**inner).clone()))
                    }
                    _ => None,
                }
            }
            _ if self.declared_optional(&expr) => None,
            other => {
                self.error(
                    expr.span(),
                    format!("a condition must be a `bool` or an optional, got `{other}`"),
                );
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
        let want = literal_expectation(expected, |ty| match ty {
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
        let want = literal_expectation(expected, |ty| match ty {
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
    /// first element sets, a `null` makes optional, and an element that fits neither is
    /// reported against.
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
            Some(so_far) => join(&so_far, &ty).unwrap_or_else(|| {
                self.error(
                    item.span(),
                    format!("expected `{so_far}`, got `{ty}`: a {what} share one type"),
                );
                so_far
            }),
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
        let mut mismatched = false;
        let want: Option<(Rc<[Type]>, Type)> = match expectation(expected) {
            Some(Type::Fn(param_tys, ret)) if param_tys.len() == params.len() => {
                Some((param_tys.clone(), (**ret).clone()))
            }
            Some(want @ Type::Fn(param_tys, _)) => {
                self.error(
                    span,
                    format!(
                        "this lambda takes {}, but a `{want}` takes {}",
                        count(params.len(), "parameter"),
                        param_tys.len()
                    ),
                );
                mismatched = true;
                None
            }
            _ => None,
        };
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

        // `x.f(y)` is `f(x, y)` when `f` names a function (UFCS); otherwise it is a
        // field holding a function value. "Names a function" means "resolves to one
        // here" — a local `f` shadows it, as a local shadows anything — until records
        // (∆cea) give a receiver fields of its own to consult first.
        let ufcs = match &callee {
            Expr::Member(member) if !member.is_optional() => {
                let name = required(member.name());
                self.names
                    .peek_callable(name.text())
                    .map(|resolved| (resolved, name))
            }
            _ => None,
        };
        let (resolved, name, receiver) = match (ufcs, &callee) {
            (Some((resolved, name)), Expr::Member(member)) => {
                (Some(resolved), Some(name), Some(required(member.object())))
            }
            (_, Expr::Name(name)) => {
                let name = required(name.name());
                match self.resolve(name.text()) {
                    Some(resolved) => (Some(resolved), Some(name), None),
                    None => {
                        self.error(
                            name.span(),
                            format!("undefined name `{}`", name.text()),
                        );
                        self.discard(None, &positional, &named);
                        return Type::Any;
                    }
                }
            }
            _ => (None, None, None),
        };

        match resolved {
            Some(Resolved::Function) => {
                let name = name.expect("a project function has a name");
                self.call_function(name.text(), receiver, &positional, &named, span)
            }
            Some(Resolved::Builtin(id)) => {
                let name = name.expect("a builtin has a name");
                let builtin = &BUILTINS[id as usize];
                let arity = positional.len() + usize::from(receiver.is_some());
                // One mistake, one diagnostic: a named argument is the argument that
                // is then missing, so only the first rule broken is reported.
                let refused = self.reject_named(&named, &name)
                    || (arity != builtin.arity && {
                        self.arity_error(
                            span,
                            &format!("`{}`", builtin.name),
                            builtin.arity,
                            arity,
                        );
                        true
                    });
                if refused {
                    self.discard(receiver.as_ref(), &positional, &named);
                    return Type::Any;
                }
                let mut types = Vec::with_capacity(arity);
                if let Some(receiver) = &receiver {
                    types.push(self.expr(receiver));
                }
                for arg in &positional {
                    types.push(self.expr(arg));
                }
                let argc = u16::try_from(arity).expect("fewer than 65536 arguments");
                self.emit(Op::CallBuiltin { id, argc }, span);
                match (builtin.ty)(&types) {
                    Ok(ty) => ty,
                    Err(message) => {
                        self.error(span, message);
                        Type::Any
                    }
                }
            }
            Some(Resolved::Local { .. } | Resolved::Ambient) => {
                let name = name.expect("a resolved name");
                if self.reject_named(&named, &name) {
                    self.discard(None, &positional, &named);
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
                    self.discard(Some(&callee), &positional, &named);
                    return Type::Any;
                }
                let ty = self.expr(&callee);
                self.call_value(&ty, &positional, span, None)
            }
        }
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
        let ret = match callee {
            Type::Fn(params, ret) => {
                if params.len() != positional.len() {
                    let what =
                        name.map_or("this function".to_owned(), |n| format!("`{n}`"));
                    self.arity_error(span, &what, params.len(), positional.len());
                    self.discard(None, positional, &[]);
                    return Type::Any;
                }
                for (arg, param) in positional.iter().zip(params.iter()) {
                    self.check(arg, param);
                }
                (**ret).clone()
            }
            Type::Any => {
                for arg in positional {
                    self.expr(arg);
                }
                Type::Any
            }
            other => {
                self.error(span, format!("cannot call a `{other}`"));
                self.discard(None, positional, &[]);
                return Type::Any;
            }
        };
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

    /// "`f` takes 2 arguments, got 1"
    fn arity_error(&mut self, span: Span, what: &str, wanted: usize, got: usize) {
        self.error(
            span,
            format!("{what} takes {}, got {got}", count(wanted, "argument")),
        );
    }

    /// Lowers a call's operands after an error that stops the call from compiling: the
    /// names they read still count as read, and the mistakes inside them are still
    /// reported. The code emitted is unbalanced, and never runs — an error yields no
    /// unit.
    fn discard(
        &mut self,
        receiver: Option<&Expr>,
        positional: &[Expr],
        named: &[NamedArg],
    ) {
        for expr in receiver.into_iter().chain(positional).chain(
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
        receiver: Option<Expr>,
        positional: &[Expr],
        named: &[NamedArg],
        span: Span,
    ) -> Type {
        let export = self.export(fn_name);
        let params: &[Param] = &export.signature.params;
        let mut slots: Vec<Option<Expr>> = vec![None; params.len()];
        let given = receiver.iter().cloned().chain(positional.iter().cloned());
        let mut failed = false;
        for (i, arg) in given.enumerate() {
            match slots.get_mut(i) {
                Some(slot) => *slot = Some(arg),
                None => {
                    self.arity_error(
                        arg.span(),
                        &format!("`{fn_name}`"),
                        params.len(),
                        i + 1 + named.len(),
                    );
                    failed = true;
                    break;
                }
            }
        }
        for arg in named {
            let name = required(arg.name());
            let Some(i) = params.iter().position(|p| &*p.name == name.text()) else {
                self.error(
                    name.span(),
                    format!("`{fn_name}` has no parameter `{}`", name.text()),
                );
                failed = true;
                break;
            };
            if slots[i].is_some() {
                self.error(name.span(), format!("`{}` is already given", name.text()));
                failed = true;
                break;
            }
            slots[i] = Some(required(arg.value()));
        }
        if !failed {
            let mut missing = params.iter().zip(&slots).filter(|(_, s)| s.is_none());
            if let Some((param, _)) = missing.next() {
                self.error(
                    span,
                    format!("missing argument `{}` in call to `{fn_name}`", param.name),
                );
                failed = true;
            }
        }
        if failed {
            self.discard(receiver.as_ref(), positional, named);
            return Type::Any;
        }
        for (param, slot) in params.iter().zip(slots) {
            self.check(&slot.expect("every slot is filled"), &param.ty);
        }
        let argc = u16::try_from(params.len()).expect("fewer than 65536 parameters");
        let func = self.func_operand(export, span);
        self.emit(Op::Call { func, argc }, span);
        export.signature.ret.clone()
    }
}

/// `expr` without the parentheses around it.
fn unparen(mut expr: Expr) -> Expr {
    while let Expr::Paren(paren) = &expr {
        expr = required(paren.inner());
    }
    expr
}

/// "1 argument", "2 arguments".
fn count(n: usize, noun: &str) -> String {
    format!("{n} {noun}{}", if n == 1 { "" } else { "s" })
}

/// The element type a list or map literal is checked against, if the expectation says:
/// `pick` reads it from the expected container, and an expected `any` opts the whole
/// literal out of checking — its elements are `any` too.
fn literal_expectation(
    expected: Option<&Type>,
    pick: fn(&Type) -> Option<&Rc<Type>>,
) -> Option<Type> {
    match expectation(expected) {
        Some(Type::Any) => Some(Type::Any),
        Some(ty) => pick(ty).map(|element| (**element).clone()),
        None => None,
    }
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

    #[test]
    fn counts_with_the_plural_the_number_wants() {
        assert_eq!(count(1, "argument"), "1 argument");
        assert_eq!(count(0, "parameter"), "0 parameters");
    }

    #[test]
    fn strips_one_optional_from_an_expectation() {
        assert_eq!(
            expectation(Some(&Type::optional(Type::Int))),
            Some(&Type::Int)
        );
        assert_eq!(expectation(Some(&Type::Int)), Some(&Type::Int));
        assert_eq!(expectation(None), None);
    }
}

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
            Some(expected) => {
                self.rule_or(span, rules::expected(actual, expected), expected.clone())
            }
            None => actual,
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
        self.rule(span, rules::member(&object_ty, name.text(), optional))
    }

    fn index(&mut self, index: &IndexExpr) -> Type {
        let span = index.span();
        let object_ty = self.expr(&required(index.object()));
        let index_ty = self.expr(&required(index.index()));
        self.emit(Op::Index, span);
        self.rule(span, rules::index(&object_ty, &index_ty))
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
                self.rule(span, rules::operator(op, token.text(), &lhs_ty, &rhs_ty))
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
            None => self.rule(span, rules::branches(&then_ty, &else_ty)),
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
            Some(so_far) => {
                self.rule_or(item.span(), rules::element(&so_far, &ty, what), so_far)
            }
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
                    || self.require(span, rules::builtin(builtin, arity));
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
                self.rule(span, (builtin.ty)(&types))
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
        let (params, ret) = match rules::callee(callee, name, positional.len()) {
            Ok(granted) => granted,
            Err(message) => {
                self.error(span, message);
                self.discard(None, positional, &[]);
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
        let given: Vec<&Expr> = receiver.iter().chain(positional).collect();
        let names: Vec<SyntaxToken> =
            named.iter().map(|arg| required(arg.name())).collect();
        let name_texts: Vec<&str> = names.iter().map(SyntaxToken::text).collect();
        let order =
            rules::arguments(fn_name, &export.signature.params, given.len(), &name_texts);
        let order = match order {
            Ok(order) => order,
            Err((blame, message)) => {
                let at = match blame {
                    Some(Arg::Positional(i)) => given[i].span(),
                    Some(Arg::Named(n)) => names[n].span(),
                    None => span,
                };
                self.error(at, message);
                self.discard(receiver.as_ref(), positional, named);
                return Type::Any;
            }
        };
        for (param, arg) in export.signature.params.iter().zip(order) {
            match arg {
                Arg::Positional(i) => self.check(given[i], &param.ty),
                Arg::Named(n) => self.check(&required(named[n].value()), &param.ty),
            };
        }
        let argc = u16::try_from(export.signature.params.len())
            .expect("fewer than 65536 parameters");
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

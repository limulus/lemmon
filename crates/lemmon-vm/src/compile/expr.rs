//! Lowering of expressions: the heads of tags.

use std::rc::Rc;

use lemmon_syntax::ast::{
    ArgList, BinaryExpr, BinaryOp, CallExpr, Expr, Lambda, Literal, LiteralKind, MapLit,
    NamedArg, Spanned, UnaryOp,
};
use lemmon_syntax::{Span, SyntaxKind, SyntaxToken};

use super::{Compiler, FnCtx, Resolved, TokenSpan, required};
use crate::Value;
use crate::builtins::BUILTINS;
use crate::bytecode::Op;

impl Compiler<'_> {
    /// Lowers `expr`, leaving its value on the stack.
    pub(super) fn expr(&mut self, expr: &Expr) {
        let span = expr.span();
        match expr {
            Expr::Literal(literal) => self.literal(literal),
            Expr::Name(name) => {
                let token = required(name.name());
                self.name(&token);
            }
            Expr::Paren(paren) => self.expr(&required(paren.inner())),
            Expr::Member(member) => {
                self.expr(&required(member.object()));
                let name = self.intern(required(member.name()).text());
                let op = if member.is_optional() {
                    Op::OptField { name }
                } else {
                    Op::GetField { name }
                };
                self.emit(op, span);
            }
            Expr::Index(index) => {
                self.expr(&required(index.object()));
                self.expr(&required(index.index()));
                self.emit(Op::Index, span);
            }
            Expr::Unary(unary) => {
                self.expr(&required(unary.operand()));
                let op = match required(unary.op()).0 {
                    UnaryOp::Neg => Op::Neg,
                    UnaryOp::Not => Op::Not,
                };
                self.emit(op, span);
            }
            Expr::Binary(binary) => self.binary(binary),
            Expr::Ternary(ternary) => {
                self.expr(&required(ternary.condition()));
                let otherwise = self.emit_jump(Op::JumpIfFalse { to: 0 }, span);
                self.expr(&required(ternary.then()));
                let end = self.emit_jump(Op::Jump { to: 0 }, span);
                self.patch(otherwise);
                self.expr(&required(ternary.otherwise()));
                self.patch(end);
            }
            Expr::List(list) => {
                let mut len = 0;
                for element in list.elements() {
                    self.expr(&element);
                    len += 1;
                }
                self.emit(Op::MakeList { len }, span);
            }
            Expr::Map(map) => self.map(map),
            Expr::Call(call) => self.call(call),
            Expr::Lambda(lambda) => self.lambda(lambda),
            Expr::TypeTest(_) => {
                self.error(span, "`is` needs the type checker, which is not here yet");
            }
        }
    }

    fn literal(&mut self, literal: &Literal) {
        let token = required(literal.token());
        let span = token.span();
        let value = match required(literal.kind()) {
            LiteralKind::String => {
                let index = self.intern(&decode_string(token.text()));
                self.emit(Op::Const { index }, span);
                return;
            }
            LiteralKind::Int => match token.text().parse::<i64>() {
                Ok(i) => Value::Int(i),
                Err(_) => {
                    self.error(span, "integer literal is out of range");
                    return;
                }
            },
            LiteralKind::Float => Value::Float(
                token
                    .text()
                    .parse()
                    .expect("the scanner only lexes well-formed floats"),
            ),
            LiteralKind::True => Value::Bool(true),
            LiteralKind::False => Value::Bool(false),
            LiteralKind::Null => Value::Null,
        };
        let index = self.constant(value);
        self.emit(Op::Const { index }, span);
    }

    fn name(&mut self, token: &SyntaxToken) {
        let span = token.span();
        match self.resolve(token.text()) {
            Some(Resolved::Local(slot)) => self.emit(Op::Local { slot }, span),
            Some(Resolved::Function(sig)) => {
                let func = self.func_operand(sig, span);
                self.emit(Op::Global { func }, span);
            }
            Some(Resolved::Builtin(_)) => self.error(
                span,
                format!(
                    "`{}` is a builtin and cannot be used as a value",
                    token.text()
                ),
            ),
            Some(Resolved::Ambient) => {
                let name = self.intern(token.text());
                self.emit(Op::Ambient { name }, span);
            }
            None => self.error(span, format!("undefined name `{}`", token.text())),
        }
    }

    fn binary(&mut self, binary: &BinaryExpr) {
        let span = binary.span();
        let (op, _) = required(binary.op());
        let lhs = required(binary.lhs());
        let rhs = required(binary.rhs());
        self.expr(&lhs);
        let short = match op {
            BinaryOp::And => Some(Op::JumpIfFalseKeep { to: 0 }),
            BinaryOp::Or => Some(Op::JumpIfTrueKeep { to: 0 }),
            BinaryOp::Coalesce => Some(Op::JumpUnlessNull { to: 0 }),
            _ => None,
        };
        if let Some(short) = short {
            let end = self.emit_jump(short, span);
            self.expr(&rhs);
            self.patch(end);
            return;
        }
        self.expr(&rhs);
        let op = match op {
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
        };
        self.emit(op, span);
    }

    fn map(&mut self, map: &MapLit) {
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
            self.expr(&required(entry.value()));
            len += 1;
        }
        self.emit(Op::MakeMap { len }, map.span());
    }

    fn lambda(&mut self, lambda: &Lambda) {
        let span = lambda.span();
        let params: Vec<SyntaxToken> = lambda
            .params()
            .map(|p| p.names().collect())
            .unwrap_or_default();
        self.ctxs.push(FnCtx::new());
        for param in &params {
            self.declare(param);
        }
        self.expr(&required(lambda.body()));
        self.emit(Op::Return, span);
        let ctx = self.ctxs.pop().expect("the lambda's context");
        let func = self.reserve("<lambda>");
        let captures = ctx.captures.clone();
        self.finish(
            func,
            ctx,
            params.iter().map(|p| Rc::from(p.text())).collect(),
        );
        for &(parent_slot, _) in &captures {
            self.emit(Op::Local { slot: parent_slot }, span);
        }
        self.emit(
            Op::MakeClosure {
                func,
                captures: u16::try_from(captures.len()).expect("fewer than 65536 captures"),
            },
            span,
        );
    }

    fn call(&mut self, call: &CallExpr) {
        let span = call.span();
        let callee = required(call.callee());
        let args = call.args();
        let positional: Vec<Expr> = args.iter().flat_map(ArgList::positional).collect();
        let named: Vec<NamedArg> = args.iter().flat_map(ArgList::named).collect();
        let argc = || u16::try_from(positional.len()).expect("fewer than 65536 arguments");

        // `x.f(y)` is `f(x, y)` when `f` names a function (UFCS); otherwise it is a
        // field holding a function value. Without types, "names a function" means
        // "resolves to one here" — a local `f` shadows it, as a local shadows anything.
        let ufcs = match &callee {
            Expr::Member(member) if !member.is_optional() => {
                let name = required(member.name());
                self.peek_callable(name.text())
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
                        return;
                    }
                }
            }
            _ => (None, None, None),
        };

        match resolved {
            Some(Resolved::Function(sig)) => {
                self.call_function(sig, receiver, &positional, &named, span);
            }
            Some(Resolved::Builtin(id)) => {
                let name = name.expect("a builtin has a name");
                let builtin = &BUILTINS[id as usize];
                let arity = positional.len() + usize::from(receiver.is_some());
                if self.reject_named(&named, &name) {
                    return;
                }
                if arity != builtin.arity {
                    self.error(
                        span,
                        format!(
                            "`{}` takes {} argument{}, got {arity}",
                            builtin.name,
                            builtin.arity,
                            if builtin.arity == 1 { "" } else { "s" }
                        ),
                    );
                    return;
                }
                if let Some(receiver) = &receiver {
                    self.expr(receiver);
                }
                for arg in &positional {
                    self.expr(arg);
                }
                let argc = u16::try_from(arity).expect("fewer than 65536 arguments");
                self.emit(Op::CallBuiltin { id, argc }, span);
            }
            Some(Resolved::Local(_) | Resolved::Ambient) => {
                let name = name.expect("a resolved name");
                if self.reject_named(&named, &name) {
                    return;
                }
                self.name(&name);
                for arg in &positional {
                    self.expr(arg);
                }
                self.emit(Op::CallValue { argc: argc() }, span);
            }
            None => {
                if let Some(first) = named.first() {
                    self.error(
                        first.span(),
                        "named arguments need a callee that is a `lem-fn` by name",
                    );
                    return;
                }
                self.expr(&callee);
                for arg in &positional {
                    self.expr(arg);
                }
                self.emit(Op::CallValue { argc: argc() }, span);
            }
        }
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

    /// Lowers a call to a project function, reordering named arguments into parameter
    /// order at compile time so the VM only ever sees positional calls.
    fn call_function(
        &mut self,
        sig: usize,
        receiver: Option<Expr>,
        positional: &[Expr],
        named: &[NamedArg],
        span: Span,
    ) {
        let (fn_name, params) = {
            let entry = &self.table.entries[sig];
            (entry.export.name.clone(), entry.export.params.clone())
        };
        let mut slots: Vec<Option<Expr>> = vec![None; params.len()];
        let given = receiver.into_iter().chain(positional.iter().cloned());
        for (i, arg) in given.enumerate() {
            match slots.get_mut(i) {
                Some(slot) => *slot = Some(arg),
                None => {
                    self.error(
                        arg.span(),
                        format!(
                            "`{fn_name}` takes {} argument{}, got {}",
                            params.len(),
                            if params.len() == 1 { "" } else { "s" },
                            i + 1 + named.len()
                        ),
                    );
                    return;
                }
            }
        }
        for arg in named {
            let name = required(arg.name());
            let Some(i) = params.iter().position(|p| &**p == name.text()) else {
                self.error(
                    name.span(),
                    format!("`{fn_name}` has no parameter `{}`", name.text()),
                );
                return;
            };
            if slots[i].is_some() {
                self.error(name.span(), format!("`{}` is already given", name.text()));
                return;
            }
            slots[i] = Some(required(arg.value()));
        }
        let mut missing = params.iter().zip(&slots).filter(|(_, s)| s.is_none());
        if let Some((param, _)) = missing.next() {
            self.error(
                span,
                format!("missing argument `{param}` in call to `{fn_name}`"),
            );
            return;
        }
        for slot in slots.into_iter().flatten() {
            self.expr(&slot);
        }
        let argc = u16::try_from(params.len()).expect("fewer than 65536 parameters");
        let func = self.func_operand(sig, span);
        self.emit(Op::Call { func, argc }, span);
    }
}

/// The text of a string literal token: the quotes dropped and every `\c` replaced by
/// `c`, the scanner's rule. Interpolation is left as written — its delimiter is a
/// deferred question (docs/decisions.md).
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

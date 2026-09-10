//! Lowering of calls: how a written argument list is bound to what it calls.
//!
//! One call form (invariant 6, D22), four callees behind it — a project `lem-fn`, a
//! builtin method reached through its receiver's type (D34), an ambient builtin function,
//! and a value that happens to be a function. Each arrives with its parameters in a
//! different place, and each ends in the same shape: the arguments lowered in parameter
//! order under the types those parameters declare, then one call instruction.
//!
//! What can be decided from types alone — a callee's arity, whether a named argument
//! matches a parameter, what a lambda argument's parameters are typed from — lives in
//! [`rules`](super::rules). What is left here is what needs the walk: emitting each
//! argument once, in the order the callee wants rather than the order it was written,
//! and reporting a refusal at the argument that caused it.

use lemmon_syntax::ast::{ArgList, CallExpr, Expr, MemberExpr, NamedArg, Spanned};
use lemmon_syntax::{Span, SyntaxToken};

use crate::Value;
use crate::builtins::{BUILTINS, Expected};
use crate::bytecode::Op;
use crate::types::Type;

use super::expr::{lambda_params, unparen};
use super::names::Resolved;
use super::rules::{self, Arg};
use super::{Compiler, TokenSpan, required};

impl Compiler<'_> {
    pub(super) fn call(&mut self, call: &CallExpr) -> Type {
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
            Some(
                Resolved::Local { .. } | Resolved::Ambient(_) | Resolved::Field { .. },
            ) => {
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
        let object_ty = self.object(&object);
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
                self.note_member_read(&object, name.text());
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
        // Arguments lower left to right, each under what the builtin's rule expects of
        // it given the types so far (D34) — how `xs.map(|a| …)` types `a` from `xs`.
        let mut types = Vec::with_capacity(arity);
        types.extend(receiver);
        for arg in positional {
            let ty = match (builtin.expect)(&types) {
                None => self.expr(arg),
                Some(Expected::Type(want)) => self.check(arg, &want),
                Some(Expected::Function(params)) => {
                    self.function_argument(arg, &builtin.written(), params)
                }
            };
            types.push(ty);
        }
        let argc = u16::try_from(arity).expect("fewer than 65536 arguments");
        self.emit(Op::CallBuiltin { id, argc }, span);
        self.rule(span, (BUILTINS[id as usize].ty)(&types))
    }

    /// Lowers an argument that `written` expects a function of `params` from, its return
    /// the argument's own (D34): a lambda takes them as its parameter types and its body
    /// is inferred; anything else is lowered and held to a function that accepts them.
    fn function_argument(&mut self, arg: &Expr, written: &str, params: Vec<Type>) -> Type {
        match unparen(arg.clone()) {
            Expr::Lambda(lambda) => {
                let shape = rules::lambda_of(written, params, lambda_params(&lambda).len());
                let mismatched = shape.is_err();
                let want = self.rule_or(lambda.span(), shape.map(Some), None);
                self.lambda_under(&lambda, want, mismatched)
            }
            other => {
                let actual = self.expr(&other);
                let verdict = rules::function_of(written, &params, &actual, self.decls());
                self.rule(arg.span(), verdict)
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
        // A root passed whole is what lets the link step count the callee's reads off
        // that parameter as the caller's own (∆HF0); asked before the argument is
        // lowered, since asking is not reading.
        let collecting = self.uses.is_some();
        let mut handed = Vec::new();
        for (param, arg) in export.signature.params.iter().zip(order) {
            let param_ty = self.known(param.ty.clone());
            let expr = match arg {
                Arg::Positional(i) => positional[i].clone(),
                Arg::Named(n) => required(named[n].value()),
            };
            if collecting {
                handed.push(self.root_of(&expr));
            }
            self.check(&expr, &param_ty);
        }
        if collecting {
            self.note_call(fn_name, handed);
        }
        let argc = u16::try_from(export.signature.params.len())
            .expect("fewer than 65536 parameters");
        let func = self.func_operand(export, span);
        self.emit(Op::Call { func, argc }, span);
        self.known(export.signature.ret.clone())
    }
}

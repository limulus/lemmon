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

use lemmon_syntax::ast::{
    ArgList, CallExpr, Expr, LiteralKind, MemberExpr, NamedArg, Spanned,
};
use lemmon_syntax::{Span, SyntaxKind, SyntaxToken};

use crate::Value;
use crate::builtins::{BUILTINS, Expected, Reach, ReceiverKind};
use crate::bytecode::{MarkKey, MarkPayload, Op};
use crate::datetime::Datetime;
use crate::decls::DeclKind;
use crate::host::{Claim, ClaimAnswer};
use crate::types::Type;

use super::expr::{decode_string, lambda_params, unparen};
use super::names::Resolved;
use super::rules::{self, Arg};
use super::{Compiler, TokenSpan, required};

/// `expr` as the token of the string literal it is, parentheses aside — what both the
/// entries that read an argument at compile time need, and nothing else is.
fn string_literal(expr: Expr) -> Option<SyntaxToken> {
    let Expr::Literal(literal) = unparen(expr) else {
        return None;
    };
    (literal.kind() == Some(LiteralKind::String)).then(|| required(literal.token()))
}

/// The string literal that the map literal `expr` gives its `field` entry, and where it
/// is written — a mark's key, where the call wrote one plainly (D42).
///
/// `None` where the payload is not a literal record, gives that field no entry, or gives
/// it something a render computes: the key is then read from the value at render, and
/// the producers' keys are the handler's to check.
fn literal_field(expr: &Expr, field: &str) -> Option<(String, Span)> {
    let Expr::Map(map) = unparen(expr.clone()) else {
        return None;
    };
    map.entries().find_map(|entry| {
        let key = entry.key()?;
        let written = if key.kind() == SyntaxKind::StringLit {
            decode_string(key.text())
        } else {
            key.text().to_owned()
        };
        if written != field {
            return None;
        }
        let literal = string_literal(entry.value()?)?;
        Some((decode_string(literal.text()), literal.span()))
    })
}

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
                // `mark()` alone takes a named argument, and reads both of them here.
                if (BUILTINS[id as usize].reach, BUILTINS[id as usize].name)
                    == (Reach::Ambient, "mark")
                {
                    return self.call_mark(&positional, &named, span);
                }
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
        // `asset()` is not a call at run time at all — the manifest is a compile-time
        // fact, so the whole thing folds to the URL it resolved to (D10). Intercepted
        // before the loop below, because there is no argument to lower.
        if (builtin.reach, builtin.name) == (Reach::Ambient, "asset") {
            return self.call_asset(positional);
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
        // Two entries read a literal argument a second time, here rather than only in
        // the VM. The reach is matched beside the name so that a later `.format()` on
        // another receiver cannot quietly inherit this.
        match (builtin.reach, builtin.name) {
            (Reach::Ambient, "datetime") => {
                self.check_literal(positional, |text| Datetime::parse(text).map(drop));
            }
            (Reach::Method(ReceiverKind::Datetime), "format") => {
                self.check_literal(positional, Datetime::check_pattern);
            }
            _ => {}
        }
        let argc = u16::try_from(arity).expect("fewer than 65536 arguments");
        self.emit(Op::CallBuiltin { id, argc }, span);
        self.rule(span, (BUILTINS[id as usize].rule)(&types))
    }

    /// Lowers `asset("…")` to the URL the manifest gives its path, as a constant.
    ///
    /// Only a literal path is accepted. The alternative — resolving at render — would
    /// make a typo a broken link on one page instead of a compile error with a span
    /// (D10), and would make every unit depend on the whole manifest rather than on the
    /// entries it actually named, which is the dependency invariant 11 exists to keep
    /// narrow. So a computed path is refused where it stands rather than deferred.
    ///
    /// A path the manifest has not got is reported by [`asset_ref`](Self::asset_ref) and
    /// a null stands in its place. The file has an error and so yields no unit; what is
    /// emitted past one is unbalanced and never runs, as [`discard`](Self::discard)'s
    /// is, and exists only so the rest of the walk has something to check against.
    fn call_asset(&mut self, positional: &[Expr]) -> Type {
        let argument = positional.first().expect("arity is checked above");
        let url = match string_literal(argument.clone()) {
            Some(token) => self.asset_ref(&decode_string(token.text()), token.span()),
            None => {
                self.error(
                    argument.span(),
                    "`asset()` needs a path written as a literal: the manifest is read \
                     while this file compiles, and a path a render computes cannot be \
                     looked up then"
                        .to_owned(),
                );
                self.discard(positional, &[]);
                None
            }
        };
        let value = url.map_or(Value::Null, Value::Asset);
        let index = self.constant(value);
        self.emit(Op::Const { index }, argument.span());
        Type::Asset
    }

    /// Lowers `mark("name")`, with the payload and default the call gave, to an
    /// `Op::Mark`.
    ///
    /// The name is a literal for [`call_asset`](Self::call_asset)'s reason: the claims
    /// are read while this file compiles, so a name a render computes could be checked
    /// against nothing and recorded as no fact. The default is named — the language's
    /// one named argument to a builtin — which is what left the positional slot free for
    /// the payload (D42). It is always lowered, an empty `html` where the call omits it,
    /// so the op pops one value however the call was written; the payload is lowered
    /// first, so the op finds the default on top and the record beneath it.
    fn call_mark(&mut self, positional: &[Expr], named: &[NamedArg], span: Span) -> Type {
        if self.require(span, rules::mark_arity(positional.len())) {
            self.discard(positional, named);
            return Type::Html;
        }
        let argument = positional.first().expect("arity is checked above");
        let Some(literal) = string_literal(argument.clone()) else {
            self.error(
                argument.span(),
                "`mark()` needs a name written as a literal: the host's claims are read \
                 while this file compiles, and a name a render computes cannot be \
                 checked then"
                    .to_owned(),
            );
            self.discard(positional, named);
            return Type::Html;
        };
        let name = decode_string(literal.text());
        let payload = positional.get(1);
        // The key is read from the payload before anything is lowered, because a claim
        // answers about a name and a key together, and what it answers is what decides
        // how the payload itself is checked.
        let keyed_by = self
            .host
            .claim(&name)
            .and_then(Claim::keyed_by)
            .map(str::to_owned);
        let written_key = payload
            .zip(keyed_by.as_deref())
            .and_then(|(expr, field)| literal_field(expr, field));
        let answer = self.claim_ref(
            &name,
            literal.span(),
            written_key.as_ref().map(|(key, at)| (key.as_str(), *at)),
        );
        let carried = self.mark_payload(&name, payload, &answer, span);
        let mut default = None;
        for argument in named {
            let written = required(argument.name());
            if written.text() == "default" && default.is_none() {
                default = Some(required(argument.value()));
            } else {
                self.error(
                    written.span(),
                    format!(
                        "`mark()` takes one named argument, `default`, and takes it once, \
                         not `{}`",
                        written.text()
                    ),
                );
                self.discard(&[required(argument.value())], &[]);
            }
        }
        match default {
            Some(expr) => {
                self.check(&expr, &Type::Html);
            }
            None => {
                let index = self.constant(Value::html(""));
                self.emit(Op::Const { index }, span);
            }
        }
        let key = match (&written_key, &keyed_by, carried) {
            (Some((key, _)), _, _) => MarkKey::Const {
                index: self.intern(key),
            },
            // The call computed what the claim keys on, so the VM reads the field from
            // the record at render and the handler checks it at build (D42).
            (None, Some(field), MarkPayload::Popped) => MarkKey::Field {
                index: self.intern(field),
            },
            _ => MarkKey::None,
        };
        let name = self.intern(&name);
        self.emit(
            Op::Mark {
                name,
                key,
                payload: carried,
            },
            span,
        );
        Type::Html
    }

    /// Lowers a mark's payload where the call gave one, checked against the record the
    /// host's claim names (D42), and says whether the op pops one.
    ///
    /// A claim that names no record takes no payload and a claim that names one takes it,
    /// so a call that disagrees is refused at the argument rather than left to a handler.
    /// Where nothing was claimed — an unclaimed name, a host with no handlers — there is
    /// nothing to check it against, and the payload only has to be something a handler
    /// could be given.
    fn mark_payload(
        &mut self,
        name: &str,
        payload: Option<&Expr>,
        answer: &ClaimAnswer,
        span: Span,
    ) -> MarkPayload {
        let declared = match answer {
            ClaimAnswer::Claimed { payload, .. } => payload.clone(),
            ClaimAnswer::Unclaimed | ClaimAnswer::Undeclared => None,
        };
        match (payload, declared) {
            (Some(expr), Some(record)) => {
                // The record is the program's to declare, so a claim naming one it has
                // not — or naming an enum, which no literal could satisfy — is a mistake
                // in the host's declaration, said at the call, the only place in this
                // file that consumed it. Refused before the payload is checked against
                // it, or the author would be told about their own literal instead.
                if self.decls().kind(&record) != Some(DeclKind::Record) {
                    let what = match self.decls().kind(&record) {
                        Some(DeclKind::Enum) => "which is an enum, not a record",
                        _ => "which nothing declares",
                    };
                    self.error(
                        span,
                        format!(
                            "the host's claim for `{name}` names a payload `{record}`, \
                             {what}"
                        ),
                    );
                    self.expr(expr);
                } else {
                    self.note_type(&record, expr.span());
                    self.require(span, rules::payload_record(name, &record, self.decls()));
                    self.check(expr, &Type::Named(record));
                }
                MarkPayload::Popped
            }
            (Some(expr), None) if matches!(answer, ClaimAnswer::Claimed { .. }) => {
                self.error(
                    expr.span(),
                    format!("the handler for `{name}` takes no payload"),
                );
                self.expr(expr);
                MarkPayload::Popped
            }
            (Some(expr), None) => {
                let ty = self.expr(expr);
                self.require(expr.span(), rules::mark_payload(&ty, self.decls()));
                MarkPayload::Popped
            }
            (None, Some(record)) => {
                self.error(
                    span,
                    format!("the producer for `{name}` takes a `{record}` payload"),
                );
                MarkPayload::None
            }
            (None, None) => MarkPayload::None,
        }
    }

    /// Reports a first argument written as a literal that `read` refuses, at the
    /// literal rather than at the render that would have hit it.
    ///
    /// `datetime("…")` is what the language has instead of a datetime literal (D21,
    /// revised 2026-08-20), so the constructor is checked where a literal would have
    /// been; `.format("…")` is a strftime pattern, and a typo in one should not wait
    /// for the one page that uses it. Both calls still lower and run like any other —
    /// the VM reads the string again — and this is only the earlier reading.
    ///
    /// What counts as the literal's text is [`decode_string`]'s answer, which is the
    /// whole of it while string interpolation is unimplemented. When interpolation
    /// lands, a literal holding a `{…}` is no longer a fixed string and this must let
    /// it through to run — refusing `datetime("{year}-01-01")` here would refuse a
    /// template that renders.
    fn check_literal(&mut self, positional: &[Expr], read: fn(&str) -> Result<(), String>) {
        let Some(argument) = positional.first() else {
            return;
        };
        let Some(token) = string_literal(argument.clone()) else {
            return;
        };
        if let Err(message) = read(&decode_string(token.text())) {
            self.error(token.span(), message);
        }
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

---
id: ∆495
completed: 2026-09-07 13:37:59 -07:00
---

## Backlog item

> - ∆495 Builtin methods — resolve `xs.filter(…)` against the receiver's type from a
>   receiver-keyed table beside `BUILTINS`, and retire UFCS with it, so `.` asks the receiver
>   and never the enclosing scope (D34, §Expressions/§Checking rewrite included). Covers the
>   `?.` null-jump and a dynamic-call op for a method on `any`; proven by `slugify` and
>   `keys`, with the argument-shaping hook and the methods it serves in ∆987

## Planning

The item did not exist when the conversation started. Eric objected to one construction —
`.someEnumValue.someFunc()`, which [[∆EN6]] had just made legal — and asked whether UFCS
could go. Costing that out found the removal cheap (a net deletion, no bytecode, eight
corpus lines) and the replacement blocked: the deferred list called a pipe operator "likely
redundant given UFCS", but `|>` cannot be written in a Lemmon head at all, because
invariant 3 ends a tag at the first `>`. `->` and `=>` are gone the same way. So the
deferred note was stale twice — not redundant, and unspellable.

That left removal with no ergonomic answer for the chained collection stdlib, which was
UFCS's only real argument. Eric asked the question that dissolved it: could builtin types
carry their own methods? They could, and better than UFCS did — `Builtin::ty` was already
a rule rather than a signature, [[∆Q6L]]'s own comment explaining that list functions are
generic over their element type, so the generics problem that would normally sink this was
designed around before it was asked. D34 followed, and the item after it.

A Fable review of the implementation plan found the thing that reshaped it. `map` and
`filter` cannot be table entries at all: `Builtin::call` is a pure `fn(&[Value])` with no
VM handle, and `Vm::call` only pushes a frame and returns to the `run` loop. A higher-order
method needs a re-entrant invoke that nothing has. Worse, that coupled to the
argument-shaping rule — the rule has no consumer without a higher-order method, and this
repo does not land inert facts. So ∆495 shrank to dispatch and [[∆987]] took the invoke,
the rule, and `map`/`filter` as one indivisible unit.

## Refinement

The `?.` mechanism turned out cheaper than D34 predicted. The record said the call "must be
jumped around when the receiver is null, which `CallBuiltin` cannot do", and I had budgeted
a `JumpIfNullKeep` op for it. `JumpUnlessNull` already does the job read backwards: jump
*to* the call when the receiver is non-null, and fall through to a null constant otherwise.
One new op in the end (`CallDynamic`, for `any`), not two.

The proof set grew from two methods to three, against the item's own text. `slugify` and
`keys` are argument-free with fixed result types, so nothing would have exercised a rule
whose *result* depends on the receiver's payload — which is the entire reason `Builtin::ty`
is a rule. `first` on `[T]` returning `T?` closes that. Without it the generic axis would
have landed inert, which is the same rule that split [[∆987]] off in the first place.

`syntax/let-fn/expression-bodied-fn.lem` is byte-identical across this change:
`<lem-fn slug(s: string): string = s.slugify() />` was UFCS and is now a method. That file
not moving is the "no grammar change" claim demonstrated rather than asserted.

A high-effort review then found five things, two of them mine and both the same shape — a
behaviour split between the typed path and the `any` path.

The first was a real regression. Grouping `Type::Null` with `Type::Any` in
`callable_member` meant a statically-known-null receiver compiled clean: `x.f` errored at
compile time, `x.f()` did not, and the failure moved to render. For a static site generator
that is `lem check` passing and the build breaking later. Only `(Type::Null, true)` needs
the dynamic path, since the `?.` jump steps over it anyway.

The second cannot be fixed, only chosen. `call_dynamic` tries the field before the method,
but the typed path never reaches `field` for a builtin type, so a map whose key spells a
method's name resolves one way typed and the other way through `any`. Neither order is
right while a record is still a `Value::Map`: method-first breaks every record field
reached through `any`, which is every ambient value until [[∆Iau]]. Field-first is the
trade; what was wrong was the comment claiming the two paths agree. It now says what it
costs and names [[∆PCK]] as the fix, so the placeholder gate fails the build when the
runtime tag arrives. The divergence is also pinned in the golden output —
`render/calls/methods-on-any` puts a callable under the key `keys`, and the two paths print
different things in the blessed file.

The other three were diagnostics: `slugify` kept only ASCII alphanumerics, so "Café Life"
and "Caf Life" produced one slug between them (Unicode `is_alphanumeric` now, with ASCII
folding left to [[∆Q6L]]); a map call lost the "index a map as `[\"x\"]`" hint its field
read still gave; and a named argument on a method sent the author looking for a `lem-fn`
that could not have helped.

## Retrospective

**A decision record can be wrong about its own implementation, and the plan is where that
shows.** D34's "no bytecode change" consequence was written from reading the call path and
was false in three directions — `?.`, `any`, and higher-order methods — none of which I saw
until the plan forced me to name the mechanism for each. The record was written first on
purpose and I would do that again; what it earns is a *design* that holds, not a cost
estimate. Correcting it after planning cost a paragraph. Discovering the re-entrant invoke
mid-implementation, with the argument-shaping rule already half-landed, would have cost the
shape of the item.

**Ask what a rule is for before deciding it is satisfied.** The proof set in the item text
was mine, written two turns earlier, and it did not satisfy the very rule that had just
split ∆987 off. "Prove the mechanism" and "prove the mechanism's *reason*" are different
bars, and `Builtin::ty` being a rule rather than a signature is the reason. I noticed while
writing the code, but only because the coverage question made me ask what would exercise
`first_type`'s match arm.

**Two paths for one construct will diverge, and the comment will be the last thing to
know.** Both review findings that were mine came from the same place: I wrote
`call_dynamic` to mirror the typed path, wrote a comment saying it did, and did not check.
The typed path had shipped an hour earlier in the same session. When a change introduces a
second implementation of one rule, the test that matters is the one that runs the same
input down both — which is what `methods-on-any` now is, and what the null-receiver case
should have been from the start.

**A stale deferred question is worse than an open one.** "Pipe operator `|>`: likely
redundant given UFCS" had been sitting in `docs/decisions/README.md` since before invariant
3 was written down, quietly asserting that an escape hatch existed. It did not, and nothing
would have discovered that until someone tried to spell it. The deferred list is prose no
gate reads, same as `docs/language.md`; entries there age against the invariants without
anyone noticing.

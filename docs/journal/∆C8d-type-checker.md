---
id: ∆C8d
completed: 2026-09-06 13:23:26 -07:00
---

## Backlog item

> - ∆C8d Type checker — types (`any` the spelled unknown; scalars, `[T]`, `{string: V}`,
>   `T?`, function types), bidirectional inference woven into the one lowering walk,
>   assignability, the type-directed print and condition rules, optional narrowing in
>   `lem-if`, unused warnings, typed signatures as compile-time facts (D25). Ambient values
>   stay `any` until ∆Iau, `is`/`lem-match` until ∆PCK, records until ∆cea; needs: ∆1vS, ∆VNk

## Planning

This item had already been split twice ([[∆up2]], [[∆VNk]]) and planning split it a third
time: records and enums went to a new item, ∆cea, because nothing produces a record value
before ∆iCW and ∆PCK, so declarations would have landed with only `any` flowing into them.
∆PCK and ∆Iau re-pointed to ∆cea. Eric took that and three more recommendations.

The design decision was where the checker lives. The architecture set lists it as a stage
between the parser and the compiler, but the compiler already walked every item and head
in evaluation order with one resolver ([[∆e4d]]) and one export index ([[∆e5i]]), and a
separate pass would have re-walked the tree with a second scoping and produced a per-node
type table nothing read until ∆PCK. The choice was to check while lowering (D31):
`expr(&Expr, expected) -> Type` emits and types in one walk, bindings carry their types in
`Names`, narrowing is a scoped entry. The other two readings were strict maps (`.` reads a
record, `[]` indexes a map; a literal's elements join, first element sets the type, a
mismatch is an error and never `any`) and unused warnings for `lem-let` and parameters
but not loop bindings.

The plan named its provisional readings for the deferred questions — no `int → float`,
`[]` is `[any]`, an unannotated lambda's parameters are `any`, a map-index miss is a
render error like a list's — and one consequence of invariant 11 that the docs had not
stated: an expression-bodied `lem-fn` must declare its return type, because a signature
read from the body would make a caller's unit depend on that body.

## Refinement

Two small deviations from the plan. `Type::Named` was not added — every non-builtin type
name is an error at the annotation until ∆cea has producers for records and enums — and
narrowing covers a plain local name only, since member paths have no field types to narrow
until ∆cea. A third was a correction: `==` between an `int` and a `float` compares
numerically, as the VM does.

The corpus rewrite was most of the visible cost. Every case that used a map literal as a
stand-in record (`article.title` on `{title: "T"}`, `photo: map` parameters) had to say
what it was — `: any` — and the runtime error cases were rerouted through `page` so the
VM's checks still fire on the `any` path. The blessed diff caught four mistakes of mine
before the review did: lambda parameters cannot carry annotations, a `printable` test that
printed a `bool`, `n ?? 0` on a plain `int`, and `p.count` on a record with no such field.

`/code-review medium` confirmed eight findings; all were fixed here.

- **Error paths returned before lowering their operands.** In a fused walk, control flow
  is shared: when lowering bailed on an arity error, a non-callable callee, a mismatched
  lambda, `is`, or the `lem-match` placeholder, the names inside the skipped operands were
  never marked read and the unused rule warned about bindings that were plainly used. A
  `discard` step now lowers the operands for their diagnostics. This is the one cost of
  D31 the plan did not name, and every new error path has to remember it.
- **`?.` on a non-optional reported twice** on the same span. One error now.
- **`T? ?? any` typed as `T`**, losing the unknown and letting a null past the print rule.
  The result is now the join of the two sides.
- **`any` was special-cased per operator**: `any + bool` errored while `any lt bool`
  passed. Arithmetic on `any` is `any`, as the doc said.
- **Parentheses blocked narrowing, and a guard inside a narrowed arm was a hard error.**
  Parens are peeled, and a binding declared optional is accepted as a condition and on
  the left of `??` inside its own guard.
- **Indexing an optional** said "cannot index" where field access said "may be null".
- The `lem-match` placeholder still said it needed the type checker; four doc sentences
  (the pipeline in CLAUDE.md and architecture.md, "the checker runs over units", D30's
  `layout` pointer) had been falsified by the change.
- Cleanups: list and map literal typing were the same twenty lines twice, four arity
  messages, three index lookups and a parameter-list clone per function reference, and
  three exports with no reader outside the crate.

Two findings were noted rather than fixed. The builtin typing hook takes argument types
and returns a result, which cannot flow an expectation into a lambda argument; ∆Q6L's
generic `map`/`filter` will need a bidirectional form. And the canonical corpus case and
the spike now opt out of checking with `: any`, so the checker proves itself on the real
program only when ∆cea lands. Eric asked afterwards whether a bigger refactor would have
been better; the answer was no to the architecture and yes to the organization, and ∆2c6
(pure rules beside the walk) went into the Refactors epic.

## Retrospective

**A fused walk shares its failure modes.** Six of the review's eight findings were the
same bug: an early `return` on an error path in code that now does two jobs. The plan
argued for one walk on the strength of one scoping and got it, but did not ask what one
control flow costs. When a change makes one pass do two things, the question to write
down is "what happens to the second job when the first one bails".

**The blessed diff is a review, and I read it as one.** Four of my own corpus mistakes
were found by reading what the bless proposed against what the source asked for — the
`tests/corpus/README.md` rule, applied to cases written minutes earlier. The corpus is
cheapest as a check on the author's own work, before anyone else reads it.

**The split's cost is on the far side.** Records were deferred because nothing could
produce one, and that was right, but it means the canonical case and the spike say `: any`
everywhere and the checker has not yet checked a real page. A split that leaves the
motivating example untyped should say so in the item it defers to, so the next person
knows the payoff is theirs to collect.

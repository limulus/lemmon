---
id: ∆2c6
completed: 2026-09-06 20:07:52 -07:00
---

## Backlog item

> - ∆2c6 Pure typing rules beside the walk — move the checker's rules that need no
>   emitting (`operator`, `field_type`, the index, condition, print, and arity rules) out of
>   `crates/lemmon-vm/src/compile/expr.rs` into a `rules.rs` of pure functions returning a
>   type or a message, leaving the walk to emit, ask, and report. The rules then test
>   without compiling a template, and ∆cea's field and variant rules have one place to go

## Planning

The item came out of [[∆C8d]]'s closing question — whether a bigger refactor would have
been better — and its answer: no to the architecture, yes to the organization. Planning
took the item as written and settled three things. The rules' shape is a plain function
returning `Result<Type, String>` (or `Result<(), String>`), the message being the
diagnostic without its span; the walk supplies the span. A rule takes the types and names
it judges and nothing else — never `&Compiler` — so ∆cea's field rule will take the
declaration table as an argument. And the walk gets one helper that turns a refusal into
a diagnostic plus a fallback type, so "a failed expression types as `any`" is one line
rather than a convention every arm remembers, which the ∆C8d retrospective had named as
the fused walk's failure mode.

What moved: operator, unary, logical, member (with field access folded in, since every
arm of it was pure), index, condition, print, iteration, arity, lambda shape, callee,
argument reordering, the literal helpers, and the branch join. What stayed: `??`, whose
right side is lowered under an expectation its left side decides. The plan noted one
behaviour change up front — a refused comparison, `!`, or `in` would type `any` rather
than `bool`, as the docs already said — and set an empty corpus diff as the acceptance
criterion.

## Refinement

The move itself was uneventful: the Rust gate and an empty corpus diff on the first run.
The argument-reordering rule returned indexes into the caller's argument lists (`Arg`)
rather than the expressions themselves, to keep the rule AST-free; that is the one place
where the walk and a rule share an invariant across files. The surplus-argument count was
corrected on the way — HEAD reported the first extra argument's ordinal, not what the
author wrote — and the reordering now stops at the first refusal, as the builtin path
already did.

`/code-review medium` on Opus confirmed seven findings and rated one plausible; all eight
were fixed here.

- **The two behaviour changes had no corpus case.** The type-error corpus wraps every
  refused comparison in `?:`, which accepts `bool` and `any` alike, and the surplus-
  argument case was exactly one argument over, where the old and new counts agree. A
  `type-refused` case and a five-argument call now pin both, and `docs/language.md`
  §Checking states the recovery model D31 already had.
- **The module docs were wrong twice.** "Two rules stay in the walk" left out the
  expectation compare and the builtin arity check, both pure; "types as `any` from then
  on" contradicted `check`'s own doc and the literal-element fallback. Both rules moved
  and both sentences now say what the code does.
- **The helper was fixed to `Type`,** so three sites re-spelled report-and-fall-back
  inline. A generic `rule_or` covers them; `condition` and `call_value` stay explicit
  for their guarded arm and their `discard`.
- Cleanups: two clones per argument in `call_function`, a test asserting two subjects,
  two over-visible helpers, and the unstated index contract on `arguments`.

Three candidates were refuted and left alone: the `unreachable!` arm in `operator`
(present at HEAD and house style), the `opt`/`func` test shims (`types.rs` has the same),
and the arity message duplicated with the VM's (pre-existing, and the two modules cannot
see each other without a new home).

## Retrospective

**A "pure refactor" still moves behaviour, and the corpus is the place to say so.** The
plan named the `bool`-to-`any` change and checked that the blessed diff was empty, and
took the empty diff as proof nothing observable changed. It proved the opposite: the
corpus had no case that could tell the two apart. When a change is expected to alter
diagnostics and the bless shows nothing, the first question is whether any case can see
it, and writing that case is part of the change — not a review finding.

**Write the doc sentence last.** Both module-doc errors were true when written and false
by the time the diff was done, because the code kept moving after the prose was set. The
sweep for falsified sentences the working agreements ask for applies to the diff's own
new prose too.

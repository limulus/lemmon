---
id: ∆VNk
completed: 2026-09-06 08:29:38 -07:00
---

## Backlog item

> - ∆VNk Diagnostics with severity and secondary spans — `severity` and `labels` on the
>   diagnostic types of lemmon-syntax and lemmon-vm, through the boundary and
>   `@limulus/lemmon-core`, in the corpus trailer, with `compile` returning warnings beside
>   a module. First consumers: "already defined … at" spans become labels, and shadowing a
>   stdlib name warns (D23). The checker's unused warnings need the channel to exist first

## Planning

This item did not exist when planning began: `∆C8d` did, and planning it split it four ways
(`∆VNk`, `∆C8d`, `∆PCK`, `∆Iau`), with Eric taking every recommendation. Severity and labels
came out first because the checker's unused warnings need a channel to land in, and a
diagnostics change touches every crate and the TS wrapper — the kind of cross-cutting diff
that is easy to review alone and hard to review inside a checker.

Three shapes were settled in the plan. `compile`, `compile_unit`, and `link` return
`Compiled<T>` — output plus every diagnostic — because a `Result` has nowhere to put the
warnings a successful compile raises. The syntax crate's `Label` is file-local and the VM's
is path-qualified, since a parser label never leaves its file and a duplicate's first
definition may be in another. And the boundary's `severity` and `labels` are always present
rather than optional: simpler for the TS side, and additive for a caller that ignores them.

The first warning was chosen so the channel had a consumer on day one — D23 already said
shadowing a stdlib name warns, and the code carried a "no warning channel yet" comment
saying so. `[[∆TNT]]`'s lesson applied: a severity nothing emits is a comment, not a fact.

## Refinement

The build was uneventful; the review was not. `/code-review medium` with Opus finders and
verifiers confirmed eight findings, all fixed here.

- **`link` still gated on emptiness.** Every other step asked `is_error`; `link` returned no
  module on any diagnostic, and `Compiled::failed`'s debug assertion would have fired on the
  first link-time warning. The fix removed the second constructor: `Compiled::new` takes an
  `Option<T>` and is the only way to build one.
- **The spike scripts kept the old contract.** `expectClean` in `bench-wasm.ts` and
  `render-the-rebound.ts` threw on any diagnostic, and the bench walks every corpus case
  outside `errors/` — so the new warnings case made `npm run wasm:bench` fail, and nothing
  in `verify` runs it. Both filter on `severity === 'error'` now.
- **The warning blamed the wrong thing.** A `lem-let raw` beside a project `lem-fn raw` was
  told it shadowed the stdlib, when the function had already done that and warned for it.
  The corpus case had blessed the false message. The check now consults the project table
  first.
- **The warning fired on every binding**, parameters and loop variables included, while the
  docs spoke of definitions. Eric chose definitions and `lem-let` only; the check moved out
  of `declare` into a `declare_definition` the two `lem-let` forms use, and the corpus
  covers the parameter, loop, and lambda half that must stay silent.
- **Labels underlined whole definitions.** `ExportedSignature::span` was the entire element;
  a block function's "first defined here" would have covered its body. Labels now point at
  `name_span`, and the field with no other consumer is gone.
- **Warnings are per call.** A warning-only file becomes a resident unit whose warning
  exists only in the return value that produced it. Documented as the contract in the
  wrapper, the boundary, and the architecture doc; retaining warnings on `Unit` waits for a
  design pass alongside the checker.
- The boundary converted to its serde type at four sites and re-derived "is an error" on it;
  it converts once now. And the Shell & Core ledger entry the CLAUDE.md agreement asks for
  had been skipped.

Two things worth recording about the environment. `∆DA2` was built concurrently in the
shared working tree by the `∆1vS` session; coordinating by message, with an explicit list of
files each side held, worked, and its commit carried this item's backlog split. And one
review finder ran `git checkout --` on a file it had probed, discarding uncommitted work; it
restored the file from the saved diff, and a byte comparison confirmed nothing was lost —
but a finder that writes is a finder that can destroy, and the prompt should say "do not
modify files" every time.

## Retrospective

**The rule I wrote and the rule I tested were different rules.** The unit test said "site
definitions may shadow the stdlib" in its comment while the code warned on every binding,
and both the test and the corpus case exercised only the shapes I had in mind — a top-level
function and a `lem-let`. Three reviewers found the parameter case in minutes by asking what
else reaches `declare`. When a check is attached to a shared entry point, the tests owe it
every caller of that entry point, not just the one that motivated it.

**A corpus case can bless a false sentence as easily as a wrong span.** The second warning
in the original case was correctly spanned and wrong in what it said; I read the blessed
diff for plausibility and it looked right. `tests/corpus/README.md`'s rule — judge blessed
output against what the source asked for — applies to message text too: the question for
that line was "what does `raw` resolve to here?", and the output `[Hi]` answered it.

**Half-converting a contract leaves the old half load-bearing.** "Empty diagnostics means
success" was replaced in three places and kept in three others (`link`, the two scripts),
and only one of the three was inside the gate. When a change redefines what a shape means,
grep for every consumer of the old meaning before declaring it done — the scripts carve-out
from coverage is exactly where such a consumer hides.

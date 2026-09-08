---
id: ∆PCK
completed: 2026-09-07 19:13:00 -07:00
---

## Backlog item

> - ∆PCK Narrowing and typed lowering — `is` and `lem-match` narrowing, a `TypeTest` op over
>   record values that carry their type (`Value::Record`), record literals as map literals
>   under an expected record type, enum exhaustiveness on the provisional reading. Split
>   from ∆C8d because it needs runtime type tags, which nothing produces before it; needs: ∆cea

## Planning

The plan settled what a record is at run time and put it in D35. A value carries its
declared name; the linked module carries ancestry; fields stay by name, because reading by
declaration index is an optimisation with no consumer and D31's rule — nothing recorded
until something reads it — applies to it. A map literal under an expected record is the
producer the item needed, filling omitted optionals with null and refusing omitted
required fields whether or not they have a default: a default is evaluated at load, where
`body` and `site` exist, and a declaration's D25 facts do not carry one. Narrowing changed
its key from a binding to a path. `lem-match` took the provisional exhaustiveness reading
`docs/language.md` already stated.

Six decisions went to Eric, each with a recommendation, and he took all six. The two that
shaped the most code: an untagged value under a type test is a render error rather than
`false`, since guessing would hide ambient data that predates [[∆Iau]]; and a map receiver
through `any` takes the method only, so the VM agrees with the checker and the divergence
[[∆495]] pinned is retired.

A Plan agent reviewed the draft before Eric saw it, and earned it again. Four findings
changed the plan rather than the code. The whitespace rules reach a `lem-match` as an
opaque tag, so case bodies would not have dedented with their enclosing `lem-fn` — the
plan gained a `Piece::Match` mirroring `Piece::If`. `link` spells out every op when it
relocates constant operands, so both new ops had to be placed there or a multi-file module
would read another file's pool. The canonical case could not be typed `[Article]` as the
draft assumed, because a string literal is not a `url` and `url()` is [[∆Q6L]]'s: the data
stays `any` and only `article.vod.poster` came back. And `render/calls/methods-on-any`
needed rewriting rather than re-blessing, since its pinned divergence became a runtime
error that would abort the render before the rest of the case ran.

## Refinement

Implementation followed the plan. The review is where the work moved, and one finding
chained into two more.

`/code-review medium` ran eight angles; seven reported, one hit a usage limit. Two
correctness bugs:

- **The relocation of both new ops' constant operands was exercised by nothing.** Deleting
  both arms from `link` left all 297 tests green — every corpus case is single-file, so it
  links at offset zero, and the one two-file relocation test covers `CallDynamic`. The
  comment beside that arm says exactly this hazard, and the plan named it; neither made a
  test exist.
- **A repeated `lem-case` string spelled with a different escape was accepted**, leaving
  the second case dead with no diagnostic. The repeat rule keyed on raw token text while
  map-literal keys are compared decoded, so the same mistake was caught in one construct
  and not the other.

Writing the test for the first exposed the third: `Module::is_a` answered that a tag the
table has no row for is itself, so a record built from the wrong constant still passed a
test against its own bad tag — the first version of the relocation test passed under
sabotage for that reason. The table is now the whole answer, which is also what let the
name → base walk become a chain closed once at link, retiring both the per-step scan and a
cycle guard against a table the compiler cannot produce.

Four of the seven angles independently reached the same line: `type_test_narrowing`
re-ran the rules the walk had just run, on a subject typed by a different route, and
mapped any refusal to "no narrowing" with `.ok()`. A disagreement would have compiled
clean and silently failed to narrow. Conditions now lower through one `lower_condition`
that keeps the verdict the rules granted. In the same pass the walk stopped classifying
enums itself: `rules::tested_type` takes which construct is asking and composes the hint,
so `is` and `lem-case` differ in one place instead of two.

The rest were corpus halves the diff had dropped or never written: the two spellings where
a map's field-first and method-first resolution differ, a non-optional path in both
positions the widened suppression covers, and a link test where a type test is the only
mention of a foreign type. One finding was deferred with its reasoning: a record's
completeness is enforced only by the literal, while `field` and `equals` assume it, and
settling that means designing against producers ([[∆iCW]], [[∆Iau]]) that do not exist —
D35 records both options as the loader's to choose.

## Retrospective

**A hazard named in a comment and in the plan still needs a test to exist.** `link`'s
relocation arm carries a comment saying a missed op reads another file's constants and
that no single-file case can catch it; the plan repeated it; I placed both ops correctly
and wrote no test. The gate that would have noticed is the one CLAUDE.md already
prescribes — feed the check a real violation and watch it fail — and I ran it only because
a reviewer asked. Placing an op in that match should mean writing the two-file case in the
same edit.

**A negative test that passes under sabotage is a finding, not a fluke.** My first
relocation test passed with the relocation removed. The temptation was to make the test
harder; the right move was to ask why a corrupted tag still answered correctly, which
found the `is_a` self-match. When a test fails to fail, the bug is usually not in the test.

**Four angles converging on one line is a signal about the design, not four findings.**
Reuse, simplification, altitude, and efficiency each arrived at `type_test_narrowing` by
different routes. Its docstring already apologised for recomputing — "asked of the rules
again" — which is the same shape [[∆cea]]'s retrospective named: a provisional reading
that needs an apology in a comment is a design question, and the cheapest time to ask it
is while writing the comment.

**The plan review keeps paying, and keeps paying for different things.** [[∆cea]] recorded
that it pays; [[∆EN6]] recorded that it finds a different class of thing than code review.
This one found two structural gaps the code review could not have found cheaply, because
by then the whitespace shape and the corpus expectations would already exist.

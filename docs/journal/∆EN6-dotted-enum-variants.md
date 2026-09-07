---
id: ∆EN6
completed: 2026-09-07 11:43:14 -07:00
---

## Backlog item

> - ∆EN6 Dotted enum variants — a variant always starts with a dot: `.hero` where the enum is
>   expected (arguments, annotated bindings, `lem-case` patterns, the right of `==`/`in`), and
>   `Placement.hero` anywhere, when nothing around it says which enum. A bare `hero` is then
>   only ever a name, so D32's variant-over-local shadowing goes; the expectation paths stay
>   as what `.hero` asks for. `.` opening an expression is new to the scanner; needs: ∆cea

## Planning

Eric named the item rather than taking what `surface` offered, and the first thing planning
did was disprove one of its sentences. "`.` opening an expression is new to the scanner"
is wrong: `Dot` has been a token since the scanner existed, `Float` is `[0-9]+\.[0-9]+` so
a leading dot can never be a number, and the mode driver treats `.` like any other
punctuation. A four-line probe showed `<lem..hero>`, `<lem.place(.hero)>`,
`<lem-let x: P = .hero />` and `<lem-case .hero>` all lexing clean with zero diagnostics.
The work was in the parser, and the scanner was never opened.

Three questions went to Eric before anything was written, because each changes the shape of
the diff. The `lem-enum` head keeps its bare variants — nothing after `=` there could be
anything else, so a dot disambiguates nothing and only makes the declaration noisier.
`lem-case` takes the dotted form only, leaving the bare slot to `Type as name`, whose
binding is optional and which the parser cannot tell from a variant without resolving a
name. And `Name.x` reads the type namespace first, so a declared enum claims that path
whatever is in scope: a local named `Placement` stays readable as a value by its bare name
and only loses its field access. The alternative — the value namespace answering first,
with the enum as fall-through — was the item's own literal wording, and it was rejected for
being the same shape as the rule this item exists to retire: a fixed spelling meaning
different things in different scopes.

A Plan agent on Fable 5.1 reviewed the plan before implementation and earned it. It found
the `lem-case` bare-identifier question left unspecified, two match-corpus cases outside the
rewrite list, a table of which expression positions actually supply an expectation, the
`.hero.foo` diagnostic I had mispredicted, and — the best of them — that the [[∆cea]] entry
in `docs/artifact-changes.md` itself instructs the reconciler to write the bare-variant
rule, so appending a new entry would have left [[∆MlD]] with two contradictory
instructions. It also supplied the both-halves corpus cases I had written only one side of.

The last correction was Eric's. The plan called `<lem..hero>` a legal-but-ugly wart to be
pinned in the render corpus. It is not legal: an access tag lowers with `self.expr(…)` and
no expectation, so a dotted variant there has no enum to resolve against. The scanner probe
had shown it *lexes* clean and I let that stand in for legal. The wart did not exist, and
the case moved to the errors group.

## Refinement

The design's own evidence arrived early: `operand_expectation` — the rule that lowers the
right of `==`/`!=`/`in` expecting the enum — needed no change at all. Under the bare
reading it was a special case existing to make bare variants work beside an enum; under the
dotted one it is ordinary expectation plumbing that a `.hero` consumes like any literal.
The `is_name` flag [[∆cea]] threaded through `rules::variant` so a refusal could be
withheld from a name in scope is deleted outright, and with it the shadowing rule and the
UFCS-receiver arm that asked the variant rule what a bare name was. Three special cases,
one dot.

`undefined()` was added along the way: a name that resolves to nothing but is declared as a
type now says which — `` `Placement` is an enum, not a value `` — because the author who
writes a bare variant is exactly who lands there.

The D25 fact needed care. `Placement.hero` is the only construct that writes an enum's name
with no annotation behind it to have recorded it, so `note_type` had to be called from the
qualified path. The first version of that test would have passed for the wrong reason: its
page file declared `place(p: Placement)`, whose annotation records the fact independently.
Rewritten so `<lem.Placement.hero>` is the only mention, then negative-tested by deleting
the `note_type` call and watching `type_refs` come back empty.

The corpus gate caught what a grep had missed: `document/blocks/type-declaration.md` passed
a bare variant as an argument, and blessing turned it into `undefined name`. That file was
in the `lem-enum` grep results at planning time and I read it for the declaration without
reading it for the *use*.

Two review passes followed. At medium: a second producer of "undefined name" inside
`call()` bypassed the new helper, so `<lem.Size>` and `<lem.Size(1)>` reported the same
author error two different ways — fixed, and pinned as two adjacent corpus lines so the
paths cannot drift apart silently. It also flagged that the dotted refusal advises
qualifying even when a *record* is expected, where qualifying produces a different error;
Eric left that as-is. At high, after a panic sweep over twenty malformed dot inputs and a
structural diff of every blessed expectation, the one finding was in prose: the new "Where
an expectation reaches" bullet said a literal's "later elements", implying the first one
resolves. It does not — an unexpected literal reaches none of its elements. Corrected in
`docs/language.md` and D32, and pinned with `[.small, Size.large]`, whose single diagnostic
lands on the *first* element.

`lem-match` lowering stays [[∆PCK]]'s; this item gave the pattern a syntax and an accessor
and left the placeholder standing.

## Retrospective

**An item's technical claims are worth five minutes of verification before planning around
them.** "`.` opening an expression is new to the scanner" would have sent me into
`scanner.rs` looking for what to change. The probe that disproved it cost one throwaway
test and reshaped the whole first stage. Items are written at the end of a different piece
of work, from memory of a codebase — their *why* is reliable in a way their *how* is not.

**Reviewing a plan finds a different class of thing than reviewing code.** [[∆cea]] already
recorded that a pre-implementation review pays; what this one adds is what it paid for.
Nothing the plan review found was architectural — it was two corpus files, an unspecified
grammar slot, a docs ledger that would have contradicted itself, and a table of where
expectations actually reach. Those are all things that are cheap to fix in a plan and
tedious to fix after the expectations are blessed.

**A normative sentence with no test is a guess that reads like a fact.** The "later
elements" wording survived writing, a medium review, and my own re-read, because nothing
exercised it — the corpus had no unexpected-literal case at all. The fix was not better
wording; it was the corpus line that makes the sentence falsifiable. `docs/language.md` is
normative and no gate reads it, which means every claim it makes about behaviour wants a
case behind it or it is prose that happens to be true today.

**Never `git checkout <file>` to undo a temporary edit.** Reverting the deliberately-broken
`note_type` call also discarded every uncommitted change in `compile/expr.rs` — the entire
stage-2 walk. It was reapplied from the session in a few minutes and the gate confirmed it,
so it cost only time, but the habit is wrong: undo a probe by reversing the probe, not by
restoring the file from HEAD.

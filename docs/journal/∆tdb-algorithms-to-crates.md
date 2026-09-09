---
id: ∆tdb
completed: 2026-09-09 15:41:28 -07:00
---

## Backlog item

> - ∆tdb Hand-rolled algorithms to crates — `edit_distance` in
>   `crates/lemmon-syntax/src/parser/template.rs` becomes `strsim::osa_distance`, and
>   `describe` in `crates/lemmon-corpus/src/lib.rs` reports a unified diff via `similar`
>   (dev-only) rather than the first differing line alone. `lemmon-vm`'s `has no field` errors
>   want a did-you-mean that private function cannot reach, and a blessed diff is read by eye

## Planning

One scoping call went to Eric and took the recommendation. The item states the
did-you-mean as its *why*, not its deliverable, so a narrow reading stops after the two
crate swaps — but that stopping point leaves nothing to point at: `strsim` alone unlocks
no more than `lemmon-vm` depending on `strsim` itself would. The plan therefore carried
the suggestion through to the three `has no field` sites, and split the work into three
checkpoints so each stayed separately reviewable.

The design question the swap actually asks is *where the policy lives*. `edit_distance`
was the boring half; the interesting half was `Builtin::nearest`, which holds the
judgement — fold case, and refuse to suggest unless the distance is at most half the
longer name — behind a private function in the parser. Two crates each depending on
`strsim` and writing their own threshold would have satisfied the item's letter and
drifted within a release. It went to `lemmon_syntax::nearest` over any candidate list,
which `lemmon-vm` already sees.

## Refinement

**The swap found a real bug in the code it replaced.** The threshold compared byte
lengths while the distance counted characters, so a non-ASCII name was judged against an
inflated denominator and could earn a suggestion it had not come close to. Invisible for
ASCII tag names; not invisible once the same policy judges field names an author picked.

**Every pre-existing corpus expectation came back from blessing unchanged.** That was the
result worth having: `byline`, `photographer`, `author`, `link` are all refused with
nothing added, so the threshold is doing the work the item claims for it. Two new cases
carry the other half — `render/errors/misspelled-field` for both compile sites,
`host/pages/misspelled-front-matter-key` for the load site. One unit test moved:
`site.nope` now suggests `name`, two of four characters apart, which is inside the
threshold if only just.

**The `similar` swap grew, and by my own hand.** Replacing a first-differing-line report
with a unified diff should have been near a wash. Reviewing it surfaced that
`check` accumulates one report across every failing case, so a stage change — which moves
every case at once — would now print forty lines per case with no bound at all. Two
budgets went in: forty lines of any one case's diff, and diffs for the first five
differing cases with the rest named. Demonstrated by breaking all 43 `syntax/` cases and
reading what came out. Right, but self-assigned: the item never asked for it.

**Eric asked why a refactor added code, which was the right question.** Measured: +389/-105
overall, but executable production code was +70/-53, a net seventeen lines. The rest was
tests (+178), doc comments (+54), corpus fixtures and the lockfile. Where the item scoped a
reduction it reduced — `template.rs` went -43/+5. A trim pass followed and took nine lines
back out of the tests.

Deferred, not done: a misspelled *read* of a builtin method (`s.slugfy`) still gets a bare
"has no field", where the same mistake written as a call gets `no_method`'s full list of
what the type has. It needs the field-versus-method phrasing decided, and `string` has one
method today, so it belongs with the stdlib (∆Q6L).

## Retrospective

**I called a test redundant without checking, and it was load-bearing.** Answering the
"why did this grow" question I listed `counts_a_transposition_as_one_step` as overlapping
its neighbour. Swapping `osa_distance` for `levenshtein` makes it the only failing test in
the crate — it is what pins the crate function the item names. Caught it before cutting,
but only because the trim started with a mutation instead of a reading. The rule that
follows: a test you are about to delete for redundancy gets the mutation that would prove
it, not an argument that it looks like its neighbour.

That worked well enough to keep. Three mutations — `levenshtein`, a vacuous threshold,
byte widths — each failed exactly the test claiming to catch it, which is what made the
smaller suite defensible rather than merely smaller.

**`git checkout -- tests/corpus/` ate an edit I had made an hour earlier.** Undoing a
deliberate corpus corruption also reverted the `README.md` change in the same directory.
Caught by reading `git status` against what I expected to be there. A path-scoped checkout
is a discard, and the blast radius is the path, not the thing being undone.

**The budget machinery should have been raised, not folded in.** It arrived as a
review-fix inside checkpoint 3, which is where scope goes to hide. The failure mode was
real and the demonstration was worth doing, but "my review of my own work found more work"
is the moment to say so out loud, not the moment to quietly widen the diff.

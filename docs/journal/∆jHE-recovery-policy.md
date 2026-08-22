---
id: ∆jHE
completed: 2026-08-22 11:53:09 -07:00
---

## Backlog item

> - ∆jHE Recovery policy — when a tag's terminator or close contradicts its shape, the name
>   wins: `<lem-if x />` stays a block, a scanner-cut head adds no second diagnostic, a wrong
>   close never adopts and the real close is never "stray", unknown tags get one consistent
>   shape, scanner Error tokens enter the error state (no consequential diagnostic); rules
>   written into docs/language.md so tree-sitter can match them

## Planning

The item listed five policies; planning probed each against the parser as it stood after
[[∆7Ib]] before deciding anything, so that the plan rested on observed behaviour rather
than the item's summary. Two of the five left real room, and both calls were made in the
plan and held through implementation:

- **Unknown tags are always void.** The alternative — `>` opens a block, `/>` is void —
  keeps one diagnostic for a misspelt block, but a block-shaped unknown tag needs its
  close matched by *name*, which a context-free tree-sitter grammar cannot express
  without an external scanner. Builtin blocks are fine there (one rule per name). The
  item's whole point is rules tree-sitter can match, so that settled it.
- **Opener-anchored second diagnostics are kept.** `<lem-if x />` with no close, and a
  wrong close followed by EOF, each cost two diagnostics, because the parser cannot tell
  a forgotten close from a mistaken `/>` or a typo'd close. Suppressing either would be
  guessing which mistake the author made; the rule is stated in the docs instead of
  special-cased.

The policies were written into `docs/language.md` as a list alongside the existing
scanner rules, mirroring that list's shape, so the future tree-sitter grammar has one
place to read both.

## Refinement

The first implementation passed `verify` and was wrong in one policy. `/code-review
xhigh` ran as a background agent and was stopped by the five-hour session limit during
its verification pass, one verdict short of done; a second launch was stopped too. The
agent itself refuses resumption once stopped, but its transcript and every verifier
subagent's transcript were on disk under the session directory, so the review was
reconstructed from those: 20 candidates, 13 confirmed, 2 refuted, 5 unfinished. Every
confirmed finding was applied:

- **Policy 5 was implemented as "arm the error state at the token after a scanner error",
  which over-suppressed.** Diagnostics about tokens that were *present* — a `/>` on a
  block, a stray close after a cut head, a reserved word as a name, an unsupported
  composite type — vanished whenever a scanner error token happened to sit just before
  them, and the same input with one more token in between reported them. Replaced with a
  `displaced` flag that silences only expectation diagnostics ("expected X") and junk-run
  reports. The doc rule is now worded to match: what the grammar expected in the error
  token's place goes unsaid; mistakes in the tokens that are there are still reported.
- **`error()` was asymmetric.** A diagnostic anchored at an opener was kept but still
  armed the error state at the current token, so `<lem-style /></lem-style>` lost its
  "cannot be self-closed" — newly load-bearing, since the tag now stays a block. Only a
  diagnostic about the current token arms the state; the run-skipping reporters
  (`skip_to_head_end`, `skip_until`, `composite_type`) use an explicit `error_skipped`.
- **Close-tag handling moved entirely into `items`.** A close naming an unknown tag gets
  the same "unknown tag; did you mean" as its opener, instead of a "found `</lem-iff>`
  while `lem-if` is still open: expected `</lem-if>`" whose advice contradicted the
  did-you-mean. A close the scanner cut adds no parser diagnostic (`</lem-` mid-typing:
  three diagnostics down to two; `</lem-if` cut: one). The stray case lives there too, so
  `root` is one `items` call and `BodyEnd::OuterClose`'s doc is true again.
- The doc claim that an outer close reports "each" implicitly closed block was false —
  only the innermost is named, and that is the better behaviour; the sentence was fixed
  and a three-deep test pins it. `block_close` reuses `void_close`; `is_block` lost its
  dead `head_has_eq` parameter; `composite_type` stops at `=`, which also removed a
  misleading "expected `=`" the review found in passing; the stale too-deep comment.

Not taken: V16 (unit tests mirror fixtures — deliberate belt-and-braces, the fixtures pin
the tree, the tests read as sentences) and the two refuted findings.

Fixtures `broken/02`, `04`, `05` changed shape and were re-checked by hand; four new
fixtures (`11`–`14`) carry the judges' probes from [[∆7Ib]] that the bake-off said belonged
in a corpus. The "Lemmon Language" artifact gained the recovery rules next to its scanner
rules, and its stale "worth a one-day spike" note on lelwel was replaced with the ∆7Ib
outcome.

Known tail: a `lem-else` inside a misspelt `lem-iff` block costs three diagnostics (unknown
open, "outside `lem-if`", unknown close); that is inherent to unknown-tags-are-void and
fixing the name fixes all three.

## Retrospective

**A review that dies mid-flight is still a review.** The orchestrator was gone, but every
verifier had written its verdict to its own transcript before the stop. Pulling the
candidate list and verdicts out of the subagent `.jsonl` files took a few minutes and
recovered twelve minutes of `xhigh` work; re-running would have cost the same again and
hit the same limit. Worth remembering before reaching for "run it again".

**The review found the bug the tests could not.** Every new test asserted what was
*suppressed*; none asserted what must *still be reported* after a scanner error. The
verifiers' method — diff NEW against HEAD on adjacent inputs — is what exposed the
adjacency dependence. The tests now carry both halves, and that pairing (silenced /
still-reported) is the shape to reach for whenever a rule suppresses diagnostics.

**The five policies were fewer than the decisions.** The item named five; implementation
surfaced three more (opener-anchored diagnostics and the error state, unknown close tags,
cut close tags) that only showed up once the first five changed the tree. Same lesson as
[[∆7Ib]]'s retrospective, one level down: a policy list is a starting point for probing,
not the finished set.

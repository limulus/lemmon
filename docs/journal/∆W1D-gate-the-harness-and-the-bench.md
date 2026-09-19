---
id: ∆W1D
completed: 2026-09-19 15:35:08 -07:00
---

## Backlog item

> - ∆W1D Gate the spike and the benchmark's fixtures — `spike:the-rebound` and `wasm:bench`
>   consume the boundary and the corpus layout yet run in no gate (∆xWs kept them out), and
>   each has since broken unseen: a retired diagnostics contract, then a corpus case ∆Bhz made
>   a deliberate error. Give each a one-pass mode without timing loops, run from `verify:js`,
>   which needs the staged artifact anyway. Planned in `docs/plans/∆W1D-plan.md`

## Planning

The item was written by ∆Bhz's session, and the 2026-09-19 organization survey left notes
for it, not a plan. An Opus agent planned it from those notes and the code, the managing
session reviewed the plan, and Eric's standing decision was to stop for him only if the
plan changed `verify:js` beyond what the item says. It did not.

**What "one pass" is.** Each script's correctness checks, once, with no timing loop, no
table and no render budget, so a slow machine cannot fail it. One pass is the default mode
and `--time` adds the rest; `npm run wasm:bench` passes `--time` and keeps its meaning.

**Where the gate lives.** Two more legs of `verify:js`'s `concurrently` list,
`test:fidelity` and `test:fixtures`. Vitest specs were considered and rejected: both entry
points are imperative scripts, and anything moved to `scripts/lib/` is held to 100% branch
coverage, which would have meant about 100 lines of synthetic spec for the fidelity
harness's forgiveness predicates, which are not what broke. What did move is the two
decisions a gate applies: which corpus fixtures are clean, and what counts as an
unacceptable diagnostic. CI needed no change, since its `verify` job downloads the staged
artifact before it runs `verify:js`.

**The fixture filter.** The bench chose fixtures by `/errors/` in the path. The corpus
README says there is no such split, and `tests/corpus/host/` has thirteen error
expectations outside any `errors/` directory; the bench had been spared only because it
does not walk `host/`. `cleanFixtures()` reads each case's blessed trailer instead.

**Cut from the plan by the managing session:** a shared `timings.ts` for `mean`,
`percentile` and the table printer. It added about 125 lines to remove about 25 duplicated
lines of timing code that no gate runs, and the two `percentile`s differ in contract.

**Added beyond the item:** the fidelity harness asserts how often each forgiveness class
fires (6, 6 and 1). ∆xWs's retrospective was about an allowlist that could silence more
than it claimed with nothing noticing.

**Eric's decision:** `spike/the-rebound/` became `tests/fidelity/the-rebound/` and the
script `test:fidelity`, inside this item. "The spike" stays the name of the Phase 0
milestone; decision records D26 and D28 keep the old path as records of their moment.

What the notes had wrong: `percentile` was not a plain duplicate; CLAUDE.md had six
mentions, two of them statements this item makes false; the front-matter reader's line
range; and that CI's `verify` job already ran `npm run wasm:bench` after `verify:js`, so
"runs in no gate" had been true of pre-push only.

## Refinement

Three checkpoints, each built by an Opus agent and reviewed by a second agent told not to
modify files (Sonnet for the rename, Opus for the other two).

The rename moved ten files, not nine, and the path constant had four uses, not three.
`test:fidelity` printed what `spike:the-rebound` had printed one commit earlier, timings
aside. The managing session's first commit of it held only the staged renames, because a
`git add` naming the deleted directory failed and its error was discarded; it was amended
at once, before anything was pushed.

Checkpoint 2's review changed the rule. The plan had a trailer line with no span count as
an error "so it surfaces", but counting it as an error drops the case, which is the
silent direction; it throws now, as a missing trailer does. The line reader mirrors
`lemmon_corpus::split_trailer_line` instead of anchoring on the span, after the reviewer
removed the anchor and no test failed. And the reviewer showed that a change to
`WARNING_PREFIX` on the Rust side would leave the gate covering 65 fixtures of 77 and
passing. Its fix was to assert the count; that number would need editing whenever a clean
corpus case lands, so two spec cases read expectations the corpus really blessed instead,
which fails on the same drift and needs no upkeep. The selection did not change: the same
77 fixtures in the same order. An `errors/` case copied into a feature directory was
dropped, and with its trailer blanked the bench failed on it, which is ∆Bhz reproduced.

Checkpoint 3's negative tests ran through `npm run verify:js`, so the gate is what failed:
an undefined name in a clean corpus fixture, a changed `publishedUtc` in the harness's
`data.json`, a forgiveness count raised by one, and an unknown argument. Its review found
three `loadData` calls in the bench whose diagnostics were discarded while the header
claimed every call was checked, the repository README's list of what `verify` runs, and a
refresh recipe that did not mention `EXPECTED_FORGIVEN`.

Each new leg takes about 0.2 s and lint is the longest leg at about 2.2 s, so `verify:js`
takes no longer. CI now runs the corpus scenarios twice per Node, once in one pass inside
`verify:js` and once timed.

Left for ∆4tG: front matter is modelled three times, in `lemmon-vm`'s corpus harness, in
the fidelity harness and in the bench's synthetic documents. When the shell has a real
front-matter reader both scripts use it, and no fourth model appears before then. Both
scripts import `packages/core/src/index.ts`, so ∆Qr2's change to that API will now fail
`npm run verify` instead of a script nobody runs.

## Retrospective

The most useful review finding asked which way the rule fails. For a filter in front of a
gate, wrongly dropping a case is silent and wrongly keeping one is loud, and two of the
plan's choices had erred in the silent direction while describing themselves as strict.

A reviewer's proposed fix is a second design and deserves the same question as the first.
The count assertion would have worked, and would have made every corpus addition edit a
constant in a benchmark script.

A discarded error stream cost one bad commit. `2>/dev/null` on a `git add` is never worth
what it hides.

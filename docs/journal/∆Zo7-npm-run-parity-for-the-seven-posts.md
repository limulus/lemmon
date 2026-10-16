---
id: ∆Zo7
completed: 2026-10-15 22:29:26 -07:00
---

## Backlog item

> - `∆Zo7` `npm run parity` for the seven posts
>   - The comparator from `scripts/lib/rendered-html.ts` run per page against the 11ty
>     `dist/`, with an allowlist of forgiveness classes and their counts

## Planning

The item's brief, folded here. From `∆hyz` Parity harness: build old (11ty) and new
(`lem`) dists, normalize, diff with an allowlist of expected differences, report per-page,
as a local `npm run parity` in limulus-dot-net (not CI: diffs are nonzero until cutover and
the tool dies with it); the acceptance bar for the whole initiative, so it lands before the
port is "done", not after. Scope here: per page, in the branch; the allowlist names each
forgiveness class and how often it fires, and fails on anything else, as the fidelity
harness does; a class that forgives more than it claims hides real differences (the `∆xWs`
retrospective), so test what the allowlist erases, not only what it selects; lift
`renderedPage` and `compareRendered` into the shell, the fidelity harness keeping them for
its own page.

Built in the slice-1 run of 2026-10-15 by an Opus agent across both repositories, from a
brief the managing session wrote around two inputs: `∆foi`'s hand-off, a classifier
prototype that landed every difference on the seven posts in fourteen named classes with
per-post counts, and its review, which had perturbed built pages and found four of those
classes absorbing a regression without a count moving — a self-card among the
recommendations, a footnote ref pointing at the wrong note, any heading id, a
function-name class on the wrong host. The brief asked for the comparator to gain a small
set of orthogonal seams rather than a hook per class, for each narrowing to come with the
test that fails without it, and for a post missing on one side to fail rather than drop
out.

## Refinement

The comparator moved from `scripts/lib/` into `@limulus/lemmon-shell` with five seams:
`attr` for one differing attribute of a matched pair, offered one at a time so several
classes may each explain part of one element; `text` for a differing run; `theirsOnly`
and `oursOnly` for an element one side lacks; and `subtree`, offered every matched pair,
which may take the children over and compare pairs inside with more classes in effect.
The first class to explain something is counted, once per element, text run or only-side
element, and a subtree hook returns its own count. The fidelity harness's three classes
migrated to the new shape with its result unchanged (585 nodes, 6, 6 and 1). On the branch,
`npm run parity` builds both sides, compares every post under `feed/` as rendered HTML
through the fourteen classes in `bin/parity/classes.ts`, holds each to its per-post count
in `bin/parity/expected.ts`, and exits 1 on an unexplained difference, a miscount, a stale
row, or a post one build lacks. The branch typechecks the shell from source through the
`source` condition, so it needs no `dist/` in the lemmon checkout; its TS 5.9 now checks
lemmon's shell and core sources, a coupling taken knowingly.

A read-only Opus review perturbed the built pages twenty-six ways and mutated the classes
seventeen. It found the recommended-cards class taking over a subtree it barely checked
(a tail card's title, the selection, text in the list all passed); the two datetime
classes blind to the written offset; the escaping class checking one direction; the
reading-time class accepting a minute either way; seven narrowings with no test behind
them; and `npx lem build` in the script, which with the `file:` link broken would have
installed an unrelated registry package named `lem`. All fixed: the recommendations
class now owns only which cards differ, compares each tail card against that post's card
on the feed index, and checks the selection against the newest-five rule from the index's
dates, which raised the scoped-class, datetime and svg-counter counts as the tail cards
joined the comparison; the offsets this site writes are pinned; the escaping class
compares word sets both ways; reading time is one minute under, exactly; the script runs
`node_modules/.bin/lem` and names the prerequisite when it is missing; the document mode
is compared beside the doctype; and the contract wording for `subtree` is honest, with
the comparator throwing on a hook that compares and then declines. `npm run verify`
(345 tests, 100% coverage) and the branch's `npm test` (139) and `npm run parity` pass;
the branch commit is `2939fd6`.

Left as they are: `heading-id` does not accept the `-1` dedupe suffix both generators add
to a repeated heading (no post has one; it will surface as unexplained); the scoped-class
table hard-codes WebC's hashes, so a component's CSS change on the branch moves a hash and
fails parity until the row is updated; `utc-revision-display` forgives a visible
inconsistency on the live site, where 14 of 52 revision dates are written in `-07:00` and
show a different wall clock from the rest.

## Retrospective

A class that explains a difference is a claim about everything it hides, and the
prototype's classes were written to make the counts come out, not to be tested against
regressions; the review's perturbations, not reading, were what separated the two. The
same held one level down: a class can have a negative test and still carry narrowings
nothing pins, which mutation testing found in seven places. Both are cheap to run and
worth running on any allowlist before it is trusted.

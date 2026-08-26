---
id: ∆Ma4
completed: 2026-08-26 08:00:01 -07:00
---

## Backlog item

> - ∆Ma4 Compare the spike's output as HTML, not as bytes — diff parse5 trees with
>   browser-style text normalization (runs collapse to a space, whitespace-only nodes between
>   block elements drop, `pre`/`textarea` stay exact), then rewrite `spike/the-rebound/site/`
>   as ordinary indented markup. Matching WebC's indentation byte for byte is what forces
>   those files to hug their bodies, against D26's point, and it settles what ∆hyz normalizes

## Phase 0, closed

This was the last item under the Phase 0 initiative, whose section in `BACKLOG.md` read:

> Exit criterion (architecture doc 3): limulus.net's `the-rebound` post renders through a
> minimal base→page→article layout chain from the WASM core, rendering the same as the
> 11ty output (D28), with VM render time under 1 ms/page. Everything here serves that demo.

[[∆xWs]] rendered the page and [[∆2vO]] made its templates honest; this item made the
fidelity gate honest. The bar stands met at 0 unexpected rendered differences and
~143 µs/page (14% of the budget), and the section is gone from the backlog.

## Planning

Eric asked for the plan to be read through D26 and D28 and reduced to a sentence that would
steer the work. It came out as: **the gate asks what a browser would render, not what bytes
were written** — everything D26 and D28 move (a definition's margin, blank lines between
blocks, the column a slot's continuation lands on) must be invisible to the comparison *by
construction*, never by allowlist, so that the only whitespace able to fail it is whitespace
HTML actually renders. The corollary shaped the diff more than the sentence did: the old
harness's guard — "indentation deltas on content lines must fail if they come back" — was
defending the byte gate, and under D28 it would be fighting the decision. The guard did not
disappear, it moved: a changed space between two inline runs, or inside `pre`, still fails.

Two things D28 decided that the item left open. dprint leaves the harness — formatting both
sides is a byte-level normalization, and the tree diff makes the "reference cannot be
formatted → compare raw" dance moot — with its two devDependencies, which [[∆F9l]] re-adds
in the shell where they belong. And "browser-style text normalization" was taken literally:
the comparator models the UA stylesheet's white-space processing (line breaks at block
edges, replaced elements as characters, collapsing across inline boundaries), not a blind
collapse. It knows nothing of the site's CSS, so the harness declares the two custom
elements limulus.net makes block-level (`article-photo`, `data-chip`), each verified against
the `.webc` source rather than assumed.

Sequencing was fixed up front and held: build the gate, negative-test it against the *real*
page (a newline between the two adjacent licence `svg`s; an added attribute — both failed),
confirm the hugged templates still pass, and only then rewrite them. The rewrite passed with
identical counts — 585 nodes matched, 6/6/1 forgiven, 0 unexpected — which is the proof that
it cost nothing.

## Refinement

**parse5 showed what the byte diff had hidden.** Both remaining "markdown-engine" classes
turned out to describe real parse trees, not text quirks. markdown-it's `<p></pho-picture>`
mangle triggers the parser's adoption agency: the reference's figure holds a ghost
`<p><a></a></p>` and an empty `<p>`, and the figcaption sits outside the link. WebC's raw
`"` inside a double-quoted `alt` cuts the value at the quote and turns the rest into ~80
junk attribute names (`all=""`, `in=""`, …, lowercased and deduplicated by the parser). Each
class now forgives exactly that shape and nothing wider — `attr-escaping` requires every
junk name to be a token of what was cut off.

**The comparator went into `scripts/lib/` with a spec, and the coverage carve-out moved.**
CLAUDE.md said `scripts/` is uncovered and to weigh that before putting logic there; a gate
whose whole value is *what it refuses* is logic worth testing, so `scripts/lib/` became a
third vitest project under the same 100% threshold, and the carve-out now names only the
scripts themselves. Two of the first verify's failures were dead branches the 100% gate
found for me — an `else if` chain whose final fall-through could never run — and were
answered by restructuring, not by `/* v8 ignore */`.

**One judgement call, flagged in the PR:** `script`/`style` text compares with each line's
indentation removed rather than exactly. It is not rendered text, and it dedents with the
definition around it per D26; comparing it byte for byte would have forced `pageLayout`'s
inline script back into a hugged shape for no reader's benefit.

The templates lost the two workarounds the byte gate had imposed on them: the
complementary `lem-if`s in `articleCard` became `<lem-else />`, and the `i gt 0` newline
hack in the revisions loop went away. The only deliberately hugged spots left are the two
places a browser would paint the space.

## Retrospective

**The plan's constraint questioning happened this time, and it paid.** [[∆2vO]]'s
retrospective was "find out what is pinning the output before planning around it"; this item
started from that question, and the answer (the UA stylesheet, plus two lines of site CSS)
was small enough to model rather than approximate. The item text's own sketch — "whitespace-
only nodes between block elements drop" — would have been wrong in inline contexts
(`<a>x</a> <a>y</a>`) and inside flex containers; reading CSS Text §4.1 instead of
implementing the sketch is what let the negative tests bite.

**I wrote three test expectations from arithmetic instead of from the code, and all three
were wrong** — a forgotten `head` element in two match counts, and a "stop at the first
difference" case built from two sibling elements, which are separate parents and rightly
each report. None was a bug; each cost a run. The forward rule: when an assertion is a
count, derive it by walking the input once, not by recalling the shape.

**Verify's Rust leg reported exit 127 and I nearly read it as a failure of mine.** It was
`cargo` missing from the tool shell's PATH, not code. Recorded outside the repo, since it is
a sandbox fact, not a project one.

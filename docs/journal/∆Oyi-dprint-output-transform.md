---
id: ∆Oyi
completed: 2026-10-17 12:53:21 -07:00
---

## Backlog item

> - `∆Oyi` dprint output transform
>   - The dprint-markup `transformOutput` (lineWidth 92) from `11ty/dprint.ts`, the first
>     output transform; the comparator, not dprint, stays the judge of parity

## Planning

The item's brief, folded here. From `∆F9l` svg-sprite + dprint plugins: the dprint-markup
`transformOutput` (lineWidth 92) from `11ty/dprint.ts`, run after `∆1Hp`'s side-channel
splice, "so formatted HTML diffs cleanly in the parity harness". Scope here: the dprint
half of the original item, the sprite being `∆IvY`; the `∆xWs` retrospective records that
`11ty/dprint.ts` catches the formatter's failure and returns its input unchanged on the
hardest pages, so formatted output is for reading diffs and the comparator decides parity.

Planned in the slice-2 run of 2026-10-17 by a read-only Opus agent and reviewed by a
second, both measuring with the branch's own `@dprint/formatter` 0.4.1 and
`dprint-plugin-markup` 0.24.0. The measurement undercut the brief's reason. Eleventy's
transform fails on six of the seven feed posts, and on 27 of its 55 pages, which it ships
unformatted: five stop at an unescaped quote WebC prints inside a recommended card's photo
alt (the `attr-escaping` class), the-rebound at the stray `<p>` markdown-it leaves inside a
photo's `figure` (`md-photo-wrap`). Lemmon's output formats on all 25 pages. So formatting
Lemmon's pages moves a text diff against Eleventy's further away, not closer: the share of
Eleventy's lines a Lemmon page keeps falls from 30–36% to 6–7% on the six posts, and rises
from 7% to 88% only on new-york-2024, the one post Eleventy formats. Parity, which compares
rendered HTML, is unchanged by formatting: every count and every `matched` total came out
identical, and `compareRendered(dprint(page), page)` found no difference on any page.

The plan recommended the formatter as the site's own plugin, `lemmon/dprint.ts` in the
branch, on a synchronous shell `transformOutput` hook, so Lemmon ships no formatter (about
1.3 MB measured).

## Refinement

The lemmon half was built before Eric had weighed the measurement: a synchronous
`transformOutput(page) → string | Uint8Array` run over each page after its marks resolve,
copied in and out, a failure a warning at the page that keeps its bytes, async transforms
refused at load (145 production, 287 test and 119 doc lines, `npm run verify` green at 463
tests). On 2026-10-17 Eric dropped dprint from slice 2, since it does not help diff
Eleventy against Lemmon. The commit, `ba9ece6`, was not merged; it is parked on the local
branch `parked/oyi-transform-output-hook`, and `∆c4j`'s brief points at it, since the
plugin host lists `transformOutput` among its hooks. Slice 2 became "Styled", ending with
`∆D3R`. Whether limulus.net wants formatted HTML after the cutover, when Eleventy's
partial formatting goes away, is `∆wOb` in Someday/Maybe.

## Retrospective

The plan's first measurement showed the item's stated reason did not hold for this site,
and the build went ahead in parallel anyway. When a plan's numbers undercut an item's *why*,
that is the moment to take it to Eric, before anything is built.

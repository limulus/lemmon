---
id: ∆tAw
completed: 2026-10-17 20:58:28 -07:00
---

## Backlog item

> - `∆tAw` Remaining schemas and components
>   - `site.lem` types for link posts, TILs and photo pages, and the components the first
>     slice did not need, so every document page has its types
>   - needs: `∆foi`

## Planning

The item's brief, folded here. From `∆rT5` Schemas, layouts, components: `site.lem` types
for articles, link posts, TILs, photos, vod and authors, and the WebC components rewritten
as `.lem` functions with `lem-style` scoping (D20). Scope here: what `∆foi` left — the
types beyond the feed posts' (`Post`, `VideoPost`, `Photo`, `Author`, `Vod` and the
revisions types had landed) and the components the remaining documents use — serving
slice 4's acceptance: Eric builds the branch and all 25 documents, code blocks highlighted
and client JavaScript bundled to hashed URLs, compare clean. The brief expected the
"You Might Enjoy…" list, once it read every kind of article, to retire parity's
`recommended-set` and `svg-desc-counter` classes.

Planned in the slice-4 run of 2026-10-17 from two read-only Opus maps — one comparing all
25 documents region by region past the comparator's early stops, with a typed scratch copy
of the site — and a review of the plan that probed its claims in a scratch copy. Built on
the branch by an Opus agent (`3b596f1`), reviewed read-only by another. Decisions:

- One collection. A page belongs to one collection and lists do not concatenate, so
  `articles: [Post] from "{feed,links,penumbra,tils}/**/*.md"` matches exactly the 25
  documents, as Eleventy's `collections.article` holds every kind; `newest` sorts it for
  the recommended list, and `posts` went, since nothing read it.
- Optional fields on `Post` rather than a `LinkPost` subtype: `linkUrl`, `linkTitle`,
  `viaUrl`, `scripts`, `imageAlt`, as the WebC tests `linkUrl` and `tilTopic` for presence.
  No link post gained a `type:` line; github-copilot gained `type: VideoPost`, front
  matter outside the revision hash, and Eleventy's output is byte-identical.
- The three computed defaults moved in from `∆ITg`, since the chips need them: `title`
  from `linkTitle`, `linkDomain` and `tilTopic`, the last two through `site.lem` functions
  matching `eleventyComputed.js`'s `URL.hostname` and `extractTilTopic` (probed on 14 cases
  against Eleventy's own functions).
- `link-chip` and `til-chip` as `.lem`, chosen in `articleChips` as the WebC header and
  card choose them. Penumbra pages link their own feed from `penumbra/penumbra.json`, the
  file Eleventy reads, in the `<link rel="alternate">` only; the Subscribe button stays
  `/feed.xml`, as the WebC hardcodes it.
- `npm run parity` compares every document, listed from the source `.md` files, so section
  indexes and TIL topic pages are never mistaken for documents; a spec ties the harness's
  sections to the `articles` glob.

## Refinement

- `svg-desc-counter` could not be retired. With identical card lists Eleventy still
  numbers a card with a preview photo after the photo-less cards that follow it — its own
  `/feed/` index shows the pattern, identically across builds — so the class stays until
  the cutover, narrowed to inside `article-card`. `recommended-set` retired, which makes
  parity stricter: the lists now compare card by card.
- The computed defaults' costs: a document with neither `title` nor `linkTitle` loads as
  "Link: " instead of failing, where Eleventy renders an empty title; `linkDomain` keeps
  non-ASCII and percent-escaped hosts as written where `URL` normalizes them. Ties in
  `newest` reverse, where Eleventy's stable sort keeps them; no two articles share a date.
- The photo pages' types were deferred to `∆hvm`, which first reads them (its brief says
  so), as was the TIL chip's `nolink` form, rendered by no page until the topic pages.
- At the end of the item 10 of the 25 documents compare clean; the other 15 differ only in
  what `∆ITg` (the appender, the Via paragraph, explicit-id heading tabindex, the Liquid
  include), `∆Nxd` (8 code blocks) and `∆d1N` (per-page scripts) fix. Rows in
  `expected.ts` for those pages count only what the comparator reaches before its first
  unexplained difference, so each later item moves them.

## Retrospective

The brief's claim that `svg-desc-counter` would stop firing was a guess about why it fired,
written before anyone compared identical card lists. A comparison that stops at the first
unexplained difference also hid whether anything of this item's remained behind the
link-post differences: the region-by-region script, written while mapping the slice, was
what showed nothing did.

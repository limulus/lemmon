---
id: ∆LsH
completed: 2026-10-18 13:29:12 -07:00
---

## Backlog item

> - `∆LsH` Index pages as page templates
>   - Home, the six section indexes, the author page and `404` as `.lem` page templates with
>     front matter (D41) reading their collections through glob bindings
>   - needs: `∆tAw`

## Planning

The item's brief, folded here. From `∆ePP` Page templates, generated and non-HTML pages:
the index pages and 404 as page templates (D41), photo and TIL topic pages as generators
(D43), three feeds and `sitemap.xml`; the last pages on WebC and 11ty plugins. This item
took the seven collection-reading pages, the author page and `404`; the two paginated
pages are `∆hvm`, feeds and sitemap `∆zu0`. Slice 5's acceptance, which it serves: Eric
opens `dist/` from `/` and reaches every page of the site, each comparing clean.

Mapping the slice on 2026-10-18 counted 55 HTML pages in Eleventy's output, 25 of them the
documents slice 4 ported, and found a ninth page the backlog line missed,
`/penumbra/journal/`, so "five section indexes" became six. It also found that the core
needed nothing new for these pages: page templates with front matter, glob collections,
`filter`, `sortBy`, `take`, `find`, `groupBy(…).keys()` and `lem-head` were all built.

## Refinement

Built on the port branch in one pass (limulus-dot-net `6efc449`, local):

- Nine `.lem` page templates beside their `.webc` sources, which Eleventy still builds
  from: `/`, `/404/` (written at `/404/index.html`, where CloudFront looks), `/eric/`,
  `/feed/`, `/links/`, `/penumbra/`, `/penumbra/journal/`, `/photos/` and `/tils/`, with a
  `sitePage` layout and a `photoList` component. `site.lem` gained `SitePage` (title,
  optional teaser, subtitle, image and hero by `site.photos`, a `nav` switch defaulting to
  true), `pages: [SitePage] from "**/index.lem"` standing in for Eleventy's
  `collections.all` so `/` reads the section pages' teasers, the link, penumbra and TIL
  lists, `tilTopics` and `gallery`. `base.lem` places `mark("head")` for `/`'s `rel="me"`
  links and writes `og:image` for a `SitePage`; the header's nav became switchable.
- `npm run parity` now compares every `index.html` either build writes, with
  `bin/parity/expected.ts`'s rows checked against that roster, and a `PENDING` list of
  the 21 pages `∆hvm` will generate (19 photos, two TIL topics) that fails once Lemmon
  writes one, so the list cannot outlive the pages. An HTML file not named `index.html`
  fails as a stray rather than going uncompared.
- Forgiveness: `source-pill` (the header's Source link names the `.lem` where it named the
  `.webc`), `photo-list-heading` (WebC's `webc:text` leaves `/photos/`'s hidden headings
  empty; ours must hold that photo's title, read from the data), and `webc-attr-leak`,
  `reading-time` and `svg-desc-counter` widened. The three reader-visible ones went to the
  cutover brief's list.

Departures from the plan: `/eric/`'s newest four are filtered in its template, since
nothing else reads them; its markdown, which carries markdown-it-attrs `{rel="me"}`
blocks comrak cannot express, is written as HTML in the template, ids and
`tabindex="-1"` as markdown-it wrote them. `/tils/` renders its article list before its
topic chips, because the shell collects component styles in render order while WebC
bundles a component after those it depends on; the cost is that `uniqueId` numbers the
topic icons after the cards', which `svg-desc-counter` forgives.

The diff review found `photo-list-heading` accepted any text in ours (a probe passed with
"undefined"), the source pill's header check untested, and non-`index.html` pages skipped
silently; all three were fixed with specs that fail when the check is removed. It also
recorded, unchanged, that the author filter proves nothing while the site has one author,
and that `sortBy(…).reverse()` reverses ties that Eleventy's stable sort keeps in input
order; no two photos or articles share a date today, and parity would report one.

## Retrospective

A forgiveness class written to select a region is easy to leave without a check on what
it lets through: `photo-list-heading` matched the right elements and accepted their
contents unseen. Asking of each class what it would erase if ours were wrong, before the
spec is written, would have caught it at build rather than at review.

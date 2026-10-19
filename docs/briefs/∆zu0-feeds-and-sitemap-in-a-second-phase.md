# Feeds and sitemap in a second phase

## Background

### From `∆ePP` Page templates, generated and non-HTML pages

Page templates, generated and non-HTML pages — the index pages and 404 as page templates
(D41), photo and TIL topic pages as generators (D43), three feeds (rss + two atom,
`/feed.xsl`), `sitemap.xml` with video entries: the last pages on WebC and 11ty plugins
(`eleventy-plugin-rss`, pagination, `sitemap.njk`). A feed renders in a second phase,
reading other pages' finished output as a string source (D42)

## Scope and constraints

- Slice 6's acceptance, which this item serves: Eric sees `npm run parity` report zero
  unexpected differences over the whole `dist/`, and limulus.net builds and deploys with
  `lem` with its CloudFront config untouched.
- The last pages on 11ty plugins (`eleventy-plugin-rss`, `sitemap.njk`).
- Mapped on 2026-10-18. Eleventy writes `/feed.xml` (RSS 2.0, the 25 articles, with the
  `/feed.xsl` stylesheet line), `/penumbra/feed.xml` (Atom, 7) and `/tils/feed.xml` (Atom,
  2, linked from no page) through eleventy-plugin-rss 2.0.4, and `/sitemap.xml` (56 entries,
  video entries on the two `VideoPost`s) from `www/sitemap.njk`. A feed item's content is
  the post's body without its layout, escaped once, after Eleventy's transforms: code
  highlighted, and every link made absolute against the post's url (44 `href`s, `../` and
  `./` included). No feed needs the whole finished page, and one string per post serves all
  three feeds. Highlighting comes from the shell's handlers and making links absolute needs
  HTML parsing (invariant 2), so both happen in the shell over a body the core rendered
  without its layout.
- Decided by Eric on 2026-10-18: the sitemap fixes Eleventy's three quirks rather than
  reproducing them — it lists `/tils/nodejs/` (Eleventy's pagination put only a
  generator's first page in `collections.all`), leaves out the three feeds, and gives an
  undated page no `lastmod` (Eleventy gives the template file's creation time, the checkout
  time) — each a counted parity class on the cutover's list.
- Decided in planning: dates keep D38's written-offset spelling, and parity compares a
  date as its instant (Eleventy's RSS `pubDate` takes the build machine's zone, `MST` here
  and `GMT` in CD); parity's XML leg (a namespace-aware tree compare, entries keyed by guid
  or id, the sitemap as a set keyed by `<loc>`, feed content through the page classes)
  lands with this item. The second phase's shape is a decision record, D46.
- Placeholders to sweep: the line in `docs/architecture.md` that waits on this item, and
  `packages/shell/README.md`, which the gate does not read.

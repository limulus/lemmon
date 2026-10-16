# Photo and TIL topic pages as generators

## Background

### From `∆ePP` Page templates, generated and non-HTML pages

Page templates, generated and non-HTML pages — the index pages and 404 as page templates
(D41), photo and TIL topic pages as generators (D43), three feeds (rss + two atom,
`/feed.xsl`), `sitemap.xml` with video entries: the last pages on WebC and 11ty plugins
(`eleventy-plugin-rss`, pagination, `sitemap.njk`). A feed renders in a second phase,
reading other pages' finished output as a string source (D42)

## Scope and constraints

- Slice 5's acceptance, which this item serves: Eric opens `dist/` from `/` and reaches
  every page of the site, each comparing clean.
- The only two pages of the site that paginate; generators (`∆99b`) exist for them.

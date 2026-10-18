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
- The photo pages' `site.lem` types were deferred here from `∆tAw` (2026-10-17), whose
  backlog line named them: nothing before these pages reads them. The TIL chip's `nolink`
  form (`www/_includes/components/til-chip.lem` on the port branch) is first rendered by
  the topic pages, since no TIL is among the newest five articles a post recommends.

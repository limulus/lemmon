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

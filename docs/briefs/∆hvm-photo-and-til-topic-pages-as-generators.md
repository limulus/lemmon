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
  the article cards of `/feed/` and `/tils/` (built in `∆LsH`), and then by the topic pages.
- A photo page's URL is its stored `alias`, never `title.slugify()` (from `∆pP5`, closed
  on 2026-10-18): the aliases were minted by `@sindresorhus/slugify`, whose German rule
  spells `Dürer` `duerer` where the core's Latin fold spells it `durer`.
- Decided by Eric on 2026-10-18: the three numbers `photo-meta.webc` parses out of EXIF
  strings with `parseFloat` (focal length `"6.765 mm"`, f-number `"f/1.78"`, and the
  second time of a composite exposure `"0; 33298/9605; …"`) come from the data, written by
  the port's `bin/lemmon-photos.ts` with the same `parseFloat`, not from a core builtin;
  `process-photos.js` writes them itself at the cutover (`∆oAl`).
- Found mapping the slice: WebC scopes `photo.webc`'s style to every element the template
  writes (`w3ey9fgef`, 34 elements, the photos sprite's symbols included), which parity's
  `scoped-class` does not cover, since it handles a component's host element; the photos
  sprite sits inside `main`; `mapbox-gl.css` (55 KB) joins the page's bundle between
  `chip-list` and `map-box`; and the photo date is ICU text (`toLocaleDateString`).

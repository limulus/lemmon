# Port: `site.lem`, three layouts, the posts' components

## Background

### From `∆rT5` Schemas, layouts, components

Schemas, layouts, components — `site.lem` types for articles, link posts, TILs, photos, vod,
authors; the base/page/article/video layouts and ~20 WebC components rewritten as `.lem`
functions with `lem-style` scoping (D20). First real exercise of the language at scale;
expect it to feed fixes back into Phase 1 items

### From `∆o24` Content + markdown parity

Content + markdown parity — typed front matter for every `.md` (globs replace `tags`, schema
defaults replace `links.json`), computed defaults replacing `eleventyComputed.js` (title
from linkTitle, tilTopic from path, link/via domains, readingTime), the link-post appender
as `transformDocument`, markdown-it's footnotes, task lists, attrs, and anchors in comrak,
and code by the highlight.js producer (`∆Nxd`)

## Scope and constraints

- Slice 1's acceptance, which this item serves: Eric runs `lem build` in the limulus-dot-net
  port branch, opens the seven `feed/` posts from `dist/` in a browser, and `npm run parity`
  there reports them against the 11ty `dist/` with every difference in a named forgiveness
  class.
- Only what the seven `feed/` posts need: `Page` and `Post` in `site.lem`, the base, page
  and article layouts, the components those posts use, and typed front matter for them. The
  remaining types, the video layout and the other components are `∆tAw`; the computed
  defaults and the appender are `∆ITg`.
- Written as an author would, not in the shape of the fidelity fixtures in
  `tests/fidelity/the-rebound/site/`, which target another engine's bytes (the lesson of
  `∆2vO`).
- An interim `<link>` to the passthrough-copied CSS in the base layout keeps the pages
  readable until `∆D3R` replaces it with `lem-style`.
- The two posts with Liquid `renderTemplate` blocks (`the-rebound`,
  `better-software-with-ai`) get their lem calls on the branch, as the fixture did by hand.

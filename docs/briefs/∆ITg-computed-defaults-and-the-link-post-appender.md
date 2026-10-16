# Computed defaults and the link-post appender

## Background

### From `∆o24` Content + markdown parity

Content + markdown parity — typed front matter for every `.md` (globs replace `tags`, schema
defaults replace `links.json`), computed defaults replacing `eleventyComputed.js` (title
from linkTitle, tilTopic from path, link/via domains, readingTime), the link-post appender
as `transformDocument`, markdown-it's footnotes, task lists, attrs, and anchors in comrak,
and code by the highlight.js producer (`∆Nxd`)

## Scope and constraints

- Slice 4's acceptance, which this item serves: Eric builds the branch and all 25 documents,
  code blocks highlighted and client JavaScript bundled to hashed URLs, compare clean.
- The feed posts' front matter landed in `∆foi`; code blocks are `∆Nxd`. Markdown-it's
  footnotes, task lists, attrs and anchors are core work where comrak lacks them, with their
  decision record.

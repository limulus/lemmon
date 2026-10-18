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
- Narrowed on 2026-10-17 (`∆tAw`'s journal entry): the typed front matter and the three
  computed defaults the chips read (`title` from `linkTitle`, `linkDomain`, `tilTopic`)
  landed in `∆tAw`. What remains: the appender, the Liquid include in four penumbra
  documents (inside a raw HTML block, where a lem call is refused — the slice-4 plan's
  amendment says how), and markdown-it's extras. Of those the 25 documents use footnotes,
  already forgiven by parity's `footnote-markup`, and `{#id}` on three headings, whose
  `tabindex="-1"` the `heading-id` class forgives only for slug ids; no document uses task
  lists or attributes beyond `{#id}`.

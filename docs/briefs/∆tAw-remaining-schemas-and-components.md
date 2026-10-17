# Remaining schemas and components

## Background

### From `∆rT5` Schemas, layouts, components

Schemas, layouts, components — `site.lem` types for articles, link posts, TILs, photos, vod,
authors; the base/page/article/video layouts and ~20 WebC components rewritten as `.lem`
functions with `lem-style` scoping (D20). First real exercise of the language at scale;
expect it to feed fixes back into Phase 1 items

## Scope and constraints

- Slice 4's acceptance, which this item serves: Eric builds the branch and all 25 documents,
  code blocks highlighted and client JavaScript bundled to hashed URLs, compare clean.
- What `∆foi` left: the types beyond those the feed posts needed (`∆foi` landed `Post`,
  `VideoPost`, `Photo`, `Author`, `Vod` and the revisions types, and the video layout, since
  one feed post uses it), and the remaining WebC components as `.lem` functions.
- The post layout's "You Might Enjoy…" list reads the feed posts alone; Eleventy's reads
  the newest articles of every kind. With link posts and TILs typed, the recommendations
  list all three, so parity's `recommended-set` and `svg-desc-counter` classes stop firing
  and are retired with their rows in the branch's `bin/parity/expected.ts`.

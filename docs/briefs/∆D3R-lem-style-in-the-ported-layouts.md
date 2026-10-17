# `lem-style` in the ported layouts

## Background

### From `∆rT5` Schemas, layouts, components

Schemas, layouts, components — `site.lem` types for articles, link posts, TILs, photos, vod,
authors; the base/page/article/video layouts and ~20 WebC components rewritten as `.lem`
functions with `lem-style` scoping (D20). First real exercise of the language at scale;
expect it to feed fixes back into Phase 1 items

## Scope and constraints

- Slice 2's acceptance, which this item serves: Eric opens the seven posts and sees them
  identical to the live site, with their CSS as per-page hashed bundles under
  `/assets/immutable/`, and `npm run parity` reports zero unexpected differences for them.
- Branch work: the layouts and components from `∆foi` gain `lem-style` scoping (D20), and
  the interim `<link>` goes.

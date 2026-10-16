# dprint output transform

## Background

### From `∆F9l` svg-sprite + dprint plugins

svg-sprite + dprint plugins — inline sprite from `assets/svg/{common,photos}` as an
`emit`/data hook replacing `eleventy-plugin-svg-sprite`, and the dprint-markup
`transformOutput` (lineWidth 92) from `11ty/dprint.ts`, run after `∆1Hp`'s side-channel
splice, so formatted HTML diffs cleanly in the parity harness

## Scope and constraints

- Slice 2's acceptance, which this item serves: Eric opens the eight posts and sees them
  identical to the live site, with their CSS as per-page hashed bundles under
  `/assets/immutable/`, and `npm run parity` reports zero unexpected differences for them.
- The dprint half of the original item; the sprite is `∆IvY`.
- The `∆xWs` retrospective records that `11ty/dprint.ts` catches the formatter's failure and
  returns the input unchanged on the hardest pages, so formatted output is for reading
  diffs, and the comparator decides parity.

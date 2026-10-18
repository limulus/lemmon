# svg-sprite plugin

## Background

### From `∆F9l` svg-sprite + dprint plugins

svg-sprite + dprint plugins — inline sprite from `assets/svg/{common,photos}` as an
`emit`/data hook replacing `eleventy-plugin-svg-sprite`, and the dprint-markup
`transformOutput` (lineWidth 92) from `11ty/dprint.ts`, run after `∆1Hp`'s side-channel
splice, so formatted HTML diffs cleanly in the parity harness

## Scope and constraints

- Slice 4's acceptance, which this item serves: Eric builds the branch and all 25 documents,
  code blocks highlighted and client JavaScript bundled to hashed URLs, compare clean.
- The sprite half of the original item; the dprint half, `∆Oyi`, was retired (its journal
  entry says why).
- A mark producer is enough (2026-10-17, `∆c4j`'s journal entry): a layout writes
  `mark("sprite")` and the producer reads the SVGs, so the need on the plugin host was
  dropped. The port inlines a hand-copied sprite today (`www/_includes/components/
  svg-sprite.lem`, from slice 1), which a new icon would have to be pasted into.

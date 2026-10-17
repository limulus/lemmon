# `lem dev`

## Background

`lem dev` — dev server with COOP/COEP headers and live reload, incremental rebuilds from
render-reported deps and `upsertRecord` (docs/architecture.md §Build pipeline), `--drafts`
(D19). Replaces `eleventy --serve --incremental`, the daily authoring loop

## Scope and constraints

- Edited by the evolutionary slices reshaping (2026-10-12): a change rebuilds everything,
  and the full rebuild time is recorded in this item's journal entry. Incrementality from
  render-reported deps (`∆xBO`) is its own epic, opened only if that time exceeds one
  second.
- Slice 3's acceptance, which this item serves: Eric runs `lem dev` in the branch, edits a
  post or a layout and sees the change in the browser, and sees `lem check` report a
  deliberate error with its span. Drafts, which the item's background names, moved to
  Someday/Maybe as `∆Han` on 2026-10-17.

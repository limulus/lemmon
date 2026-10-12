# `lem dev`

## Background

`lem dev` — dev server with COOP/COEP headers and live reload, incremental rebuilds from
render-reported deps and `upsertRecord` (docs/architecture.md §Build pipeline), `--drafts`
(D19). Replaces `eleventy --serve --incremental`, the daily authoring loop

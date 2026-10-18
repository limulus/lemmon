# Document and asset hooks

## Background

Split from `∆c4j` Plugin host on 2026-10-17, when Eric chose to build only the hooks slice 4
consumes (`∆c4j`'s journal entry says why). The architecture's plugin sketch
(docs/architecture.md §Shell plugin API) has `transformDocument?(doc, ctx): Document`, a
pre-markdown text transform, and `assets?(ctx): Promise<AssetManifest>`, a logical path
mapped to `{url, hash, deps}`. D38 settled the manifest the core takes: finished URL
strings on the host declaration (`HostDecl.assets` in `packages/core/src/lib/types.ts`),
`asset("…")` a compile-time fact the link step stales. D06: no build-time logic in the
language or the core; D42: mark handlers are synchronous.

## Scope and constraints

- Consumers: the link-post appender (`∆ITg`), which appends `[link]: url` reference
  definitions and a "Via" paragraph to a link post's markdown, and the esbuild pipeline
  (`∆d1N`), which bundles client scripts and hands `compile` their URLs. Build what they
  need and no more.
- `transformDocument` runs in the shell before `compile`. What a document is to it (path,
  front matter as parsed, body), whether it may change front matter, and whether it sees
  `.lem` page templates are this item's to settle. Invariant 8: the text the core compiles
  is the text diagnostics are framed against (`CheckResult.texts`), so the transformed text
  is what goes there; an append keeps every offset the author's file has.
- `assets` may be async (only mark handlers are bound by D42), runs before the first
  `compile`, and its files go through the write's collision checks with every other output
  (`packages/shell/src/lib/write.ts`), written only after every check passes, as produced
  files are. The sketch's `{url, hash, deps}` predates D38; the declaration takes URL
  strings.
- Plugin objects outlive builds in `lem dev`, which reads the config once: a hook runs per
  build, and nothing it caches at factory time may go stale across rebuilds.
- `validatePlugins` (`packages/shell/src/lib/plugins.ts`) refuses every field but `name`
  and `marks` today; the new hooks join it, and every other hook stays refused.

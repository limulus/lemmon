# Content API

## Background

Split from `∆c4j` Plugin host on 2026-10-17 and placed in slice 6 with the revisions plugin
(`∆shS`), its one consumer. The architecture's sketch: the shell exposes a content API
(`loadSite(config).collection('articles')`) for tools like the revisions updater.

## Scope and constraints

- The revisions updater hashes each article's title, subhead and body
  (limulus-dot-net's `11ty/update-revisions.ts`, `11ty/revisions.ts`, `get-articles.ts`).
  A link post's `title` is computed (`"Link: " + linkTitle`), so an API over raw front
  matter would hash a different title and record a revision for every link post with a
  `linkTitle`. The API must hand back the loaded record, computed fields included.
- `Core` has `compile`, `loadData`, `plan` and `render` (`crates/lemmon-wasm/src/lib.rs`);
  nothing reads a loaded record back. That is a new boundary call, with its wire shape,
  `npm run rust:wasm:test`, and its decision record.
- The body the updater hashes is the markdown as written, front matter removed — what
  Eleventy's `gray-matter` hands it — not text a `transformDocument` plugin rewrote.
- A generated page's `prev` and `next` hold `Value::Neighbour`, a position in the store's
  page table that a field read resolves (`∆99b`); `json_from_value` in
  `crates/lemmon-wasm/src/wire.rs` reaches it only through an `unreachable!` arm today,
  since a mark payload refuses it. Records read back to a tool need a form for it first,
  such as the neighbour's `{path, index}` reference.

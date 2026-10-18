# The `watch` and `emit` hooks

## Background

Split from `∆c4j` Plugin host on 2026-10-17 and placed in Someday/Maybe: the architecture's
plugin sketch has `watch?(ctx): AsyncIterable<Change>` and `emit?(ctx):
Promise<OutputFile[]>`, and nothing in the port consumes either. `lem dev` watches the
whole input directory, which holds every file limulus.net's plugins read (its client
TypeScript and `assets/svg/**` included); the esbuild pipeline's files come through the
`assets` hook (`∆ETy`), and the feeds and sitemap are templates (`∆zu0`).

## Scope and constraints

- Promote only with a consumer: a plugin reading files outside the input directory, or one
  writing files no template or `assets` hook can.

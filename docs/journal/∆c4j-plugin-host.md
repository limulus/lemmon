---
id: ∆c4j
completed: 2026-10-17 20:05:35 -07:00
---

## Backlog item

> - `∆c4j` Plugin host
>   - The `Plugin` hooks in full (data, transformDocument, assets, transformOutput, emit,
>     commands, watch), plugin `.lem` via `lem-import` in `site.lem` (D23, D06), a content API

## Planning

The item's brief, folded here. From `∆PgF` Plugin host: the `Plugin` hooks (data,
transformDocument, assets, transformOutput, emit, commands, watch, and `marks`: producers
with keys and a payload record, decorators, two producers on one key a load error, the
shell's `styles`/`scripts`/`head` bundler a default plugin the site lists — D42), plugin
`.lem` bound via `lem-import` in `site.lem` (D23, D06), and a content API for out-of-build
tools. Gates the port (D12). Its scope was slice 4's acceptance: Eric builds the branch and
all 25 documents, code blocks highlighted and client JavaScript bundled to hashed URLs,
compare clean. The marks part had landed in `∆F4y`, and `transformOutput` had been built
once for `∆Oyi` and parked unmerged on `parked/oyi-transform-output-hook` (`ba9ece6`).

## Refinement

Split on 2026-10-17, before anything was built, as slice 4 started. Mapping the slice
against the port branch found that its items consume two of the hooks: the link-post
appender (`∆ITg`) must add its `[link]: url` definitions to a document's text before the
markdown compiles, which is `transformDocument`, and the esbuild pipeline (`∆d1N`) hands
`compile` a manifest, which is `assets`. Nothing in the slice reads the others. The
highlighter (`∆Nxd`) and the sprite (`∆IvY`) are mark producers, which the shell has run
since `∆F4y`, so their needs on this item were dropped; the revisions data already loads as
a plain data file. Eric chose to build only what is consumed:

- `∆ETy` Document and asset hooks stays in slice 4, needed by `∆ITg` and `∆d1N`.
- `∆wOn` Plugin data, commands and `.lem` goes to slice 5, beside the photos plugin
  (`∆pP5`), the first consumer of a data hook and a `lem` subcommand. Its brief carries the
  open question planning found: D23's qualified `lem.photos.picture(…)` meets D34, where
  `.` asks the receiver's type and nothing else.
- `∆Gg3` Content API goes to slice 6 with the revisions plugin (`∆shS`), which Eric moved
  there too: its command needs a link post's computed title read back from the core, a new
  boundary call, and the first thing to run it is the cutover's deploy job. The link and
  asset integrity check (`∆yOJ`) moved to slice 6 as well, since in slice 4 it would fail
  on the twenty-odd links to pages slices 5 and 6 write.
- `∆fwn` the `transformOutput` hook goes to Someday/Maybe beside `∆wOb`, its one consumer,
  with the parked commit as its starting point; `∆HzB`, the `watch` and `emit` hooks, goes
  there with no consumer at all.

The placeholders naming `∆c4j` now name `∆ETy`, `∆wOn` or `∆fwn`.

## Retrospective

Nothing was built. The item was cut from the architecture's plugin sketch, which lists every
hook a plugin might want, before the slices were cut; nobody counted which hooks the slice's
other items would call. Slice 4's description inherited "the plugin host in full" from it.

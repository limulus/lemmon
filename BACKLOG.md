# Backlog

This backlog is managed with the **delto** skill — before adding or changing items, consult
it (and the authoring reference it points to) so the conventions below stay enforced.

Organized as Initiative (`##`) → Epic (`###`) → Item (`-`); initiatives are roughly
priority-ordered, items 5 lines max. One standing epic, Refactors, precedes the initiatives.

Each item starts with a “deltoid” — `∆` followed by 3 alphanumerics, e.g. `∆7hy`. Deltoids
are immutable and travel with the item into its completed-work journal entry in
`docs/journal/`. To mint a collision-free deltoid, run
`npx @limulus/delto@1 mint --journal-dir docs/journal` (see `delto mint --help`).

Hard prerequisites use a trailing `; needs: ∆aaa[, ∆bbb]` suffix — the only dependency
mechanism: `delto surface` trusts these edges to decide what is eligible to work on.

### Refactors

Standing epic for reshaping working code that belongs to no milestone. An item sits here
only while doing it first makes the work below it smaller; if that work is claimed first, it
absorbs the item and the item is removed.

## Phase 1 — Language complete, tooling from day one

Seed items for the next phase (architecture doc 3, phase 1); expect these to be split
further at claim time.

- ∆EN6 Dotted enum variants — a variant always starts with a dot: `.hero` where the enum is
  expected (arguments, annotated bindings, `lem-case` patterns, the right of `==`/`in`), and
  `Placement.hero` anywhere, when nothing around it says which enum. A bare `hero` is then
  only ever a name, so D32's variant-over-local shadowing goes; the expectation paths stay
  as what `.hero` asks for. `.` opening an expression is new to the scanner; needs: ∆cea
- ∆PCK Narrowing and typed lowering — `is` and `lem-match` narrowing, a `TypeTest` op over
  record values that carry their type (`Value::Record`), record literals as map literals
  under an expected record type, enum exhaustiveness on the provisional reading. Split
  from ∆C8d because it needs runtime type tags, which nothing produces before it; needs: ∆cea
- ∆Iau Host-declared ambient types — `compile(files, host)` taking the host's `ambient.lem`
  and a per-document `record` (D30, docs/architecture.md §WASM API): `page`/`build` typed
  from the declaration, `site` from `site.lem`'s top-level bindings, the record as a
  compile-time fact, the entry field checked on page schemas. Until then every ambient
  value is `any`; needs: ∆cea
- ∆iCW Schemas at load — `site.lem`'s `from` bindings and each document's record (its
  collection's element type, narrowed by `type:`) type the content model; `loadData`
  validates records (the host's envelope too) at their YAML positions, runs computed
  defaults in dependency order, resolves `by` references and strings by expected type, and
  rejects extra keys, so limulus.net's build throws become load errors; needs: ∆PCK, ∆Iau
- ∆Q6L Stdlib — strings/lists/maps, `datetime("…")` (compile-time checked on a literal
  argument; there is no datetime literal) + rfc3339/rfc822 formatting, `slugify`,
  `readingTime`, `url()`, `asset()`, `raw()`, `uniqueId()`, with typed signatures the
  checker understands, since feeds/sitemap/layout ports all block on these
- ∆Hbw Document mode, full rules — nested `lem-let` capture bodies as sub-documents, and
  string-literal reference resolution in prose calls (docs/language.md §Document mode
  items 2 and 6) with the resolved ids reported as compile-time facts (D25); front matter
  itself is validated by ∆iCW; needs: ∆iCW
- ∆BfR Syntax highlighting for code fences — comrak's `SyntaxHighlighterAdapter` wired to
  syntect with class-based output plus a generated theme stylesheet (D08,
  docs/architecture.md §Crates). Deferred out of document mode: it adds ~1 MB to the .wasm
  and the spike's page has no fences, but limulus.net's archive does, so the port needs it
- ∆RGQ Side channels + generators — `lem-style`/`lem-script`/`lem-head`/`lem-emit`
  collection and dedup, `lem-page` generators (prev/next), `plan()` filling each page's
  entry from the host-named field (D30), serializable modules, render-reported deps, and
  compile-reported facts (signatures, reference ids, manifest entries, a document's record)
  keying the unit cache so a content edit never recompiles its layouts (D25); needs: ∆Iau
- ∆5MM tree-sitter-lemmon + LSP skeleton — embedded-template grammar with HTML/markdown
  injections, corpus agreement test wired into CI, `@limulus/lemmon-lsp` serving diagnostics
  and schema completions from `core.analyze()`; editor tooling is a day-one commitment
  (docs/decisions/README.md D16)
- ∆pXO Lemmon authoring skill — a checked-in agent skill (`agents/skills/lemmon/`) for
  writing `.lem`: tag forms and head-shape rules, expression syntax, type-directed escaping,
  document mode — distilled from docs/language.md, not a copy of it. Porting limulus.net's
  layouts means writing a lot of Lemmon by agent, and rereading the whole language doc every
  time is neither reliable nor cheap.
- ∆MlD Reconcile the architecture artifacts — apply every entry in
  `docs/artifact-changes.md` to the three Claude Artifacts Eric owns ("Shell & Core", "The
  Lemmon Language", "From Eleventy to Lemmon") and clear the ledger. The artifacts are the
  rationale record; changes are logged per pass and applied once, at the end of Phase 1,
  so this is deliberately last; needs: ∆Hbw, ∆Q6L, ∆BfR, ∆RGQ, ∆5MM, ∆pXO

## Phase 2 — The static site generator: limulus.net from a Lemmon port

Exit criterion (architecture doc 3, D15): `lem build` over a Lemmon-ported limulus-dot-net
produces a `dist/` that the parity harness reports as zero unexpected diffs against the 11ty
build, and limulus.net cuts over with its CloudFront config untouched (D13). The shell is
built here (`@limulus/lemmon-shell`, docs/architecture.md §Build pipeline, §Shell plugin
API); the port itself lives in limulus-dot-net on a branch while 11ty stays green on main.

### Shell: `lem build`

- ∆4tG Build pipeline + CLI — `lem build` walks the input, classifies files, parses front
  matter with YAML positions, loads `lemmon.config.ts`, and drives the core across a worker
  pool: compile `ambient.lem` + templates, match documents to the reported `from` globs and
  compile them with their record (D30), loadData, plan, render. `lem check` prints spanned
  diagnostics without writing. The backbone every other shell item plugs into
- ∆7Xs Output writer — HTML to disk, per-page hashed CSS bundles collected from `lem-emit
  styles` (`name.<8hex>.ext` under `/assets/immutable/…`, D13/D20), passthrough copy by
  glob (limulus.net copies `assets/**` plus png/svg/jpg/xsd/xsl/txt/pdf), and a link/asset
  integrity check. Preserving the hash conventions is what keeps CloudFront untouched at
  cutover; needs: ∆4tG, ∆RGQ
- ∆d1N esbuild asset pipeline — bundle `**/index.ts` + `**/worker.ts` with `/lib/*`
  external and the wasm loader (D09), resolve the `XXXXXXXX` hash placeholder in client TS
  (D10), and hand the manifest to `loadData` before compile so `asset()` is compile-checked,
  the entries it checked against reported as compile-time facts (D25). Ports limulus-dot-net's
  `11ty/esbuild.ts` 1:1; needs: ∆4tG, ∆Q6L
- ∆PgF Plugin host — the `Plugin` hooks (data, transformDocument, assets, transformOutput,
  emit, commands, watch), plugin `.lem` bound via `lem-import` in `site.lem` (D23, D06), and
  the content API (`loadSite(config).collection('articles')`) for out-of-build tools.
  Everything site-specific in limulus.net is a plugin (D12), so this gates the port;
  needs: ∆4tG
- ∆Vhi `lem dev` — dev server with COOP/COEP headers and live reload, incremental rebuilds
  from render-reported deps and `upsertRecord` (docs/architecture.md §Build pipeline),
  `--drafts` (D19). Replaces `eleventy --serve --incremental`, the daily authoring loop;
  needs: ∆4tG, ∆RGQ
- ∆V7i `lem init` — scaffold a working blog as plain site code: `site.lem` with a `Post
  extends Page` schema, a `posts` glob binding, a `layout` default, and base/post layouts,
  plus an `index.lem` page. The core has no content conventions (D30), so this is where
  a new site gets them — readable and editable, never built in; needs: ∆4tG

### limulus.net plugins (D12)

- ∆F9l svg-sprite + dprint plugins — inline sprite from `assets/svg/{common,photos}` as an
  `emit`/data hook replacing `eleventy-plugin-svg-sprite`, and the dprint-markup
  `transformOutput` (lineWidth 92) from `11ty/dprint.ts`, so formatted HTML diffs cleanly
  in the parity harness; needs: ∆PgF
- ∆pP5 Photos plugin — `photos.json` as typed data with alias resolution (today's
  `getPhoto` shortcode), and `lem photos add` wrapping `bin/process-photos.js` (resize,
  blurhash, upload); needs: ∆PgF
- ∆shS Revisions plugin — `revisions.json` as data feeding `lastModified`, plus the
  `update-revisions` command reimplemented on the content API instead of spinning up
  Eleventy (`11ty/get-articles.ts`); needs: ∆PgF

### The port (limulus-dot-net, on a branch)

- ∆rT5 Schemas, layouts, components — `site.lem` types for articles, link posts, TILs,
  photos, vod, authors; the base/page/article/video layouts and ~20 WebC components
  rewritten as `.lem` functions with `lem-style` scoping (D20). First real exercise of the
  language at scale; expect it to feed fixes back into Phase 1 items; needs: ∆iCW, ∆Q6L,
  ∆RGQ
- ∆o24 Content + markdown parity — typed front matter for every `.md` (globs replace `tags`,
  schema defaults replace `links.json`), computed defaults replacing `eleventyComputed.js`
  (title from linkTitle, tilTopic from path, link/via domains, readingTime), the link-post
  reference appender as `transformDocument`, and comrak matched to markdown-it (footnote
  markup, task lists, attrs, heading anchors, highlight classes); needs: ∆Hbw, ∆rT5, ∆BfR
- ∆ePP Generated and non-HTML pages — TIL topic pages, links and photo pages via
  `lem-page` generators, the three feeds (rss + two atom, `/feed.xsl`), `sitemap.xml`
  with video entries, 404 — the last things still depending on 11ty plugins
  (`eleventy-plugin-rss`, pagination, `sitemap.njk`). A feed reads other pages' rendered
  bodies, which D30 left without a form (`body` is not a `Page` field); needs: ∆rT5, ∆RGQ

### Parity and cutover (D15)

- ∆hyz Parity harness — build old (11ty) and new (`lem`) dists, normalize, diff with an
  allowlist of expected differences, report per-page, as a local `npm run parity` in
  limulus-dot-net (not CI: diffs are nonzero until cutover and the tool dies with it). The
  acceptance bar for the whole initiative, so it lands before the port is "done", not
  after; needs: ∆7Xs, ∆d1N, ∆F9l, ∆pP5, ∆shS, ∆o24, ∆ePP
- ∆oAl Cutover — at zero unexpected diffs, replace Eleventy with `lem` in limulus-dot-net
  (`build`, `dev`, `verify` scripts, CI, `11ty/` removed), single switch, no dual-serving,
  CloudFront config unchanged; needs: ∆hyz, ∆Vhi

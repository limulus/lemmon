# Backlog

This backlog is managed with the **delto** skill — before adding or changing items, consult
it (and the authoring reference it points to) so the conventions below stay enforced.

Organized as Initiative (`##`) → Epic (`###`) → Item (`-`); initiatives are roughly
priority-ordered, items 5 lines max.

Each item starts with a “deltoid” — `∆` followed by 3 alphanumerics, e.g. `∆7hy`. Deltoids
are immutable and travel with the item into its completed-work journal entry in
`docs/journal/`. To mint a collision-free deltoid, run
`npx @limulus/delto@1 mint --journal-dir docs/journal` (see `delto mint --help`).

Hard prerequisites use a trailing `; needs: ∆aaa[, ∆bbb]` suffix — the only dependency
mechanism: `delto surface` trusts these edges to decide what is eligible to work on.

## Phase 0 — Spike: one real page from the WASM core

Exit criterion (architecture doc 3): limulus.net's `the-rebound` post renders through a
minimal base→page→article layout chain from the WASM core, byte-similar to the 11ty output,
with VM render time under 1 ms/page. Everything here serves that demo.

### Core spike

- ∆Ldn Document mode — fence-aware segmenter, in-core comrak rendering of joined prose,
  `<!--lem:N-->` sentinel splice with lost-sentinel build errors, untyped front matter
  passthrough for now (docs/language.md §Document mode)
- ∆TNT WASM boundary — wasm-bindgen build of the core, `@limulus/lemmon-core` typed wrapper
  (compile/loadData/render), and a small Node harness that renders fixtures, measuring
  boundary marshalling cost so the few-large-calls design is validated early

### Prove it

- ∆xWs Render `the-rebound` end to end — port minimal base/page/article layouts to `.lem`,
  render the post (document mode included) through the Node harness, diff against the
  11ty-built HTML, and record VM + boundary timings against the <1 ms/page exit criterion;
  needs: ∆Ldn, ∆TNT

## Phase 1 — Language complete, tooling from day one

Seed items for the next phase (architecture doc 3, phase 1); expect these to be split
further at claim time.

- ∆C8d Type checker + schemas — `lem-type`/`lem-enum`, reference fields (`by`), computed
  defaults, narrowing, project-flat + ambient namespaces (docs/language.md §Types, §Names),
  diagnostics with primary/secondary spans; turns limulus.net's build-time throws into
  compile errors
- ∆Q6L Stdlib — strings/lists/maps, `datetime("…")` (compile-time checked on a literal
  argument; there is no datetime literal) + rfc3339/rfc822 formatting, `slugify`,
  `readingTime`, `url()`, `asset()`, `raw()`, `uniqueId()`, with typed signatures the
  checker understands, since feeds/sitemap/layout ports all block on these; needs: ∆C8d
- ∆Hbw Document mode, full rules — nested `lem-let` capture bodies as sub-documents, typed
  front matter validated at YAML positions, string-literal reference resolution in prose
  calls (docs/language.md §Document mode items 2 and 6); needs: ∆Ldn, ∆C8d
- ∆RGQ Side channels + generators — `lem-style`/`lem-script`/`lem-head`/`lem-emit`
  collection and dedup, `lem-page` (including `for` generators with prev/next), serializable
  modules and render-reported deps for incremental dev; needs: ∆C8d
- ∆5MM tree-sitter-lemmon + LSP skeleton — embedded-template grammar with HTML/markdown
  injections, corpus agreement test wired into CI, `@limulus/lemmon-lsp` serving diagnostics
  and schema completions from `core.analyze()`; editor tooling is a day-one commitment
  (docs/decisions.md D16); needs: ∆TEJ, ∆C8d
- ∆pXO Lemmon authoring skill — a checked-in agent skill (`agents/skills/lemmon/`) for
  writing `.lem`: tag forms and head-shape rules, expression syntax, type-directed escaping,
  document mode — distilled from docs/language.md, not a copy of it. Porting limulus.net's
  layouts means writing a lot of Lemmon by agent, and rereading the whole language doc every
  time is neither reliable nor cheap.

## Phase 2 — The static site generator: limulus.net from a Lemmon port

Exit criterion (architecture doc 3, D15): `lem build` over a Lemmon-ported limulus-dot-net
produces a `dist/` that the parity harness reports as zero unexpected diffs against the 11ty
build, and limulus.net cuts over with its CloudFront config untouched (D13). The shell is
built here (`@limulus/lemmon-shell`, docs/architecture.md §Build pipeline, §Shell plugin
API); the port itself lives in limulus-dot-net on a branch while 11ty stays green on main.

### Shell: `lem build`

- ∆4tG Build pipeline + CLI — `lem build` walks the input, classifies files
  (document/template/data/passthrough), parses front matter with YAML positions, loads
  `lemmon.config.ts`, and drives the core (compile → loadData → plan → render) across a
  worker pool, one WASM instance per worker; `lem check` prints spanned diagnostics without
  writing. The backbone every other shell item plugs into; needs: ∆TNT
- ∆7Xs Output writer — HTML to disk, per-page hashed CSS bundles collected from `lem-emit
  styles` (`name.<8hex>.ext` under `/assets/immutable/…`, D13/D20), passthrough copy by
  glob (limulus.net copies `assets/**` plus png/svg/jpg/xsd/xsl/txt/pdf), and a link/asset
  integrity check. Preserving the hash conventions is what keeps CloudFront untouched at
  cutover; needs: ∆4tG, ∆RGQ
- ∆d1N esbuild asset pipeline — bundle `**/index.ts` + `**/worker.ts` with `/lib/*`
  external and the wasm loader (D09), resolve the `XXXXXXXX` hash placeholder in client TS
  (D10), and hand the manifest to `loadData` before compile so `asset()` is compile-checked.
  Ports limulus-dot-net's `11ty/esbuild.ts` 1:1; needs: ∆4tG, ∆Q6L
- ∆PgF Plugin host — the `Plugin` hooks (data, transformDocument, assets, transformOutput,
  emit, commands, watch), plugin `.lem` bound via `lem-import` in `site.lem` (D23, D06), and
  the content API (`loadSite(config).collection('articles')`) for out-of-build tools.
  Everything site-specific in limulus.net is a plugin (D12), so this gates the port;
  needs: ∆4tG
- ∆Vhi `lem dev` — dev server with COOP/COEP headers and live reload, incremental rebuilds
  from render-reported deps and `upsertRecord` (docs/architecture.md §Build pipeline),
  `--drafts` (D19). Replaces `eleventy --serve --incremental`, the daily authoring loop;
  needs: ∆4tG, ∆RGQ

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
  language at scale; expect it to feed fixes back into Phase 1 items; needs: ∆C8d, ∆Q6L,
  ∆RGQ
- ∆o24 Content + markdown parity — typed front matter for every `.md`, computed defaults
  replacing `eleventyComputed.js` (title from linkTitle, tilTopic/tags from path, link/via
  domains, readingTime), link-post reference appender as `transformDocument`, and comrak
  configured to match markdown-it output (footnote section markup, task lists, attrs,
  heading anchors, highlight classes); needs: ∆Hbw, ∆rT5
- ∆ePP Generated and non-HTML pages — TIL topic pages, links and photo pages via
  `lem-page` generators, the three feeds (rss + two atom, `/feed.xsl`), `sitemap.xml`
  with video entries, 404 — the last things still depending on 11ty plugins
  (`eleventy-plugin-rss`, pagination, `sitemap.njk`); needs: ∆rT5, ∆RGQ

### Parity and cutover (D15)

- ∆hyz Parity harness — build old (11ty) and new (`lem`) dists, normalize, diff with an
  allowlist of expected differences, report per-page, as a local `npm run parity` in
  limulus-dot-net (not CI: diffs are nonzero until cutover and the tool dies with it). The
  acceptance bar for the whole initiative, so it lands before the port is "done", not
  after; needs: ∆7Xs, ∆d1N, ∆F9l, ∆pP5, ∆shS, ∆o24, ∆ePP
- ∆oAl Cutover — at zero unexpected diffs, replace Eleventy with `lem` in limulus-dot-net
  (`build`, `dev`, `verify` scripts, CI, `11ty/` removed), single switch, no dual-serving,
  CloudFront config unchanged; needs: ∆hyz, ∆Vhi

# Backlog

This backlog is managed with the **delto** skill — before adding or changing items, consult
it (and the authoring reference it points to) so the conventions below stay enforced.

Organized as Initiative (`##`) → Epic (`###`) → Item (`-`); initiatives are roughly
priority-ordered, items 5 lines max. One standing epic, Refactors, precedes the initiatives,
and two standing sections follow them: Defects, then Someday/Maybe.

Each item starts with a “deltoid” — `∆` followed by 3 alphanumerics, e.g. `∆7hy`. Deltoids
are immutable and travel with the item into its completed-work journal entry in
`docs/journal/`. To mint a collision-free deltoid, run
`npx @limulus/delto@1 mint --journal-dir docs/journal` (see `delto mint --help`).

Hard prerequisites use a trailing `; needs: ∆aaa[, ∆bbb]` suffix — the only dependency
mechanism: `delto surface` trusts these edges to decide what is eligible to work on.

### Refactors

Standing epic for reshaping working code. It comes first so that document order gives a
refactor priority: reshaping is cheapest before more code is built on what it reshapes.

- ∆W1D Gate the spike and the benchmark's fixtures — `spike:the-rebound` and `wasm:bench`
  consume the boundary and the corpus layout yet run in no gate (∆xWs kept them out), and
  each has since broken unseen: a retired diagnostics contract, then a corpus case ∆Bhz made
  a deliberate error. Give each a mode without timing loops — one pass, its diagnostics and
  rendered-HTML checks — run from `verify:js`, which already needs the staged artifact.

## Phase 1 — Language complete, tooling from day one

Seed items for the next phase (architecture doc 3, phase 1); expect these to be split
further at claim time.

- ∆99b Generators and neighbours — a page template whose `for:` key names a field and an
  `<ambient value>.<field>` list becomes one page per item at load, its url from its type's
  default; the loader fills the `prev`/`next` fields a generated page's type declares, in
  list order, as references into the page table (D43). limulus.net's photo and TIL topic
  pages (∆ePP) are generators; needs: ∆Bhz
- ∆xBO Render-reported deps — `render` reports which units and records a page read, so
  `lem dev` (∆Vhi) re-renders only a change's dependents; answers D25's open question of
  whether a new photo id re-renders through a render-time map read or a load-time `by`
  resolution
- ∆5MM tree-sitter-lemmon + LSP skeleton — embedded-template grammar with HTML/markdown
  injections, corpus agreement test wired into CI, `@limulus/lemmon-lsp` serving diagnostics
  and schema completions from `core.analyze()`; editor tooling is a day-one commitment
  (docs/decisions/README.md D16)
- ∆pXO Lemmon authoring skill — a checked-in agent skill (`agents/skills/lemmon/`) for
  writing `.lem`: tag forms and head-shape rules, expression syntax, type-directed escaping,
  document mode — distilled from docs/language.md, not a copy of it. Porting limulus.net's
  layouts means writing a lot of Lemmon by agent, and rereading the whole language doc every
  time is neither reliable nor cheap
- ∆Qr2 Facts and modules across the boundary — `compile` reports each unit's compile-time
  facts (signatures, ambient bindings, manifest entries, claims, a document's record) so
  ∆4tG keys its unit cache on `hash(text) + hash(facts)` and a content edit never
  recompiles its layouts (D25, D42), and `exportModule`/`importModule` serialize the linked
  program (D14). Deferred from ∆TNT because a fact nothing consumes is a comment
- ∆MlD Reconcile the architecture artifacts — apply every entry in
  `docs/artifact-changes.md` to the three Claude Artifacts Eric owns ("Shell & Core", "The
  Lemmon Language", "From Eleventy to Lemmon") and clear the ledger. The artifacts are the
  rationale record; changes are logged per pass and applied once, at the end of Phase 1,
  so this is deliberately last; needs: ∆Q6L, ∆BfR, ∆1Hp, ∆7wK, ∆Bhz, ∆99b, ∆Qr2, ∆xBO, ∆5MM,
  ∆pXO

## Phase 2 — The static site generator: limulus.net from a Lemmon port

Exit criterion (architecture doc 3, D15): `lem build` over a Lemmon-ported limulus-dot-net
produces a `dist/` that the parity harness reports as zero unexpected diffs against the 11ty
build, and limulus.net cuts over with its CloudFront config untouched (D13). The shell is
built here (`@limulus/lemmon-shell`, docs/architecture.md §Build pipeline, §Shell plugin
API); the port itself lives in limulus-dot-net on a branch while 11ty stays green on main.

### Shell: `lem build`

- ∆4tG Build pipeline + CLI — `lem build` walks the input, classifies files, parses front
  matter with YAML positions, loads `lemmon.config.ts`, and drives the core across a worker
  pool: compile `ambient.lem` + templates without front matter, match documents and page
  templates to the reported `from` globs and compile them with their record (D30, D41),
  loadData, plan, render; `lem check` prints spanned diagnostics. The shell's backbone;
  needs: ∆Bhz
- ∆7Xs Output writer — HTML to disk with marks resolved, the `styles` producer writing
  per-page hashed CSS bundles of what `lem-style` collects under `/assets/immutable/…`
  (today ten characters of a sha256; `name.<8hex>.ext` is esbuild's and passthrough's
  convention, D13/D20), passthrough copy by glob (`assets/**` plus png/svg/jpg/xsd/xsl/
  txt/pdf), and a link/asset integrity check, CloudFront untouched; needs: ∆4tG, ∆1Hp
- ∆d1N esbuild asset pipeline — bundle `**/index.ts` + `**/worker.ts` with `/lib/*` external
  and the wasm loader (D09), resolve the `XXXXXXXX` placeholder in client TS (D10), and hand
  the manifest to the host declaration at `compile`, its entries reported as compile-time
  facts (D25, D38). Not `11ty/esbuild.ts` 1:1: values are finished URLs, not the bare hashes
  it stores, since the core does no path arithmetic; needs: ∆4tG, ∆Q6L
- ∆PgF Plugin host — the `Plugin` hooks (data, transformDocument, assets, transformOutput,
  emit, commands, watch, and `marks`: producers with keys and a payload record, decorators,
  two producers on one key a load error, the shell's `styles`/`scripts`/`head` bundler a
  default plugin the site lists — D42), plugin `.lem` bound via `lem-import` in `site.lem`
  (D23, D06), and a content API for out-of-build tools. Gates the port (D12); needs: ∆4tG
- ∆Vhi `lem dev` — dev server with COOP/COEP headers and live reload, incremental rebuilds
  from render-reported deps and `upsertRecord` (docs/architecture.md §Build pipeline),
  `--drafts` (D19). Replaces `eleventy --serve --incremental`, the daily authoring loop;
  needs: ∆4tG, ∆xBO
- ∆V7i `lem init` — scaffold a working blog as plain site code: `site.lem` with a `Post
  extends Page` schema, a `posts` glob binding, a `layout` default, and base/post layouts,
  plus an `index.lem` page. The core has no content conventions (D30), so this is where
  a new site gets them — readable and editable, never built in; needs: ∆4tG
- ∆Nxd Default highlighter plugin — highlight.js as a `code` producer shipped with the
  shell and listed by `lem init`, claiming its languages, so a new site highlights out of
  the box and limulus.net's `<pre><code class="hljs">` markup is reproduced exactly (D42);
  `lem theme` turning a highlight.js or `.tmTheme` theme into CSS. A syntect-in-wasm
  producer is a plugin a site adds, paying its 2 MB only there; needs: ∆PgF, ∆7wK

### limulus.net plugins (D12)

- ∆F9l svg-sprite + dprint plugins — inline sprite from `assets/svg/{common,photos}` as an
  `emit`/data hook replacing `eleventy-plugin-svg-sprite`, and the dprint-markup
  `transformOutput` (lineWidth 92) from `11ty/dprint.ts`, run after ∆1Hp's side-channel
  splice, so formatted HTML diffs cleanly in the parity harness; needs: ∆PgF
- ∆pP5 Photos plugin — `photos.json` as typed data with alias resolution (today's
  `getPhoto` shortcode), and `lem photos add` wrapping `bin/process-photos.js` (resize,
  blurhash, upload). A photo's URL is its stored `alias`, never `title.slugify()`: the
  aliases were minted by `@sindresorhus/slugify`, whose German rule spells `Dürer`
  `duerer` where the core's Latin fold spells it `durer`, so three live URLs move if a
  page recomputes one; needs: ∆PgF
- ∆shS Revisions plugin — `revisions.json` as data feeding `lastModified`, plus the
  `update-revisions` command reimplemented on the content API instead of spinning up
  Eleventy (`11ty/get-articles.ts`); needs: ∆PgF

### The port (limulus-dot-net, on a branch)

- ∆rT5 Schemas, layouts, components — `site.lem` types for articles, link posts, TILs,
  photos, vod, authors; the base/page/article/video layouts and ~20 WebC components
  rewritten as `.lem` functions with `lem-style` scoping (D20). First real exercise of the
  language at scale; expect it to feed fixes back into Phase 1 items; needs: ∆Q6L, ∆1Hp, ∆eag
- ∆o24 Content + markdown parity — typed front matter for every `.md` (globs replace `tags`,
  schema defaults replace `links.json`), computed defaults replacing `eleventyComputed.js`
  (title from linkTitle, tilTopic from path, link/via domains, readingTime), the link-post
  appender as `transformDocument`, markdown-it's footnotes, task lists, attrs, and anchors
  in comrak, and code by the highlight.js producer (∆Nxd); needs: ∆rT5, ∆Nxd, ∆PgF
- ∆ePP Page templates, generated and non-HTML pages — the index pages and 404 as page
  templates (D41), photo and TIL topic pages as generators (D43), three feeds (rss + two
  atom, `/feed.xsl`), `sitemap.xml` with video entries: the last pages on WebC and 11ty
  plugins (`eleventy-plugin-rss`, pagination, `sitemap.njk`). A feed renders in a second
  phase, reading other pages' finished output as a string source (D42); needs: ∆rT5, ∆99b

### Parity and cutover (D15)

- ∆hyz Parity harness — build old (11ty) and new (`lem`) dists, normalize, diff with an
  allowlist of expected differences, report per-page, as a local `npm run parity` in
  limulus-dot-net (not CI: diffs are nonzero until cutover and the tool dies with it). The
  acceptance bar for the whole initiative, so it lands before the port is "done", not
  after; needs: ∆7Xs, ∆d1N, ∆F9l, ∆pP5, ∆shS, ∆o24, ∆ePP
- ∆oAl Cutover — at zero unexpected diffs, replace Eleventy with `lem` in limulus-dot-net
  (`build`, `dev`, `verify` scripts, CI, `11ty/` removed), single switch, no dual-serving,
  CloudFront config unchanged; needs: ∆hyz, ∆Vhi

## Defects

Standing section for confirmed bugs in landed code that block nothing else. It sits last so
that document order stays priority order: an item here is picked up when the code it sits in
is next touched, or in the slack between initiative items, and no initiative item waits on
one. Each names where the bug shows — a corpus case or a journal entry — so it can be
reproduced cold.

- ∆3oX `??` with a mismatched default reports twice — the mismatch, then "cannot print a
  `T?`" — because `Compiler::check` falls back to the expected type on failure and the `??`
  lowering in `crates/lemmon-vm/src/compile/expr.rs` cannot tell that from a genuinely
  optional right side. Decide what type a refused `??` takes so one mistake costs one
  diagnostic (docs/language.md §Checking); see `render/errors/higher-order-errors.html`
- ∆ugN Long operator chains overflow the `.wasm` stack — `<lem.1 + 1 + …>`, or `.first()`
  chained a few thousand times, traps `compile` with no span, and the trapped instance fails
  every later call, so one such file stops a ∆4tG worker. No depth cap counts a chain, but
  the compiler's walk in `crates/lemmon-vm/src/compile/expr.rs` recurses per term, as does
  dropping rowan's tree. Decide: a length cap, or walks that do not recurse (found in ∆vnI)
- ∆aaw `readingTime` counts markup as words — `words()` in `crates/lemmon-vm/src/builtins/
  text.rs` counts a code block's text like prose and an entity's name as a word (its own
  test asserts `a &gt; b` is three). Decide whether code counts at all or by an estimate
  from line length; ∆hyz allowlists what then differs. (The `syn-` span breaks D40 added
  go with ∆7wK: a block's default content carries none.)

## Someday/Maybe

Standing section for work no initiative needs — ideas kept so they are not lost, usually
serving consumers beyond limulus.net. Nothing here is scheduled; promote an item into an
initiative before starting it.

- ∆SDN Pure wasm plugin values — revisit D06 ("plugins cannot inject logic") for a
  plugin-shipped wasm module with no imports, called by the core at render time, for what
  a template must compute on and a mark cannot carry (D42): locale formatting, plurals,
  collation. Output-producing extensions — fence renderers, Pikchr, TeX → MathML — are
  `code` producers instead and need nothing here; needs: ∆PgF, ∆7wK
- ∆5pk `lem` code blocks highlighted by the core's scanner — a `code` producer for `lem`
  built from `lemmon-syntax`'s real scanner and shipped with the default highlighter
  (∆Nxd), so no `.sublime-syntax` Lemmon grammar drifts from the parser and
  tree-sitter-lemmon (invariant 9); today a `lem` fence is a block no producer claims;
  needs: ∆Nxd
- ∆M5x Raw text in templates — a `lem-let x: string` block whose body is scanned as text
  to its close tag (a fourth scanner mode beside D29's fields mode), so a template can
  hand a multi-line source to a `code` or diagram producer as a fence does; over it,
  call-with-body sugar (`lem-with f(args)` for an `html` slot, `lem-raw f(args)` for a
  string one) lowering to capture-then-call, revising D22 for both (D42); needs: ∆7wK

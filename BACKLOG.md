# Backlog

This backlog is managed with the [delto](https://github.com/limulus/delto) skill. Consult
it before adding or changing items.

Two standing sections follow the initiatives, Defects and then Someday/Maybe, so that
document order stays priority order. An item planned by an earlier session names its plan
in `docs/plans/`.

## Refactors

Standing initiative for reshaping working code. It comes first so that document order
gives a refactor priority: reshaping is cheapest before more code is built on what it
reshapes.

## Phase 1 — Language complete, tooling from day one

Seed items for the next phase (architecture doc 3, phase 1); expect these to be split
further at claim time.

- `∆99b` Generators and neighbours
  - A page template whose `for:` key names a field and an `<ambient value>.<field>` list
    becomes one page per item at load, its url from its type's default (D43)
  - needs: `∆Bhz`
- `∆xBO` Render-reported deps
  - `render` reports which units and records a page read, so `lem dev` (`∆Vhi`) re-renders
    only a change's dependents
- `∆5MM` tree-sitter-lemmon + LSP skeleton
  - Embedded-template grammar, corpus agreement test wired into CI, `@limulus/lemmon-lsp`
    serving diagnostics and schema completions; editor tooling is a day-one commitment (D16)
- `∆pXO` Lemmon authoring skill
  - A checked-in agent skill for writing `.lem`, distilled from docs/language.md, not a copy
    of it: porting limulus.net's layouts means writing a lot of Lemmon by agent
- `∆Qr2` Facts and modules across the boundary
  - `compile` reports each unit's compile-time facts so `∆4tG` keys its unit cache on
    `hash(text) + hash(facts)` and a content edit never recompiles its layouts (D25, D42)
- `∆MlD` Reconcile the architecture artifacts
  - Apply every entry in `docs/artifact-changes.md` to the three Claude Artifacts Eric owns
    and clear the ledger; applied once, at the end of Phase 1, so this is deliberately last
  - needs: `∆Q6L`, `∆BfR`, `∆1Hp`, `∆7wK`, `∆Bhz`, `∆99b`, `∆Qr2`, `∆xBO`, `∆5MM`, `∆pXO`

## Phase 2 — The static site generator: limulus.net from a Lemmon port

Exit criterion (architecture doc 3, D15): `lem build` over a Lemmon-ported limulus-dot-net
produces a `dist/` that the parity harness reports as zero unexpected diffs against the 11ty
build, and limulus.net cuts over with its CloudFront config untouched (D13). The shell is
built here (`@limulus/lemmon-shell`, docs/architecture.md §Build pipeline, §Shell plugin
API); the port itself lives in limulus-dot-net on a branch while 11ty stays green on main.

### Shell: `lem build`

- `∆4tG` Build pipeline + CLI
  - `lem build` walks the input, loads `lemmon.config.ts`, and drives the core across a
    worker pool; `lem check` prints spanned diagnostics. The shell's backbone
  - needs: `∆Bhz`
- `∆7Xs` Output writer
  - HTML to disk with marks resolved, the `styles` producer writing per-page hashed CSS
    bundles, passthrough copy by glob, and a link/asset integrity check
  - needs: `∆4tG`, `∆1Hp`
- `∆d1N` esbuild asset pipeline
  - Bundle `**/index.ts` + `**/worker.ts` (D09), resolve the `XXXXXXXX` placeholder (D10),
    and hand the manifest to the host declaration at `compile` as compile-time facts
  - needs: `∆4tG`, `∆Q6L`
- `∆PgF` Plugin host
  - The `Plugin` hooks (D42), plugin `.lem` bound via `lem-import` in `site.lem` (D23, D06),
    and a content API for out-of-build tools. Gates the port (D12)
  - needs: `∆4tG`
- `∆Vhi` `lem dev`
  - Dev server with COOP/COEP headers, incremental rebuilds from render-reported deps.
    Replaces `eleventy --serve --incremental`, the daily authoring loop
  - needs: `∆4tG`, `∆xBO`
- `∆V7i` `lem init`
  - Scaffold a working blog as plain site code. The core has no content conventions (D30),
    so this is where a new site gets them — readable and editable, never built in
  - needs: `∆4tG`
- `∆Nxd` Default highlighter plugin
  - highlight.js as a `code` producer shipped with the shell, so a new site highlights out
    of the box and limulus.net's `<pre><code class="hljs">` markup is reproduced exactly
  - needs: `∆PgF`, `∆7wK`

### limulus.net plugins (D12)

- `∆F9l` svg-sprite + dprint plugins
  - Inline sprite replacing `eleventy-plugin-svg-sprite`, and the dprint-markup
    `transformOutput`, so formatted HTML diffs cleanly in the parity harness
  - needs: `∆PgF`
- `∆pP5` Photos plugin
  - `photos.json` as typed data with alias resolution, and `lem photos add` wrapping
    `bin/process-photos.js`. A photo's URL is its stored `alias`, never `title.slugify()`
  - needs: `∆PgF`
- `∆shS` Revisions plugin
  - `revisions.json` as data feeding `lastModified`, plus the `update-revisions` command
    reimplemented on the content API instead of spinning up Eleventy
  - needs: `∆PgF`

### The port (limulus-dot-net, on a branch)

- `∆rT5` Schemas, layouts, components
  - `site.lem` types and ~20 WebC components rewritten as `.lem` functions (D20). First real
    exercise of the language at scale; expect it to feed fixes back into Phase 1 items
  - needs: `∆Q6L`, `∆1Hp`, `∆eag`
- `∆o24` Content + markdown parity
  - Typed front matter for every `.md` (globs replace `tags`), computed defaults replacing
    `eleventyComputed.js`, the link-post appender as `transformDocument`
  - needs: `∆rT5`, `∆Nxd`, `∆PgF`
- `∆ePP` Page templates, generated and non-HTML pages
  - The index pages and 404 as page templates (D41), photo and TIL topic pages as generators
    (D43), three feeds, `sitemap.xml`: the last pages on WebC and 11ty plugins
  - needs: `∆rT5`, `∆99b`

### Parity and cutover (D15)

- `∆hyz` Parity harness
  - Build old (11ty) and new (`lem`) dists, diff with an allowlist, report per-page. The
    acceptance bar for the whole initiative, so it lands before the port is "done"
  - needs: `∆7Xs`, `∆d1N`, `∆F9l`, `∆pP5`, `∆shS`, `∆o24`, `∆ePP`
- `∆oAl` Cutover
  - At zero unexpected diffs, replace Eleventy with `lem` in limulus-dot-net, single switch,
    no dual-serving, CloudFront config unchanged
  - needs: `∆hyz`, `∆Vhi`

## Defects

Standing section for confirmed bugs in landed code that block nothing else. It sits last so
that document order stays priority order: an item here is picked up when the code it sits in
is next touched, or in the slack between initiative items, and no initiative item waits on
one. Each item or its brief names where the bug shows — a corpus case or a journal entry —
so it can be reproduced cold.

- `∆3oX` `??` with a mismatched default reports twice
  - The mismatch, then "cannot print a `T?`", because `Compiler::check` falls back to the
    expected type on failure; see `render/errors/higher-order-errors.html`
- `∆ugN` Long operator chains overflow the `.wasm` stack
  - `<lem.1 + 1 + …>`, or `.first()` chained a few thousand times, traps `compile` with no
    span, and the trapped instance fails every later call
- `∆aaw` `readingTime` counts markup as words
  - `words()` in `crates/lemmon-vm/src/builtins/text.rs` counts a code block's text like
    prose and an entity's name as a word (its own test asserts `a &gt; b` is three)
- `∆RuS` A builtin's arity mistake is worded twice
  - The compiler says `.slugify` takes 0 arguments and the VM says `.slugify()`; see
    `render/errors/method-errors.html` and `…/runtime-method-arity.html`
- `∆1gY` An ambient binding accepts a misspelled field
  - `AmbientBinding` in `crates/lemmon-wasm/src/wire.rs` is the one inbound shape without
    `deny_unknown_fields`, so `{"bindingsOf": "site.lem", "typo": 1}` is taken with no error

## Someday/Maybe

Standing section for work no initiative needs — ideas kept so they are not lost, usually
serving consumers beyond limulus.net. Nothing here is scheduled; promote an item into an
initiative before starting it.

- `∆SDN` Pure wasm plugin values
  - Revisit D06 ("plugins cannot inject logic") for a plugin-shipped wasm module with no
    imports, for what a template must compute on and a mark cannot carry (D42)
  - needs: `∆PgF`, `∆7wK`
- `∆5pk` `lem` code blocks highlighted by the core's scanner
  - A `code` producer for `lem` built from `lemmon-syntax`'s real scanner, so no
    `.sublime-syntax` Lemmon grammar drifts from the parser and tree-sitter-lemmon
  - needs: `∆Nxd`
- `∆M5x` Raw text in templates
  - A `lem-let x: string` block whose body is scanned as text to its close tag, so a
    template can hand a multi-line source to a `code` or diagram producer as a fence does
  - needs: `∆7wK`

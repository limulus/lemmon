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

## limulus.net on Lemmon

Exit criterion (architecture doc 3, D15): `lem build` over a Lemmon-ported limulus-dot-net
produces a `dist/` that the parity harness reports as zero unexpected diffs against the 11ty
build, and limulus.net cuts over with its CloudFront config untouched (D13). The work is cut
into slices, each an epic that ends in software Eric runs: the last sentence of its
description is the acceptance test, and nothing from the next slice starts before it is
true. The shell is built here (`@limulus/lemmon-shell`, docs/architecture.md §Build
pipeline, §Shell plugin API); the port lives in limulus-dot-net on a branch while 11ty stays
green on main, and that branch is where every slice is accepted. A language or core feature
lands inside the slice that needs it.
Slice 1, a page from `lem build`, was accepted on 2026-10-17; its five items' journal
entries record it.

### 2. Styled

Marks resolved in the shell and CSS as per-page bundles. Eric opens the seven posts and
sees them identical to the live site, with their CSS as per-page hashed bundles under
`/assets/immutable/`, and `npm run parity` reports zero unexpected differences for them.

- `∆D3R` `lem-style` in the ported layouts
  - The three CSS files and the components' scoped styles become `lem-style` blocks in the
    port, replacing the interim `<link>` to passthrough-copied CSS

### 3. The daily loop

The authoring loop Eleventy provides today, without incrementality: a change rebuilds
everything, and the rebuild time recorded here decides whether the Incrementality epic
opens. Eric runs `lem dev` in the branch, edits a post or a layout and sees the change in
the browser, sees `lem check` report a deliberate error with its span, and sees a draft
under `--drafts` that `lem build` omits.

- `∆Vhi` `lem dev`
  - Dev server with COOP/COEP headers and live reload; a change rebuilds everything, and the
    full rebuild time is recorded for the Incrementality epic. Replaces `eleventy --serve`
  - needs: `∆muD`
- `∆1rD` `lem check` and `--drafts`
  - `lem check` prints spanned diagnostics without writing; the `draft` field on `Page` and
    the `--drafts` flag (D19) show a draft in `lem dev` that `lem build` omits
  - needs: `∆muD`, `∆Vhi`

### 4. Every document page

The plugin host in full, and the plugins and types the remaining sections need. Eric builds
the branch and all 25 documents, code blocks highlighted and client JavaScript bundled to
hashed URLs, compare clean.

- `∆c4j` Plugin host
  - The `Plugin` hooks in full (data, transformDocument, assets, transformOutput, emit,
    commands, watch), plugin `.lem` via `lem-import` in `site.lem` (D23, D06), a content API
- `∆d1N` esbuild asset pipeline
  - Bundle `**/index.ts` + `**/worker.ts` (D09), resolve the `XXXXXXXX` placeholder (D10),
    and hand the manifest to the host declaration at `compile` as compile-time facts
  - needs: `∆muD`, `∆Q6L`
- `∆tAw` Remaining schemas and components
  - `site.lem` types for link posts, TILs and photo pages, and the components the first
    slice did not need, so every document page has its types
  - needs: `∆foi`
- `∆ITg` Computed defaults and the link-post appender
  - Typed front matter for links, penumbra and TILs, computed defaults replacing
    `eleventyComputed.js`, the link-post appender, markdown-it's extras in comrak
  - needs: `∆c4j`, `∆tAw`
- `∆Nxd` Default highlighter plugin
  - highlight.js as a `code` producer shipped with the shell, so a new site highlights out
    of the box and limulus.net's `<pre><code class="hljs">` markup is reproduced exactly
  - needs: `∆c4j`, `∆7wK`
- `∆shS` Revisions plugin
  - `revisions.json` as data feeding `lastModified`, plus the `update-revisions` command
    reimplemented on the content API instead of spinning up Eleventy
  - needs: `∆c4j`
- `∆IvY` svg-sprite plugin
  - Inline sprite from `assets/svg/{common,photos}` as an `emit`/data hook replacing
    `eleventy-plugin-svg-sprite`, run after the side-channel splice
  - needs: `∆c4j`
- `∆yOJ` Link and asset integrity check
  - The link and asset integrity check over `dist/`: links resolve to written pages or
    passthrough files, and `asset()` URLs to manifest entries, CloudFront untouched
  - needs: `∆d1N`, `∆muD`

### 5. Index and generated pages

Generators land in the core here, needed by exactly two pages; the seven collection-reading
pages are page templates. Eric opens `dist/` from `/` and reaches every page of the site,
each comparing clean.

- `∆99b` Generators and neighbours
  - A page template whose `for:` key names a field and an `<ambient value>.<field>` list
    becomes one page per item at load, its url from its type's default (D43)
  - needs: `∆Bhz`
- `∆LsH` Index pages as page templates
  - Home, the five section indexes, the author page and `404` as `.lem` page templates with
    front matter (D41) reading their collections through glob bindings
  - needs: `∆tAw`
- `∆pP5` Photos plugin
  - `photos.json` as typed data with alias resolution, and `lem photos add` wrapping
    `bin/process-photos.js`. A photo's URL is its stored `alias`, never `title.slugify()`
  - needs: `∆c4j`
- `∆hvm` Photo and TIL topic pages as generators
  - `photos/photo` and `tils/topics`, the two paginated pages, as generator templates whose
    `for:` key names a list (D43), with `prev` and `next` filled by the loader
  - needs: `∆99b`, `∆pP5`, `∆tAw`

### 6. Feeds, sitemap, cutover

The pages that read other pages' finished output, the whole-site parity run, and the switch.
Eric sees `npm run parity` report zero unexpected differences over the whole `dist/`, and
limulus.net builds and deploys with `lem` with its CloudFront config untouched.

- `∆zu0` Feeds and sitemap in a second phase
  - The three feeds (rss, two atom, `/feed.xsl`) and `sitemap.xml` with video entries as
    templates rendered after every other page, reading finished output as a string source
  - needs: `∆LsH`, `∆hvm`
- `∆Zg9` Whole-site parity
  - `npm run parity` over the whole `dist/` against the 11ty build, every difference in a
    forgiveness class with its count, at zero unexpected differences; the acceptance bar
  - needs: `∆zu0`, `∆Zo7`
- `∆oAl` Cutover
  - At zero unexpected diffs, replace Eleventy with `lem` in limulus-dot-net, single switch,
    no dual-serving, CloudFront config unchanged
  - needs: `∆Zg9`, `∆Vhi`

## After the cutover

Work the port does not need, in priority order. Each epic holds one or two items today.

### A second site

`lem init`'s scaffold is the second site, which keeps the shell honest about limulus.net's
specifics (D17).

- `∆V7i` `lem init`
  - Scaffold a working blog as plain site code. The core has no content conventions (D30),
    so this is where a new site gets them — readable and editable, never built in
  - needs: `∆muD`

### Editor tooling

Independent of every slice and usable on its own, so it may run in a parallel session at any
point; this is only its default place.

- `∆5MM` tree-sitter-lemmon + LSP skeleton
  - Embedded-template grammar, corpus agreement test wired into CI, `@limulus/lemmon-lsp`
    serving diagnostics and schema completions; editor tooling is a day-one commitment (D16)

### Incrementality

Opens only if the full rebuild time recorded in the daily-loop slice exceeds one second;
until then, rebuilding the whole site is the incremental build.

- `∆xBO` Render-reported deps
  - `render` reports which units and records a page read, so `lem dev` (`∆Vhi`) re-renders
    only a change's dependents
- `∆Qr2` Compile-time facts across the boundary
  - `compile` reports each unit's compile-time facts so `∆muD` keys its unit cache on
    `hash(text) + hash(facts)` and a content edit never recompiles its layouts (D25, D42)

### Reconcile the architecture artifacts

- `∆MlD` Reconcile the architecture artifacts
  - Apply every entry in `docs/artifact-changes.md` to the three Claude Artifacts Eric owns
    and clear the ledger; applied once, after the cutover, so this is deliberately last
  - needs: `∆oAl`, `∆V7i`, `∆5MM`, `∆xBO`, `∆Qr2`

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
- `∆bah` `docs/language.md` describes three forms the compiler lacks
  - String interpolation, `lem-import` and `concat` read as working there, with no "not yet"
    note; the canonical example writes `[md]` and `size: small`, which do not compile

## Someday/Maybe

Standing section for work no initiative needs — ideas kept so they are not lost, usually
serving consumers beyond limulus.net. Nothing here is scheduled; promote an item into an
initiative before starting it.

- `∆SDN` Pure wasm plugin values
  - Revisit D06 ("plugins cannot inject logic") for a plugin-shipped wasm module with no
    imports, for what a template must compute on and a mark cannot carry (D42)
  - needs: `∆c4j`, `∆7wK`
- `∆5pk` `lem` code blocks highlighted by the core's scanner
  - A `code` producer for `lem` built from `lemmon-syntax`'s real scanner, so no
    `.sublime-syntax` Lemmon grammar drifts from the parser and tree-sitter-lemmon
  - needs: `∆Nxd`
- `∆M5x` Raw text in templates
  - A `lem-let x: string` block whose body is scanned as text to its close tag, so a
    template can hand a multi-line source to a `code` or diagram producer as a fence does
  - needs: `∆7wK`
- `∆8Kj` A comment form for `.lem`
  - There is no compile-time comment, so the port could not say why a sort key is negated
    or a capture keeps a `pre` on one line; HTML comments ship to the output
- `∆qLe` Refusing a render on purpose
  - A layout only a subtype can use cannot refuse the base record: D32 forbids
    `layout: (VideoPost, html)`, and a `lem-default` arm has no way to fail the render
- `∆CxX` Report a computed mark key no producer covers
  - A key read from the record at render reaches no handler unless one lists it, so a
    mistyped computed key keeps its default content and nothing says so (D42)
- `∆wOb` Formatted HTML for limulus.net
  - Eleventy's dprint transform formats 28 of its 55 pages and fails on the rest; after the
    cutover none would be formatted unless the site adds a `transformOutput` plugin
  - needs: `∆c4j`
- `∆v6d` Serialize the linked program
  - `exportModule`/`importModule` serialize the linked program (D14 headroom), deferred from
    `∆TNT` because a fact nothing consumes is a comment

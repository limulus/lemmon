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
Slices 1 to 3, a page from `lem build`, styled output, and the daily loop of `lem dev` and
`lem check`, were accepted on 2026-10-17, slices 4 and 5, every document page and then the
index and generated pages, on 2026-10-18, and slice 6, the feeds, the sitemap and the
cutover prepared on a local branch, on 2026-11-07; their items' journal entries record
them, `∆Oyi`'s records why dprint left slice 2, and `∆H7Q`'s records the last parity run.

### 7. Live on Lemmon

What Eric checks before the merge, the S3 and CDK dry runs, the logo on a P3 and an sRGB
display, and the merge commit's subject, is listed in `∆oAl`'s journal entry.
Eric merges the cutover branch and limulus.net deploys with `lem`, its CloudFront config
untouched.

- `∆PiI` Publish the core and shell to npm
  - Both ship built code, the core with its `.wasm` and the shell's `lem` as JavaScript,
    released from lemmon's CI by `semantic-release` through trusted publishing
- `∆FnH` limulus.net's CI gets lemmon
  - The site installs the published `@limulus/lemmon-shell` in place of `file:../lemmon`,
    which a CI checkout cannot resolve
  - needs: `∆PiI`

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
until then, rebuilding the whole site is the incremental build. Recorded on 2026-10-17: a
median of 250 ms for the port's 25 documents (`∆Vhi`'s journal), and 375 ms once slice 4's
esbuild plugin added its 200 ms (`∆d1N`'s journal); on 2026-10-18, 421 ms for all 55 pages
(`∆hvm`'s journal). Worth taking again with the feeds at the whole-site run.

- `∆xBO` Render-reported deps
  - `render` reports which units and records a page read, so `lem dev` (`∆Vhi`) re-renders
    only a change's dependents
- `∆Qr2` Compile-time facts across the boundary
  - `compile` reports each unit's compile-time facts so `∆muD` keys its unit cache on
    `hash(text) + hash(facts)` and a content edit never recompiles its layouts (D25, D42)

### Large sites

What a site much larger than limulus.net needs from the build.

- `∆mRe` Builds that hold no site in memory
  - Each page's links and ids are taken as it renders and the page written to a temporary
    directory swapped in at the end, so memory no longer grows with the site's output

### Retire the architecture artifacts

- `∆JXK` Retire the architecture artifacts
  - The repo docs take in what only the three private Claude Artifacts hold, D01–D24's
    rationale, the diagrams and the migration plan, so that the docs are the whole record

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
- `∆Gsn` An asset's did-you-mean points at the wrong script
  - `scripts: ./lib/index.js` is offered `/lib/rum/index.js`, the nearest key by edit
    distance, not the page's own `…/<slug>/lib/index.js`; see `∆d1N`'s journal entry
- `∆MX8` Trap specs ride V8's frame size
  - The operator-chain trap in `trap.spec.ts` and `packages/core`'s `load.spec.ts` overflow
    the `.wasm`'s stack with about 1.3× room on arm64 before V8's runs out first instead

## Someday/Maybe

Standing section for work no initiative needs — ideas kept so they are not lost, usually
serving consumers beyond limulus.net. Nothing here is scheduled; promote an item into an
initiative before starting it.

- `∆wOn` Plugin data, commands and `.lem`
  - The `data` and `commands` hooks, and plugin `.lem` bound via `lem-import` in `site.lem`
    (D23, D06); parked with no consumer in the port, its qualified names a deferred question
- `∆x1B` `lem photos add`
  - A `lem` subcommand wrapping `bin/process-photos.js`, which `npm run photos` runs today
    without Eleventy; the first consumer of the `commands` hook
  - needs: `∆wOn`
- `∆Gg3` Content API
  - `loadSite(config)` for out-of-build tools: each document's loaded record, computed
    fields included, read back from the core; parked when `∆shS` needed only a script
- `∆SDN` Pure wasm plugin values
  - Revisit D06 ("plugins cannot inject logic") for a plugin-shipped wasm module with no
    imports, for what a template must compute on and a mark cannot carry (D42)
  - needs: `∆wOn`, `∆7wK`
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
  - needs: `∆fwn`
- `∆fwn` The `transformOutput` hook
  - A plugin rewrites each page's finished bytes before the write; built once for `∆Oyi`
    and parked on `parked/oyi-transform-output-hook` (`ba9ece6`), its only consumer `∆wOb`
- `∆HzB` The `watch` and `emit` hooks
  - A plugin's own change source for `lem dev`, and files it writes after render; in
    the architecture's plugin sketch, with no consumer in the port
- `∆qoe` Floats print as JavaScript writes them
  - `<lem.x>` prints a float with Rust's `{}`: `inf`, `-0`, and every digit of `1e21` or
    `1e-7`, where `Number#toString` writes `Infinity`, `0`, `1e+21` and `1e-7`
- `∆0uV` Output a build no longer writes is removed
  - Neither `lem build` nor `lem dev` deletes from the output, so a page whose item left a
    generator's list, or a moved document, stays served with stale links until cleaned
- `∆gsV` An intentionally unused parameter
  - A layout that ignores its record warns that the parameter is never used, so limulus.net's
    XML layout reads it for nothing; no spelling marks a parameter as unused on purpose
- `∆v6d` Serialize the linked program
  - `exportModule`/`importModule` serialize the linked program (D14 headroom), deferred from
    `∆TNT` because a fact nothing consumes is a comment
- `∆Han` Drafts
  - `draft: true` leaves a page out of the build and its collections unless `--drafts`
    shows it in `lem dev` (D19); limulus.net has no drafts today, so the port needs none
  - needs: `∆Vhi`

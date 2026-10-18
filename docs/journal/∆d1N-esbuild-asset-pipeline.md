---
id: ∆d1N
completed: 2026-10-17 23:47:52 -07:00
---

## Backlog item

> - `∆d1N` esbuild asset pipeline
>   - Bundle `**/index.ts` + `**/worker.ts` (D09), resolve the `XXXXXXXX` placeholder (D10),
>     and hand the manifest to the host declaration at `compile` as compile-time facts
>   - needs: `∆ETy`, `∆muD`, `∆Q6L`

## Planning

The item's brief, folded here. Bundle `**/index.ts` and `**/worker.ts` with `/lib/*`
external and the wasm loader (D09), resolve the `XXXXXXXX` placeholder in client TS (D10),
and hand the manifest to the host declaration at `compile`, its entries compile-time facts
(D25, D38) — not `11ty/esbuild.ts` 1:1, since the core takes finished URLs. Constraints:
limulus.net's module scripts carry `async`, which `bundler()`'s `scripts` producer does
not write, and load in three places (`/lib/common` in `head`, the vod script inside
`article-vod`, `/lib/rum` through an inline loader at the end of `body`), where `bundler()`
places every collected URL at every mark; and slice 2's `npm run parity:look` refused
`/lib/**` until this item. Scope: slice 4's acceptance, client JavaScript bundled to hashed
URLs, comparing clean.

The plan's review found that parity compares script URLs exactly, so the bundles had to be
byte-identical to Eleventy's: esbuild pinned to the port's 0.24.0, not the architecture's
0.28, every option that moves a hash reproduced, the manifest keyed by unhashed output URL
as `11ty/esbuild.ts` keys its map, and a fourth placement the brief missed — four penumbra
pages' own `scripts:`, written page-relative in front matter, which a flat manifest cannot
answer. Built in two halves in the slice-4 run of 2026-10-17, each by an Opus agent and
reviewed read-only by another: the shell's `esbuild(options?)` in an isolated lemmon
worktree, fast-forwarded onto main (`ab0e8ca`), and the port's wiring (`6454fa1`).

- `esbuild()` is a shell-shipped plugin on `∆ETy`'s `assets` hook (D45). D09's entries and
  `/lib/*` external are its defaults; the site passes what is its own — the asset
  directory and the names of the environment variables a script reads, defined from the
  build's `env`. The layouts write their own `<script async>` tags with `asset()` URLs, so
  `bundler()`'s `scripts` mark needed no option.
- The penumbra pages' front matter names the absolute path (`scripts:
  /penumbra/journal/<slug>/lib/index.js`, typed `asset?`), decided in planning rather than
  asked: front matter outside the revision hash, and Eleventy's output differs only in those
  four `src` attributes, now the absolute form of the same hashed URL.
- Eleventy's 18 unhashed stub `index.js` files are not written: nothing loads them. The
  whole-site run (`∆Zg9`) will see them as a file-set difference.

## Refinement

- A probe found esbuild's `[hash]` moved by its working directory alone, every byte of the
  bundle and its map unchanged, so the hook runs esbuild from the config's directory —
  `AssetsContext.root`, added to D44's context — through real paths, never
  `process.cwd()`. The hook at the port's own paths writes 37 files byte-identical to
  Eleventy's, which the review reproduced through a symlinked shell and oddly spelled paths.
- The review found the shell's package entry importing esbuild for every site, so esbuild
  and its wasm plugin now load inside the hook; relative placeholders recomputed where
  Eleventy splices the hash into the string as written, now spliced; an entry's CSS key
  skipping the collision check; and a placeholder naming nothing reported at the bundle
  rather than the source that wrote it, now at the source's line and column.
- esbuild's warnings go to stderr and its errors become one diagnostic at the config: D45
  names a `diagnostics` field on `Assets`, carrying esbuild's locations through `Io`, as
  the way forward.
- `npm run parity` now compares every file esbuild writes, wherever its entry sits, byte
  for byte, setting Eleventy's stubs aside only when each is exactly a re-export of its
  hashed sibling; `unbuilt-scripts` retired. `npm run parity:look` runs the scripts: masks
  cover only what is drawn by the clock (`pixel-clock canvas`) and the render-time HUDs, the
  sphere-shadow canvases and the vod player, and a page error fails a shot.
- limulus.net's blur-hash script has two races, live today — an element connected before
  its `<img>` is parsed throws, and a placeholder painted after its photo loaded stays
  behind it — so `parity:look` holds scripts until DOMContentLoaded and photos until every
  placeholder is painted, the same on both sides; `∆oAl`'s brief records them. Two more
  hazards Eleventy shares and the plugin keeps for parity are there too: `XXXXXXXX` is
  rewritten after hashing, so a worker-only edit ships its loader under an old immutable
  name, and the wasm worker's map names its module by absolute path.
- A `lem dev` rebuild now takes a median 373 ms, about 200 ms of it esbuild, under the
  Incrementality epic's one second; an incremental esbuild context would save about 55 ms.
  Old hashed bundles accumulate in the output, as all stale output does.
- `scripts: ./lib/index.js` gets the did-you-mean `/lib/rum/index.js`, the nearest key by
  edit distance rather than the page's own script: `∆Gsn`, in Defects.

## Retrospective

Probing what moves a hash before writing the plugin was the step that mattered: the
working-directory dependence would otherwise have surfaced as hashes differing by where
`lem` was started. "Wait for images" was the plan's guess for blur-hash, and instrumenting
the element showed two races instead; the stall had first looked like CPU contention.

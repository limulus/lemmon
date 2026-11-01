# limulus.net uses `@limulus/blur-hash`

## Value

limulus.net's photo placeholders stop racing their photos: an element connected before its
`<img>` is parsed no longer throws, and a placeholder painted after its photo has loaded no
longer stays behind it.

## Background

Both races are live on limulus.net today. They were found while building the esbuild asset
pipeline (`∆d1N`, whose journal entry records them), when instrumenting the element showed
why `npm run parity:look` stalled; `parity:look` holds scripts until DOMContentLoaded and
photos until every placeholder is painted, the same on both builds, to step around them.

Eric chose to fix them by pulling the element out of the site into its own public package,
`@limulus/blur-hash` in `~/Developer/blur-hash`, scaffolded with `@unallocate/create` on
2026-10-18: he did not want to introduce the testing infrastructure into limulus-dot-net
yet, and expects others may find the component useful. The package is built (1.0.0, done
by 2026-11-01); the site still carries its own copy in `www/lib/blur-hash/`.

## Scope and constraints

- It is part of slice 6 and lands on the `lemmon-cutover` branch, so that the live site
  switches to the package as part of the cutover (Eric, 2026-11-01).

## Implementation suggestions

- The package's element takes the markup `pho-picture.lem` writes, `<blur-hash
  blurhash="…">` around the `<img>`; check its attributes against the component's.
- `BlurHash.workerUrl` lets a site bundle `@limulus/blur-hash/worker` itself where its
  bundler does not handle `new Worker(new URL('./worker.js', import.meta.url))`, which
  esbuild does not. A site entry that re-exports it, pointed at with the
  `worker.XXXXXXXX.js` placeholder the site's other workers use, should need no change to
  Lemmon's `esbuild()` plugin.

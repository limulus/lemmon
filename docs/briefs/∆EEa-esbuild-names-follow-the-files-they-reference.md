# `esbuild()` names follow the files they reference

## Value

A fix to one of limulus.net's client workers reaches every reader on the next deploy, and the
Lemmon port keeps comparing its scripts with Eleventy's byte for byte once it takes that fix.

## Background

limulus-dot-net's `11ty/esbuild.ts` takes each bundle's hash from esbuild's output names,
fixed before it rewrites the `….XXXXXXXX.js` placeholders inside the bundles. A loader's
name therefore does not follow the hashed names it refers to: after a change to a worker
alone, a loader such as `/lib/common` keeps its file name while its contents name the new
worker. CloudFront serves `/*.????????.*` with a 365-day minimum TTL and an `immutable`
`Cache-Control`, and the deploy's `aws s3 sync` does not delete old files, so browsers and
edges that cached the loader keep running the old worker. The Lemmon port found this
reproducing the step (`∆d1N`); lemmon's `esbuild()` keeps the same order for parity
(D45: placeholders rewritten after hashing, the hash spliced into the string as written).

On 2026-10-18 Eric started an interactive session to fix the naming on limulus-dot-net's
main, so every file's name follows its final content, references included, and asked for
this item so lemmon's plugin follows. That session may also stop the sphere-shadow-wasm
worker's source map from naming its wasm module by absolute path, a question it puts to
Eric.

## Scope and constraints

- Waits on the limulus-dot-net change landing on main; take the exact naming rule from it
  (that session was asked to report it).
- `esbuild()`'s output must stay byte-identical to Eleventy's after the port branch merges
  main: the port's `npm run parity` compares every file esbuild writes.
- Revise D45, and the cutover brief's note on the hazard, to match.

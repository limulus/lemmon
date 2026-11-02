# `esbuild()` without `entryNames`

## Background

Found by a review of the slices on 2026-10-20. `esbuild()`'s `entryNames` option
(`packages/shell/src/lib/esbuild.ts`, D45) lets a site move its bundles away from their
sources. No site sets it: limulus.net passes only `assetNames` and `env`. Yet it drives the
plugin's most intricate code: the `hashed` flag, writing esbuild's own `[hash]` after the
placeholder and cutting it off before hashing, and the branch of `placed()` that rewrites
a placeholder as a relative URL when the bundle it names has moved.

Eric decided on 2026-10-24 to drop `entryNames` alone; `entryPoints` and `external` are
simple and stay.

(corrected on 2026-11-01: the hash cutting stays, now unconditional; only the `hashed`
flag that made it optional goes. Without esbuild's own `[hash]` after the placeholder,
two entries at one logical path, such as `index.ts` and `index.tsx` beside it, collide
inside esbuild, which names one output file, before the plugin's own check can refuse
them naming both sources. D45's alternatives already gave that reason.)

## Scope and constraints

- Bundles always ship beside their sources (`[dir]/[name].[hash]`); the code only a custom
  `entryNames` needs goes, with its specs, and D45 and `docs/shell.md` are revised.
- Byte-identical output for limulus.net, which `npm run parity` checks on `lemmon-port`.

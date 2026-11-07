---
id: ∆FbD
completed: 2026-11-06 22:44:50 -07:00
---

## Backlog item

> - `∆FbD` `esbuild()` without `entryNames`
>   - Bundles always ship beside their sources, so the moved-bundle placeholder rewriting
>     that only a custom `entryNames` needs goes (D45)

## Planning

The item's brief, folded here. Found by a review of the slices on 2026-10-20: `esbuild()`'s
`entryNames` option (D45) let a site move its bundles away from their sources, and no site
set it — limulus.net passes only `assetNames` and `env` — yet it drove the plugin's most
intricate code: a `hashed` flag, and a branch of `placed()` that rewrote a placeholder as a
relative URL when the bundle it named had moved. Eric decided on 2026-10-24 to drop
`entryNames` alone; `entryPoints` and `external` are simple and stay. Bundles always ship
beside their sources, `[dir]/[name].[hash]`, and limulus.net's output stays byte-identical.

The plan, checked against `9927735` on 2026-11-01, corrected the brief: it had said that
writing esbuild's own `[hash]` after the placeholder, and cutting it off before hashing,
went with the option. Probed with the shell's esbuild 0.24.0, two entries at one logical
path, `index.ts` beside `index.tsx`, collide inside esbuild without that hash, before the
plugin's own check can refuse them naming both sources. So the cut stays, now
unconditional, and only the `hashed` flag goes; the brief was corrected the same day, and
D45's alternatives already gave the reason.

The plan also found that nothing checked `esbuild()`'s options: a config runs with its types
stripped, so `entryNames` would have been ignored silently once removed. An option the
factory does not take is now refused when the config loads, generally rather than by a
message about `entryNames`, since nothing has been released. limulus.net's 14 placeholder
strings already took the splice arm, so its bundles do not change.

## Refinement

Built on 2026-11-01 as lemmon `2266899`, `refactor(shell)!: esbuild() without entryNames`.
It was no line simplification: `esbuild.ts` went from 367 lines to 366, code +21/−19, since
the comment on why esbuild's hash stays grew. What went was the moved-bundle branch and the
flag. D45 carries the revision in `∆EEa`'s form.

The diff's review found:

- `posix.join` closes up an empty segment, so `'/lib//b/index.XXXXXXXX.js'` and
  `'//lib/b/…'` resolved to an entry and now shipped as written: a different URL, or
  another host, with no error, where the removed branch had shipped the clean one. Such a
  placeholder now names no entry, refused by the existing error (Eleventy fails on these
  too), and D45's Consequence, which had claimed Eleventy splices them, was corrected.
- `highlighter()` refused no unknown option either, so `highlighter({ exlude: ['rust'] })`
  built silently. It landed as its own commit, `a850334`: one `takesOnly()` in
  `plugins.ts` for both factories, which also refuses options that are not an object,
  naming what they are, where `null` had thrown a `TypeError` naming no plugin.

A finding from planning landed as `68cb812`: a bundle was named by replacing every
`XXXXXXXX` in its placeholder name, so an entry under a directory spelled that way had the
directory renamed too; only the file name's placeholder is replaced now.

`npm run verify` passed before each commit. Eric accepted it on 2026-11-06. His
`npm run parity` on `lemmon-port`, which `∆GJ9` waits on, is still to run.

## Retrospective

The brief's how was wrong where it mattered, and probing esbuild before planning around the
claim caught it. Removing the option was what exposed the larger hole, that a stripped
config ignores any misspelt option, in both factories.

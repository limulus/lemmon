# `lem build` writes the-rebound to `dist/`

## Background

### From `∆4tG` Build pipeline + CLI

Build pipeline + CLI — `lem build` walks the input, classifies files, parses front matter
with YAML positions, loads `lemmon.config.ts`, and drives the core across a worker pool:
compile `ambient.lem` + templates without front matter, match documents and page templates
to the reported `from` globs and compile them with their record (D30, D41), loadData, plan,
render; `lem check` prints spanned diagnostics. The shell's backbone

### From `∆7Xs` Output writer

Output writer — HTML to disk with marks resolved, the `styles` producer writing per-page
hashed CSS bundles of what `lem-style` collects under `/assets/immutable/…` (today ten
characters of a sha256; `name.<8hex>.ext` is esbuild's and passthrough's convention,
D13/D20), passthrough copy by glob (`assets/**` plus png/svg/jpg/xsd/xsl/ txt/pdf), and a
link/asset integrity check, CloudFront untouched

## Scope and constraints

- Slice 1's acceptance, which this item serves: Eric runs `lem build` in the limulus-dot-net
  port branch, opens the eight `feed/` posts from `dist/` in a browser, and `npm run parity`
  there reports them against the 11ty `dist/` with every difference in a named forgiveness
  class.
- The first milestone inside slice 1: `lem build` over a directory holding the-rebound and
  its layouts produces the page `test:fidelity` renders, then widens to the eight posts as
  `∆foi` lands them.
- The host declares `site`, `page` and `build`, `document: page`, `entry: layout`, `url:
  url`. `HostDecl.marks` and `assets` are omitted, so marks keep their default content and
  no `asset()` resolves; both arrive with later slices.
- HTML and passthrough copy only. The `styles` producer is `∆F4y`; the integrity check is
  `∆yOJ`.

## Implementation suggestions

- `loadCore()` finds the staged core through the nearest `artifacts/wasm/` above the
  package, resolved from `import.meta.url`, so a `file:` dependency from the branch needs no
  copy; `loadCore(dir)` takes an explicit directory if that ever moves.

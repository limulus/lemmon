---
id: ∆x8D
completed: 2026-10-15 20:16:02 -07:00
---

## Backlog item

> - `∆x8D` Shell skeleton: walk, front matter, config
>   - `@limulus/lemmon-shell` with a `lem` bin: walk the input, classify files, parse front
>     matter with YAML positions into spans, and read a minimal `lemmon.config.ts`

## Planning

The item's brief, folded here. From `∆4tG` Build pipeline + CLI: `lem build` walks the
input, classifies files, parses front matter with YAML positions, loads `lemmon.config.ts`,
and drives the core across a worker pool: compile `ambient.lem` + templates without front
matter, match documents and page templates to the reported `from` globs and compile them
with their record (D30, D41), loadData, plan, render; `lem check` prints spanned
diagnostics. The shell's backbone. Scope here was discovery only: the walk, the
classification, front matter parsed into `LoadedValue.spans` with positions, and a
`lemmon.config.ts` holding input, output and passthrough globs; no plugin list, and no
worker pool before slice 3 measures a rebuild. The brief suggested the bin run `src/`
directly so the port branch can depend on this checkout with `file:` and see every edit
without a build step, and that `scripts/render-the-rebound.ts` showed how front matter is
read today.

Built in the slice-1 run of 2026-10-15 by an Opus agent from a brief the managing session
wrote, which settled the cross-package mechanism: a `source` export condition on both
packages, listed before `types` and `import`; `customConditions: ["source"]` in
`tsconfig.base.json`; a project reference from the shell's build config to core's so
`npm run build` reads core's emitted declarations; Vite's server conditions with `source`
in front for the specs; and a bin shebang of `#!/usr/bin/env -S node --conditions=source`.
The brief asked for the mechanism to be proved from a scratch project with
`npm install file:…`, which it was: npm 11 links the directory and leaves its dependencies
to the checkout's `node_modules`, so no override for core is needed, and `lem build` runs
there with no `dist/` anywhere.

## Refinement

The front-matter reader matches `front_matter()` in `crates/lemmon-syntax` line by line,
parses with the `yaml` package's document API, converts its string indexes to byte offsets
and UTF-16 positions, keys spans as `field_path` in `crates/lemmon-vm/src/load/place.rs`
does, strips `type:` and `for:`, and derives `url` and `inputPath`. The walk recurses one
directory at a time, skipping `node_modules`, `.git` and an output directory inside the
input, and matches passthrough globs with picomatch as Eleventy does (dotfiles,
case-insensitive); a linked directory is not followed, which Eleventy does. Over
limulus-dot-net's `www/`: 25 documents, 34 passthrough files, no diagnostics.

A read-only Opus review found one bug and settled one design point. The bug: the `yaml`
package reports an error's position as one code unit, which inside an emoji is a split
surrogate pair, and the reader threw where the README promised a diagnostic; parser
positions are now widened to whole characters, with the `SourceText` guard kept as the
invariant-8 backstop. The design point: `inputPath` is the file's path from the directory
holding `lemmon.config.ts` (`www/feed/the-rebound/index.md`), not from the input
directory, because limulus.net's Source link and revisions lookup are keyed by that path;
`docs/language.md` says so now. The review also had the case-insensitive match, a
diagnostic for integers beyond 2^53, the config syntax error's location, percent-encoding
of `#`, `?` and `%` in a derived url, the default ignores, and the note that the core's
`recordSpan` takes `{start, end}` alone, which the next item must slice from the span the
reader carries. Fourteen specs came with the fixes; `npm run verify` passed with 260
tests at 100% coverage.

Left as they are: a front-matter key containing `.` shares a spans-table key with a nested
path (the format is the core's, and only a diagnostic's placement is affected); a block
collection's span runs over a trailing comment; the condition is named `source`, which no
package in either repo exports.

## Retrospective

The proof from a scratch project was worth its ten minutes: it replaced a belief about
how npm installs a `file:` dependency with what npm 11 does, and the README now says that
instead. The review's fuzzing found what a hundred happy-path specs could not, a parser
position no author would type and every emoji would produce; a reader that converts
another library's positions owes a test on positions that library reports, not only on
positions the test computed itself.

---
id: ∆muD
completed: 2026-10-15 21:18:54 -07:00
---

## Backlog item

> - `∆muD` `lem build` writes the-rebound to `dist/`
>   - Declare the host, compile, load, plan, render, write HTML and passthrough; reproduces
>     what `test:fidelity` renders from a directory of files. The first usable `lem`

## Planning

The item's brief, folded here. From `∆4tG` Build pipeline + CLI: `lem build` walks the
input, classifies files, parses front matter with YAML positions, loads `lemmon.config.ts`,
and drives the core across a worker pool: compile `ambient.lem` + templates without front
matter, match documents and page templates to the reported `from` globs and compile them
with their record (D30, D41), loadData, plan, render; `lem check` prints spanned
diagnostics. From `∆7Xs` Output writer: HTML to disk with marks resolved, the `styles`
producer writing per-page hashed CSS bundles under `/assets/immutable/…`, passthrough copy
by glob, and a link/asset integrity check, CloudFront untouched. Scope here: the first
milestone inside slice 1, `lem build` over a directory holding the-rebound and its layouts
producing the page `test:fidelity` renders; the host declares `site`, `page` and `build`
with `document: page`, `entry: layout`, `url: url`, no `marks` and no `assets`; HTML and
passthrough only, the `styles` producer being `∆F4y` and the integrity check `∆yOJ`. The
brief noted that `loadCore()` finds the staged core by walking up from the core package, so
a `file:` dependency needs no copy.

Built in the slice-1 run of 2026-10-15 by an Opus agent from a brief the managing session
wrote. The brief found the gap the item turned on: the core did not report `site.lem`'s
`from` bindings, so a shell could not know which glob typed which document or what to
load, and the corpus harness had derived both for itself. So the item added a core API,
`from_bindings` in `lemmon-vm` (the harness's `collects` moved there and the harness now
calls it), reported across the boundary as `CompileResult.sources`:
`{name, kind, literal, collects?, path, span}` per binding, in declaration order, from any
compile that relinks cleanly. The shell compiles in two calls — every `.lem` first, under
the standard host, reading the sources back; then every page, matched against the
collecting globs and compiled as its record — loads an envelope (`build`, the glob lists,
data files parsed with positions, environment values, and `pages` for typed pages no glob
holds), plans, renders every page into memory, and only then writes. The brief asked for
the smoke test the item is named for to be run from scratch rather than committed, so no
second copy of the fidelity fixture exists in another shape.

## Refinement

Decisions the builder took: `ambient.lem` is a real file at the package root, found by
walking up to the nearest `package.json`, so a diagnostic labelled at `Page` names a file
an author can open; a missing `site.lem` is an error, since without it no glob matches and
every document would quietly render with no layout; a document no glob holds and no
`type:` names renders its own output with `page` bound to its front matter by a
`loadData` of its own, which re-sends the whole envelope (18 of the port branch's 25 pages
are such documents today and the whole build takes about 0.35 s); `build.time` is
`Date.toISOString()`; the generator is `Lemmon <core.version>`, for which the core wrapper
gained `version`.

The smoke site — the fidelity fixture reshaped as a real site, with `Post` defaults for
what the language could compute and data files for the rest — built in 3.9 ms warm and
compared against the 11ty reference with 583 nodes matched, the three forgiveness classes
firing as in the harness, and two differences the port will see too: the generator string
and a reading time of 5 minutes where 11ty counts 6 from the markdown source. What the
language could not compute from front matter: UTC text for a datetime (printing stays in
the written offset), a number as a string, and the values that come from git history or
the asset pipeline. Two language gaps hit: a record literal inside a `map` lambda is not
typed by the expected element type, and `p.url` needs `.text()` to pass as a string.

A read-only Opus review found no bug in the new API or the envelope and four risks, all
fixed: output files differing only in case or Unicode form overwrote each other silently
on APFS (the writer registry now folds, and refuses what it could not tell apart, with a
label at the first writer); a directory-prefix collision crashed mid-write (refused before
anything is written, and a write the file system then refuses is reported as
`lem: could not write …`, never a stack trace); the two-phase compile hid a page
template's definitions from templates without front matter (every `.lem` is in the first
call now, page templates without their record); and `[Page] from "…"` was reported as
collecting `Page` and then refused at every page, because `collects` was reflexive where
the compiler's `extends` is strict — the compiler now refuses the binding at its type,
with a corpus case in the new `host/bindings/` group, and `docs/language.md` says so. The
review also found three tests pinning less than they claimed (a subtype-typed page in a
glob, `nocase` on collection globs, the url decode), a data file readable outside the
input, and the misplaced item numbers and missing step 10 in the docs; each is fixed with
a test. `npm run verify` (322 specs, 100% coverage), `npm run rust:wasm:test` and
`npm run wasm:check` pass.

Left as they are: the placeholders in `packages/` that name an item in a message or
comment (`∆c4j` for a plugin source, `∆99b` for a generator, `∆F4y`, `∆d1N`, `∆zu0` in the
README), which the gate does not read; the-rebound through `lem build` is not pinned in
this repo, since the port branch is where the slice is accepted and `∆Zo7` brings the
comparator there.

## Retrospective

The item's real work was discovering that the core had to report something before the
shell could exist, and the harness had been quietly doing the shell's job. Reading the
consumer that already existed found it in minutes; the brief would otherwise have sent a
builder into the shell with no way to type a document. The review's value was again in
the paths no author types: case folding on APFS, a directory where a file is, a `[Page]`
collection — each a small input with a silent or misleading outcome, found by trying the
input rather than reading the rule.

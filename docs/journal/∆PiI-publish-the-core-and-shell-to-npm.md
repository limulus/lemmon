---
id: ∆PiI
completed: 2026-11-08 13:26:56 -07:00
---

## Backlog item

> - `∆PiI` Publish the core and shell to npm
>   - Both ship built code, the core with its `.wasm` and the shell's `lem` as JavaScript,
>     released from lemmon's CI by `semantic-release` through trusted publishing

## Planning

This section folds in the item's brief and the plan written on 2026-11-08 against `6826560`.
On 2026-11-01 Eric decided that lemmon goes public (`∆P6l`) and publishes its packages so
that limulus.net's CI can install them (`∆FnH`), released by semantic-release as penumbra
is, through npm's trusted publishing. The packages could not run from `node_modules`: the
shell's bin was TypeScript run under the `source` condition, which Node refuses to strip
there, and the core found its `.wasm` in the repo's `artifacts/wasm/`.

**Versions.** The brief credited "one shared version" to Eric on 2026-11-01. It was in the
description of the option he picked that day, "semantic-release, as penumbra", and was never
asked on its own. On 2026-11-08 he chose lockstep after comparing it with independent
versions: the shell works only with the matching core; most of the core's code is in
`crates/`, where tools that assign commits to packages by path would miss it; the shell
re-exports four of the core's types, so a core major is a shell major; every page names one
product version; and one version per repo is semantic-release's own model.

**The published shape.** The first plan staged a rewritten copy of each package to publish:
the `source` condition dropped, the bin swapped, a hand-written `clean-publish`. Asked what
that solved, it kept `npx lem` running from source through a `file:` link, which `∆FnH`
removes next. Eric took the smaller shape: the checkout and the tarball are the same files,
and a site linked to a checkout runs its build, or the bin under `--conditions=source`.

**Settled on 2026-11-08, as the plan proposed:**

- `core.version`, which every page's generator meta prints, comes from the core's
  `package.json`, which a release sets. The `.wasm` cannot know it, since the release picks
  it after the `.wasm` is built and tested; rebuilding it in the release job would ship
  bytes no job tested.
- npm configures a trusted publisher only on a package that exists (`npm trust` docs), so
  Eric created both with `npm stage publish`, which leaves a public `0.0.0-stage`
  placeholder and a staged version he rejected afterwards.
- The trusted publisher may publish directly. One created after 2026-09-03 only stages
  unless told otherwise, which would hold every release for Eric's 2FA after the tag was
  pushed.
- A retired name still gets no alias after 1.0.0: it is a major release with a `BREAKING
  CHANGE:` footer, as the READMEs warn.

**Decided against.** `beta` and `next` channels: nothing would consume one, and moving
`latest` through trusted publishing needs dist-tag permission and npm 11.21.0, newer than
Node 24's 11.19.1 or Node 26's 11.20.0. A CHANGELOG.md: it needs a commit back to main on
every release, which `@semantic-release/git`'s README itself advises against, and which
would leave every checkout behind the push that released; the GitHub releases are the
changelog. A guard refusing an npm too old for trusted publishing: every Node 24 since
24.5.0 (2025-07-31) bundles one new enough. A main-only `if` on the release job: it repeated
`.releaserc.json`'s branches.

## Refinement

Built on 2026-11-08 as four commits:

- `f214bc8`, `feat!`: the `.wasm` is staged into `packages/core/artifacts/wasm/`, the first
  place the core's loader looks, and ships in the core; `core.version` reads the core's
  manifest; the bin is plain JavaScript, `bin/lem.js`, importing the CLI through a private
  `#cli` import that is `src/` under the `source` condition and `dist/` otherwise; each
  package carries the root `LICENSE`, publishes as public, and leaves its specs and test
  mocks out, which took the shell's tarball from 299 kB to 182 kB. `npm run test:pack`
  (`scripts/check-pack.ts`, its predicates in `scripts/lib/packed.ts`) packs both packages,
  installs the tarballs into an empty project and builds a site with the installed `lem`.
- `0c722da`, `ci`: the `release` job and `.releaserc.json`. semantic-release, the exec
  plugin and the conventionalcommits preset, which reads a `!` header as commitlint does,
  sets both versions, pins the shell to that core, reruns `test:pack` at the release version
  before any tag exists, then publishes the core and the shell.
- `3ab05dd`, `ci`: `test:pack` became a step of the `verify` job, whose setup was about 40
  of the separate `pack` job's 45 seconds.
- `3f0e3f3`, `chore(shell)`: the bin is named `bin/lem.js` rather than `./bin/lem.js`. npm
  normalizes the latter as it publishes and warns that the script name "was invalid and
  removed", though it keeps the bin; 1.0.0's release printed that.

The evidence before the release: `test:pack` passed at 0.0.0 and, after the release's own
version commands, at 9.9.9, where the installed `lem` wrote `Lemmon 9.9.9` and the lockfile
was untouched. Planted defects each failed it: `#cli` pointing at TypeScript, where Node
refused with `ERR_UNSUPPORTED_NODE_MODULES_TYPE_STRIPPING`; `core.version` read from the
`.wasm` again, where the page said 0.0.0; a missing file, in its spec. A type error planted
in `bin/lem.js` failed `tscc`, so `checkJs` covers the bin. A dry run found 249 commits and
no previous release, and showed that the tag is created after the prepare step.

The bin's move broke both limulus-dot-net worktrees, whose `.bin/lem` linked to `lem.ts`.
`npm install` there left no link at all, and `npm rebuild @limulus/lemmon-shell
--ignore-scripts` made it. Each branch's `package-lock.json` changed by four lines, the bin
and three older lemmon changes it had never been reinstalled for, and stays uncommitted for
`∆FnH`, which replaces the `file:` dependency. The cutover's own verify passed after, 142
tests and `lem check` over 59 pages, built and from source.

Run 42518897808 on `0c722da` released 1.0.0 of both packages with provenance, tagged
`v1.0.0` and created its GitHub release. For a minute after, the registry served the shell
without 1.0.0, and a local npm cache kept serving the `0.0.0-stage` packument to an install
until `--prefer-online`. A fresh install from npm then built a page naming `Lemmon 1.0.0`,
and `npm audit signatures` verified attestations on five packages. Eric rejected both staged
`0.0.0` versions and set both packages to require 2FA and disallow tokens.

Every install reports GHSA-67mh-4wv8-2f99, moderate, against the shell's exact esbuild
0.24.0 pin. The advisory concerns esbuild's dev server, and `esbuild()` only calls
`build()`; the pin keeps limulus.net's bundle hashes identical for parity, so it can move
once the site is live on Lemmon.

## Retrospective

A decision written into another option's description was recorded as made, and the brief
carried it for a week. Ask each decision on its own.

The first shape preserved a convenience whose only user goes away in the next item, at the
cost of a rewrite step between the checkout and what publishes. Before a design keeps
something working, ask how long anyone needs it.

Two guards went in and came out at Eric's question, a version check and a branch test, each
against a failure nothing made plausible. Name what a guard prevents before writing it.

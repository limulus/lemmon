---
id: ∆FnH
completed: 2026-11-08 21:18:54 -07:00
---

## Backlog item

> - `∆FnH` limulus.net's CI gets lemmon
>   - The site installs the published `@limulus/lemmon-shell` in place of `file:../lemmon`,
>     which a CI checkout cannot resolve

## Planning

This section folds in the item's brief. The problem was found mapping slice 6 on
2026-10-18. The port depended on `@limulus/lemmon-shell` as `file:../lemmon/packages/shell`,
which resolves only where a lemmon checkout sits beside limulus-dot-net. A GitHub Actions
checkout has none, so `npm ci` and `lem build` would fail there, and the cutover's first
push would deploy nothing. Eric chose to settle it before the merge rather than in slice 6,
since nothing deployed in that slice.

On 2026-11-01 he decided the route: Lemmon goes public (`∆P6l`) and its packages are
published to npm (`∆PiI`). A private npm package, as CD already installs for
`@limulus/penumbra` with its `NPM_TOKEN`, was ruled out because Eric cannot create new
private npm packages. Building lemmon inside the site's CD, the other route that kept it
private, was set aside for going public. `∆PiI` published `@limulus/lemmon-core` and
`@limulus/lemmon-shell` at 1.0.0 on 2026-11-08, together and with provenance.

## Refinement

Built on 2026-11-08 as limulus-dot-net `3b64e12` on the cutover branch, with lemmon
`db075cf` for the docs:

- The site depends on `@limulus/lemmon-shell@^1.0.0`. The lockfile gains 39 packages: the
  core, esbuild 0.24.0 and its platform packages, highlight.js 11.9.0, esbuild-plugin-wasm
  1.1.0, and the shell's other dependencies.
- `tsconfig.json`'s `customConditions: ["source"]` and the unit project's
  `ssr.resolve.conditions` in `vitest.config.ts` go. With the release installed, the
  condition reaches the release's `src/`: Node refuses to strip types under
  `node_modules`, tsc typechecks the release's source and fails on picomatch's types, and
  Vitest quietly ran the specs against the release's source, passing, but testing the
  wrong files.
- The site's README drops the `../lemmon` prerequisite, and its `CLAUDE.md` says how to
  build with an unreleased Lemmon: `npm install --no-save <lemmon>/packages/shell`, which
  links the checkout without touching `package.json` or the lockfile, so `npm ci` puts the
  release back. Its pages then read `Lemmon 0.0.0`, which shows the link is in place.
  lemmon's `docs/shell.md` §Running from a checkout and `CLAUDE.md` say the same from this
  side.

The evidence before the merge:

- A fresh clone with no lemmon beside it ran `npm ci` from the registry. `npm audit
  signatures` verified both packages' provenance.
- Its `npm run verify` passed, and its build matched the linked reference except for the
  generator meta on 55 pages, `Lemmon 1.0.0` where the link wrote `Lemmon 0.0.0`.
- The `--no-save` link worked both through the checkout's build and with the bin run from
  source.
- The review found two things, both fixed: lemmon's `CLAUDE.md` left out that the port
  branch still links with `file:`, and the `customConditions` sentence in `docs/shell.md`
  did not say it holds only for a linked checkout.

The merge, on 2026-11-08 at Eric's go-ahead, was `c84f74c` (`--no-ff`, its message ending
in `∆i6r`'s `Revision: quiet`), pushed to `public` from the cutover worktree, so that
Eric's own checkout, which holds his drafts, was left alone. Before it, with Eric signed in
to AWS, `cdk diff` against the deployed stack found no differences, and CD's two syncs, run
with `--dryrun` and checked file by file against the bucket's ETags, showed that every page
and feed would be rewritten and no page left behind. The second sync's `--include '*.xsd'`
matched every file. It had worked only because the first sync had just refreshed every
object's timestamp, and it was fixed as `909ffde`.

CD run 42560699139 passed all three jobs:

- `npm ci` added 901 packages from the registry.
- `update-revisions` recorded seven quiet revisions, committed by the bot as `3e07869`.
- `lem build` wrote 59 pages.
- 139 files were uploaded, and then the one `.xsd` as XML.
- CloudFormation updated only `AWS::CDK::Metadata`, so CloudFront was untouched.

Fetched from limulus.net afterwards, 68 of the 87 pages, feeds, stylesheets and scripts
were byte-identical to the local build, the generator meta among them reading
`Lemmon 1.0.0`. The 19 that were not (18 photo pages and the map script) differed by the
Mapbox token alone, and by the map script's hashed name, which follows from its content.
CD's secret holds a different token from the one the local build used, and with both
masked, the files are identical. The name the live pages use was already in the bucket, so
the map script did not change. Eric accepted the item on 2026-11-08, and with it slice 7,
the last of "limulus.net on Lemmon": the initiative's exit criterion was met.

## Retrospective

The registry install in a fresh clone stood in for CD well, but only the run itself could
show that CI gets lemmon, so Eric held the journal until it had. Comparing the live site
against the local build first reported a missing map script: the comparison had asked for
the local build's hashed name, which a different token renames. Compare by the names the
live pages reference. A build whose input includes a secret differs from CD's by that
secret, so a comparison masks it rather than calling it identical.

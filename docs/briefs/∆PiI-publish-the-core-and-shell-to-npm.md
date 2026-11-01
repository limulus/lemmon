# Publish the core and shell to npm

## Value

limulus.net's CI installs Lemmon from npm (`∆FnH`), and so can any other site.

## Background

The packages cannot run from `node_modules` as they stand (`∆FnH`'s brief, 2026-10-18):
Node refuses to strip types under `node_modules`, so they must ship their built `dist/`.
The shell's `lem` bin is `bin/lem.ts`, and `@limulus/lemmon-core` finds its `.wasm` by
looking upward for the repo's `artifacts/wasm/` (`packages/core/src/lib/load.ts`), so the
published core must carry it. Both packages are `"private": true` at `0.0.0`, and the shell
depends on the core at `0.0.0`.

`@limulus/penumbra`, a Rust-to-wasm package of Eric's, is released from its own CD by
`semantic-release` (penumbra's `.github/workflows/cd.yaml` and `.releaserc.json`).
Lemmon's history is kept release-grade under commitlint so that release tooling can derive
versions from it (`CLAUDE.md`, Committing).

## Scope and constraints

- `@limulus/lemmon-core` and `@limulus/lemmon-shell` are published as public packages,
  released by `semantic-release` as penumbra is: a `feat` or `fix` commit on lemmon's main
  publishes both at one shared version derived from the commits, the first 1.0.0 (Eric,
  2026-11-01).
- They publish through npm's trusted publishing (Eric, 2026-11-01), whose trusted
  publisher Eric configures on npm.
- Nothing publishes before `∆P6l` has given the repo its license.

## Implementation suggestions

- Lemmon's CI already stages the `.wasm` once in its `wasm` job and hands it to the jobs
  after it (`CLAUDE.md`, Commands), which a publish job could consume.
- `CLAUDE.md` says nothing is released, so a retired name or tag needs no alias or
  deprecation message; that stops being true with the first release.

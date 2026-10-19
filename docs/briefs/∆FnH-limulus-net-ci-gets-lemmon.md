# limulus.net's CI gets lemmon

## Background

Found mapping slice 6 on 2026-10-18. The port branch depends on `@limulus/lemmon-shell` as
`file:../lemmon/packages/shell`, which resolves only where a lemmon checkout sits beside
limulus-dot-net; a GitHub Actions checkout of limulus-dot-net has none, so `npm ci` and a
`lem` build fail there. The lemmon repo is private, limulus-dot-net's is public, and both
packages are `"private": true` with nothing published (D18; CLAUDE.md: nothing is
published without Eric). A packed tarball does not run either: Node refuses to strip types
under `node_modules`, so the packages must ship their built `dist/`, and `@limulus/lemmon-core`
must carry its `.wasm`.

Eric chose on 2026-10-18 to decide this before merging the cutover branch rather than in
slice 6, since nothing deploys in that slice.

## Scope and constraints

- The routes found then: a restricted npm publish of both packages (CD already passes an
  `NPM_TOKEN`), which needs release tooling the repo has not chosen; building lemmon in CD
  (a token for the private repo, Rust, `wasm-bindgen-cli` at the pinned version); or a
  vendored tarball in limulus-dot-net, which makes lemmon's built code public.
- Codespaces and any other fresh checkout of limulus-dot-net meet the same wall.

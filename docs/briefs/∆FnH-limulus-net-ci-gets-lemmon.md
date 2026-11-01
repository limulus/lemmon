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

On 2026-11-01 Eric decided the route: Lemmon goes public (`∆P6l`) and its packages are
published to npm (`∆PiI`). A private npm package, as limulus.net's CD already installs for
`@limulus/penumbra` with its `NPM_TOKEN`, was ruled out because Eric cannot create new
private npm packages, and the other route that kept lemmon private, building it in the
site's CD, was set aside for going public.

## Scope and constraints

- The site installs the published `@limulus/lemmon-shell` in place of `file:../lemmon`.
- Lemmon's packages are published through npm's trusted publishing (`∆PiI`). They are
  public, so the site's `npm ci` installs them with no token; its `NPM_TOKEN` stays for
  `@limulus/penumbra`.
- Codespaces and any other fresh checkout of limulus-dot-net meet the same wall.

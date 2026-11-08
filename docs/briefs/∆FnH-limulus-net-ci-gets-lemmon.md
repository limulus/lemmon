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

`∆PiI` published `@limulus/lemmon-core` and `@limulus/lemmon-shell` at 1.0.0 on 2026-11-08,
released together at one version with provenance, so the site can depend on
`@limulus/lemmon-shell@^1.0.0`. Its pages then name `Lemmon 1.0.0` in their generator meta
where a checkout's said `Lemmon 0.0.0`. Through a `file:` link the shell now runs the
checkout's build, so both limulus-dot-net worktrees were reinstalled for the new bin; each
branch's `package-lock.json` holds a four-line change, uncommitted, that this item's
dependency change replaces. Every install reports GHSA-67mh-4wv8-2f99 against the shell's
exact esbuild 0.24.0 pin, which concerns esbuild's dev server, not the `build()` the shell
calls.

## Scope and constraints

- The site installs the published `@limulus/lemmon-shell` in place of `file:../lemmon`.
- Lemmon's packages are published through npm's trusted publishing (`∆PiI`). They are
  public, so the site's `npm ci` installs them with no token; its `NPM_TOKEN` stays for
  `@limulus/penumbra`.
- Codespaces and any other fresh checkout of limulus-dot-net meet the same wall.

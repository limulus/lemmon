# Lemmon goes public

## Value

Lemmon's packages can be published to npm, where limulus.net's CI can install them, and
anyone can read the code and use it.

## Background

limulus.net's CI cannot resolve the site's `file:../lemmon` dependency (`∆FnH`). Keeping
lemmon private would have meant publishing its packages as private npm packages, as
`@limulus/penumbra` is, but Eric cannot create new private npm packages; a tarball vendored
into the public site repo would have published the code anyway. Eric decided on 2026-11-01
to go public with Lemmon, which `∆PiI` then publishes.

What the public surfaces lacked on 2026-11-01:

- No license: no `LICENSE` file, `"license": "UNLICENSED"` in the root, `packages/core` and
  `packages/shell` manifests, and no `license` in Cargo's `[workspace.package]`.
- No root README; `packages/core` and `packages/shell` have their own, which npm shows.
- `CLAUDE.md` points readers at "three Claude Artifacts owned by Eric" with "Ask Eric for
  access", which a public reader cannot do.
- The package manifests have `repository` and `description`, and no `homepage`, `bugs` or
  `keywords`.

The history was checked the same day: all 226 commits scanned for token and key patterns
and for added `.env`, key and credential files, with none found; no tracked file names a
`/Users/eric` path; every commit is authored as `eric@limulus.net`. CI runs on `push` and
reads no secrets.

## Scope and constraints

- The license is MIT, copyright Eric McCarthy, as `@limulus/blur-hash` carries it, and it
  covers everything in the repo, the text of the-rebound post in `tests/fidelity/`
  included (Eric, 2026-11-01).
- The public surfaces are updated for public readers — in Eric's words, "update the public
  surfaces (readme, what else?), add a license".
- Making the repository public is Eric's own step, in GitHub's settings.

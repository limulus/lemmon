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
- A root README written before the implementation, calling the project
  "pre-implementation" and the core a stub (corrected on 2026-11-01: an earlier check here
  said there was none, misread from a failed shell glob); `packages/core` and
  `packages/shell` have their own, which npm will show.
- `CLAUDE.md` points readers at "three Claude Artifacts owned by Eric" with "Ask Eric for
  access", which a public reader cannot do.
- The package manifests have `repository`, `description`, `author`, `homepage` and
  `bugs`, and no `keywords` (corrected the same day: the first reading missed `homepage`
  and `bugs`).

The history was checked the same day: all 226 commits scanned for token and key patterns
and for added `.env`, key and credential files, with none found; no tracked file names a
`/Users/eric` path; every commit is authored as `eric@limulus.net`. CI runs on `push` and
reads no secrets.

The plan's review found one piece of third-party content: the ten Font Awesome Free icons in
`tests/fidelity/the-rebound/site/sprite.lem` and `fixtures/reference.html`, © Fonticons,
Inc., under CC BY 4.0, each keeping the attribution comment that license asks for; nothing
else in the repo is third-party. The post itself is Eric's, published on limulus.net under
CC BY 4.0. Eric asked on 2026-11-01 whether the notices could go, and they did, with the
icons: removed from both files alike, they leave the fidelity comparison passing with no
forgiveness class added, 575 nodes matched where there had been 585, the ten `<path>`s,
while removing them from the sprite alone fails it ten times. Earlier commits keep each
copy beside its attribution comment.

## Scope and constraints

- The license is MIT, copyright Eric McCarthy, as `@limulus/blur-hash` carries it, and it
  covers everything in the repo that is Eric's, the text of the-rebound post in
  `tests/fidelity/` included (Eric, 2026-11-01). The Font Awesome icons are removed rather
  than credited (Eric, 2026-11-01), so nothing in the repo is anyone else's.
- The public surfaces are updated for public readers — in Eric's words, "update the public
  surfaces (readme, what else?), add a license".
- The two package READMEs are written "with the audience of an `npm` consumer in mind"
  rather than as internal documentation (Eric, 2026-11-01). The shell's README was the
  normative text D44–D47 name, so that text moves to `docs/shell.md` and the README
  introduces it.
- Making the repository public is Eric's own step, in GitHub's settings.

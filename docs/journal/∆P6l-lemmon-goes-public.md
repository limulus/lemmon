---
id: ∆P6l
completed: 2026-11-06 22:56:53 -07:00
---

## Backlog item

> - `∆P6l` Lemmon goes public
>   - An MIT license, a root README, and docs and package metadata written for public
>     readers, so that Eric can open the repo and its packages can be published

## Planning

The item's brief, folded here. limulus.net's CI cannot resolve the site's `file:../lemmon`
dependency (`∆FnH`). Keeping lemmon private would have meant publishing its packages as
private npm packages, as `@limulus/penumbra` is, but Eric cannot create new private npm
packages, and a tarball vendored into the public site repo would have published the code
anyway. So on 2026-11-01 Eric decided to go public with Lemmon, which `∆PiI` then
publishes, and asked to "update the public surfaces (readme, what else?), add a license".

What the public surfaces lacked that day:

- a license: no `LICENSE` file, `"license": "UNLICENSED"` in the npm manifests, and none
  in Cargo's `[workspace.package]`;
- a root README written before the implementation, which called the project
  "pre-implementation" and the core a stub;
- `CLAUDE.md` pointing readers at three Claude Artifacts with "Ask Eric for access", which
  a public reader cannot do;
- `keywords` in the package manifests.

The history was checked the same day: none of the 226 commits added a token, a key, an
`.env` or a credential file; no tracked file names a `/Users/eric` path; every commit is
authored as `eric@limulus.net`; and CI runs on `push` and reads no secrets.

Eric's decisions, 2026-11-01:

- MIT, copyright Eric McCarthy, as `@limulus/blur-hash` carries it, covering everything in
  the repo that is his, the-rebound's text in `tests/fidelity/` included.
- The two package READMEs written "with the audience of an `npm` consumer in mind"
  rather than as internal documentation. The shell's README had been the normative text
  D44–D47 name, so that text moved to `docs/shell.md`, and the README introduces it.
- No word in the READMEs about npm status ("It'll be there soon enough"), and no status
  sections. Instead, a notice near the top that Lemmon is in early development, likely to
  see frequent major versions with significant breaking changes.
- Making the repository public is his own step, in GitHub's settings.

The plan's review found two things the brief had wrong. The root README existed, stale,
so it was rewritten rather than written. And the repo held third-party content: ten Font
Awesome Free icons in the fidelity fixtures' sprite and reference page, © Fonticons, Inc.,
under CC BY 4.0, each with the attribution comment that license asks for. MIT could not
cover those. Eric asked whether the notices could go, and they went with the icons:
emptied from both files alike, they leave the fidelity comparison passing with no
forgiveness class added (575 nodes matched, where there had been 585), while removing them
from the sprite alone fails it ten times. Earlier commits keep each copy beside its
attribution.

## Refinement

Landed as lemmon `d06ded9`:

- an MIT `LICENSE` and the license in every npm and Cargo manifest;
- the root README, rewritten for public readers;
- both package READMEs, rewritten for npm readers, and the shell's normative text moved
  to `docs/shell.md`, with the pointers to it in D44–D47, CLAUDE.md and the briefs;
- the architecture artifacts named as private in the docs, until `∆JXK` retires them;
- the icons removed from the fidelity fixtures.

A fact-check of the new READMEs against the code found six claims to correct before the
commit, among them a throw from a `Core` method called always the caller's bug, where a
trap in the `.wasm` poisons the instance; the core described as reading no files, where
`loadCore()` reads the `.wasm`; and esbuild's bundles described as written beside their
sources, where they are written to the output directory. Every snippet in the READMEs was
run and typechecked.

The same review led Eric to retire the artifacts rather than reconcile them (`∆JXK`,
closing `∆MlD`). Eric made the repository public by 2026-11-06 and kept its GitHub
description as it was, and accepted the item that day.

## Retrospective

The two misses in the brief were both reads of the tree, a failed glob and an unread
fixture, and the plan's review caught both before they cost anything. Asking whether the
attribution could go, rather than writing it, turned a licensing footnote into deleted
content that the fidelity gate showed was never load-bearing.

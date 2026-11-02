# The parity comparator leaves the shell

## Background

Found by a review of the slices on 2026-10-20. `packages/shell/src/lib/rendered-html.ts`,
a 625-line page comparator (D28), moved from `scripts/lib` into the shell so that the
limulus-dot-net port's `npm run parity` could import it from `@limulus/lemmon-shell`; the
shell's barrel exports it. No build, check or dev path uses it. Once the cutover branch
merges, the port's parity is gone and its only consumer is lemmon's own
`scripts/render-the-rebound.ts` (`npm run test:fidelity`).

Eric decided on 2026-10-24 to move it after the cutover, and on 2026-11-01 to move it
before the first release instead, since removing it from the shell's API afterwards would
be a major version. What needs it until then is `lemmon-port`'s `npm run parity`; the
cutover branch (`lemmon-cutover` at `28dde17`) imports none of it, checked the same day.

## Scope and constraints

- Lands before `∆PiI` publishes the packages, and after Eric's last `npm run parity` on
  `lemmon-port`, which it stops from running: the refactors before it, `∆Lj6`, `∆H7Q` and
  `∆FbD`, are checked by that run first (Eric, 2026-11-01).
- It returns to `scripts/lib`, covered as that directory is, and leaves the shell's API.

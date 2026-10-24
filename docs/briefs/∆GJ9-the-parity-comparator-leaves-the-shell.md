# The parity comparator leaves the shell

## Background

Found by a review of the slices on 2026-10-20. `packages/shell/src/lib/rendered-html.ts`,
a 625-line page comparator (D28), moved from `scripts/lib` into the shell so that the
limulus-dot-net port's `npm run parity` could import it from `@limulus/lemmon-shell`; the
shell's barrel exports it. No build, check or dev path uses it. Once the cutover branch
merges, the port's parity is gone and its only consumer is lemmon's own
`scripts/render-the-rebound.ts` (`npm run test:fidelity`).

Eric decided on 2026-10-24 to move it after the cutover.

## Scope and constraints

- Waits for the cutover branch to merge: until then `lemmon-port`'s parity imports it.
- It returns to `scripts/lib`, covered as that directory is, and leaves the shell's API.

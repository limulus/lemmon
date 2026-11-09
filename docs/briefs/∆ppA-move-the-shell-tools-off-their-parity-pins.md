# Move the shell's tools off their parity pins

## Background

Found reviewing the cutover branch on 2026-11-08; Eric asked that it be marked for cleanup
after the cutover.

`@limulus/lemmon-shell` depends on highlight.js 11.9.0, esbuild 0.24.0 and
esbuild-plugin-wasm 1.1.0 exactly, because a release inside a range moves the highlighted
spans and the client scripts' hashes, which limulus.net's parity harness compared exactly
(CLAUDE.md, `docs/shell.md`). Parity retires with the cutover.

Every install of the shell reports GHSA-67mh-4wv8-2f99, moderate, against esbuild 0.24.2
and earlier. It concerns esbuild's dev server; the shell calls only `build()`. limulus.net's
main depended on the same esbuild before the port.

A bump changes limulus.net's output once: its code blocks' markup for highlight.js, and its
client scripts' hashed names for esbuild.

## Scope and constraints

- CLAUDE.md and `docs/shell.md` give parity as the pins' reason, so that text changes with
  them.

# ∆omv Negative-test the wasm import allowlist — plan

Written on 2026-09-19 by the session that surveyed the repository's organization for Eric,
against `main` at `d3c8f11`. An analyst proposed it and a second agent confirmed the facts
below by reading the files. Nothing was built. When ∆omv's journal entry is written, its
Planning section absorbs this file and the file is deleted.

## Read this first: nothing is broken

`npm run wasm:check` works, and the staged artifact imports only what the allowlist names.
This item guards one drift. `CLAUDE.md` calls the allowlist "the mechanical check on
invariant 1 — a host capability pulled in by a dependency shows up there as a new import",
and its testing rules say a check claimed to enforce an invariant "is negative-tested
first: feed it a real violation and watch it fail." This one never has been, and the code
that decides sits where no test can reach it.

## Start here

1. Read `CLAUDE.md` § Commands (the `wasm:check` paragraph) and § Testing, the ∆omv item
   in `BACKLOG.md`, and `docs/journal/∆Ma4-compare-the-spike-as-rendered-html.md`, the
   precedent for moving a gate's logic into `scripts/lib/` with a spec: "a gate whose
   whole value is what it refuses is logic worth testing".
2. `npx @limulus/delto@1 claim ∆omv`.

## What exists today

- `scripts/wasm-artifact.ts` (72 lines) is not a script. It exports the staging paths,
  `EXPECTED_EXPORTS`, `withoutShimHash` and `ALLOWED_IMPORT_NAMES`, and is imported by
  `scripts/check-wasm.ts:3–8` and `scripts/stage-wasm.ts:5`. It sits beside the scripts, so
  `vitest.config.ts:18` (`include: ['packages/*/src/**/*.ts', 'scripts/lib/**/*.ts']`)
  does not cover it.
- The decision is in `scripts/check-wasm.ts:17–29`, an entry point: filter
  `EXPECTED_EXPORTS` against the module's exports; strip each import's shim hash and keep
  the names not on the allowlist.
- `withoutShimHash` strips `/_[0-9a-f]{16}$/`. If wasm-bindgen changed that suffix, every
  import name would stop matching. The check would fail closed, with a message that
  blames a dependency for reaching a host capability.
- `ALLOWED_IMPORT_NAMES`'s own doc gives the violation to test with: `js_sys::Date::now()`
  "would show up here as `__wbg_now`".

## The change

1. Move the file to `scripts/lib/wasm-artifact.ts`, content unchanged. Header sentence:
   "The staged wasm artifact's names — where it lives, what it must export, and which
   imports a pure core may reference (invariant 1)."
2. Add two predicates there, lifted from `check-wasm.ts`:
   `missingExports(names: string[]): string[]` and
   `unexpectedImports(names: string[]): string[]`, the second de-duplicating as the
   script's `new Set` does today.
3. `scripts/check-wasm.ts` keeps the I/O: read the bytes, `WebAssembly.compile`, call the
   two predicates, throw with the messages it has, print the summary. About 22 lines.
4. `scripts/lib/wasm-artifact.spec.ts`:
   - `unexpectedImports(['__wbg_now_0123456789abcdef'])` returns `['__wbg_now']`.
   - an allowlisted name, with and without a shim hash, is not returned;
   - a name with a 15-digit or uppercase suffix is returned unstripped, which pins what
     `withoutShimHash` accepts;
   - `missingExports` given a surface without `core_render` returns it, and returns
     nothing for the full surface.
5. Paths to update: the two imports (`check-wasm.ts`, `stage-wasm.ts`), the path in
   `check-wasm.ts`'s thrown message (line 33), `CLAUDE.md`'s `wasm:check` paragraph, and
   the comment at `packages/core/src/lib/load.ts:10`, which names
   `scripts/wasm-artifact.ts`.

About 8 production lines are added (the two signatures) and about 55 spec lines.

## The inert-test trap

`scripts/lib` is held to 100% coverage, and the constant lists are data. A spec that
restates `ALLOWED_IMPORT_NAMES` or `EXPECTED_EXPORTS` would pass forever and prove
nothing, and coverage cannot tell, because an executed line counts. The spec exercises
the predicates only. Say so in the spec's header, so the next reader does not add the
restating test.

## Also probe the real gate, once

After the move, make the staged artifact fail on purpose: take `'__wbg_Error'` out of
`ALLOWED_IMPORT_NAMES`, run `npm run rust:wasm` and `npm run wasm:check`, and watch the
check name `__wbg_Error`. Put the line back by hand, not with `git checkout -- <path>`.
Record the probe and its output in the journal; it is the negative test of the wiring,
which the spec cannot give.

## Acceptance

- `npm run verify` passes with the new spec and coverage still at 100% for `scripts/lib`.
- `npm run wasm:check` passes against a freshly staged artifact.
- The probe above was run and reversed.

## For ∆Qr2's planner

∆Qr2 adds `exportModule` and `importModule` to the boundary and must add their
wasm-bindgen names to `EXPECTED_EXPORTS`, which only `npm run wasm:check` reads.

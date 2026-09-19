# ∆W1D Gate the spike and the benchmark's fixtures — notes

These are findings for the session that plans ∆W1D, not a plan of it. They were written on
2026-09-19 by the session that surveyed the repository's organization for Eric, against
`main` at `d3c8f11`. That session shortened the backlog item to make room for the pointer
to this file; the clause it removed is kept in the first section. Nothing was built. Fold
this file into ∆W1D's plan or its journal entry, and delete it then.

## What the item said before it was shortened

"Give each a mode without timing loops — one pass, its diagnostics and rendered-HTML
checks — run from `verify:js`, which already needs the staged artifact."

## The benchmark picks its fixtures by directory name

`scripts/bench-wasm.ts:104–110` takes every `render/` and `document/` corpus input whose
path does not contain `/errors/`. `tests/corpus/README.md:58–60` says: "There is no `ok/`
versus `broken/` split. Whether a case parses clean is _data_: its diagnostics section is
empty." The bench is the one consumer of a convention the corpus says it does not have.
This has cost a session already: ∆Bhz's journal (lines 124–129) records a case that became
a deliberate error and stayed where it was, which the corpus gate accepted and the bench —
in no gate — did not. The fix then was a habit, "move it under `errors/` in the same
pass", and commit `bf02cf1`.

The filter is correct today: no `.html` expectation under `render/` or `document/` outside
an `errors/` directory carries a non-warning diagnostic. It is correct because people keep
it so, not because anything checks it.

The proposal: `scripts/lib/corpus-fixtures.ts` exporting `cleanFixtures(groups)`, which
keeps a `.lem` or `.md` input when every line after the last `--- diagnostics` in its
sibling `.html` is blank or a warning. `bench-wasm.ts` loses `walk` and `fixtures` (about
22 lines); the new module is about 34 lines with about 70 of spec — the first tests these
lines have had. Correct the README paragraph in the same pass: `errors/` and `warnings/`
directories do exist (`render/errors`, `render/warnings`, `host/errors`, `host/warnings`,
`document/errors`) and group cases whose subject is the diagnostic; the top-level names are
still not a clean/broken partition; no tool decides cleanliness from a path.

The cost: the bench then depends on the trailer format. The literal `--- diagnostics` is
already spelled in three places — `crates/lemmon-corpus/src/lib.rs:197` (the reader),
`crates/lemmon-vm/tests/corpus.rs:307` and `crates/lemmon-syntax/src/parser/dump.rs:55`
(two writers) — and this would be a fourth. ∆rKx gives the three Rust sites one constant
in `lemmon-syntax`; a TypeScript reader cannot import it, so it would name the constant in
a comment beside its own copy.

## The two scripts carry copies of the same helpers

- `expectClean` is byte-identical, comment included: `bench-wasm.ts:87–93` and
  `render-the-rebound.ts:133–139`. ∆VNk's journal (lines 42–45) records one contract change
  fixed twice because of it.
- `percentile`: `bench-wasm.ts:301` and `render-the-rebound.ts:127`. The table printer:
  `bench-wasm.ts:332–340` and `render-the-rebound.ts:342–347`. The `record` timing
  recorder: `bench-wasm.ts:61` and `render-the-rebound.ts:113`; the bench's version also
  counts payload bytes. These three were read by the analyst only, not by the verifier.

`CLAUDE.md` says logic worth testing goes in `scripts/lib/`. The proposal is
`scripts/lib/harness.ts` with `expectClean`, `percentile`, `printTable`, and a `recorder()`
serving both shapes: about 60 lines plus about 90 of spec, against about 37 deleted. If
one recorder for both shapes turns awkward, share the other three and leave the recorders
where they are. ∆W1D's one-pass modes will call `expectClean` from both scripts, so the
shared module is the natural first step of that item.

## Both scripts import the package's source

`scripts/bench-wasm.ts:32` and `scripts/render-the-rebound.ts:33` import
`../packages/core/src/index.ts`. A change to `@limulus/lemmon-core`'s public API breaks
both, and today no gate sees it. ∆Qr2 changes that API, which is an argument for landing
∆W1D first.

## `spike/` will stop being a spike

Phase 0 is closed. Once ∆W1D runs the comparison from `verify:js`, `spike/the-rebound/`
(nine files, 1,595 lines: a vendored 11ty reference page, its data, six `.lem` layouts) is
a gate's fixture set, and `tests/` is where this repository keeps checked-in inputs with
checked-in expectations. The proposal is `tests/fidelity/the-rebound/` and an npm script
named `test:fidelity`. What it touches: the `SPIKE` constant at
`scripts/render-the-rebound.ts:35`, `package.json:26`, five mentions in `CLAUDE.md`, and
this item's own text. The journal's mentions of "the spike" are history and stay. It is a
rename with no other effect; do it inside ∆W1D or not at all.

## Three models of the shell's front matter

`crates/lemmon-vm/tests/corpus.rs:476–692` parses front matter with saphyr and computes
positions per field path; `scripts/render-the-rebound.ts` does the same job with the `yaml`
package; `scripts/bench-wasm.ts:150–204` computes front-matter spans by arithmetic for its
synthetic documents. Nothing is wrong with fixtures in two languages. When ∆4tG gives
`@limulus/lemmon-shell` a real front-matter reader, the two scripts should use it instead
of a fourth model appearing.

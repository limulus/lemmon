---
id: ∆omv
completed: 2026-09-19 14:38:20 -07:00
---

## Backlog item

> - ∆omv Negative-test the wasm import allowlist — `wasm:check` is the mechanical check on
>   invariant 1, yet its decision lives in the uncovered `scripts/check-wasm.ts` and no test
>   has fed it a forbidden import. Move `scripts/wasm-artifact.ts` into `scripts/lib/` with
>   `unexpectedImports` and `missingExports` predicates and a spec that feeds them a
>   violation. Planned in `docs/plans/∆omv-negative-test-the-wasm-check.md`

## Planning

The plan came from the 2026-09-19 organization survey, and opened by saying nothing was
broken: `npm run wasm:check` works, and the staged artifact imports only what the
allowlist names. The item guards one drift. CLAUDE.md calls the allowlist "the mechanical
check on invariant 1", and its testing rules say a check claimed to enforce an invariant
is negative-tested first. This one never had been, and the code that decides sat in
`scripts/check-wasm.ts`, where no test reaches.

`scripts/wasm-artifact.ts` was not a script: it exported the staging paths, the expected
exports, the allowlist and the hash-stripping helper. The plan moved it to `scripts/lib/`,
which is covered at 100%, with two pure predicates holding the decision, and left
`check-wasm.ts` the I/O. ∆Ma4 was the precedent: "a gate whose whole value is what it
refuses is logic worth testing". The plan said the spec must not restate the constant
lists, since a list compared with a copy of itself pins nothing.

## Refinement

`missingExports(names)` gives the expected exports a module lacks, in the expected order;
extra exports are fine. `unexpectedImports(names)` gives the import names not on the
allowlist, with wasm-bindgen's hash stripped, once each, in first-seen order. The
stripping rule is unchanged: one trailing `_` and exactly sixteen lowercase hex digits.

The spec has 18 tests where the plan expected about four. The additions pin the stripping
rule at its edges — no hash, fifteen digits, uppercase, a hash-shaped run in the middle of
a name, a doubled suffix — because that rule is what stands between a forbidden import and
an allowlisted name. The builder perturbed the module nine ways; eight failed tests. The
ninth, adding the regex's `g` flag, changes nothing while the `$` anchor is there, so it
was no perturbation. Violations and clean cases are written as literals, so emptying the
allowlist fails the clean cases.

The real probe: with `__wbg_Error` removed from the allowlist, `npm run wasm:check` exits
1 and names that import. The edit was reversed by hand.

The review, run on Sonnet because the change is small and TypeScript only, found no
defect. It noted that the allowlist constant was still exported with no importer left, as
the hash-stripping helper had been before the builder un-exported it. Both are private
now, so the predicates are the only way to the decision.

Vitest's text coverage table lists only files under 100%, so the new file never appears
in it; the figures were read from `coverage/coverage-final.json`.

## Retrospective

Nothing went wrong. The plan had been checked line by line by a second agent before it
was accepted, the change was small, and it was built in one run.

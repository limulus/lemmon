---
id: ∆iCW
completed: 2026-09-09 02:33:00 -07:00
---

## Backlog item

> - ∆iCW Schemas at load — the content model is typed (∆Iau); `loadData` now fulfils it:
>   validating records (the host's envelope and ambient values too) at their YAML positions,
>   tagging each with its declared type so `is` and `lem-match` reach loaded data (D35),
>   running computed defaults in dependency order, resolving `by` references and strings by
>   expected type, and rejecting extra keys, so limulus.net's build throws become load
>   errors

## Planning

Three checkpoints, each reviewed with `/code-review medium --fix` and committed on its
own: the compile side (f494c63 — defaults, `by` indexes, and `from` bindings lowered to
loader-run functions with their reads recorded, ordered and cycle-checked at link), the
loader itself in `crates/lemmon-vm/src/load/` (cb2e182), and the boundary (this commit).
D37 was written at the first checkpoint and revised at each. D35's tagging clause and
completeness question closed into it; D33's open question — the index a load diagnostic
about the host's YAML needs — closed the other way, with the host sending complete spans.

Settled in planning, each with its alternatives in D37: defaults run at load, not at
compile or render; the reads are lexical and known at compile, so a cycle is refused before
any data exists; ordering happens at link, since an inherited default is another file's
body (invariant 11); the core parses no YAML — the shell sends values with a table from
field path to `{start, end, range}`; and the envelope grows additively, `ambient` by bound
name and `sources` by binding name.

## Refinement

Checkpoint 1 was a breaking change: the bindings file's top-level `lem-let`s stopped being
locals of its entry, so a bare sibling read compiles to a read of the ambient value's
field.

Checkpoint 2's review found eight defects, all fixed with tests, and settled the rules the
plan had left open: the host's own values load first, then each bindings file, and a step
reading a value not loaded yet errors before it runs; once a value fails no later plan
runs; a string is never `html`; a whole number is a `float` when exactly representable; a
page-backed record loads only from its own document; `from` names are unique across
bindings files; and an omitted field is `null` exactly when `omittable(ty)` — one predicate
for the loader and record literals, after the review's suggestion to drop `any` from the
loader's rule broke `any?`.

Checkpoint 3's review found eight more, three of them wasm traps the finders reproduced
through the staged artifact. All three came from resident state that had drifted from the
program it was derived from: the store outlived the module a relink replaced, so a stored
`layout` closure named a renumbered function; load diagnostics were placed against unit
indexes that run ahead of the module when a link fails; and a failed compile installed a
new declaration over the old program. The fix is one struct, `Linked`, holding the module
with the declaration, indexes, and store it linked with, replaced as a unit — a new program
binds nothing until data is loaded against it. The other five: host spans reaching the
line index unchecked (now refused at the door); the declaration's order sorted away and
repeated keys swallowed by the maps the wire used (an ordered visitor — strictly needed
only for the declaration's order, kept for the envelope so every host meets the loader's
rules); source entries deserialized three times (now once); `Map::from_iter`'s per-key
scan quadratic on keys already unique (`Map::from_unique`); and the envelope's document
value read typed by templates while bound unvalidated — a gap `plan()` closes, recorded as
a placeholder naming ∆RGQ rather than validating a value that unmatched documents share.

The `host/` corpus plays the shell: front matter parsed by `saphyr` with its character
positions converted to bytes, each case the sole member of the glob that matches it. Two
items spun off: [[∆HF0]] (reads through calls count for ordering) and [[∆SSw]] (a
span-by-text test helper).

## Retrospective

**Resident state that is a function of other resident state has to live with it.** Three
traps and one silent misplacement came from `store`, `host`, and `unit_indexes` each having
been valid for the module at some point and drifting on their own. "Stale until fixed" was
stated for the module and assumed for the rest; bundling them made it true by
construction. A `Core` field derived from another goes in one struct with it from the
start.

**In a release wasm a debug assertion is not a guard, and a panic is not an error.**
`Span::new`'s `debug_assert` let inverted spans through, and any panic under a
wasm-bindgen borrow poisons the instance for the rest of the process. Offsets a host
supplies are checked where the text is in hand, before anything keeps them.

**Reproducing against the staged artifact is what separates a finding from a worry.** Both
review rounds had finders and verifiers drive `compile`/`loadData`/`render` through
`packages/core` with node one-liners. Every trap above was confirmed that way, and one
plausible candidate — the cost of cloning `Rc` values per render — was dismissed the same
way.

**Tests that assert hand-counted byte offsets are written by trial.** Most diagnostic
assertions in the loader were guessed, run, and copied back from the failure. That is
[[∆SSw]].

**A forward reference the gate cannot read will rot.** The placeholder gate matches "not
yet"; a note phrased "still to come (∆d1N)" would have survived ∆d1N landing. Placeholders
use the gated phrase, and since the gate walks `crates/` and `docs/` only, a promise made
in `packages/` is at best a `grep` away.

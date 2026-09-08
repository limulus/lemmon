---
id: ∆ZK8
completed: 2026-09-08 08:43:19 -07:00
---

## Backlog item

> - ∆ZK8 Lift the wire types out of `crates/lemmon-wasm/src/lib.rs` and the
>   `dump`/`tag_dump` renderer out of `crates/lemmon-syntax/src/parser/mod.rs` — 267 lines
>   of serde DTOs around 152 of boundary logic, and a debug-format renderer that is not
>   parsing. ∆iCW adds more DTOs to the first

## Planning

Split off [[∆FQw]]'s measurement pass as its own item, because the two files are in other
crates and neither is `compile/`. Both had the same shape as the main finding: a module
whose `//!` states one thing while the file holds two. `crates/lemmon-wasm/src/lib.rs`
carried 267 lines of serde DTOs around 152 lines of boundary logic and 25 of
`#[wasm_bindgen]` shim; `crates/lemmon-syntax/src/parser/mod.rs` carried a 156-line
debug-format renderer that is not parsing.

Both were in scope under the Refactors epic's own rule — ∆iCW adds more DTOs to the wasm
boundary — and both were cheap: the wasm file already separated its shims from its
`*_json` logic, so this only moved the third layer out.

## Refinement

Two deviations from the plan, both found by reading rather than assuming.

**The wasm tests did not move.** The plan had the diagnostic-placement and JSON→`Value`
tests going to `wire.rs` with the code they cover. Every one of them drives `Core` through
`compile_json`/`load_data_json`/`render_json`, so they are boundary tests, not wire-type
tests, and they stayed in `lib.rs`. `wire.rs` therefore has no tests of its own and is
covered entirely through them, at 97.03% lines.

**Visibility was got wrong first, mechanically.** A regex gave every wire item and field
`pub(crate)`, which compiled and was wrong: eight items and twelve fields are named nowhere
in `lib.rs`. The concern is real rather than stylistic — `SpanOut::placed` and
`Diagnostic::placed` own the byte-offset-to-UTF-16 conversion (D33), and a crate-visible
`SpanOut.start` invites a later edit to bypass them and emit a diagnostic whose `range`
disagrees with its span. The fix could not be hand-guessed either, because a type reachable
through a `pub(crate)` field must itself be `pub(crate)`; the compiler's `private_interfaces`
lint names them, so the promotion was driven off that until it settled. Five items are now
private, eleven crate-visible, and the boundary enforces it.

`Core` and `version` stayed at the crate root throughout: `tests/boundary.rs` imports them
and exercises them across the real ABI.

## Retrospective

**A mechanical visibility pass is a code smell that compiles.** Widening everything to
`pub(crate)` because the regex was easy to write produced a diff that passed the whole gate
and still weakened the boundary's one invariant. What caught it was asking, per item,
whether the consumer names it — a check worth running after any extraction, since moving
code is exactly when visibility gets widened without thought.

**Read the tests before deciding where they go.** The plan assigned them by subject matter
from a structural map; one look at what they actually call showed they belonged elsewhere.
The map was accurate and the inference from it was still wrong.

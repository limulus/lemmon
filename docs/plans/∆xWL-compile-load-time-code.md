# ∆xWL Give `compile/mod.rs`'s load-time code a module — plan

Written on 2026-09-19 by the session that surveyed the repository's organization for Eric,
against `main` at `d3c8f11`. An analyst proposed moving this code into one new file,
`compile/loader.rs`; an agent told to refute the proposal confirmed the line arithmetic
and rejected the shape. So this plan states the facts, the question the building session
has to settle with Eric before moving anything, and a recommendation. Nothing was built.
Re-derive the line numbers if the files have changed. When ∆xWL's journal entry is
written, its Planning section absorbs this file and the file is deleted.

## Start here

1. Read `CLAUDE.md`, the ∆xWL item in `BACKLOG.md`,
   `docs/journal/∆FQw-split-the-compile-module.md` (the earlier split of this directory,
   its test, and its method), and D37.
2. `npx @limulus/delto@1 claim ∆xWL`.
3. Settle "The question" below with Eric before the first move.

## What exists today

`crates/lemmon-vm/src/compile/mod.rs` has 983 production lines and 1,648 test lines. Its
`//!` (lines 10–12) says: "What is left in this module is the two entry points and the
machine they drive: [`compile`] over a whole site, [`compile_unit`] over one prepared
file, and the [`Compiler`] itself — its frames, its emission, and its delegation to the
resolver." That sentence is false today. Under the `// --- Driver ---` marker (line 349)
the `impl Compiler` block also holds about 400 lines that none of ∆FQw's five extractions
could have taken, because none of it existed at ∆FQw's commit (`1a3d800`); it arrived with
∆iCW, ∆HF0 and ∆eag. It is two things.

**The functions a file compiles for the loader to run before any render (D37)** — about
270 lines:

- `compile_defaults` (`mod.rs:438`), which lowers every record's field defaults and
  records every `by` index; `index_field` (536); `compile_step` (600);
  `function_for_loader` (634).
- `Runs` and `refused_at_load` (`compile/expr.rs:248`, `:260`), about 40 lines sitting
  between two `impl Compiler<'_>` blocks in the expression lowerer.

**The recorder behind `Unit::uses`** — about 130 lines: `Implicit` (314), `Collecting`
(320) with `Collecting::root` (337), `collecting` (669), `note_local_read` (691),
`note_read` (701), `note_member_read` (712), `note_call` (737), `root_of` (751). It
records which ambient values and record fields a function reads and which functions it
calls, so the link step can order what the loader runs. It is not load-time-only code:
`compile_functions` (368), which stays, opens a collector for every `lem-fn` (line 407),
and the `note_*` methods fire from the ordinary expression walk.

Who calls in from outside the cluster:

- `compile_defaults` from `mod.rs:225`; `function_for_loader` from `items.rs:535`;
  `collecting` from `mod.rs:407`.
- `note_local_read` from `expr.rs:177`; `note_read` from `expr.rs:205` and `:221`;
  `note_member_read` from `calls.rs:200` and `expr.rs:350`; `note_call` from
  `calls.rs:649`; `root_of` from `calls.rs:644`.
- `Implicit` is named at `items.rs:17`, `:539` and `mod.rs:407`. `Collecting` is the type
  of the `Compiler.uses` field (`mod.rs:304`); its field `uses` is read by
  `compile_functions` (`mod.rs:427`) and its field `runs` by `unavailable_at_load`
  (`expr.rs:239`).
- `index_field`, `compile_step` and `Collecting::root` have no caller outside the cluster.

A child module sees its ancestors' private items, so no private `Compiler` field or method
has to change for code that moves down into `compile/`. The reverse is what costs: every
moved item that `mod.rs` or a sibling still calls becomes `pub(super)`. For the whole
cluster that is twelve items — the eight methods and two types listed above, plus the
fields `Collecting::uses` and `Collecting::runs`. (The original proposal counted ten and
missed the two fields.)

Tests: `mod.rs:1052–1638`, 587 lines, open with the marker "Defaults and `by` indexes,
compiled for the loader (∆iCW)". There is no closing marker, so line 1638 is a judgement
from the tests' subjects. The section has its own `shell_host` (1056), `PAGE` (1064),
`shown_with` (1066), `unit_with` (1071), `FNS` (1157), `uses_of` (1168) and `param`
(1173), but it is not self-contained: it calls `untyped_host` (defined at 1003) ten times,
and `shown_with` calls `shown` (1037). A new file's test module is a sibling of
`compile::tests`, not a descendant, so it cannot see those two. Either they widen to
`pub(super)` and are reached as `super::super::tests::…` (`load/validate.rs:480–482` does
this), or they move to `crates/lemmon-vm/src/testing.rs`. ∆fBj's first checkpoint moves
`shell_host`, `shown`, `untyped_host` and the Page declaration there; if it has landed,
use it.

## The question

Where does the cut go, and what are the files called? `compile/loader.rs` is out on two
grounds: `crate::load` exists and `load/mod.rs:439` defines `struct Loader`, so the name
reads as that module; and a file holding both halves cannot state one responsibility in
its `//!`, which is this repository's only test for file size.

1. **Two files (recommended).** `compile/uses.rs` takes the recorder, with `//!`: "What a
   function reads and calls, recorded as it is lowered, so the link step can order what
   the loader runs (D37, ∆HF0)." `compile/load_time.rs` takes `compile_defaults`,
   `index_field`, `compile_step`, `function_for_loader`, `Runs` and `refused_at_load`,
   with `//!`: "What a file compiles for the loader to run before any render: a record's
   computed defaults and `by` indexes (D37)." Read `function_for_loader`'s caller at
   `items.rs:535` before fixing that sentence; if computed bindings come through it, the
   sentence names them too. `mod.rs` keeps `reserve`, `compile_functions` and
   `compile_entry`, and its existing `//!` sentence becomes true with two bullets added
   to its module list.
2. **The recorder alone**, as `compile/uses.rs`. The smallest honest move: the recorder is
   the part that is plainly not "driver". `mod.rs` stays near 850 production lines and its
   `//!` needs a sentence about the load-time phase.
3. **One file with a two-sentence `//!`**, `compile/load_time.rs`. The least churn, and it
   fails the one-responsibility test on arrival. Not recommended.

Under option 1 the test section divides too: the tests about `uses_of` go with the
recorder, the defaults and `by` index tests with `load_time.rs`. Read each test before
assigning it; ∆ZK8's journal is the precedent for that.

## Doc links at risk

Three links to `refused_at_load`: `unit.rs:942`, `expr.rs:237`, `mod.rs:322`. Keep
`pub(crate) use <new module>::{Runs, refused_at_load};` in `mod.rs` so `unit.rs:22`'s
import path does not change. `npm run rust` cannot see a broken intra-doc link; run
`cargo doc --workspace --no-deps` by hand.

## Acceptance

- `npm run corpus:bless` leaves an empty diff.
- ∆FQw's check: the new files' logical lines, as a multiset, equal the originals' once
  imports, doc headers and visibility qualifiers are normalised away.
- `cargo doc --workspace --no-deps` by hand adds no errors.
- `npm run verify`. `npm run rust:wasm:test` is not needed: nothing at the boundary moves.

## Out of scope, noted

- `mod.rs`'s test module would still hold tests whose production code lives in siblings:
  about 509 lines (1677–2185) written from a `<lem.mark(…)>` call site, exercising
  `calls.rs`, `schema.rs` and `rules.rs`, and about 215 lines (2389–2602) on `names.rs`'s
  rules. `calls.rs` (657 production lines), `surfaces.rs` and `narrowing.rs` have no test
  module. Routing those tests is reading work, not a mechanical move, and saves about ten
  lines; it was judged the weakest proposal in the survey and was not minted.
- `index_u32` is defined twice — `compile/mod.rs:980` (`pub(crate)`, "fewer than 2^32
  items") and `document/mod.rs:279` (private, "sources are smaller than 4 GiB") — and
  `add_u32` (`unit.rs:657`) is a third of the same family. Having `document/` import
  `compile`'s copy would create a `document` → `compile` import for a three-line
  function. One copy needs a home above neither module and a choice between the two panic
  messages. Not minted.

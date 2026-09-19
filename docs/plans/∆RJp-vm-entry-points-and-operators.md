# ∆RJp Make `vm.rs` a directory — plan

Written on 2026-09-19 by the session that surveyed the repository's organization for Eric,
against `main` at `d3c8f11`. An analyst proposed the split and an agent told to refute it
confirmed every number below by reading the source, including the call sites of all six
operator functions. Nothing was built. Re-derive the line numbers if `vm.rs` has changed.
When ∆RJp's journal entry is written, its Planning section absorbs this file and the file
is deleted.

## Start here

1. Read `CLAUDE.md`, the ∆RJp item in `BACKLOG.md`, and
   `docs/journal/∆FQw-split-the-compile-module.md` for the test and the method.
2. `npx @limulus/delto@1 claim ∆RJp`.
3. Copy `crates/lemmon-vm/src/vm.rs` aside, for the multiset diff in Acceptance.

## What exists today

`crates/lemmon-vm/src/vm.rs` has 912 production lines and 1,565 test lines. Its `//!`
(lines 1–15) opens "The stack machine that runs a [`Module`]" and goes on to describe the
value stack, the buffer stack, the iterator stack, `Vm::fail` and `Machine::invoke`. It
describes one of the three things in the file. ∆FQw measured the file at 619 production
lines and found it fine; the growth since is mostly the entry points (`render_page`
arrived with ∆Bhz).

- **The three ways a module is run**, lines 50–169: `render` (62), `render_page` (89),
  `entry_of` (134), `invoke` (153). `render_page` carries D43's protocol — read the
  record's entry field, push the record and the body, call it — and words three failures
  that have no instruction to blame. It is why `vm.rs:24` imports
  `crate::load::LoadedPage`.
- **What an operator means for a pair of values**, lines 799–911: `index_into` (799),
  `binary` (824), `int_arith` (862), `float_arith` (875), `ordered` (887), `symbol` (897).
  Free functions over two `Value`s and an `Op`, with no `&self`, no stack and no frame.
  They are the same kind of code as `Value::equals` and `Value::order`, and the comment
  at `vm.rs:848` already states that seam.
- **The machine**, everything else: the depth caps, `Frame`, `Iter`, `Vm`, `step`,
  `impl Machine for Vm`.

## The move

Rename `crates/lemmon-vm/src/vm.rs` to `crates/lemmon-vm/src/vm/mod.rs` with `git mv`, then
extract two files.

- `vm/entry.rs` takes lines 50–169 and the test module `mod pages` (2328–2477). `//!`:
  "The three ways a module is run: a file's entry, a loaded page through its record's
  entry field (D43), and one function for the loader."
- `vm/operators.rs` takes lines 799–911. `//!`: "What an arithmetic, comparison,
  membership, or index operator means for a pair of values, with `Value::equals` and
  `Value::order` owning the two questions a sort also asks."
- `vm/mod.rs` keeps the rest, adds `pub use self::entry::{invoke, render, render_page};`
  so `lib.rs:59` and `crate::vm::invoke` (`load/mod.rs:34`, `load/validate.rs:21`) are
  untouched, and its `//!` then describes what is left in it.

233 production lines and 150 test lines move; `vm/mod.rs` is left with about 680
production lines. The change adds about 20 lines: two `//!` headers, two `mod` lines, and
the imports each new file repeats.

## Visibility

- `binary` and `index_into` become `pub(super)`: `step` calls them from `vm/mod.rs`, and a
  parent does not see a child's private items. `int_arith`, `float_arith`, `ordered` and
  `symbol` are called only from within the moved range and stay private.
- `entry.rs` needs nothing widened. A child module sees its ancestors' private items and
  private fields, so `Vm`, its `out`, `stack` and `fragments` fields, and `new`,
  `run_entry`, `run_until`, `call_value`, `pop` and `finish` are all reachable as they are.
- `entry_of` is called only by `render` and `render_page` and stays private.

## Tests

- `mod pages` (2328–2477) uses none of the main test module's helpers (`function`,
  `module`, `run`). It imports `crate::load::tests::{build, object, shell}` and
  `crate::testing::at`. Its `use super::*` resolves today through `vm.rs`'s `mod tests`,
  so check which names it takes from that module's own `use` lines; in `entry.rs` it has
  to import them itself.
- The arithmetic and index tests drive the operators through `run()` (`vm.rs:2024–2041`).
  They are machine tests and stay in `vm/mod.rs`.

## Doc links

The `[`render`]` links at `vm.rs:73` and `:88` are inside `render_page`'s doc, which is why
all three entry points move together. `rendered.rs:206` writes `[`render`](crate::render)`
and `lib.rs:6` and `:73` resolve against the root re-export; none changes. Run
`cargo doc --workspace --no-deps` by hand anyway: ∆FQw's retrospective records six links
broken by a move of this size with every gate green.

## What this does not do

`vm` and `load` still import each other — `LoadedPage` one way, `invoke` the other. The
import moves into `entry.rs` with `render_page`. Ending it would mean moving `LoadedPage`
to a neutral module or changing `render_page`'s signature, which is not a pure move and
not worth it at this size.

## Why before ∆xBO and ∆99b

∆xBO makes `render` report which units and records a page read. D43's consequences section
says the VM gains a reference value and "render needs the store's page table beside the
globals", which ∆99b implements in `render_page`. Both land in the entry points.

## Acceptance

- `npm run corpus:bless` leaves an empty diff.
- `cargo test -p lemmon-vm` passes, not only the corpus: `mod pages` renders a page end to
  end through compile and load, so a mistake there shows in the unit tests while the
  corpus stays green.
- ∆FQw's check: the new files' logical lines, as a multiset, equal the originals' once
  imports, doc headers and visibility qualifiers are normalised away.
- `cargo doc --workspace --no-deps` by hand adds no errors.
- `npm run verify`. `vm.rs` holds `MAX_DEPTH` and `MAX_NATIVE_DEPTH`; neither value
  changes and both stay in `vm/mod.rs`, so `npm run rust:wasm:test` is not required. Run
  it if either constant is touched for any reason.

## Noted while reading, not part of this item

- The `Op::Mark` arm of `step` (`vm.rs:608–691`) is 84 of `step`'s 326 lines. Lifting it
  to a method would not remove `step`'s `too_many_lines` allow, so the only gain is
  readability.
- `Vm::constant_str` (`vm.rs:294–296`) is a one-line wrapper over `constant_name`; sixteen
  call sites use one or the other with no stated rule. A sentence on `constant_name`
  saying which to reach for would do.
- `Apply::next_id` (`builtins/mod.rs:720–724`), a test double, reimplements `Vm::next_id`
  (`vm.rs:785–796`). If the VM's counting changed, `uniqueId`'s unit test would still pass
  and only the corpus would notice.

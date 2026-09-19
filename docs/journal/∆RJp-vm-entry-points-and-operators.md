---
id: ∆RJp
completed: 2026-09-19 14:24:00 -07:00
---

## Backlog item

> - ∆RJp Make `crates/lemmon-vm/src/vm.rs` a directory — its `//!` describes the stack
>   machine, but the file also holds the three run entry points (`render`, `render_page`,
>   `invoke`) and the operator table, 233 production lines with no `Vm` in them. ∆xBO and
>   ∆99b both land in the entry points. Planned in
>   `docs/plans/∆RJp-vm-entry-points-and-operators.md`

## Planning

The plan came from the 2026-09-19 organization survey, and every claim in it had been
checked by the refuting pass. `vm.rs` held the machine and two things that are not the
machine: the four entry points that start a run (`render`, `render_page`, `entry_of`,
`invoke`), and six free functions that say what an operator means for a pair of values
and touch no stack or frame. The move was `git mv vm.rs vm/mod.rs`, then `vm/entry.rs`
and `vm/operators.rs`, with re-exports so `crate::vm`'s paths stay as they were. The plan
predicted two widenings, `binary` and `index_into`, both called from `Vm::step`.

It was placed before ∆xBO and ∆99b because both land in the entry points: ∆xBO makes
`render` report what a page read, and D43 has `render_page` take the store's page table.

What it does not do: `vm` and `load` still import each other, `LoadedPage` one way and
`invoke` the other. The import moved into `entry.rs` with `render_page`. Ending it means
moving `LoadedPage` or changing `render_page`'s signature, which is not a pure move.

## Refinement

Built and reviewed together with ∆KIl by one pair of Opus agents, and committed
separately by path; the builder showed each item passes its tests without the other.

The visibility prediction held: the builder moved the code with no qualifiers, and the
compiler named exactly `index_into` and `binary`. Since ∆rKx, `invoke` is `pub(crate)`, so
its re-export is `pub(crate) use`. `operators.rs` names nothing from the machine.
`entry.rs` names ten private members of `Vm`, which its `//!` says.

Nothing on the depth-capped recursive path moved. `step`, `Machine::invoke`, `run_until`
and `call_value` stay in `vm/mod.rs`; the `invoke` that moved is the loader's entry point,
and `binary` and `index_into` return before `step` can recurse. No `#[inline]` or `#[cold]`
attribute existed to lose. `rust:wasm:test` was run anyway and passes.

The review moved two tests the plan had not named. `mod pages` went to `entry.rs` as
planned, but the tests that pin `invoke` (it refuses to collect fragments) and `entry_of`
(it panics on a file the module lacks) had stayed in `vm/mod.rs`, so `cargo test
vm::entry` could pass with `invoke` broken. They are in `entry.rs` now, and the two
fixtures they use, `function` and `module`, are `pub(super)` in `vm::tests`.

`vm/mod.rs` is 685 production lines, from 911; `entry.rs` is 134 and `operators.rs` 123.
The corpus diff was empty and the moved lines were proved equal as a multiset; the only
changed lines are four test continuation lines rustfmt rejoined after a dedent.

The move also made ∆RuS's pointer to `call_dynamic` in `vm.rs` false; the backlog item
now says `vm/mod.rs`.

Noted in the plan and still true, none of it part of this item: the `Op::Mark` arm is 84
of `step`'s lines, and lifting it would not remove `step`'s `too_many_lines` allow;
`Vm::constant_str` is a one-line wrapper over `constant_name` and sixteen call sites pick
between them by no stated rule; and the test double `Apply::next_id` in `builtins/mod.rs`
reimplements `Vm::next_id`, so a change to the VM's id counting would leave `uniqueId`'s
unit test passing and only the corpus would notice.

## Retrospective

This was the one plan in the survey's set whose every claim had been verified, and it was
the one build with no departure in production code. The review still found something,
and it was about tests: a plan that lists the test module to move should list the tests
of each moved function, found by grepping for the function's name in the test module.

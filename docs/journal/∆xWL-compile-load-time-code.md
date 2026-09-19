---
id: ∆xWL
completed: 2026-09-19 13:52:30 -07:00
---

## Backlog item

> - ∆xWL Give `compile/mod.rs`'s load-time code a module — about 400 production lines that
>   ∆iCW, ∆HF0 and ∆eag added after ∆FQw (record defaults, `by` indexes, the recorder of
>   reads and calls behind `Unit::uses`) sit in the driver, whose `//!` claims only the entry
>   points and the `Compiler`. They are two responsibilities, so the cut is decided at plan
>   time. Planned in `docs/plans/∆xWL-compile-load-time-code.md`

## Planning

The plan came from the 2026-09-19 organization survey. About 400 production lines of
`compile/mod.rs` were what a file compiles for the loader and the recorder of what a
function reads and calls; the file's `//!` named neither. The first proposal was one file,
`compile/loader.rs`. The refuting pass rejected it on two grounds: `crate::load` already
defines `struct Loader`, so the name reads as that module, and the cluster is two
responsibilities, since every `lem-fn` records through the recorder whether or not the
loader ever runs it. A file holding both could not state one thing in its `//!`.

The plan ended in a question with three answers: two files, the recorder alone, or one
file. Eric chose two: `compile/uses.rs` for the recorder and `compile/load_time.rs` for
the load-time functions, with `Runs` and `refused_at_load` from `compile/expr.rs`, and a
re-export in `compile/mod.rs` so that `link/` imports them as before.

The plan counted twelve widenings to `pub(super)` for the whole cluster, and noted that
the load-time tests were not self-contained: they called `compile::tests`' `untyped_host`
and `shown`.

## Refinement

Fifteen items and fields widened, not twelve. The plan's count was for the cluster moved
as one; cut in two, each file names the other's fields (`Implicit`'s two binders,
`Collecting::body_read`). The builder was told to derive the set itself, after ∆fBj's plan
had got a widening wrong, and confirmed each by removing it and watching the build fail;
the reviewer repeated that. `pub(super)` inside `compile/uses.rs` means visible within
`compile/`, and nothing became visible outside it.

`Implicit` went to `uses.rs`. `load_time.rs`, `items.rs` and `mod.rs` all construct it, but
only `Collecting::root` and `note_local_read` read it, so it is the recorder's input.

The two new files import each other: `uses.rs` takes `Runs`, which `load_time.rs` defines,
for `Collecting::runs`, and `load_time.rs` takes `Implicit`. The edge is that one field.
Moving `Runs` to `uses.rs` would put a load-time notion in the recorder's file, so it
stays, deliberately.

The review changed the tests more than the code:

- Both new test modules had reached into `compile::tests` for `untyped_host`, which widened
  that fixture. They now use `testing::untyped_host`. The two differ only in mark claims,
  and neither file's tests use a mark.
- `unit_with` had arrived in `testing.rs` as a third copy of one 14-line body. `unit_of`
  and `unit_against` are now written in terms of it, and `testing.rs` is 99 lines again.
- One test asserted the recorder's rows and also the default function's name and
  parameters, which `load_time.rs` produces. It is two tests now, one in each file, so
  658 tests run where 657 did.

`compile/mod.rs` is 554 production lines, from 916; `uses.rs` is 156 and `load_time.rs`
299. The corpus diff was empty, the moved lines were proved equal as a multiset, and
`cargo doc --workspace --no-deps` is silent.

Left out, as the plan had them: the second `index_u32` (one in `lemmon-syntax`, one in
`compile`), and about 700 lines of `compile/mod.rs`'s test module that test `calls.rs`,
`schema.rs` and `names.rs` and could move beside them.

## Retrospective

A widening count in a plan is a prediction about a cut that has not been made. It was
wrong here and in ∆fBj, in opposite directions. The instruction that worked was to give
the builder the visibility rule and have it derive the set by removing each qualifier;
the plan's number is then only something to compare against.

The builder was told not to move a test onto a different host, and obeyed by widening a
private fixture. The reviewer showed the hosts were interchangeable for these tests. The
instruction was too blunt: what mattered was that no test's outcome depend on the swap,
and that can be checked.

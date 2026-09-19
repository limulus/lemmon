---
id: ∆fBj
completed: 2026-09-19 12:35:21 -07:00
---

## Backlog item

> - ∆fBj Split `crates/lemmon-vm/src/unit.rs` into the unit data model and a `link/` directory
>   — its `//!` names two responsibilities, and all 978 production lines it has gained since
>   ∆FQw landed in the link half, which also causes the two-way imports with `exports` and
>   `closure`. ∆Qr2 serialises the data half and ∆99b grows the plans, so reshape first.
>   Planned in `docs/plans/∆fBj-split-unit-and-link.md`

## Planning

The plan came from the 2026-09-19 organization survey. `unit.rs`'s `//!` named two
responsibilities, the unit data model and the link step, and all 978 production lines the
file had gained since ∆FQw were in the second. The survey weighed three cuts and took the
one that follows the `//!`: `unit.rs` keeps `Unit` and its sixteen satellite types, and a
`link/` directory takes the rest, with `closure.rs` beside it because the link step was
its only caller.

Inside `link/`, the plan builders (`Places` and its three builders, the ordering helpers,
and `plan.rs`'s `order`) are a second responsibility and became `link/plans.rs`, reached
through one function. A third file for the staleness comparisons was considered and set
aside: the signature comparison sits inside the import-resolution loop and shares its
lookup, so lifting it means walking `unit.imports` twice to keep today's diagnostic
order. That would not be a pure move.

Three checkpoints, so that each step was provable on its own: the shared test helpers
first, because a test module divided between two files cannot share private helpers; then
the coarse move with the test module whole; then `plans.rs` and the division of tests.

The plan said plainly that this item does not end the two-way import between `compile`
and the link step. After ∆rKx the link step imports `Runs`, `index_u32` and
`refused_at_load` from `compile`, and `compile` imports nothing from it. The two-way
imports that ended here are `unit` with `exports` and `unit` with `closure`.

## Refinement

An Opus agent built each checkpoint and a second, told not to modify files, reviewed it.
Every checkpoint ran the same 667 tests, left `npm run corpus:bless` with an empty diff
(`tests/corpus/host/` is the only two-file coverage of the link step), and left
`cargo doc --workspace --no-deps` silent. The moves were proved as ∆FQw's was, and the
second reviewer also compared the moved blocks byte for byte.

Where the plan was wrong:

- It said `add_u32` must widen to `pub(super)`. It need not: `link/plans.rs` is a child of
  `link`, and a child names its parent's private items. The builder followed the plan and
  the review caught it; `pub(super)` on an item of `crate::link` means the whole crate.
  Nothing widened in the end, and `plan::order` narrowed to private.
- It said the `Decls` trait is used only by the code that moved to `plans.rs`.
  `check_host_fields` calls a `Decls` method too, so both files import it.
- Its `//!` for `unit.rs` called the unit's outputs "the whole of what a unit is a
  function of". A unit is a function of its file's text plus the facts it
  consumed (D25); the review caught that the header had the relation backwards.
- It said `compile/mod.rs` already called the shared declaration `AMBIENT`. It called it
  `PAGE`; only `compile/schema.rs` used `AMBIENT`, and that name was kept.

Checkpoint 1's review changed the shape of the shared fixtures. The shared `shell_host`
names no url field, because `compile/schema.rs`'s tests need it that way, so `unit.rs`'s
tests had chained `.addressed_by("url")` at 21 call sites though only two tests depend on
it. A test written later against the bare host would pass with D43's url check unarmed.
`testing.rs` now has `addressed_host`, and the link tests use it. Three look-alike helpers
whose bodies differ from the shared ones stayed where they were. `host.rs`'s and
`load/mod.rs`'s `shell()` stayed too: the ambient order they build is pinned by a test.

After the change `unit.rs` is 304 production lines, `link/mod.rs` 588, `link/plans.rs`
636, `link/closure.rs` 184 and `plan.rs` 111. The item's diff is +3,170 −3,112, nearly
all of it moved lines; test lines fell by about 20, from the shared fixtures.

Left as it is: `mod closure` is declared in `link/mod.rs`, but after checkpoint 3 only
`plans.rs` uses it. Putting it under `plans` means making `plans.rs` a directory. It is
worth doing if `link/mod.rs` ever reaches for `ReadClosure`.

## Retrospective

The visibility rule the survey wrote down for every split — a child module sees its
ancestors' private items, and a parent does not see a child's — was in the same document
as the claim that `add_u32` must widen. The rule was right and the application was wrong,
and neither the planner nor the builder checked one against the other. A plan's
visibility budget should be derived by asking, for each item, which module is the
ancestor; and `pub(super)` directly under the crate root should be read as `pub(crate)`.

The coarse move before the fine one was worth its extra checkpoint. Checkpoint 2's review
could be almost entirely mechanical, which left checkpoint 3's review free to read the one
new function and the division of the tests.

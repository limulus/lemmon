---
id: ∆KIl
completed: 2026-09-19 14:24:01 -07:00
---

## Backlog item

> - ∆KIl Lift the page table out of `crates/lemmon-vm/src/load/mod.rs` into `load/pages.rs` —
>   `LoadedPage`, `PlannedPage`, `Store::page`/`plan`, the url checks and `Loader::page`. The
>   file's `//!` still states one responsibility; the reason is ∆99b, which adds generator
>   expansion and neighbours to exactly this code, so drop this item if ∆99b has landed.
>   Planned in `docs/plans/∆KIl-load-page-table.md`

## Planning

The plan came from the 2026-09-19 organization survey, and opened by saying nothing was
broken. `load/mod.rs` had 697 production lines and a `//!` that stated one responsibility,
so by this repository's test the file was not too big. The reason to act was what lands
next: ∆99b adds a `for:` key check, the expansion of a page template into a page per
item, neighbour fields, and changes to what `plan()` reports, all of it in the page table.
The file had grown from 524 to 697 production lines over five commits, 100 of them in
∆Bhz. The item guards against the page table growing inside the loader until the `//!` no
longer covers it, and was to be dropped if ∆99b had landed first. It had not.

The move: `load/pages.rs` takes `LoadedPage`, `PlannedPage`, `Store::page`, `Store::plan`,
`Loader::page` and the url rules, with their tests, and `load/mod.rs` re-exports the two
types. The refuting pass had corrected the first proposal's widening list to
`url_diagnostics` and `Loader::page`.

Not taken: `load/envelope.rs` for `check_envelope`, `check_page` and `collects`. The `//!`
already covers them, since a caller bug being an `Err` is what they are, and ∆99b does not
land there.

## Refinement

Built and reviewed together with ∆RJp and committed separately by path; each item passes
its tests without the other.

Three widenings, not two. `url_diagnostics` and `Loader::page` are `pub(super)` because
`load()` calls them. The third is the test fixture `load::tests::page`: the plan said the
child's tests could reach the fixtures because `load::tests` is `pub(crate)`, which is
true only of the fixtures that are themselves `pub(crate)`.

Six tests moved, the ones about urls and the plan. Two tests of `Loader::page`'s refusals
stayed in `load::tests`: one asserts the seam between `bindings`, which stayed, and `page`,
which moved; the other asserts `Loader::page` alone, but moving it means widening three
more fixtures. `pages.rs`'s test module says where they are.

The builder left `load/mod.rs`'s `//!` unchanged, since nothing it described had moved.
The review pointed out that it named none of its child modules, against the convention
`compile/mod.rs` follows. It now names `validate`, `place` and `pages`.

`load/mod.rs` is 539 production lines, from 697, and `load/pages.rs` 183. The corpus diff
was empty and no moved line changed.

For ∆99b's planner: D43's consequences section also has `render` take the store's page
table beside the globals, so ∆99b changes `render_page`, now in `vm/entry.rs`, and its
plans land in `link/plans.rs`. This item concentrates the load side of ∆99b in one file.
It does not contain all of ∆99b.

## Retrospective

"A `pub(crate)` module's items are reachable" was the plan's error, and it is the same
kind as ∆fBj's: a statement about visibility made from the declaration of the container,
not of the item. The builder's method of moving with no qualifiers and widening what the
compiler names found it at once.

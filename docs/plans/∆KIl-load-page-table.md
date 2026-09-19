# ∆KIl Lift the page table out of `load/mod.rs` — plan

Written on 2026-09-19 by the session that surveyed the repository's organization for Eric,
against `main` at `d3c8f11`. An analyst proposed splitting `load/mod.rs` in two places; an
agent told to refute the proposal corrected its visibility list and could not confirm its
test line counts. This plan keeps the smaller half of the proposal. Nothing was built.
Re-derive the line numbers if the file has changed. When ∆KIl's journal entry is written,
its Planning section absorbs this file and the file is deleted.

## Read this first: nothing is broken

`crates/lemmon-vm/src/load/mod.rs` has 697 production lines, and its `//!` does state one
responsibility: the loader behind `loadData` and the two kinds of problem it reports. By
this repository's own test the file is not too big. The reason to act is what lands next.
∆99b (generators and neighbours, D43) adds a `for:` key check, the expansion of one page
template into a page per item, the `prev`/`next` neighbour fields, and changes to what
`plan()` reports — all of it in the page table this item moves. The file grew from 524 to
697 production lines over five commits, 100 of them in ∆Bhz; nobody has measured what
∆99b adds.

So this item guards against one specific drift: the page table growing inside the loader
until the `//!` no longer covers it. If ∆99b has already landed, drop this item; whoever
next touches the file applies the `//!` test to what is there then.

## Start here

1. Read `CLAUDE.md`, the ∆KIl and ∆99b items in `BACKLOG.md`,
   `docs/decisions/D43-generated-pages-and-neighbours.md`, and
   `docs/journal/∆FQw-split-the-compile-module.md` for the method.
2. `npx @limulus/delto@1 claim ∆KIl`.
3. Copy `load/mod.rs` aside, for the multiset diff in Acceptance.

## The move

New file `crates/lemmon-vm/src/load/pages.rs`. `//!`: "The page table: one page's front
matter loaded as the record it was compiled as, where each page stands, and what `plan()`
reports (D41, D43)." It takes, from `load/mod.rs`:

- `LoadedPage` (95) and `PlannedPage` (108), with their doc comments from line 91.
- `Store::page` (116) and `Store::plan` (123), as a second `impl Store` block.
  `Store::globals` (141) stays: it is what a render binds, and it reaches `self.page`
  through the `pub` method.
- `page_url` (162), `url_place` (175), `url_diagnostics` (185), through line 219.
- `Loader::page` (626–669), as a second `impl Loader<'_>` block. `load/validate.rs`
  already holds one; this is the crate's pattern.

About 150 production lines. `load/mod.rs` adds `mod pages;` and
`pub use pages::{LoadedPage, PlannedPage};`, so `lib.rs:47–50` and `vm.rs:24` do not
change.

## Visibility

`load/pages.rs` is a child of `load`, so it sees `Store`'s and `Loader`'s private fields
and `load/mod.rs`'s private items, the `Files` alias (436) included. Nothing it reads has
to change. What `load/mod.rs` still calls in the child does:

- `url_diagnostics`: private → `pub(super)`. `load()` calls it at line 267.
- `Loader::page`: private → `pub(super)`. `load()` calls it at line 264 and
  `Loader::bindings` at line 555. `Loader::record` in `validate.rs:232` is `pub(super)`
  for the same reason.
- `page_url` and `url_place` are called only from code that moves (lines 130, 189–203) and
  stay private.

The original proposal listed `check_envelope` and `Files` as the two items to widen. Both
were wrong for this half: `Files` needs nothing, `check_envelope` does not move here, and
the two items above were missed.

## Tests

The url and `plan()` cluster sits at the end of the test module: the fixtures `placed`
(1645), `routed_module` (1651), `placed_page` (1669) and `article` (1677), then six tests
from `refuses_two_pages_at_one_url_across_a_glob_and_the_envelope` (1682) to
`reports_a_loaded_page_that_holds_no_url_rather_than_trusting_the_link` (1839). The test
at 1860 belongs to the loader and stays. That is about 215 lines by my reading; the
survey's figure of 230 was never checked test by test, so read each one. Tests about
`Loader::page` earlier in the module (front matter that is no mapping, a record that is
not what the binding collects) may build their input through the loader's fixtures and
exercise `check_envelope` on the way. Assign those by what they assert, and leave a test
where it is when it asserts both.

`load::tests` is `pub(crate)`, and `vm.rs:2330` already imports `build`, `object` and
`shell` from it, so `load::pages::tests` can reach the same fixtures by path.

## Not taken: `load/envelope.rs`

The proposal also lifted `check_envelope`, `check_page` and `collects` (281–433, about 155
lines) into `load/envelope.rs`. The `//!` already covers them — "A caller bug … is an
`Err`" is what they are — and ∆99b does not land there. Skipped.

## Acceptance

- `npm run corpus:bless` leaves an empty diff.
- ∆FQw's check: the new file's logical lines, as a multiset, equal the originals' once
  imports, doc headers and visibility qualifiers are normalised away.
- `cargo doc --workspace --no-deps` by hand adds no errors. The moved comments link
  `[`link`](crate::link)` and `[`Spans`]`, which resolve from anywhere in the crate.
- `npm run verify`. `npm run rust:wasm:test` is not needed.

## For ∆99b's planner

D43's consequences section also says the VM gains a reference value and that `render`
needs the store's page table beside the globals, so ∆99b changes `render_page` in `vm.rs`
too (∆RJp moves it to `vm/entry.rs`). Its plans land in the link step (∆fBj moves those to
`link/plans.rs`). This item concentrates the load side of ∆99b in one file; it does not
contain all of ∆99b.

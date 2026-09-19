# ∆fBj Split `unit.rs` into the unit data model and `link/` — plan

Written on 2026-09-19 by the session that surveyed the repository's organization for Eric,
against `main` at `d3c8f11`. Two analysts proposed this split independently and a third
agent, told to refute it, checked every claim below against the code. Nothing was built:
no file was moved and nothing was compiled, so every line range and visibility claim here
is a reading of the source, not the result of a build. Re-derive the ranges if `unit.rs`
has changed; functions are named so they can be found again. When ∆fBj's journal entry is
written, its Planning section absorbs this file and the file is deleted.

## Start here

1. Read `CLAUDE.md`, the ∆fBj item in `BACKLOG.md`, and
   `docs/journal/∆FQw-split-the-compile-module.md`, which set the test this file fails
   and the method for proving a move is a move.
2. `npx @limulus/delto@1 claim ∆fBj`.
3. Copy `crates/lemmon-vm/src/unit.rs`, `closure.rs` and `plan.rs` aside before starting,
   so the multiset diff in Acceptance has its originals without reaching for git.

## What exists today

`crates/lemmon-vm/src/unit.rs` is 3,148 lines: 1,392 of production code and a 1,756-line
test module (`#[cfg(test)]` at line 1393). It is the largest production file in the
repository. Its `//!` opens "Per-file compiled units and the link step that joins them",
which is two responsibilities, and the code divides the same way:

- Lines 34–318 are the data model: `Unit` and its sixteen satellite types (`DefaultFn`,
  `IndexDecl`, `BindingFn`, `Supplied`, `Uses`, `Access`, `Call`, `Root`, `TypeRef`,
  `AmbientRef`, `AssetRef`, `ClaimRef`, `ImportRef`, `Param`, `ExportedSignature`,
  `ImportedSignature`), with one method, `ExportedSignature::ty`.
- Lines 320–1391 are the link step: `link` (336), `add_u32` (657), `check_host_fields`
  (668), the loader's plans — `Places`, `Planned`, `Edge` and `impl Places` (740–1200:
  `func`, `record_plans`, `refuse_page_templates`, `refuse_reach`, `bindings_plans`,
  `bindings_plan`, `records_reachable`), `ordered`/`chain`/`edge`/`needs` (1202–1250) —
  and the staleness messages `stale_signature_message`, `stale_claim`,
  `stale_type_message`, `signature` (1254–1391).

The data half names nothing from `crate::compile`, `crate::exports` or `crate::closure`:
`Compiled`, `Diagnostic`, `EntryFault`, `Runs`, `entry_fault`, `index_u32`,
`misshaped_entry_field`, `no_entry_field`, `refused_at_load`, `ExportIndex`, `closure`,
`Closed` and `ReadClosure` occur zero times in lines 34–318. (The word "Closed" at line
179 is prose.) Every file that imports `crate::unit` — eight of them under `compile/`,
plus `closure.rs` and `exports.rs` — names only data-half types.

Growth is the reason to act now. At ∆FQw's commit (`1a3d800`, 2026-09-08) the file had 414
production lines; it has 1,392 eleven days later, and all of the growth is in the link
half (∆iCW, ∆HF0, ∆eag, ∆Q6L, ∆BfR, ∆1Hp, ∆7wK, ∆Bhz). ∆Qr2 will add serialization to the
data half, and ∆99b will grow the plans.

Two neighbours belong with the link half and nowhere else:

- `closure.rs` (185 production, 41 test lines) has one importer in the crate, `unit.rs:20`,
  which calls it at lines 833, 874 and 1061 — all inside `impl Places`.
- `plan::order` with `cycles_among` and `cycle_from` (`plan.rs:113–176`) is a topological
  sort with cycle extraction. Its one production caller is `ordered` (`unit.rs:1207`), and
  all 84 lines of `plan.rs`'s test module (178–261) test it. `plan.rs`'s `//!` ends
  "Everything here is plain data on the `Module`", which these 64 lines contradict. The
  sort stays hand-written: its contract — declaration order kept, every cycle reported,
  each entered from the first node that leads into it — is shaped by the diagnostics, and
  ∆tdb's "use a crate" rule does not reach it.

## The cut

Two cuts were proposed. The coarse one (everything from line 320 on becomes
`link/mod.rs`, test module included) leaves a 2,840-line file and fixes only the `//!`.
Take the finer one:

- `crates/lemmon-vm/src/unit.rs` keeps lines 1–318. New `//!`: "One file's compiled
  output: its own constant pool and functions, the surfaces it offers other files, and
  the compile-time facts it consumed — the whole of what a unit is a function of (D25,
  invariant 11)." About 320 lines, no test module (it has no tests of its own today;
  every test reaches these types through `link`).
- `crates/lemmon-vm/src/link/mod.rs` takes `unit.rs` 320–736 and 1252–1391: `link`,
  `add_u32`, `check_host_fields`, and the four staleness builders. About 557 production
  lines. `//!`: "Joining units into the `Module` the VM runs (D25, invariant 11): every
  compile-time fact each unit recorded checked against what the index and the host say
  now, the constant pools concatenated, the operands rebased, and the imports resolved."
- `crates/lemmon-vm/src/link/plans.rs` takes `unit.rs` 738–1250 plus `plan.rs` 113–176.
  About 577 production lines. `//!`: "The loader's plans, built at link: every record's
  defaults and every bindings file's bindings placed in the module and ordered by what
  they read, with a cycle refused at the field or binding that closes it (D37, ∆HF0)."
- `crates/lemmon-vm/src/link/closure.rs` is `closure.rs` moved verbatim, `//!` unchanged.
- `plan.rs` keeps lines 1–111, plain data with no test module, and its `//!` becomes true
  as written.

`link` builds a `Places` and calls three of its methods inline (`unit.rs:513–524`):

```rust
let closure = ReadClosure::new(units, index, &positions);
let places = Places { units, index, host, positions: &positions,
                      function_offsets: &function_offsets, closure: &closure };
let records = places.record_plans(&mut diagnostics);
let bindings = places.bindings_plans(&records, &mut diagnostics);
diagnostics.extend(places.refuse_page_templates(&records));
```

Those lines move into `link/plans.rs` as one `pub(super) fn plans(…)` that takes `units`,
`index`, `host`, `&positions`, `&function_offsets` and `&mut diagnostics` and returns the
records and bindings, keeping the three calls in this order so the diagnostics come out in
the order they do today. This entry is what keeps the move nearly pure: a parent module
cannot see a child's private struct or its private fields, so without it `Places`, its six
fields, and `record_plans`, `bindings_plans` and `refuse_page_templates` would all have to
widen — ten items instead of none.

## Visibility

Production code, with the `plans()` entry in place:

- Widens: `add_u32` (`unit.rs:657`), private → `pub(super)`, because `Places::func`
  (`unit.rs:772`) calls it from the plans side. Nothing else on the `link/mod.rs` side is
  named between lines 738 and 1250.
- New: `pub(super) fn plans`.
- Narrows, `pub(crate)` → `pub(super)`: seven items in `closure.rs` — `Closed` (24),
  `ReadClosure` (29), `ReadClosure::new` (43), `ReadClosure::step` (87), `own_reads`
  (141), `ambient_values` (158), `via` (176). `plan::order` becomes private, since it
  lands in the module of its one caller.
- `unit.rs:25` imports the `Decls` trait for its methods, not its name: it is used only
  at lines 793 and 1192, both plans-side. A grep for the name makes the import look dead.
  It moves to `link/plans.rs`; do not delete it.

`lib.rs`: delete `mod closure;` (line 13), add `mod link;` after `mod host;`, and split
`pub use unit::{…, link}` (lines 52–55) into `pub use unit::{…}` and `pub use link::link;`.
The public API is unchanged — `crates/lemmon-wasm/src/lib.rs:24–27` imports `link` from the
crate root. `compile/mod.rs:50–53` takes `link` out of its `crate::unit` import and adds
`use crate::link::link;`.

## Tests: this is most of the work

The test module divides as follows (line numbers at `d3c8f11`):

- To `link/mod.rs`, about 985 lines: 1459–1828, 1942–2043, 2326–2838.
- To `link/plans.rs`, about 690 lines: 1847–1941 (`steps_of`, `page_template_of`, three
  page-template tests), 2044–2325, and 2839–3148 — the block headed
  `// --- Reads through calls (∆HF0) ---`, whose expected messages are built in
  `bindings_plans` and `refuse_reach`. An earlier count of 398 lines missed this last
  block. `plan.rs` 178–261 joins them.
- Shared by both sides, about 68 lines: `untyped_host` (1403), `unit_of` (1409),
  `unit_against` (1430), `const PAGE` (1827), `shell_host` (1829), `linked` (1839),
  `const POST` (1937), `shown_all` (2138).

`link::tests` and `link::plans::tests` are siblings, so neither sees the other's private
helpers. Move the eight shared items into `crates/lemmon-vm/src/testing.rs` as
`pub(crate)`; its `//!` is already "Helpers shared by this crate's unit tests". Points to
hold while doing it:

- `shell_host` has three byte-identical copies elsewhere (`compile/mod.rs:1056`,
  `compile/items.rs:696`, `compile/schema.rs:504`); `unit.rs`'s copy adds
  `.addressed_by("url")`. The shared fixture is the narrow one, and `unit.rs`'s tests write
  `shell_host().addressed_by("url")`. `compile/schema.rs`'s host must not gain a url
  field: that would arm D43's url check and change its diagnostics.
- Do not fold in `host.rs:526` or `load/mod.rs:709` (both named `shell()`). They bind
  `build` between `page` and `site`, `Host::ambient` is an order-preserving `Vec`, and
  `keeps_the_ambient_names_in_the_order_the_host_gave_them` (`host.rs:755`) pins
  `[page, build, site]`. Chaining `.bind("build", …)` onto a shared base gives
  `[page, site, build]`.
- `shown_all` is the same three lines as `shown` in `compile/mod.rs:1037`,
  `compile/names.rs:606` and `compile/schema.rs:498`. One `shown` in `testing.rs` serves
  all four; `compile/items.rs:706` has a different function of the same name and stays.
- `const PAGE` at `unit.rs:1827` is the Page declaration, and the name `PAGE` is shadowed
  by unrelated template source inside tests at lines 1484, 1758 and 2421. `AMBIENT`, the
  name `compile/schema.rs:514` and `compile/mod.rs:1064` use for the same string, avoids
  the shadowing. The doc comment at `unit.rs:1825–1826` is attached to the wrong item
  (the `shell_host` sentence sits above `const PAGE`); fix it as it moves.
- After sharing, perturb the shared `shell_host` once (change `.rendered_through("layout")`)
  and watch tests fail in every module that adopted it, then reverse the perturbation.

## Doc links

`mod link;` beside `pub use link::link;` is legal Rust — `compile` and `load` are already
both a module and a function (`lib.rs:14/40`, `:20/:45`). Rustdoc is what notices:
`[`link`](crate::link)` at `exports.rs:6`, `load/mod.rs:10` and `plan.rs:7` becomes
ambiguous and wants `crate::link()`. `unit.rs:5` is rewritten with the `//!`; `unit.rs:35`
stays in the data half and needs a qualified path. `compile/mod.rs:67` resolves against
its local `use`. `docs/journal/∆Tsy-rename-colliding-types-and-fields.md` records that
`cargo doc` already reports this class of error for `compile` and `load`, and that no gate
runs rustdoc. Run `cargo doc --workspace --no-deps` by hand and add no new errors.

## What this does not do

It does not make `compile` and the link step one-way. `compile/mod.rs:108` calls `link`
from the whole-site `compile()` driver, and `link/` still imports `Compiled`,
`Diagnostic`, `index_u32`, `Runs`, `refused_at_load`, `EntryFault`, `entry_fault`,
`no_entry_field` and `misshaped_entry_field` from `crate::compile`. The two-way imports
that do end are `unit` ↔ `exports` and `unit` ↔ `closure`. Do not write "breaks the
cycles" in the commit or the journal.

Three small moves found by the same survey shrink that remaining edge. They are the first
checkpoint of ∆rKx (`docs/plans/∆rKx-small-moves.md`), not part of this item, and each is
provable by an empty corpus diff:

- `compile/diagnostics.rs` → `src/diagnostics.rs`. Its own `//!` (lines 9–10) says the
  types are "the crate's vocabulary rather than the compiler's". 235 lines move verbatim;
  seven `use` sites change (`unit.rs:21`, `document/mod.rs:27`, `document/adapters.rs:97`,
  `load/place.rs:15`, `load/mod.rs:28`, `load/validate.rs:13`, `lib.rs:38–39`); one item
  widens (`Diagnostic::from_syntax`, `pub(super)` → `pub(crate)`, one caller at
  `compile/surfaces.rs:102`); one doc link breaks (`compile/mod.rs:24`). A private
  `use crate::diagnostics::{…}` in `compile/mod.rs` keeps every `use super::Diagnostic`
  under `compile/` working. Afterwards `document/` imports nothing from `compile`.
- `EntryFault`, `entry_fault`, `entry_shape`, `no_entry_field`, `misshaped_entry_field`
  (`compile/schema.rs:436–489`) → `decls.rs`. They take no `Compiler`, read only
  `&dyn Decls`, and have two callers (`schema.rs:221`, `unit.rs:718`). 54 lines, nothing
  widens, and `decls.rs`'s `//!` (12–14) already gives this reason for `no_field`.
- `compile()` and `repeated_path()` (`compile/mod.rs:66–132`) → `lib.rs`. Only the corpus
  harness and unit tests call `compile()`; `lemmon-wasm` drives `prepare`, `compile_unit`
  and `link` itself. With the other two done, `link/` imports only `index_u32`, `Runs`
  and `refused_at_load` from `compile`, and `compile` imports nothing from `link/`.

If any of these has landed before this item starts, the import lists above are shorter;
nothing else in the plan changes.

## Checkpoints

Each ends with a plain-language preview for Eric, `/code-review medium --fix`, and Eric's
commit.

1. **Shared test helpers.** Move the eight helpers into `testing.rs` and adopt them in
   `unit.rs` and in the three `compile/` modules with identical copies. Test-only; about
   25 fewer test lines.
2. **`link/` as a coarse move.** Create `link/mod.rs` from `unit.rs` 320–1391 with the
   whole test module, and `link/closure.rs` from `closure.rs`. No test seam exists yet, so
   this is a pure move whose only visibility changes are the seven narrowings.
3. **`link/plans.rs`.** Lift lines 738–1250 out of `link/mod.rs` behind `plans()`, fold in
   `plan::order` and `plan.rs`'s tests, and divide the test module. `add_u32` widens here.

## Acceptance

- `npm run corpus:bless` leaves an empty diff, `tests/corpus/host/` included: those cases
  are the corpus's only two-file coverage of the link step.
- ∆FQw's check: the new files' logical lines, as a multiset, equal the originals' once
  imports, doc headers and visibility qualifiers are normalised away. The intended
  differences are the `plans()` wrapper and the test helpers' new home.
- `cargo doc --workspace --no-deps` by hand: no errors beyond the ones ∆Tsy recorded.
- `npm run verify`. `npm run rust:wasm:test` is not needed: no boundary shape or depth cap
  changes and the crate's public API is identical.
- No live doc names these files by path (`CLAUDE.md`, `docs/architecture.md`,
  `docs/language.md`, `docs/decisions/README.md` and `tests/corpus/README.md` were
  grepped); journal entries do, and are history.

## Out of scope

A further `link/facts.rs` holding the staleness comparisons was considered and set aside.
The signature comparison (`unit.rs:449–481`) sits inside the import-resolution loop and
shares its `index.lookup`, so lifting it means walking `unit.imports` twice to keep
today's diagnostic order, which `reports_a_units_stale_types_beside_its_stale_imports`
pins. That is not a pure move. Revisit only if `link/mod.rs`'s `//!` stops stating one
thing.

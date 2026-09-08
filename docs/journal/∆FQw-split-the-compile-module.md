---
id: ∆FQw
completed: 2026-09-08 08:43:18 -07:00
---

## Backlog item

> - ∆FQw Split `crates/lemmon-vm/src/compile/` — `mod.rs` (1459 lines of code) and
>   `expr.rs` (1115, one `impl Compiler` block) each hold several responsibilities: lift the
>   diagnostic types, the read-every-surface-from-heads phase (invariant 11, D36), the host
>   and declaration checks, the call path, and D35's narrowing into modules of their own.
>   ∆iCW, ∆Q6L and ∆RGQ all land in these two files

## Planning

The item came from Eric asking whether the Rust files had grown large enough to need
splitting. Measuring first changed the answer: total lines are mostly the inline-test
convention doing its job — `scanner.rs` is 1232 lines over 322 of code, `vm.rs` 1694 over
619 — and inline `#[cfg(test)] mod tests` is canonical Rust, so it stays. By code above
the test module only five files cleared 700 lines, and three of them (`parser/template.rs`,
`compile/rules.rs`, `parser/mod.rs`) hold to one responsibility each. Two did not.

The tell was not the line count but the module doc comment: `compile/mod.rs`'s `//!` read
as a table of contents for six sibling modules plus "the driver lives in this module", and
then the module also held the crate's diagnostic types, the read-surfaces-from-heads phase,
and the host and declaration checks. That is the test worth keeping — if a file's `//!`
cannot state one responsibility, the file is too big — and it is why no line-count lint was
added: a threshold would fire on the test-heavy files where nothing is wrong.

Growth made it urgent rather than tidy: `compile/mod.rs` went 702 → 2188 over fourteen
commits, +534 in [[∆Iau]] alone, and ∆iCW, ∆Q6L and ∆RGQ all land in these two files.
Five extractions were planned, with an empty corpus diff as the acceptance criterion.

## Refinement

The move itself was uneventful — the five extractions went in as planned, and the empty
corpus diff held on the first run. Three things were worth more than the move.

**The seams were already named by the docs.** `surfaces.rs` is invariant 11 and D36's own
sentence: every surface a file offers another is read from its heads alone. That phase
takes no `Compiler` and runs before one exists, so it lifted whole. `schema.rs` is what
those same surfaces cannot answer without the index, and it went in as a second
`impl Compiler<'_>` — the pattern `expr.rs` and `items.rs` already use. `mod.rs`'s tests
split along the same seam the code did, because cluster B already had its own
`shell_host`/`AMBIENT` helpers.

**`narrowings` stopped reaching into a sibling.** `Compiler`'s one field typed from another
module was `Vec<(expr::Path, Type)>`; it is now `narrowing::Path`, which is what it was
always about.

**The equivalence was checked mechanically, not by eye.** Every new file's logical lines
were diffed as a multiset against the originals at HEAD, normalising away imports, doc
headers and visibility qualifiers. The only differences were import rewraps, the deliberate
`expr::Path` rename, six intra-doc link repairs, and the new module braces. That is a
stronger statement than the empty corpus diff alone, and it is cheap.

`/code-review high --fix` found two real things, both fixed: `expected_enum` was
`pub(super)` with no caller outside `rules.rs` (predating the diff), and the mechanical
`pub(crate)` pass over the wasm wire types over-exposed eight items and twelve fields
([[∆ZK8]]). A duplicated two-line `shown` test helper was left alone — sharing it would
need a cross-module fixture, which costs more than the duplication.

## Retrospective

**`npm run rust` cannot see a broken doc link, and this codebase is mostly prose.** Moving
code out of `compile/mod.rs` broke six intra-doc links — `[`compile_unit`]` and
`[`ExportIndex`]` stopped resolving once they left the module — and the gate was green
throughout. They were found only by running `cargo doc` by hand, on a hunch. Seven real
regressions in one day, invisible to every gate the repo has. A doc leg belongs on
`verify`; it cannot go on today, because `unit.rs:53`'s private-item link had to be fixed
here first for the workspace to be clean enough to add one.

**Measure before agreeing that a file is too big.** The obvious reading of the file sizes
was wrong for seven of the nine largest files, and acting on it would have split
`scanner.rs` — 322 lines of code — by responsibility. Code above the test module was the
number that meant anything, and it took one command to get.

**A pure move should be proved as a move.** The corpus diff proves behaviour did not
change; it says nothing about whether a line went missing. The normalised multiset diff
says that, takes a few minutes to write, and is the thing that would have caught a dropped
method. Worth doing on every move of this size.

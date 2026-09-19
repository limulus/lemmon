# ∆8jR Move the span-by-text test helper to `lemmon-corpus` — plan

Written on 2026-09-19 by the session that surveyed the repository's organization for Eric,
against `main` at `d3c8f11`. Two analysts proposed this independently (one from
`lemmon-syntax`'s side, one from `lemmon-vm`'s), and an agent told to refute it read the
three `Cargo.toml` files and recounted the assertions; its counts replace the analysts'.
Nothing was built. When ∆8jR's journal entry is written, its Planning section absorbs this
file and the file is deleted.

## Start here

1. Read `CLAUDE.md` § Testing (the span rule), the ∆8jR item in `BACKLOG.md`, and
   `docs/journal/∆SSw-span-by-text-test-helper.md`, which built the helper and named this
   promotion: "if they want the helper, it promotes to `lemmon-corpus`, already a
   dev-dependency of both crates."
2. `npx @limulus/delto@1 claim ∆8jR`.

## What exists today

`CLAUDE.md` says a diagnostic's span "is asserted by the source text it covers, never by a
counted offset", and names `at(source, needle)` in `crates/lemmon-vm/src/testing.rs`. That
module is `#[cfg(test)] mod testing;` (`lemmon-vm/src/lib.rs:23–24`) and its two functions
are `pub(crate)`, so two places that assert spans cannot reach it:

- `lemmon-syntax`, the crate that owns `Span`, because `lemmon-vm` depends on it.
- `lemmon-vm`'s integration tests, because a `#[cfg(test)]` module is compiled only into
  the unit-test build.

Both assert hand-counted offsets instead. Counted over literal source strings:

- `crates/lemmon-syntax/src/document.rs`: 23.
- `crates/lemmon-syntax/src/front_matter.rs`: 11 (lines 101, 102, 112, 113, 123, 124, 133,
  135, 145, 152, 156). Its other three `Span::new` calls are production code.
- `crates/lemmon-syntax/src/scanner.rs`: at least 9 (lines 596, 600, 648, 652, 809, 834,
  858, 880, 1117).
- `crates/lemmon-syntax/src/ast/tags.rs`: 6 (lines 792, 795, 798, 809, 819, 830).
- `crates/lemmon-syntax/src/parser/template.rs`: 4 (lines 1026, 1085, 1102, 1128).
- `crates/lemmon-vm/tests/corpus.rs`: 4 (lines 715–739), one of them over
  `"---\ntitle: café\ntopic: x\n---\nbody"` — the multibyte case `testing.rs`'s own test
  `counts_the_bytes_a_multibyte_source_puts_before_the_needle` exists for.

About 57 in all. The analysts' first count covered three files and found 40; recount
before relying on either number, and grep the rest of `lemmon-syntax` for more.

Not to convert: the `Span::new` calls in `lemmon-syntax/src/lib.rs` (8) and
`diagnostic.rs` (13) build spans with no source text behind them — `Display` and plumbing
tests — which `CLAUDE.md` says stay plain.

The dependency facts: `lemmon-corpus` depends on `lemmon-syntax` (for `LineIndex`), and is
already a dev-dependency of both `lemmon-syntax` and `lemmon-vm`; Cargo allows that cycle
because only test targets link it (`lemmon-corpus/src/lib.rs:9–11` says so). Dev-
dependencies are in scope for inline `#[cfg(test)]` modules and for `tests/`. The
workspace sets `publish = false`, so making `at` and `all_of` `pub` puts them on no
published surface.

## The move

1. `crates/lemmon-vm/src/testing.rs` (68 production lines: `at`, `all_of`, and the private
   `unbracketed` and `offset`; 75 test lines) moves to `crates/lemmon-corpus/src/spans.rs`,
   with `at` and `all_of` made `pub` and re-exported from `lemmon-corpus`'s `lib.rs`.
   `//!`: "A byte span named by the source text it covers, for the span assertions every
   stage's tests make."
2. `lemmon-corpus`'s crate `//!` (lines 1–11) lists three responsibilities — where cases
   live, how an expectation is compared and rewritten, how a diagnostics trailer is
   displayed — "and nothing about scanning, parsing, or rendering". The helper is
   stage-agnostic in that sense, but it is a fourth responsibility, and the sentence has to
   say so rather than the crate growing one unannounced.
3. In `lemmon-vm`, nine `use crate::testing::…` lines become `use lemmon_corpus::…`:
   `vm.rs:2332`, `unit.rs:1398`, `document/mod.rs:286`, `load/mod.rs:704`,
   `load/validate.rs:486`, `compile/mod.rs:992`, `compile/annotations.rs:132`,
   `compile/items.rs:691`, `compile/schema.rs:495`. The roughly 300 call sites do not
   change.
4. Whether `lemmon-vm/src/testing.rs` is deleted depends on ∆fBj. Its first checkpoint
   puts the shared host fixtures (`shell_host`, `shown`, `untyped_host`, …) in that file,
   and those need `lemmon-vm`'s own types, so they cannot move to `lemmon-corpus`. If they
   are there, the file stays with its `//!` unchanged and only the span helper leaves. If
   not, the file and its `mod` line go.
5. `CLAUDE.md` § Testing names `crates/lemmon-vm/src/testing.rs` as the helper's home;
   change the path in the same pass.

A new `lemmon-testing` crate was considered for step 1. It would leave `lemmon-corpus`'s
`//!` alone and cost a manifest and a workspace member for 68 lines. Not taken.

## Then convert

- The `lemmon-syntax` assertions listed above, to `at(src, needle)`.
- The four in `crates/lemmon-vm/tests/corpus.rs`.
- `fn messages(src: &str) -> Vec<String>` is byte-identical three times in
  `lemmon-syntax` — in `parser/expr.rs`'s tests, at `parser/mod.rs:681`, and at
  `parser/template.rs:1461`. Add `crates/lemmon-syntax/src/testing.rs` behind
  `#[cfg(test)]`, `//!` "Helpers shared by this crate's unit tests.", holding one copy.
  The function of the same name at `template.rs:991` delegates to a local `diagnostics()`
  and is a different function; leave it.

∆SSw's retrospective warns that converting both the producer and the assertion to `at` can
make a test agree with itself. These conversions touch assertions only. Still, perturb
what each file's tests claim to pin: shift the production span by a byte in `document.rs`,
`front_matter.rs`, the scanner and the parser in turn, watch that file's converted tests
fail, and reverse the perturbation by hand — never with `git checkout -- <path>`.

## Checkpoints

1. The move: `spans.rs`, the re-export, `lemmon-corpus`'s `//!`, the nine imports,
   `CLAUDE.md`. A pure move; every test passes unchanged.
2. `lemmon-syntax`: `testing.rs` with `messages`, then the span conversions file by file.
3. `crates/lemmon-vm/tests/corpus.rs`: the four conversions.

## Acceptance

- `npm run verify`; `npm run corpus:bless` leaves an empty diff, since no production line
  changes.
- `npm run rust:cov` is CI-only, but the helper's 75 test lines move with its code, so the
  workspace-wide ratchet should not move. Say in the journal whether it did.
- `cargo doc --workspace --no-deps` by hand adds no errors; nothing links into a
  `#[cfg(test)]` module today.

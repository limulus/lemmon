---
id: ∆8jR
completed: 2026-09-19 13:12:27 -07:00
---

## Backlog item

> - ∆8jR Move the span-by-text test helper (`at`, `all_of`) from `lemmon-vm`'s `testing.rs` to
>   `lemmon-corpus`, the promotion ∆SSw named — `lemmon-syntax` cannot reach it, so about 53
>   of its span assertions and four in `crates/lemmon-vm/tests/corpus.rs` are hand-counted
>   offsets, which CLAUDE.md forbids. Convert them, and share `lemmon-syntax`'s thrice-copied
>   `messages` test helper. Planned in `docs/plans/∆8jR-span-helper-to-corpus.md`

## Planning

The plan came from the 2026-09-19 organization survey. Two analysts proposed the move
independently, and an agent told to refute it recounted the assertions: 53 hand-counted
spans over literal source in `lemmon-syntax`'s tests (`document.rs` 23, `front_matter.rs`
11, `scanner.rs` 9, `ast/tags.rs` 6, `parser/template.rs` 4) and four in
`crates/lemmon-vm/tests/corpus.rs`, one of them over a multibyte source. The helper was
`pub(crate)` in a `#[cfg(test)]` module of `lemmon-vm`, so neither `lemmon-syntax`, which
`lemmon-vm` depends on, nor `lemmon-vm`'s integration tests could reach it.

The plan followed ∆SSw's journal, which had named the promotion: "it promotes to
`lemmon-corpus`, already a dev-dependency of both crates." It also moved three
byte-identical copies of a `messages` test helper in `lemmon-syntax`'s parser tests into
one `#[cfg(test)]` module, and left spans with no source text behind them (`lib.rs`,
`diagnostic.rs`) as plain `Span::new`, as CLAUDE.md says.

**The destination was wrong, and the plan had not checked it.** `lemmon-corpus` depends on
`lemmon-syntax`, and `lemmon-syntax` takes it back as a dev-dependency. Under that cycle
Cargo compiles `lemmon-syntax` twice for its unit tests, and a `Span` returned by
`lemmon-corpus` is the other copy's type. The builder showed it with a stub before
writing anything: `error[E0308]: expected Span, found lemmon_syntax::Span`, "multiple
different versions of crate `lemmon_syntax`". The refuting agent had read the three
manifests and confirmed the dev-dependency existed; nobody asked what type would cross it.

## Refinement

The helper is `lemmon_syntax::spans`, in the crate that owns `Span`. Every crate that
asserts a span already depends on it, so `lemmon-corpus` re-exports nothing.

The builder made the module plain `pub`, and measured what that cost: 1,299 bytes of the
staged `.wasm`. It is test code, so it is compiled under `cfg(test)` or a `testing`
feature that `lemmon-vm` turns on in a dev-dependency, and the staged `.wasm` is the same
2,322,733 bytes as before the item. The review found what that gate does not cover:
`cargo test --workspace` resolves with the feature on, so production code that named the
module would pass `npm run rust`. `npm run rust:wasm` builds without dev-dependencies and
fails on it (probed: E0433), and `verify` runs that first. Adding `cargo build
--workspace` to the Rust half was considered and not done; the manifest comment says
which command catches the leak.

All 57 conversions were checked mechanically by the reviewer, who reimplemented `at` in
Python and compared every computed span with the literal at HEAD: no mismatch. Every
needle is a substring of the test's own input and `at` is on the expectation side only,
so none is inert. The builder perturbed fifteen needles by one character and each failed.

The review's other fixes: `spans.rs`'s private `offset` duplicated `crate::index_u32`,
which the move had made reachable, and now calls it; `all_of`'s doc spoke only of hosts,
which `lemmon-syntax` does not have; and four import groups had been split by the blank
line a deleted import left.

`npm run rust:cov` was run by both agents: 98.5% of lines, the three helper files at 100%.

Found and left, for whoever next touches those files: source-backed counted spans at
`ast/mod.rs:137` and `parser/mod.rs:642` that the plan's count missed, and
`document.rs`'s `items()` helper, which asserts strings like `"Text@13..18"` — counted
offsets in a different projection.

## Retrospective

A refuting pass that confirms a fact ("the dev-dependency exists") has not tested the
claim built on it ("so the helper can live there"). The experiment that settled it took
the builder a few minutes. For a plan whose whole content is a dependency direction, the
plan should contain that experiment's output.

The builder reported the `.wasm` cost of its own choice without being asked, which is
what made the feature gate an easy call.

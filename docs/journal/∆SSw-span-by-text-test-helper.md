---
id: ∆SSw
completed: 2026-09-09 03:39:24 -07:00
---

## Backlog item

> - ∆SSw Span-by-text test helper — a helper for the Rust tests in `crates/lemmon-vm` that
>   finds a byte span by its text in a source (`at(source, "banner")`), and the diagnostic
>   assertions that hand-count `path:start..end` offsets converted to it. Today an offset is
>   guessed, run, and copied back from the failure, so those tests are written by trial and
>   cannot be read as the spans they assert

## Planning

A census of every span assertion in `lemmon-vm` came before the design, and it refuted the
item's own API. About half the sites cannot name their span with a plain substring, and
they are the same half the tests exist for: a name bound twice, a function defined twice, a
parameter repeated. The repeat is the subject, so it cannot be renamed away to make the
needle unique. An occurrence ordinal only moves the counting — in the repeated-declaration
test the two asserted spans are the `t` of two `t: string` lines, and a bare `t` matches ten
times, inside `<lem-type`, `string`, and `</lem-type>`.

So the needle may bracket part of itself: `at(src, "f(«a»: int")` spans the first `a` of
`f(a: int, a: int)`, and the rest is context that makes the match unique. An unbracketed
needle is the plain form the item asked for, which is what most sites use.

Two alternatives were weighed and rejected in planning. **Marked fixtures** — the
rust-analyzer pattern, markers in the source string and a helper returning clean text plus
ordered spans — read better at each site but break on the `const AMBIENT`/`SITE`/`TEXT`
fixtures that a dozen tests each want a different span from; a needle scales there, marks
do not. **Rendering the text a diagnostic points at, instead of its offsets** needs no
fixture bindings and gives better failure output, but two identical texts render
identically, so a span pointing at the wrong `a` would pass — and it does nothing for the
other direction.

That other direction decided the return type. `spans(TEXT, …)` and `.named_at(…)` build
the positions a host sends, which a diagnostic then echoes; those were hand-counted too,
twice over, once at each end. A helper returning a `Span` serves both, so `Span` gained a
`Display` writing `start..end` (f1e1397), and `Diagnostic`, its labels, and `RuntimeError`
dropped their three copies of that format.

Two checkpoints: the helper with the loader and document-mode tests (c8cf1c0), then the
compiler's (this commit).

## Refinement

Checkpoint 1's review found one real defect, and it was the helper's own guard. "The source
holds this exactly once" was checked with `str::match_indices`, whose matches are
non-overlapping: `at("---", "--")` returned `0..2` without complaint although `--` also sits
at offset 1. The helper written to stop tests guessing would have guessed. It now scans
every char boundary and reports the true count.

Three things the item did not name fell out of the conversion. `all_of(source)` spans a
whole text, which is what a host sends for a value as a whole and what three fixtures had
spelled as a counted length. `record()` in `load/validate.rs` names the one host position
that eighteen assertions echo. And `annotations.rs` gained `annotated(src)`, the text its
`resolved()` builds, because its spans are into a string the test never held.

Two spans stayed literal on purpose: `TEXT`'s front matter has no `hero` or `related` key,
so the host's position for a nested field is any span inside the record's and distinct from
it. Those are bound once and used on both sides, with a comment saying so. The same
judgment covers a larger set the sweep left alone — `Display` and plumbing tests over
hand-built spans with no source behind them, in `lib.rs`, `compile/diagnostics.rs`,
`load/place.rs`, `vm.rs`, `exports.rs`, `builtins.rs`, `bytecode.rs`, `decls.rs`,
`surfaces.rs`, and most of `names.rs`. The rule written into `CLAUDE.md` carries that
carve-out, so the next reader does not take the sweep for an oversight.

`names.rs` had its own `at(start)` — a one-byte span at an arbitrary offset, no source
behind it — renamed to `binder(start)` across 69 call sites, so `at` means one thing in
the crate.

Checkpoint 2's review found no correctness issues; two cleanups were applied. Nothing was
deferred. `lemmon-syntax` has a handful of span assertions of its own, out of this item's
scope; if they want the helper, it promotes to `lemmon-corpus`, already a dev-dependency of
both crates.

## Retrospective

**The census refuted the item, and that was the work.** `at(source, "banner")` reads
perfectly in a backlog item and fails on half the real call sites, because the tests that
assert a span are disproportionately the tests about a repeated name. Nothing but counting
the ambiguous sites would have shown that, and it turned a one-function item into a
two-mechanism one before any code was written.

**A guard is worth exactly its counting.** The uniqueness check looked obviously right and
was quietly wrong, because `match_indices` skips overlapping matches. It was found by asking
what would make the helper silently wrong, not by asking whether it worked — the tests
written first all passed against the broken version.

**Converted assertions have to be shown to be load-bearing.** Using `at` on both the
producer and the assertion side risks a test that agrees with itself. Shifting every host
span by one byte failed ten loader tests; shifting every diagnostic's span failed
eighty-three. Two throwaway edits, and without them the conversion is a claim rather than a
fact.

**`git checkout <path>` is not an undo for a one-line experiment.** Reverting the first
perturbation that way discarded the whole file's conversion, which had to be redone.
Copying the file aside first costs nothing and is what the second perturbation did.

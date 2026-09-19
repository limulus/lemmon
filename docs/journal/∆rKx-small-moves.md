---
id: ∆rKx
completed: 2026-09-19 11:32:36 -07:00
---

## Backlog item

> - ∆rKx Land the small moves the 2026-09-19 organization survey verified — about twenty
>   one-commit changes no other item owns: `compile/diagnostics.rs` to the crate root (its
>   `//!` says it is not the compiler's), `entry_fault` to `decls.rs`, `is_boundary` derived
>   from `ast::Item` so a new tag cannot vanish from `.tags`, stale `//!` sentences, over-wide
>   visibility, two colliding names. Planned in `docs/plans/∆rKx-small-moves.md`

## Planning

The plan was written on 2026-09-19 by the session that surveyed the repository's
organization, from eight analyst reports and a second pass told to refute them. It marked
each claim "verified" or "analyst only", and grouped the changes into four checkpoints by
crate: the `lemmon-vm` moves, `lemmon-syntax`, the small `lemmon-vm` changes, and the
boundary. Nothing in the item was broken; each entry was a module sitting where its own
`//!` said it did not belong, a list or literal kept by hand in two places, a `//!`
sentence or a `pub` that later work had made false, or a name that meant two things.

Two entries were questions, and Eric answered both yes. `compile()` and `repeated_path()`
moved to `lib.rs`: only the corpus harness and unit tests call `compile()`, and with it
gone `compile/` imports nothing from the link step. `nearest` got its own file,
`suggest.rs`, so that `diagnostic.rs`'s `//!` states one thing.

The plan listed what it considered and left out, and that list is still the record of
it: `ScanError` to `token.rs`, renaming `document/adapters.rs`, `parser/types.rs`,
`wire/placement.rs`, the second `index_u32` (∆xWL), `decls`' test fixtures into
`testing.rs`, renaming `Data` to `Envelope`, re-exporting `TypeRef` and the three other
fact types (∆Qr2), and splitting `lemmon-vm`'s corpus harness (revisit with ∆4tG or ∆99b).

## Refinement

An Opus agent built each checkpoint and a second Opus agent, told not to modify files,
reviewed it; checkpoints 3 and 4 were built and reviewed together and committed
separately by path. Every checkpoint left `npm run corpus:bless` with an empty diff and
`cargo doc --workspace --no-deps` with no warnings. The six rustdoc errors ∆Tsy recorded
were already gone. Checkpoint 4 also passed `wasm:check` and `rust:wasm:test`.

Where the plan was wrong:

- Its private import in `compile/mod.rs` named `Label`, which nothing there uses, and
  clippy refuses. The review then replaced the five `use super::Diagnostic` lines under
  `compile/` with `crate::diagnostics::Diagnostic`, so a grep for the module finds every
  user; the plan had kept them to save five lines.
- `PreparedFile`'s fields are `pub(super)`, so `compile()` in `lib.rs` reads them through
  the public accessors, as `lemmon-wasm`'s driver already did. Nothing widened.
- `document::Piece` had 22 sites, not 26, and `compile/mod.rs` was one of the files.
- `host.rs`'s old `//!` said a document host adds two ambient names. It adds three:
  `document`, `entry`, and `url` (D43).

The moves were proved as ∆FQw's was: the moved blocks' logical lines, as a multiset, are
equal before and after once imports, doc headers and visibility qualifiers are removed.
The script was negative-tested by deleting one line from the moved file. `is_boundary`
was negative-tested in both halves: dropping `ScriptVoid` from `Item` removed five lines
from `.tags` expectations, and dropping `ElseTag` from the explicit list removed seven. A
reviewer compared the new predicate with the old 24-kind list over every `SyntaxKind` and
both values of `in_items`, and found no disagreement.

The three reviews found no behaviour defect and eleven doc or visibility findings; nine
were fixed. The two that mattered most: `Host::assets`, a test comment and `HostInput`
all said the resident `Core` compares declarations "to decide whether a recompile is
needed", and it does not — the one comparison is in `loadData`, refusing an envelope built
for a declaration the program has not linked under. And dropping `invoke` from the
re-export had left it `pub` in a private module; it is `pub(crate)` now.

The change is +557 −457 across 38 files. It adds lines: the two constants, `suggest.rs`'s
header, and the module docs three files in `lemmon-syntax` did not have.

One finding was left for its own item: `AmbientBinding`'s `{bindingsOf}` form is an
untagged serde variant without `deny_unknown_fields`, so the boundary accepts a misspelled
field inside a binding in silence. `types.ts`'s new header documents the exception. Also
left: line 8 of `wire.rs`'s `//!` is wider than 92 columns, which rustfmt does not wrap.

## Retrospective

The survey's second pass paid for itself again here: every departure above was small,
and none was a surprise about what the code does. The wrong claims were counts and one
unused import, which is what a plan written from reports gets wrong.

A rewritten `//!` is where the reviews found their defects. Two of the four new headers in
checkpoint 2 said something false about the file they head, and both builders wrote them
from the plan's wording. Ask the builder to check each new doc sentence against the code,
as checkpoints 3 and 4's prompt did; that review found one defect in new doc text.

Building two checkpoints with disjoint paths in one agent run and committing them
separately saved a build and a review, and a reviewer confirmed each half builds without
the other. It works only when the paths are disjoint before the work starts.

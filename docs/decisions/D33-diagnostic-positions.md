# D33 — Diagnostic positions

- **Status:** Accepted 2026-09-07 (`∆cE6`)
- **Backlog item:** `∆cE6`
- **Normative text:** `docs/architecture.md` § WASM API (`Diagnostic`);
  `tests/corpus/README.md` § Positions in a trailer

## Context

Every diagnostic carries a byte span and always will — that is invariant 8, and it is what
makes a span exact and reproducible. But nothing that shows a diagnostic to a person shows
a byte offset. The corpus trailers printed `89..94`, and blessing is a proposal rather than
a fix: the whole safety mechanism is that someone reads the diff, and wrong output had been
blessed in twice. Eric could not match a blessed trailer line to its source by eye.

The same conversion is owed twice more. `lem check` (`∆4tG`) has to print a place in a file,
and the LSP (`∆5MM`) has to hand an editor an actual position. Both live in TypeScript and
both receive the boundary's diagnostics — so if the core kept emitting byte offsets alone,
each would convert independently, in the language where the conversion is worst: a byte
offset into a UTF-16 column means re-encoding the source, because JavaScript string indices
are already UTF-16.

## Decision

A span crosses the boundary as both: `span: {start, end, range}`, where `range` is the two
ends as `{line, character}` — the LSP's `Position`, zero-based, `character` counting UTF-16
code units. The core computes it, in `lemmon-syntax`, because the core is where the bytes
are; `LineIndex` wraps rust-analyzer's `line-index`, which shares rowan's `text-size` and
stores line starts and each line's non-ASCII characters rather than the text.

An offset that is past the end of a file, or inside a character, is a panic and not a
rounded-off position: it means a span that did not come from that file, which is a bug in
whatever produced it (invariant 10).

The index a span is placed against is the text that produced the *unit* the span belongs
to, not the latest text at that path. The two part company exactly when a recompile fails:
that file keeps its previous unit (D25), whose spans are offsets into the older text.

Display is a separate matter from the type. The corpus trailers render `line:col`
**counting from one**, since they are read by people; the zero-based `Position` is the
machine-readable contract. Byte offsets stay in the `.cst` tree and throughout `.tags`,
which is what a tree-sitter grammar is held to (invariant 9), and are dropped from the
trailers, which nothing reads programmatically.

## Alternatives considered

- **Leave the conversion to each consumer.** Rejected: `lem check` and the LSP would each
  re-derive it in JavaScript from a byte offset, which is the expensive direction, and the
  two could disagree about what a column is. The corpus would have needed a third copy.
- **Replace the byte span at the boundary rather than adding to it.** Rejected: the byte
  span is what the core works in and what a diagnostic is reproducible by; a consumer that
  wants to slice the source wants offsets. `range` is additive, which is the boundary's
  established pattern for `severity` and `labels`.
- **One-based positions over the boundary, matching the display.** Rejected: an off-by-one
  in a language server is a bug the user sees on every diagnostic, and `∆5MM` should be able
  to pass a position straight through. The `+1` belongs to whoever is printing.
- **Character columns, or byte columns, instead of UTF-16.** Rejected: byte columns
  mislead on any non-ASCII line, which is the thing this decision exists to fix, and
  character columns agree with UTF-16 until the first astral character and then quietly
  stop. `syntax/recovery/error-after-wide-characters` pins the difference: an emoji before
  the error puts the column at 38 where characters give 37 and bytes give 44.
- **Positions rendered by each stage into its own trailer.** Rejected: `lemmon-syntax` and
  `lemmon-vm` would each hold the display convention and have to agree. The harness renders
  both trailers, so the convention lives in one place.

## Consequences

- `lem check` (`∆4tG`) and the LSP (`∆5MM`) print or forward `range` and compute nothing.
  The reverse direction — a `Position` back to an offset, which an LSP needs for incremental
  edits — is deliberately absent: that consumer holds the document text in TypeScript, so
  a Rust version would be a recorded fact with no reader.
- The resident `Core` keeps a `LineIndex` per resident unit, updated in lockstep with the
  units, so it grows with them and no faster.
- `loadData`'s diagnostics point into files the host holds — YAML front matter, data
  files — which no resident index describes. D37 closed the question the other way: the
  host sends complete spans, offsets and positions both, keyed by field path, and a
  diagnostic about such a file carries them (`Diagnostic::range`); the boundary places
  it by them rather than by an index, and falls back to the index only for a diagnostic
  at a resident declaration.
- `lemmon-corpus` now depends on `lemmon-syntax`, and `lemmon-syntax` takes it back as a
  dev-dependency — a cycle Cargo allows, since only the test target links back.
- A blessed diff can be reviewed against the source, which is what the corpus asks of a
  reviewer. The trailer's byte offsets are gone; `.tags` and the `.cst` tree keep theirs.

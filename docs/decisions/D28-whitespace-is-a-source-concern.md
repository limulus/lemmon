# D28 — Whitespace is a source concern

- **Status:** Accepted 2026-08-24; applied 2026-08-25 (`∆Ma4`), when the spike's gate
  became a rendered-HTML comparison
- **Backlog item:** `∆Ma4`
- **Normative text:** `docs/language.md` § Whitespace; `docs/architecture.md` § Testing

## Context

The Phase 0 spike's fidelity gate compared Lemmon's output bytes against WebC's. D26
exists to stop a definition's indentation reaching the output, and that gate demanded
another engine's indentation reach it — the two had been in contradiction since D26 was
written, and `∆2vO` planned around the gate as a fixed constraint, down to choosing the
hugged-body reading partly because the fixture needed it. The contradiction surfaced only
when Eric said output whitespace does not matter to him.

## Decision

Lemmon optimizes how a `.lem` file reads, not how its HTML reads. Indentation and the
blank lines between tags are not a product surface: a formatter or minifier downstream
owns that (D12's dprint plugin), and a reader who wants the output legible has devtools.

So the renderer never re-indents at a boundary — a definition loses its own margin (D26)
and nothing else moves. A multi-line value keeps its own columns wherever it is
substituted, so a slot's second line lands where its definition put it rather than at the
call's column, and a flow body keeps its newlines; re-indenting either would make the
output depend on a value's shape at run time.

What this does *not* license is touching whitespace HTML renders — inline formatting
contexts, `pre`/`textarea` — which is output semantics and survives byte for byte. Which
is why a fidelity gate compares rendered HTML rather than raw bytes (D15's "normalized";
`∆Ma4` for the spike), and never by collapsing whitespace blindly.

## Alternatives considered

- **Keep the byte gate and make the templates match.** Rejected: it forces exactly the
  hugged, hand-indented templates D26 exists to abolish; the reversal cost a reformat of
  all six spike files that was then reverted.
- **Re-indent substituted values to the call's column.** Rejected: the output would then
  depend on a value's shape at run time.
- **Normalize by collapsing whitespace blindly.** Rejected: it would hide changes to
  whitespace HTML renders — a changed space between two inline runs, or inside `pre`,
  must still fail.

## Consequences

- The gate asks what a browser would render, not what bytes were written: everything D26
  and D28 move must be invisible to the comparison by construction, never by allowlist.
  `rendered-html.ts` (now in `packages/shell/src/lib/`) models the UA stylesheet's
  white-space processing, not a blind collapse, and knows nothing of the site's CSS.
- The old harness's guard — indentation deltas on content lines must fail — moved rather
  than vanished: a changed space between two inline runs, or inside `pre`, still fails.
- dprint leaves the harness: formatting both sides is a byte-level normalization the tree
  diff makes moot. Its devDependencies return in the shell (`∆F9l`), where they belong.
- The spike's templates were rewritten as an author would write them.

---
id: ∆cE6
completed: 2026-09-07 10:22:21 -07:00
---

## Backlog item

> - ∆cE6 Line and column in corpus diagnostic trailers — the `.cst`/`.html` trailers print byte
>   spans only (`89..94`, `path:89..94`), which a reader cannot match to the source by eye; have
>   `crates/lemmon-corpus` render `line:col` beside each span (labels too), update
>   `tests/corpus/README.md`, and re-bless once. Spans stay bytes in the core (invariant 8);
>   the display step is one `lem check` (∆4tG) and the LSP (∆5MM) will each need as well

## Planning

Eric picked this over the three Phase 1 items `surface` also offered, and the plan started
from the item's literal reading: render `line:col` inside `crates/lemmon-corpus`, whose own
module doc calls it deliberately dependency-free so every stage can dev-depend on it. That
would have honoured the item's letter and dropped its stated reason — the display step
[[∆4tG]] and [[∆5MM]] "will each need as well" — since nothing shipping can use a test
harness. Eric pushed on exactly that, and the scope widened twice: the mapping moved into
`lemmon-syntax`, and then the boundary itself gained positions, because
`crates/lemmon-wasm` hands the shell raw byte offsets and both consumers live in
TypeScript, where a byte offset into a UTF-16 column means re-encoding the source.

Two questions were put to Eric before anything was written, since both get baked into 263
trailer lines. He asked whether the raw byte offsets were even needed; they were not — the
only expectation file parsed by code is `.tags`, via `boundary_spans`, and `.tags` has no
trailer — so the trailers show positions alone. And he chose UTF-16 code units over
characters or bytes, which settled the boundary type as the LSP's `Position` exactly:
zero-based, so [[∆5MM]] passes one straight through, with the `+1` belonging to whoever
prints. That split is the substance of D33, along with the rule the review later forced:
a span is placed against the text that produced its unit.

The plan also recorded, honestly, that this had stopped being a standing-Refactors item —
four crates and a package — and might belong in Phase 1 instead. Eric left it where it was
and added one instruction: `/code-review high` on Sonnet, fixing what it confirmed.

## Refinement

Two things moved after the plan. `LineIndex` was going to be hand-rolled; the first draft
stored a copy of every source file to count UTF-16 units, which would have made the
resident `Core` retain the text of every file it ever compiled — the opposite of what the
plan promised. CLAUDE.md's "don't re-invent the wheel" pointed at rust-analyzer's
`line-index`, which stores line starts and each line's non-ASCII characters instead, shares
rowan's `text-size`, and pulls only `nohash-hasher` besides. Its `try_line_col` returns
`None` for both an out-of-range offset and one inside a character, which is precisely the
refusal invariant 10 wants; the wrapper adds the naming and the panic.

The review found one real bug, and it was mine. Line indexes were keyed to the *latest*
text at each path, but a file whose recompile fails keeps its previous unit (D25), whose
spans are offsets into the text that produced it. A later clean compile's relink diagnostic
about that stale unit was placed against newer, shorter text and panicked past the end —
across the boundary that aborts the worker rather than returning a `JsError`. Indexes are
now updated in lockstep with `units`, with the call's own files as an overlay, and
`places_a_stale_unit_s_span_against_the_text_that_produced_it` is the regression test; it
was written and watched to fail before the fix. That also subsumed the review's third
finding, unbounded index growth: the map is now keyed exactly to the units and asserted so.
Its second finding — a mid-blessing panic that dropped the case name — was fixed as found.

Two checks claimed to enforce something were negative-tested rather than assumed: breaking
the per-file label lookup made the label report line 0 instead of 1, and dropping index
retention made the resident-file test panic. The blessed diff came out as 263 insertions
and 263 deletions across 66 files — a pure 1:1 replacement, with no `.tags` file changed
and nothing above a `--- diagnostics` marker moved, which is what invariant 9 needed.

## Retrospective

**The item's stated reason was worth more than its stated instruction.** The item said
"have `crates/lemmon-corpus` render `line:col`" and also said why: two later items need the
same step. The first plan followed the sentence and quietly abandoned the reason, and the
sentence was defensible enough that it read as complete. Eric's one-line question is what
turned a test-harness tweak into something [[∆4tG]] and [[∆5MM]] inherit. When an item
gives a rationale, the plan should be checked against the rationale, not only the verb.

**A doc comment promised a property the code did not have.** The first `LineIndex` doc said
it holds "line starts and not the text" while the struct held a `String`. Nothing would
have caught that — no gate reads the docs — and it would have shipped a memory regression
into the resident core described as the opposite. Reaching for the well-used crate fixed
the contradiction by making the promise true, which is the better order: the claim came
first and the implementation had to earn it.

**The review paid for itself on the one thing local reasoning had blessed.** "Insert the
index before `prepare` so a file that fails to parse can still be placed" was right about
the incoming file and wrong about the resident one, and the gate was green either way,
because no corpus case and no existing boundary test involves a failed recompile followed
by a relink. The habit worth keeping is the one the finding itself models: when two maps
are supposed to describe the same thing, ask what happens on the path where one is updated
and the other is not.

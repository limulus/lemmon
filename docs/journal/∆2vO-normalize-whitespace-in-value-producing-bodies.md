---
id: ∆2vO
completed: 2026-08-25 20:05:20 -07:00
---

## Backlog item

> - ∆2vO Normalize whitespace in value-producing bodies — implement D26: trim and dedent
>   `lem-fn` block bodies and `lem-let` capture bodies, leaving `lem-if`/`lem-for` verbatim.
>   Today a component's own indentation leaks into the output bytes of every page that calls
>   it, and changing that once templates exist re-blesses every corpus expectation and
>   re-derives the parity allowlist

## Planning

D26 was underspecified in two corners that changed the shape of the work, so both went to
Eric before any code existed.

**A body that opens on its own tag's line stays verbatim.** `<lem-fn f()><svg>` … leaves the
first line with no indentation, so the common margin is empty and nothing is stripped. The
alternative — exclude the first partial line, let the lines below it set the margin — makes a
component structurally unable to emit a margin of its own, and every template in
`spike/the-rebound/site/` depends on emitting one. Hugging became the deliberate escape
hatch.

**The line rule from [[∆c8h]] widened in the same pass.** It covered void keyword tags; a
block `lem-fn` or a `lem-let` capture renders nothing where it stands either, and a file of
component definitions was leaking a blank line per definition —
`render/canonical/article-card.html` opened with six of them. That is the same gap the rule
was written to close, so folding it in was one line of code plus docs, against a second full
re-bless of every `.html` if it were filed separately. It became D27, and
`render/text/void-line-rule` was renamed `statement-lines` to match.

A third reading needed no asking. D26 says flow constructs "stay verbatim", which turns out
to mean they take no edge trim and no dedent *of their own*: their text still dedents with
the definition around them, and its lines still set the margin. Any other reading emits
garbage from an `lem-if` nested one level in.

## Refinement

The line rule ran per-body at lowering time, and D26 cannot: it has to measure one margin
across bodies that do not exist yet. So `Body` became an eagerly-built tree of pieces — text,
tag, `If { arms }`, `For { body, otherwise }` — the cross-arm and body→else trims moved into
construction, and the whole whitespace story moved to a new `compile/whitespace.rs`, leaving
`items.rs` 280 lines lighter and doing only lowering.

**D26's own caveat had to be built, not inherited.** "It runs after the void-line rule (a
line that rule deletes must not set the margin)" reads like a note about sequencing. It is a
data requirement: `trim_statement_line` now reports whether it fired, and a tag whose line is
gone is transparent to the measurement. Without that, a `<lem-let x = 1 />` written on its
own line leaves a zero-indent mark behind and flattens the margin to nothing — the exact bug
the sentence warns about, and it would have passed every test that existed when I wrote it.

Ten mutation runs checked that the tests bite: each edge trim removed, shortest-count instead
of shared prefix, the rejected hugging reading, non-transparent erased lines, flow bodies
normalized as value bodies, void-only silence, the `\r` kept. **One test was passing for the
wrong reason** — the hugged-body case had no leading whitespace on its first line, so it
agreed with both readings of the rule it claimed to pin. Perturbing the code found that;
rereading the test would not have.

The spike cost two newlines. Every definition there hugs, so the dedent is inert on them by
construction; only `articleLayout` and `pageLayout` close on their own line, and each lost one
trailing newline to the edge trim. Back to 96.5% of lines identical, ten hunks in three
classes, zero unexpected.

## Retrospective

**I planned around a constraint instead of questioning it.** The whole item took "the spike
must match WebC's bytes" as fixed, and designed a language rule to fit inside it — down to
choosing the hugged-body reading partly because the fixture needed it. But D26 exists to stop
a definition's indentation reaching the output, and that gate demands another engine's
indentation reach it; the two had been in contradiction since the moment D26 was written. I
noticed only when Eric said output whitespace does not matter to him. That produced D28 and
∆Ma4 — and the reversal cost a reformat of all six spike files that I then reverted. The
forward rule: when an item's *why* is "X leaks into the output", find out what is pinning that
output before planning around it.

**I cited my own blessed output as evidence without reading it.** I claimed
`article-card.html` shows a real template dedenting cleanly all the way down. It does not — it
still has ragged seams at every flow construct and every multi-line interpolated value, which
is D26 working as designed. Eric had to ask me to justify it. A corpus expectation I have just
blessed is evidence of what the code does, never of what I hoped it would demonstrate.

The mutation pass, by contrast, is the thing that worked, and it is in `CLAUDE.md` only
because [[∆c8h]] learned the same lesson about the same rule. It earned its place twice now.

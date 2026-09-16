# D26 — Body whitespace

- **Status:** Accepted 2026-08-23; the hugged, flow, and nested readings settled
  2026-08-24 when it was implemented (∆2vO); (∆1Hp) `lem-style` and `lem-head` bodies are
  value bodies, and the trailing edge is found past the statements whose lines D27 took —
  a body ending in one had kept the newline above it
- **Backlog item:** ∆2vO
- **Normative text:** `docs/language.md` § Whitespace

## Context

The Phase 0 spike showed a component's source indentation leaking into the output bytes
of every page that calls it: a definition written indented inside its `lem-fn` carried
that indentation to each call site. Lemmon optimizes how a `.lem` file reads (D28), so a
definition's own layout must never reach the output.

## Decision

Value-producing block bodies — a `lem-fn` block body and a `lem-let` capture body (D22's
slots) — are normalized at compile time:

- one newline-and-indent is trimmed at each edge, so a deliberate blank line survives as
  one;
- the lines that remain are dedented by the longest whitespace prefix they share, blank
  lines ignored;
- a body that opens on its own tag's line (`<lem-fn f()><svg>` …) has no indentation on
  that first line, so the margin is empty and the body stays verbatim — hugging is how a
  definition emits a margin of its own;
- flow constructs (`lem-if`, `lem-for`) take no edge trim and no dedent of their own — a
  loop body's leading newline is what separates iterations — but their text lies inside
  the definition around them, so it dedents with it and its lines set the margin like any
  others;
- a nested capture body is a root of its own: normalized on its own, opaque to the body
  around it.

The rule is lexical, over the template's literal text only. It runs after D27 (a line
that rule deletes must not set the margin) and never touches interpolated values, which
arrive at run time. No element awareness (invariant 2): `<pre>`/`<textarea>` content
dedents like everything else — only the shared margin goes, relative indentation
survives.

## Alternatives considered

- **Exclude the first partial line of a hugged body and let the lines below it set the
  margin.** Rejected: it makes a component structurally unable to emit a margin of its
  own, and every template in `spike/the-rebound/site/` depends on emitting one.
- **Flow bodies fully verbatim** — no dedent at all inside `lem-if`/`lem-for`, not even
  with the definition around them. Rejected without asking: any reading other than "no
  trim and no dedent *of their own*" emits garbage from a `lem-if` nested one level in.

## Consequences

- "Runs after D27" is a data requirement, not a sequencing note: `trim_statement_line`
  reports whether it fired, and a tag whose line is gone is transparent to the margin
  measurement. Without that, a `<lem-let x = 1 />` on its own line leaves a zero-indent
  mark that flattens the margin to nothing — and every test that existed passed.
- The whole whitespace story lives in `crates/lemmon-vm/src/compile/whitespace.rs`, over
  an eagerly built `Body` tree, because one margin has to be measured across bodies that
  lowering has not built yet.
- Every `.html` corpus expectation was re-blessed; the spike lost two trailing newlines
  (`articleLayout` and `pageLayout`, the only definitions there that close on their own
  line).
- D26 exists to stop a definition's indentation reaching the output, while the spike's
  byte gate demanded that another engine's indentation reach it. That contradiction
  produced D28.

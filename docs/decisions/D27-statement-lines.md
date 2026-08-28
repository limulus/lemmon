# D27 — Statement lines

- **Status:** Accepted 2026-08-24 (∆2vO), widening the void-tag-only line rule from ∆c8h
- **Backlog item:** ∆2vO
- **Normative text:** `docs/language.md` § Whitespace

## Context

The line rule from ∆c8h covered void keyword tags: a `<lem-let x = e />` alone on its
line took the line with it. A block `lem-fn` or a `lem-let` capture renders nothing where
it stands either, and once D26 landed a file of component definitions was still leaking a
blank line per definition — `render/canonical/article-card.html` opened with six of
them. That is the same gap the rule was written to close.

## Decision

A tag that renders nothing where it stands takes its line with it when nothing else
stands on that line: `lem-let` and `lem-fn`, in their void form and in their block form
alike. So a file laid out one statement per line leaks no blank line per statement, and
a file of component definitions leaks none per definition.

`<lem.…>` renders in place and keeps the line it is written on; `lem-if`/`lem-for`
render their bodies and keep theirs.

## Alternatives considered

- **File the widening as its own backlog item.** Rejected: one line of code plus docs
  inside ∆2vO, against a second full re-bless of every `.html` if done separately.

## Consequences

- `tests/corpus/render/text/void-line-rule` was renamed `statement-lines`.
- D26 measures its margin after this rule runs, and a line this rule deletes must not
  set it — see D26's consequences for why that is a data requirement.

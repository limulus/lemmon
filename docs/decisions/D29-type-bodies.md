# D29 — Type bodies

- **Status:** Accepted 2026-08-26 (∆up2); field order extended with `from` by D30 on
  2026-08-28
- **Backlog item:** ∆up2
- **Normative text:** `docs/language.md` § Scanner (fields mode); § Types › Schemas

## Context

∆up2 was the first time the parser had to read a `lem-type` body. The scanner drops back
to text mode after every block head, but a field list is expression-language tokens where
newlines matter — the scanner's two modes were one short.

## Decision

A `lem-type` block's body is not text. Its opener's name alone (invariant 4) puts the
scanner into *fields mode* — the tag-mode vocabulary with a line ending as a token — until
the first `<`, which rescans as text so the close tag lexes as every close does. One field
per line, `name: Type [by …] [from …] [= default]`; a line ending inside open brackets is
whitespace, so a default can be a multi-line literal (D24). `extends`, `by` and `from` are
contextual keywords, plain names elsewhere. One rule, at the scanner, that both grammars
mirror.

## Alternatives considered

- **A text body the parser re-lexes.** Rejected: the parser would become a second
  scanner, and the tree-sitter grammar a third.
- **A tag per field.** Rejected: three tags of noise per line, for a body that is never
  markup.

## Consequences

- The scanner switches modes, not the parser — `scan` runs to completion before the
  parser sees a token, so the switch is in the scanner's driver, on the opener's name.
- Newlines are suppressed at bracket depth; the first corpus bless showed a multi-line
  default costing three diagnostics before that.
- The tree-sitter grammar has one rule to mirror (invariant 9).

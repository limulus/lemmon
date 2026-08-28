---
id: ∆up2
completed: 2026-08-28 14:40:11 -07:00
---

## Backlog item

> - ∆up2 Type syntax — composite annotations (`[T]`, `{K: V}`, `(A, B): T`, `T?`),
>   `lem-type` blocks (fields with `= default` and `by index`, one per line, lexed in a fields
>   mode the tag name selects) and `lem-enum`, `lem-match`/`lem-case`/`lem-default`: parser,
>   AST views, corpus `.cst`/`.tags`, no semantics. Split from ∆C8d so the checker starts on
>   a tree that already holds every declaration it reads

## Planning

This item did not exist when planning began: `∆C8d` did, and planning it turned into
splitting it. The seed item bundled a type system, a checker, and record loading —
parser work, a whole new pass, and a `loadData` that validates — behind one deltoid.
Eric took the three-way split: this item for the syntax, `∆C8d` for the checker itself,
and `∆iCW` for schemas at load. `∆Hbw` lost its "typed front matter validated at YAML
positions" clause to `∆iCW` and both it and `∆rT5` had their `needs:` re-pointed there,
since front-matter validation is what they were actually waiting on.

Two decisions were put to Eric and both were taken as recommended: the checker will keep
`site`/`page`/`build` on an internal unknown type until `∆iCW` gives them schemas — so
the spike keeps rendering while the checker lands — and the three "beyond minimal" pieces
(secondary spans, unused warnings, `Value::Record` by index) stay in scope rather than
becoming follow-ups.

The one real design question was how to lex a `lem-type` body. The scanner drops back to
text mode after every block head, but a field list is expression-language tokens where
newlines matter. The plan chose a third scanner mode selected by the opener's name, which
keeps invariant 4 intact and gives the tree-sitter grammar one rule to mirror. Two
alternatives were rejected in the write-up and are recorded in D29: a text body the parser
re-lexes (the parser becomes a second scanner, tree-sitter a third) and a tag per field
(three tags of noise per line for a body that is never markup).

## Refinement

Two things about fields mode were wrong in the plan and only showed up against real input.

**The scanner switches modes, not the parser.** The plan said "the parser morphs the
scanner", which cannot happen: `scan` runs to completion before the parser sees a token.
The switch is in the scanner's driver, on the opener's name — which is the same rule, at
the only place that can apply it.

**A newline inside brackets is not a field boundary.** The first corpus bless showed a
multi-line default (`= [\n 1,\n]`) costing three diagnostics, because each line ended a
field. Newlines are suppressed at bracket depth, so a default lays out one element per
line like every other literal (D24).

`/code-review high` then found five things; all five were fixed here.

- **A field the close tag cut short reported nothing.** `error_expected` goes silent at a
  cut head, and `at_cut_head` treated any text-mode token as a cut. True in tag mode — the
  scanner only abandons a head at a `<`, and it says so — and false in fields mode, where
  reaching `</lem-type>` is the normal ending and the scanner says nothing. So
  `<lem-type A>\n  a: </lem-type>` parsed clean and handed the checker a `Field` whose
  type is `None`. The parser now tracks whether it is inside a field. This is exactly
  CLAUDE.md's "suppression keyed on a coincidence": the coincidence was that both
  endings are text-mode tokens, and every corpus case happened to put a newline before
  the close tag.
- **`Field::index()` returned the default for a field named `by`.** The scan for the
  contextual keyword started at the field's first token — which is its name. A byline
  field (`by: string = "Eric"`) read as a reference field indexed by a string literal.
  It now starts after the type annotation, where the keyword can actually be.
- The depth-limit recovery in `type_expr` and `expr_bp` used the head-end skip, which
  inside a field swallows every remaining line; both now dispatch on the mode.
  `type_expr`'s `bool` return was dead at all five call sites and went. The architecture
  doc's scanner row still said two modes.

Left alone by design: an unclosed bracket in a default swallows the lines after it and
costs one diagnostic at the close tag, which is the honest reading of "the author did not
finish this literal".

## Retrospective

**The corpus caught what the unit tests could not, and then did not catch the rest.**
The multi-line default was found by reading a blessed diff — the bless-as-proposal
discipline working exactly as `tests/corpus/README.md` describes. But the cut-field hole
survived both the unit tests and the corpus, because every case I wrote put a newline
before the close tag. Cases written from the author's seat still inherit the author's
habits; the shape that broke it was one nobody would type on purpose, which is precisely
the shape a suppression rule needs tested against. [[∆c8h]]'s lesson — "when a helper
takes the thing on either side, the tests owe it every kind of thing that can be there"
— has now recurred as "when a rule silences a diagnostic, the tests owe it every way the
construct can end".

**A mode flag needs a second reader before it is trusted.** `at_field_end` was written
for this feature and `at_cut_head` was not updated to match, even though the two answer
the same question in different modes. Adding a distinction is only half the work; the
other half is grepping for everything that asked the old question.

**Splitting the item was worth more than planning it.** The original `∆C8d` would have
been a checker landing on a parser that could not yet read a schema, with the type
grammar invented mid-pass. Doing the syntax first means `∆C8d` starts against a tree that
already holds every declaration, with a corpus that pins the shapes. The cost was one
planning session; the alternative was discovering the fields-mode question halfway
through the checker.

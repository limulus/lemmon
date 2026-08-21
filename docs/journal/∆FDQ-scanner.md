---
id: ∆FDQ
completed: 2026-08-20 21:21:33 -07:00
---

## Backlog item

> - ∆FDQ Scanner — logos lexer with text/tag modes per `docs/language.md` §Scanner (triggers
>   `<lem.` `<lem-` `</lem-`, first-`>` termination), plus the round-trip property test; it
>   is the substrate every stage sits on; needs: ∆059

## Planning

The plan was small and survived mostly intact: two logos lexers over one buffer joined by
`morph()`, a lossless token stream (whitespace as trivia, unlexable bytes as `Error` tokens
paired with a spanned `Diagnostic`), payload-free `TokenKind`s so a token is just its span,
and proptest holding the byte-for-byte round trip. Tag-name validity and string
interpolation were left to the parser from the start — the scanner finds boundaries, it does
not judge names.

**Planning reopened D21 three times, each at Eric's prompting.** The plan's token list
included the named function close (`</lem-fn.articleCard>`) straight from the repo docs;
Eric asked where it came from, judged it a confusing construct, and settled on the bare
`</lem-fn>` like every other block. Then the `<lem-- -->` comment tag went (HTML comments
already pass through, and the tag was a fourth trigger to teach tree-sitter). Then the
datetime literal went: the few legitimate uses — a fixed cutoff, a site launch date — are
served by a `datetime("…")` stdlib constructor that the checker can validate at compile
time, and the literal was the only place `-` inside a token did not mean subtraction. Each
change was recorded in `docs/decisions.md`'s chronology, `docs/language.md`, and the "The
Lemmon Language" artifact (updated in place from this session) in the same pass.

## Refinement

`/code-review` ran over the working tree before commit and produced ten verified findings,
six of them confirmed by running probe inputs through `scan`. All were taken:

- **Diagnostics were not in span order**, contradicting their doc comment: a head's
  "unterminated" diagnostic is only known after the errors inside it. Sorted on exit, with a
  property test.
- **An unterminated string swallowed the rest of the file** into one `Error` token. Strings
  now end at their line at the latest.
- **Tag names were strict** (`[a-z][a-z0-9-]*`), so `<lem-If x>` was reported as "expected a
  tag name" and the head leaked out as text — the parser could never produce the
  did-you-mean invariant 5 requires. Names are now any `[A-Za-z0-9_-]*`, empty included; the
  parser owns the builtin table, so it owns the rejection.
- **No recovery boundary inside a head short of `>`**: a forgotten `>` absorbed the next
  HTML element and reported "unexpected character" at its `<`. A `<` in tag mode now ends
  the head with an "unterminated tag" diagnostic spanning opener→`<`, and a fresh text-mode
  lexer resumes at the `<`. This is a tag-boundary rule tree-sitter must mirror, so it is
  now stated in `language.md` §Scanner rather than left implicit in the driver loop.
- **The datetime regex backtracked** (`2024-11-24T1` → `Datetime` + `Ident`). Resolved by
  dropping the literal, see Planning.
- **`TagMode` was a 42-variant mirror of `TokenKind`** with an identity mapping. logos
  accepts pattern-less variants, so `TokenKind` now derives `Logos` directly and the driver
  morphs between `TextMode` and `TokenKind`. One consequence: `ScanError` had to become
  `pub` as the public enum's logos error type (it is not re-exported).
- Deleting the inline annotations from the schema example in `language.md` had also deleted
  two facts that lived only there (the scope a computed default sees; the load-time check of
  `author` against `site.authors`). Re-homed as §Schemas bullets.
- `CLAUDE.md` had been edited to say "scanner done" ahead of acceptance; reworded.

Two provisional readings are on record, not decided: an interpolated expression inside a
string may not itself contain a string literal until the interpolation delimiter is settled
(the scanner ends a literal at the first unescaped quote), and trailing-newline consumption
for a lone void tag belongs to the parser, which knows tag shape.

Policy set here: `proptest-regressions/` files are committed, per proptest's own guidance —
a minimised counterexample is worth more in the repo than on one machine.

## Retrospective

**The first cut guessed where it should have deferred.** Strict tag names and the
never-produced `MissingTagName` pseudo-variant existed to satisfy invariant 10 ("errors over
guesses") at the scanner layer — and in doing so produced a wrong error message and withheld
the head from the stage that could have given a right one. The boundary finder should find
boundaries; every judgement it makes on the way is one the parser has to re-make or live
with. The rewrite is smaller than the original.

**Recovery has to be designed, not assumed.** The `scan` doc promised that "a half-typed
template still yields a complete token stream," and the tests agreed, because every test
input was either well-formed or broken right at the end. The review's probes — a missing `>`
followed by real HTML, a missing `"` followed by three more lines — showed the promise was
false in the common case and only true at EOF. The lesson generalises to the parser bake-off
(`∆7Ib`): recovery claims need adversarial fixtures that break _early_ in a realistic file,
and those fixtures should go into the corpus (`∆TEJ`) so that the tree-sitter grammar is
held to the same boundaries.

**`wasm:check` is weak evidence for this change.** logos is `no_std`-clean and the check
passed, but nothing in `lemmon-wasm` calls `scan` yet, so the linker almost certainly
dropped it. The import allowlist proves invariant 1 for the scanner only once `∆TNT` exports
it — worth re-running deliberately then, not trusting the green from today.

**Language changes mid-item were cheap because the docs are the spec.** Three D21 revisions
cost three small doc diffs, a test each, and an artifact republish. That is the arrangement
working as intended; keeping `language.md` normative and the artifact in sync from the same
session is what made it cheap, and is worth keeping up.

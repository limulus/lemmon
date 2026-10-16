# A comment form for `.lem`

## Background

`docs/language.md` §Scanner: there is no compile-time comment tag, and an HTML comment is
text that passes through to the output, a trigger inside it still firing. The review of
the limulus.net port (`∆foi`, 2026-10-15) found four places where an author's reason could
not be written down in the file:

- `sortBy(|r| 0 - r.width)` keeps ties stable where `.reverse()` would flip them (two
  renditions of equal width).
- `tags` is declared on `Post` only because Eleventy still reads the same posts until the
  cutover.
- The base layout splits `p is Post` into two blocks to keep the original's `meta` order.
- `revision-history` captures its message so the `<pre>` stays on one line.

CLAUDE.md's rule that a comment explains why something unusual is done has no home in
Lemmon source today.

## Scope and constraints

- A design question for Eric before any syntax: a `lem-` tag that the compiler drops
  (`<lem-- … -->`? a `<lem-comment>` block?) must keep invariant 3 (no `>` in a head) and
  invariant 4 (shape decidable from the head), and must say what happens to a trigger
  inside it. Whether comments belong in the CST for tooling (D16) is part of the question.
- Not a `.md` concern: a document already has HTML comments and prose.

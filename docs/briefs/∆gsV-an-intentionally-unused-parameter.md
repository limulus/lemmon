# An intentionally unused parameter

## Background

Found on 2026-10-18 porting limulus.net's feeds (`∆zu0`): the plan's pass-through layout
for XML files, `(file, body) = body`, drew `lem check`'s warning that `file` is never used,
since every `lem-fn` parameter counts as a binding (D31's walk warns on an unused one, as
`docs/language.md` says of a name read by nothing). A layout's signature is fixed by the
entry field's type, so it must take the record whether it reads it or not; the port wrote
its XML layout to read the record instead.

## Scope and constraints

- A spelling for "unused on purpose": a leading `_` (Rust's convention), a bare `_`, or no
  warning for a parameter a fixed signature requires. Whichever it is updates
  `docs/language.md`, the corpus, and the decision index.

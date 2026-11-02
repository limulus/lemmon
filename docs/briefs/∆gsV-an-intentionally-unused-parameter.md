# An intentionally unused parameter

## Background

Found on 2026-10-18 porting limulus.net's feeds (`∆zu0`): the plan's pass-through layout
for XML files, `(file, body) = body`, drew `lem check`'s warning that `file` is never used,
since every `lem-fn` parameter counts as a binding (D31's walk warns on an unused one, as
`docs/language.md` says of a name read by nothing). A layout's signature is fixed by the
entry field's type, so it must take the record whether it reads it or not; the port wrote
its XML layout to read the record instead. Since D48 every page has a type, so a page that
wants no layout takes a pass-through one, which meets the same warning: the shell's spec
fixture `MEMO_LEM` (`packages/shell/src/mocks/sites.ts`) reads `m.draft` for it, and the
placeholder gate does not read `packages/`, so the fix sweeps that read by hand.

## Scope and constraints

- A spelling for "unused on purpose": a leading `_` (Rust's convention), a bare `_`, or no
  warning for a parameter a fixed signature requires. Whichever it is updates
  `docs/language.md`, the corpus, and the decision index.

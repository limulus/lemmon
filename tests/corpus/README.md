# The golden corpus

Hand-written inputs with checked-in expectations, shared by every stage of the core and —
once it exists — by `tree-sitter-lemmon`. This is what invariant 9 in `CLAUDE.md` means by
"corpus coverage": a change to syntax or semantics lands with a case here, and the two
grammars are held to the same tag boundaries for every case.

The harness is `crates/lemmon-corpus`; each stage crate has an integration test that calls
it. Nothing in this directory is stage-specific — a case can grow expectations for stages
that did not exist when it was written.

## Layout

```
tests/corpus/
  syntax/<feature>/<name>.lem     a template, parsed by lemmon-syntax
```

The top level groups by **what a case is for**. `syntax/` is parse-only; `render/` (∆c8h,
input + data → HTML) and `document/` (∆Ldn, `.md` in document mode) join it as those stages
land. Below that, directories group by **language feature**, and names are descriptive —
no numeric prefixes, because a corpus that only grows makes them meaningless.

There is no `ok/` versus `broken/` split. Whether a case parses clean is *data*: its
diagnostics section is empty. That keeps a case in the directory that says what it tests
even after a language change turns it from clean to broken, or back.

## Expectations

Each case is a `.lem` input plus one expectation file per stage, sharing its basename:

| File | Produced by | Contents |
|---|---|---|
| `<name>.cst` | `lemmon_syntax::dump` | The lossless tree, one node or token per line, then `--- diagnostics` and one `start..end: message` per line |
| `<name>.tags` | `lemmon_syntax::tag_dump` | The tag-boundary projection: text runs, tag nodes, close tags, and the errors standing where an item would, with heads, expressions, and `Block` wrappers elided |

`.tags` is the agreement target for `tree-sitter-lemmon` (∆5MM). A context-free grammar can
say where a tag begins and ends; it cannot be held to the shape a Pratt parser gave a head,
so the two parsers can be diffed here without either having to model the other's interior.

The projection is deliberately *not* a partition of the file. Trivia between items belongs
to no boundary, and a void tag whose head the scanner cut ends at the last token its head
consumed rather than at the cut — see `syntax/recovery/unterminated-string`, where the
tag's own bytes 16..33 fall outside `LetVoid@0..16`. What the corpus test does assert is
that the boundaries are well formed as a tree: one root covering the whole file, every
boundary inside its parent and after its previous sibling.

`.html` joins the table with ∆c8h.

## Blessing

```sh
npm run corpus:bless     # UPDATE_CORPUS=1 cargo test --workspace
```

That rewrites every expectation from what the code currently produces. **It is not a fix —
it is a proposal.** Read the resulting `git diff` before keeping it:

- Lines appearing in a `--- diagnostics` trailer that was empty mean a case that used to
  parse clean no longer does. That is a regression until proven otherwise.
- A span shifting in `.tags` means a tag boundary moved, which is a language change and
  needs `docs/language.md` to say so.

## Adding a case

Write the `.lem`, run `npm run corpus:bless`, read the diff, commit both. The harness
enforces the rest:

- **No orphans.** Every file here is a `.lem`, an expectation belonging to one, or this
  README. A renamed case cannot leave stale expectations behind.
- **Every `SyntaxKind` is reached.** A node or token kind that no case produces fails the
  build, so a new piece of syntax cannot land without a case. The only escape is the
  `UNREACHABLE` list in `crates/lemmon-syntax/tests/corpus.rs`, and each entry carries the
  design reason it can never appear in a tree.

## Adding a stage

Add the extension to `EXPECTATIONS` in `crates/lemmon-corpus/src/lib.rs`, then call
`lemmon_corpus::check("<group>", "<ext>", |case| …)` from the stage crate's integration
test. Discovery, blessing, and failure reporting are already there; do not write a second
walker.

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
  render/<feature>/<name>.lem     a template, compiled and rendered by lemmon-vm
  document/<feature>/<name>.md    a document, segmented by lemmon-syntax and rendered by lemmon-vm
```

The top level groups by **what a case is for**. `syntax/` is parse-only; `render/` runs the
compiler and VM, and supplies its own data with `lem-let` literals (`page` and `site` are
bound to small fixed records by the test); `document/` is `.md` in document mode, checked by
both crates — its `.cst` shows the segmentation and its `.html` the spliced output. Below
that, directories group by **language feature**, and names are descriptive — no numeric
prefixes, because a corpus that only grows makes them meaningless.

There is no `ok/` versus `broken/` split. Whether a case parses clean is _data_: its
diagnostics section is empty. That keeps a case in the directory that says what it tests
even after a language change turns it from clean to broken, or back.

## Expectations

Each case is a `.lem` or `.md` input plus one expectation file per stage, sharing its
basename:

| File          | Produced by                    | Contents                                                                                                                                                         |
| ------------- | ------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `<name>.cst`  | `lemmon_syntax::dump`          | The lossless tree, one node or token per line, then `--- diagnostics` and one `line:col..line:col: message` per line; for a `.md`, the tree `parse_document` assembled |
| `<name>.tags` | `lemmon_syntax::tag_dump`      | The tag-boundary projection: text runs, tag nodes, close tags, and the errors standing where an item would, with heads, expressions, and `Block` wrappers elided |
| `<name>.html` | `lemmon_vm::{compile, render}` | The rendered output, then `--- diagnostics` and one `path:line:col..line:col: message` per compile diagnostic — warnings too — or for the runtime error that stopped the render |

### Positions in a trailer

The stages produce byte spans — that is what the core works in (invariant 8) — and
`crates/lemmon-corpus` renders them here as **one-based** `line:col`, because the point of
a blessed diff is that someone reads it, and nobody can match `89..94` to a place in a file
by eye. `syntax/recovery/error-after-wide-characters` pins the convention: columns count
UTF-16 code units, so an emoji before the error costs two columns and three bytes costs
one. (The core's own `Position` is the LSP's, counting from zero; the extra one is the
display's.)

A span in *another* file — a label pointing where a name was first defined — stays in
bytes: the harness holds one case's source and has nothing to place it against.

Everywhere else the byte ranges stand. The tree in a `.cst` keeps `@start..end`, and all
of `.tags` does, because that projection is what a tree-sitter grammar is held to.

In both trailers a warning's message is prefixed `warning: `, and each secondary span a
diagnostic carries follows it on its own line, indented two spaces, as
`line:col..line:col: note` (path-qualified in `.html`). A warning is data like any diagnostic: a case that warns says
so in its trailer, and a rule that warns is tested on both halves — the case it fires on
and the neighbouring name it must leave alone.

`.tags` is the agreement target for `tree-sitter-lemmon` (∆5MM). A context-free grammar can
say where a tag begins and ends; it cannot be held to the shape a Pratt parser gave a head,
so the two parsers can be diffed here without either having to model the other's interior.

The projection is deliberately _not_ a partition of the file. Trivia between items belongs
to no boundary, and a void tag whose head the scanner cut ends at the last token its head
consumed rather than at the cut — see `syntax/recovery/unterminated-string`, where the tag's
own bytes 16..33 fall outside `LetVoid@0..16`. What the corpus test does assert is that the
boundaries are well formed as a tree: one root covering the whole file, every boundary
inside its parent and after its previous sibling.

## Blessing

```sh
npm run corpus:bless     # UPDATE_CORPUS=1 cargo test --workspace
```

That rewrites every expectation from what the code currently produces. **It is not a fix —
it is a proposal.** Read the resulting `git diff` before keeping it:

- Lines appearing in a `--- diagnostics` trailer that was empty mean a case that used to
  parse (or render) clean no longer does. That is a regression until proven otherwise.
- A span shifting in `.tags` means a tag boundary moved, which is a language change and
  needs `docs/language.md` to say so.
- A trailer line that moved without its message changing is a span that moved. Open the
  input at the line and column it now names and check that is what the diagnostic is
  about — that is what the positions are there for.

## Adding a case

Write the `.lem`, run `npm run corpus:bless`, read the diff, commit both. The harness
enforces the rest:

- **No orphans.** Every file here is an input, an expectation belonging to one, or this
  README. A renamed case cannot leave stale expectations behind.
- **Every `SyntaxKind` is reached.** A node or token kind that no case produces fails the
  build, so a new piece of syntax cannot land without a case. The only escape is the
  `UNREACHABLE` list in `crates/lemmon-syntax/tests/corpus.rs`, and each entry carries the
  design reason it can never appear in a tree.
- **Every `Op` is emitted.** Likewise for the VM: an instruction no `render/` case compiles
  to fails the build (`crates/lemmon-vm/tests/corpus.rs`).

## Adding a stage

Add the extension to `EXPECTATIONS` in `crates/lemmon-corpus/src/lib.rs`, then call
`lemmon_corpus::check("<group>", "<ext>", |case| …)` from the stage crate's integration
test. Discovery, blessing, and failure reporting are already there; do not write a second
walker.

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
  host/<feature>/<name>.{lem,md}  compiled against a declared host, rendered by lemmon-vm
```

The top level groups by **what a case is for**. `syntax/` is parse-only; `render/` runs the
compiler and VM, and supplies its own data with `lem-let` literals (`page` and `site` are
bound to small fixed records by the test); `document/` is `.md` in document mode, checked by
both crates — its `.cst` shows the segmentation, with any front matter as one
`FrontMatter` token in front (the same token a `.lem` file's leading `---` block is,
`syntax/front-matter/`), and its `.html` the spliced output. Below
that, directories group by **language feature**, and names are descriptive — no numeric
prefixes, because a corpus that only grows makes them meaningless.

`host/` is where the host's declaration is the subject (D30, D36, D37). Those cases
compile against the two files in `host/environment/` — an `ambient.lem` declaring `Page`
and `Build`, and a `site.lem` declaring the site's schemas, collections, and bindings —
with `page`, `build`, and `site` typed from them. The group's directory is the site's
content root: a page under it — a `.md`, or a `.lem` that opens with front matter (D41)
— is compiled as the record its own `type:` key names — the one raw key a shell reads
before a page has a type — or else as the element type of the collection whose glob
matched it, and it is loaded the way the shell will load it (∆4tG): its front matter
parsed with positions and handed to the loader as the one member of that collection, or
on its own in `pages` when its key typed it and no glob holds it, beside fixed fixtures
for the environment's other sources. So `host/pages/` and `host/tils/` hold documents of
two collections, `host/sections/` the page templates of a third, `host/standalone/` the
pages no glob holds, `host/unmatched/` a document and a template no glob matches, and
a load error is a trailer line at the front-matter entry it is about. A page that loads
renders through its record's `layout` with its own output as the body, so its
expectation opens with what the layout prints around it, and a page whose load fails
renders nothing. A `.lem` under a typed host that has no front matter is a component or
layout file, and `host/warnings/` shows what output at its top level earns. The two environment files are cases
themselves, so nothing about them is hidden. `render/` and `document/` deliberately keep
an untyped host, which names the same three values and types none: that is a host a
site really has (the Phase 0 spike's), and what any page no collection matched sees.

Every code block a document writes, but a `math` fence, is a `code` mark over the block
as comrak writes it (D42), so `document/fences/` holds the cases about code blocks: which
are marked, what key and payload each carries, and where a language no producer covers
warns.

There is no `ok/` versus `broken/` split. Whether a case is clean is _data_: it is what
its `--- diagnostics` trailer says and nothing else, and a case that only warns is clean.
Some directories are named `errors/` or `warnings/` because the diagnostic is what those
cases are about, but the name decides nothing — `host/pages/` holds error cases too. That
keeps a case in the directory that says what it tests even after a language change turns
it from clean to broken, or back. The boundary harness needs only the clean cases, so it
reads the trailer: `scripts/lib/corpus-fixtures.ts`.

## Expectations

Each case is a `.lem` or `.md` input plus one expectation file per stage, sharing its
basename:

| File          | Produced by                    | Contents                                                                                                                                                         |
| ------------- | ------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `<name>.cst`  | `lemmon_syntax::dump`          | The lossless tree, one node or token per line, then `--- diagnostics` and one `line:col..line:col: message` per line; for a `.md`, the tree `parse_document` assembled |
| `<name>.tags` | `lemmon_syntax::tag_dump`      | The tag-boundary projection: the front-matter token, text runs, tag nodes, close tags, and the errors standing where an item would, with heads, expressions, and `Block` wrappers elided |
| `<name>.html` | `lemmon_vm::{compile, render}` | The rendered output; then each fragment the page's collect blocks gathered, under a `--- styles`, `--- head`, or `--- scripts` line of its own (below); then `--- diagnostics` and one `path:line:col..line:col: message` per compile diagnostic — warnings too — or for the runtime error that stopped the render |

### Marks in a render

A mark is a named range of the page the host fills after the render (D42), so an `.html`
expectation shows each one where it stands: `⟦mark styles⟧` where the range is empty, and
`⟦mark toc⟧…⟦/mark⟧` around default content. A key follows the name and a payload follows
the key — `⟦mark code bash Code {language: "bash", …}⟧` — so a case shows what the host's
handler would be dispatched on and given, which is the whole of what a mark carries. The
notation is the harness's alone — nothing in the core writes it — and it is applied back to
front, so the offsets in it are the ones the render gave. Marks at one offset stand in the
order they were printed.

The `render/` and `document/` host claims `styles`, `head` and `scripts` bare, as the
shell's own bundler does, and `code` as a highlighter does: four languages, a `Code`
payload, keyed by its `language` field. So a case can place a mark under any of them, and
a case that misspells a name or a language warns.

### Fragments in a render

A `lem-style`, `lem-head`, or `lem-script` renders nothing where it stands: its output is
a fragment the host resolves (D42). So an `.html` expectation prints each fragment after
the page, one `--- name` line and then its text, names in sorted order and each name's
fragments in the order the render first reached them — which is also where dedup shows: a
component called three times leaves one section. The sections sit above the diagnostics
trailer, which is found by its last `--- diagnostics` line, so fragment text never reads
as one. The notation exists only here; nothing in the core writes it.

### Payloads in a render

A code block's mark carries only the id of its payload, `⟦mark code sh #4429e5aa07a9b5e3⟧`,
since the payload was reported when the document compiled (D42). So an `.html`
expectation resolves each id under a `--- payloads` line after the fragments: one line per
payload the page's marks name, in the order it first names each — `#id {language: …,
info: …, code: …}` — which is also where dedup shows, as two marks naming one id. The ids
are hashes of the content, so a change to how they are computed moves every one of them
here, which is what should happen: a host's cache is keyed on them.

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
it is a proposal.** A failing case already prints a unified diff of what it produced
against what is checked in — the first five differing cases in full, the rest named — so
blessing is for after you have read that and decided the change is wanted, never how you
find out what the change was. Read the resulting `git diff` before keeping it:

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

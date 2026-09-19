# ∆rKx Land the small moves the organization survey verified — plan

Written on 2026-09-19 by the session that surveyed the repository's organization for Eric,
against `main` at `d3c8f11`. Eight analysts read the repository area by area; five agents
told to refute them checked the claims. What follows is what survived and is too small to
be an item of its own. Each entry says how far it was checked: "verified" means a second
agent confirmed it against the code, "analyst only" means one agent read it. Nothing was
built. Re-derive line numbers if a file has changed. When ∆rKx's journal entry is written,
its Planning section absorbs this file and the file is deleted.

## Start here

1. Read `CLAUDE.md` and the ∆rKx item in `BACKLOG.md`. For the moves in checkpoint 1, read
   `docs/journal/∆FQw-split-the-compile-module.md` for the method of proving a move.
2. `npx @limulus/delto@1 claim ∆rKx`.
3. These changes are independent of one another. If Eric declines one at a checkpoint
   preview, drop it and carry on; none of the others depends on it.

## What kind of item this is

Nothing here is broken, with one exception that is not in this item (the arity wording,
∆RuS). The entries are of four kinds: a module that sits where its own `//!` says it does
not belong; a hand-kept list or literal that exists twice and can drift; a `//!` sentence
or a `pub` that later work made false; and two names that mean two things. Only the trailer
constants and the `suggest.rs` split add lines, and each says why.

Every checkpoint is accepted the same way: `npm run corpus:bless` leaves an empty diff,
`npm run verify` passes, and `cargo doc --workspace --no-deps` — which no gate runs — adds
no errors to the ones `docs/journal/∆Tsy-rename-colliding-types-and-fields.md` recorded.
Each checkpoint ends with a plain-language preview for Eric, `/code-review medium --fix`,
and Eric's commit.

## Checkpoint 1 — two modules to where their own docs put them (`lemmon-vm`)

**`compile/diagnostics.rs` → `src/diagnostics.rs`.** Verified; three analysts proposed it
independently. The file's `//!` (lines 9–10) says: "These are the crate's vocabulary rather
than the compiler's: the linker, document mode, and the WASM boundary all speak them." It
imports only `std::fmt` and `lemmon_syntax`, nothing from `compile`. Seven sites reach into
`compile` for it: `unit.rs:21`, `document/mod.rs:27`, `document/adapters.rs:97` (a test),
`load/place.rs:15`, `load/mod.rs:28`, `load/validate.rs:13`, and `lib.rs:38–39`, whose one
`pub use compile::{…}` splits in two. 235 lines move verbatim (205 production, 30 test).

- One item widens: `Diagnostic::from_syntax` (line 119), `pub(super)` → `pub(crate)`; its
  one caller is `compile/surfaces.rs:102`.
- A private `use crate::diagnostics::{Compiled, Diagnostic, Label};` in `compile/mod.rs`
  keeps every `use super::Diagnostic` under `compile/` working — a private `use` in a
  parent is nameable from its descendants, as `compile::tests` already relies on for
  `Uses` and `ExportIndex`. So the six files under `compile/` that name these types need
  no edit.
- One doc link breaks: the `[`diagnostics`]` bullet at `compile/mod.rs:24`. Shorten the
  moved `//!` so its last sentence states the module's position instead of excusing it.
- Afterwards `document/` imports nothing from `compile`, and `load/place.rs` no longer
  names the compiler at all.

**`EntryFault`, `entry_fault`, `entry_shape`, `no_entry_field`, `misshaped_entry_field`
(`compile/schema.rs:438–489`) → `decls.rs`.** Verified. They take no `Compiler`, read only
`&dyn Decls` and `decls` types, and have two callers, `schema.rs:221` and `unit.rs:718`.
54 lines move; nothing widens; `entry_shape` stays private. `compile/mod.rs:61`'s
`pub(crate) use schema::{…}` is deleted and `unit.rs:22–23` imports the four names from
`crate::decls`. `decls.rs`'s `//!` (lines 12–14) gives this exact reason for `no_field`
living there; rewrite that sentence to cover both rather than adding a second one.

**Optional: `compile()` and `repeated_path()` (`compile/mod.rs:66–132`) → `lib.rs`.**
Analyst only, except that `lemmon-wasm` was checked: it imports `compile_unit`, `prepare`,
`link` and `repeated_path` and never `compile`, so the whole-site driver serves only the
corpus harness (`crates/lemmon-vm/tests/corpus.rs:107`, `:318`, `:359`) and unit tests.
It is the only place the compiler calls the link step (`compile/mod.rs:108`). Moving it to
`lib.rs`, which already holds `SourceFile`, `RuntimeError` and `is_document`, leaves the
public paths `lemmon_vm::compile` and `lemmon_vm::repeated_path` unchanged; about 66 lines
move and the test module gains one `use crate::compile;`. With the two moves above, the
link step then imports only `index_u32`, `Runs` and `refused_at_load` from `compile`, and
`compile` imports nothing from the link step. The objection: a reader looks for a
whole-site `compile` in `compile/`. Put it to Eric as a question, not a default.

Prove checkpoint 1 as ∆FQw did: the moved files' logical lines, as a multiset, equal the
originals' once imports, doc headers and visibility qualifiers are normalised away.

## Checkpoint 2 — lists and literals that exist twice (`lemmon-syntax` and its consumers)

**`is_boundary` derived from `ast::Item`.** Verified. `is_boundary`
(`parser/dump.rs:105`) lists 24 `SyntaxKind`s; 17 are exactly the kinds `ast::Item` accepts
(`ast/tags.rs:22–65`), and nothing holds the two lists together. The `.tags` projection is
invariant 9's agreement target. A tag added to `Item` and not to `is_boundary` disappears
from every `.tags` file and lifts its children a level, and no test would say so:
`reaches_every_syntax_kind_from_some_case` walks the `.cst`, and
`projects_tag_boundaries_that_nest_and_never_overlap` checks nesting, not membership.
Replace the 17 with `Item::can_cast(kind)` (generated by `ast_enum!`, `ast/mod.rs:82–96`)
and keep an explicit list of the seven that are not items: `Root`, `FrontMatter`,
`ElseIfTag`, `ElseTag`, `CaseTag`, `DefaultTag`, `CloseTag`. About 17 fewer lines.
`is_boundary` is called with token kinds too (`dump.rs:143`): `Item::can_cast(Text)` is
false and `CloseTagToken` is in neither list, but recheck. Every `tests/corpus/**/*.tags`
must come back byte-identical. Then negative-test it: drop one variant from `Item`'s
`can_cast` locally, watch `.tags` files change, and reverse the edit by hand. Do this
before ∆5MM builds on the projection.

**`Token::tag_name` gets a production caller.** Verified. `Token::tag_name`
(`token.rs:203`) is documented and has four tests, and only tests call it. The parser
open-codes the same slicing four times: `parser/template.rs:199`, `:208`, `:334`, `:702`.
Add `fn current_tag_name(&self) -> Option<&'src str>` to `Parser` in `parser/mod.rs` and
use it at the four sites; the last becomes
`p.at(TokenKind::CloseTag) && p.current_tag_name() == Some(name)`. `scanner.rs:232` slices
a logos match before any `Token` exists and stays as it is.

**The export list matches what is reachable.** Verified. `VariantRef` is the payload of the
public `Expr::Variant` (`ast/expr.rs:32`, defined at `:417`) and is missing from
`ast/mod.rs`'s `pub use expr::{…}`; add it. `pub type SyntaxElement`
(`syntax_kind.rs:295`, re-exported at `lib.rs:27`) has no user in `crates/`, `packages/` or
`scripts/`; delete it. If ∆5MM's LSP wants it back it is two lines, and
`rowan::SyntaxElement<Lemmon>` is spellable meanwhile.

**Three files without a `//!`, and `nearest` out of `diagnostic.rs`.** Verified that
`diagnostic.rs`, `token.rs` and `syntax_kind.rs` are the only three Rust source files in
`crates/` with no module doc, which puts them outside the one test this repository judges
file size by. Proposed sentences: `token.rs`, "What a run of source bytes is: the token
vocabulary, which doubles as the tag-mode lexer."; `syntax_kind.rs`, "The one flat kind
enumeration rowan stores, and the language marker over it: every token kind in `TokenKind`
order, then every node kind."; `diagnostic.rs`, "What a stage says is wrong, and where: a
message, the bytes it is about, a severity, and any secondary spans worth reading beside
it." That last sentence is true only if `nearest` leaves: since ∆tdb the file also holds
the did-you-mean policy (lines 87–131, with 8 of the file's 12 tests), which is not a
diagnostic. Move it to `crates/lemmon-syntax/src/suggest.rs` — "The did-you-mean policy:
the candidate closest to a name, or none when nothing is close enough to be worth
suggesting." — with `pub use suggest::nearest;` in `lib.rs`, so `lemmon_syntax::nearest`,
the path ∆tdb settled, does not change and no consumer is edited. 45 production and 55
test lines move; about 8 lines are added. If Eric would rather not add a file, write
`diagnostic.rs`'s `//!` to name both things and leave `nearest` where it is.

**One constant for the diagnostics trailer.** The literal `--- diagnostics` is spelled in
three places (verified): `crates/lemmon-corpus/src/lib.rs:197` (`const TRAILER`, the
reader that finds the trailer and rewrites its spans as `line:col`),
`crates/lemmon-vm/tests/corpus.rs:307` and `crates/lemmon-syntax/src/parser/dump.rs:55`
(two writers). If a writer drifts, the reader stops finding the trailer, and
`corpus:bless` rewrites the affected trailers back into byte offsets — a diff that reads
as "spans moved". The `warning: ` prefix is likewise written at `dump.rs:60` and
`crates/lemmon-vm/src/compile/diagnostics.rs:150` (checked by grep in this session).
Declare `pub const DIAGNOSTICS_TRAILER` and `pub const WARNING_PREFIX` in `lemmon-syntax`,
which all three other crates already depend on, and name them at the five sites. About 6
lines are added. Do not go further and share the line renderer: `lemmon_vm::Diagnostic`'s
`Display` is also a user-facing error message, and tying it to a test format would serve
two audiences with one function. The notes for ∆W1D
(`docs/plans/∆W1D-gate-spike-and-bench.md`) propose a fourth reader of this literal, in
TypeScript, which cannot import the constant.

## Checkpoint 3 — `lemmon-vm`: one helper, two `pub`s, one name, three doc sentences

- **`element_of` → `builtins/lists.rs`.** Verified. `builtins/mod.rs:672` defines it;
  `lists.rs` is its only user (imported at line 18, called six times). Its counterpart for
  maps, `value_of`, is private in `builtins/maps.rs:25`. Seven lines move and one name
  leaves an import list.
- **`escape_html` is `pub` to nobody.** Verified. `value.rs:397` is `pub fn`; the `value`
  module is private, `lib.rs` does not re-export it, and both callers are in `value.rs`.
  Make it private. If that fails to compile, the caller it names is worth knowing about.
- **`invoke` at the crate root.** Verified. `lib.rs:59` re-exports `vm::invoke`, and
  neither `lemmon-wasm`, the corpus harness nor `lemmon-corpus` names it; inside the crate
  it is reached as `crate::vm::invoke`. Drop it from the re-export.
- **`document::Piece` → `ProsePiece`.** Verified that the collision is real:
  `document/code.rs:96` and `compile/whitespace.rs:57` both define `enum Piece`, and
  `compile/items.rs:21` imports one `as ProsePiece` to tell them apart. Rename the
  `document` one at its definition and drop the alias. About 26 sites across `document/`,
  `compile/items.rs` and `compile/surfaces.rs`; the compiler finds every one. This is the
  ∆Tsy pattern, for a collision that arrived after ∆Tsy (with ∆7wK).
- **`host.rs`'s `//!`** (lines 1–12) describes the ambient names. The file also holds the
  asset manifest (`assets`, `with_assets`, `asset`, `asset_paths`, `no_asset` — D38) and
  the mark-claim vocabulary (`Claim`, `ClaimAnswer`, `agree`, `with_marks`, `with_claims`,
  `claim`, `answer`, `claimed_names` — D42), by one agent's estimate about half its 521
  production lines. The struct's own doc (line 76) has the right sentence: "Everything a
  host declares about the environment its programs compile in". Rewrite the `//!` to open
  with that and to name the three parts and why each rides on the declaration. No code
  moves. Splitting `host/claims.rs` was considered and set aside: it would separate a
  struct's fields from the methods that read them.
- **`builtins/values.rs`'s `//!`** (lines 5–7) says the ambient namespace is "three names,
  each turning a string into something the checker then knows more about than a string".
  The table has six (`raw`, `datetime`, `url`, `asset`, `uniqueId`, `mark`); `asset` is
  folded at compile and `mark` lowers to `Op::Mark`, so neither runs as a call. Verified.
- **`rendered.rs`'s `//!`** says the file is "what a render returns". `Buffer`
  (lines 79–154) is how a render accumulates, never leaves the crate, and is the reason
  `value.rs` imports this module. Add one sentence. Verified.

## Checkpoint 4 — the boundary (`lemmon-wasm`, `packages/core`)

- **Twelve `pub(crate)` fields in `wire.rs` that nothing outside `wire.rs` names.**
  Verified by an audit of `lib.rs`'s field accesses: `HostInput.ambient`, `.entry`,
  `.assets` (lines 141, 145, 154); `PlannedOut.path`, `.url` (451, 453);
  `Diagnostic.path`, `.span` (469, 470); `RenderMeta.marks`, `.fragments`, `.diagnostics`
  (687–689); and two fields of the private struct `LabelOut` (494–496). ∆ZK8's
  retrospective explains why it matters here: `SpanWire` and `Diagnostic` exist so a byte
  offset and its UTF-16 positions are computed together (D33), and a crate-visible path
  into them invites an edit that emits a `range` disagreeing with its `span`. Delete the
  twelve qualifiers, then let the `private_interfaces` lint say whether `AmbientBinding`,
  `SpanWire`, `MarkOut` and `FragmentOut` can narrow too — ∆ZK8 recorded that this cannot
  be guessed by hand.
- **`PayloadOut` → `CodePayloadOut`.** Verified, six occurrences: `wire.rs:866`, `871`,
  `878`; `lib.rs:35`, `258`, `289`. `wire.rs` also imports `lemmon_vm::Payload`, a mark's
  payload, so the wire type drops the one word that tells the two apart; the TypeScript
  side already says `CodePayload`. The JSON does not change.
- **A doc comment on the wrong function.** Verified. The paragraph at `wire.rs:819–822`
  describes JSON coming in ("integral numbers become `Int` … Every string arrives
  untrusted") and sits above `json_from_value` (830), which goes out; `value_from_json`
  (889) has no doc. ∆7wK inserted one function between the other and its comment. Move the
  paragraph, and move `value_from_json` up beside `json_from_value`.
- **`packages/core/README.md`** (lines 4–6) says `plan` is still to arrive; `core.ts`
  ships it, and the README never mentions `resolveMarks` or `free()`. List what exists
  today and keep `analyze`/`format` as the ones to come. Verified.
- **Neither side of the boundary names the other.** `packages/core/src/lib/types.ts`
  mirrors `wire.rs`'s shapes by hand and has no file-level doc; nothing under
  `crates/lemmon-wasm/src` names `types.ts`. Give `types.ts` a header that says what it
  mirrors and states the asymmetry: an inbound field the core does not know is refused by
  `deny_unknown_fields`, while an outbound field renamed in Rust is caught only by a spec
  that reads it (`core.ts` casts the parsed replies unchecked). Add one sentence to
  `wire.rs`'s `//!` naming the TypeScript file. Generating `types.ts` with ts-rs or
  typeshare was considered and rejected: of its 325 lines about 208 are prose written for
  a host author, which the Rust comments are not.

## Considered and left out

- `ScanError` and `string_literal` from `scanner.rs` to `token.rs`, ending the
  `scanner` ↔ `token` two-way import. `ScanError`'s own doc says it is public only as
  `TokenKind`'s logos error type, but a reviewer can fairly call it the scanner's error.
  Medium confidence; raise it with Eric if `token.rs` is open anyway.
- `document/adapters.rs` → `headings.rs`. The file's `//!` already explains the split from
  `code.rs`.
- `parser/types.rs`. ∆FQw cleared `parser/template.rs` as one responsibility; symmetry
  with `ast/types.rs` is the only argument.
- `wire/placement.rs`. About 112 lines with a real second responsibility (which text
  places a span, D33), but `wire/mod.rs` would still be 798 production lines.
- `index_u32` defined twice; see `docs/plans/∆xWL-compile-load-time-code.md`.
- `decls::tests::{Table, record, enumeration}` into `testing.rs`. They are already
  `pub(crate)` and used from five modules; moving them changes only where a reader looks.
- Renaming `Data` to `Envelope`. Every doc comment calls it "the envelope", but the name
  echoes the `loadData` method D37 settled; changing the prose may be the cheaper fix.
  Needs one owner's decision across `lemmon-vm`, `lemmon-wasm` and `packages/core`.
- Re-exporting `TypeRef`, `AmbientRef`, `AssetRef` and `ClaimRef` from `lemmon-vm`'s root.
  They are the types of `Unit`'s public fact fields and cannot be named outside the crate.
  ∆Qr2 reports those facts across the boundary; it is one line and belongs there.
- Splitting `crates/lemmon-vm/tests/corpus.rs` into a directory. Its shell model (about
  384 lines) is where ∆4tG and ∆99b will land, so revisit it then.
- The shared host fixtures in `lemmon-vm`'s tests are ∆fBj's first checkpoint; the span
  helper is ∆8jR; the arity wording is ∆RuS.

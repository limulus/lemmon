# ∆1Hp Side channels — plan

The plan for ∆1Hp, written by the session that split ∆RGQ and handed on to the session
that builds this item. Its decisions were settled with Eric; they are not open unless he
reopens them. When ∆1Hp's journal entry is written, its Planning section absorbs this
file and the file is deleted.

## Start here

1. Read `CLAUDE.md` and the ∆1Hp item in `BACKLOG.md`.
2. `npx @limulus/delto@1 claim ∆1Hp`.
3. If the checkout has no `target/` or `artifacts/wasm/`, the first `npm run rust:wasm`
   and `cargo test` are cold builds.
4. Open checkpoint 1 with a plain-language preview for Eric, then build it.

## Already done: checkpoint 0, the split

Landed with this file.

- ∆RGQ was split into four new items and closed with a journal entry of its own, which
  records the split and the designs settled while planning: **∆1Hp** side channels (this
  item), **∆c9S** page templates and `plan()`, **∆Qr2** compile-time facts and serialized
  modules across the boundary, **∆xBO** render-reported deps. `lem-page` is retired: a
  template is a page through its front matter (D41).
- Edges: ∆7Xs needs ∆4tG, ∆1Hp; ∆Vhi needs ∆4tG, ∆xBO; ∆rT5 needs ∆Q6L, ∆1Hp, ∆eag; ∆MlD
  waits on all four; ∆F9l's text says dprint runs after ∆1Hp's splice.
- References re-pointed: the parser's refusal row reads "(∆1Hp)" for `lem-style`,
  `lem-head`, `lem-script` and `lem-emit`, and `lem-page` is no longer a builtin, so it
  reports as an unknown tag (`crates/lemmon-syntax/src/parser/template.rs`); the tests
  spelling those messages; `tests/corpus/syntax/unsupported/{builtins,wrong-close}.cst`;
  the `channels` comments in `crates/lemmon-wasm/src/wire.rs:566` and
  `packages/core/src/lib/types.ts:180`;
  `docs/architecture.md` (93, 184, 208, 241), `docs/decisions/D25-separate-compilation.md`,
  and `docs/artifact-changes.md:325`.

## Why this design

- **Side channels resolve in the host.** The core collects each channel per render and
  dedups by content. The CSS bundle's URL is the shell's to decide (D38: the core does no
  path arithmetic), and a core that resolved `head` and `scripts` but not `styles` would
  treat one name specially, which D30 removed.
- **An emit is a mark, not text.** `<lem-emit name />` writes nothing into the page. It
  records a mark, a channel and a byte offset, and `render` returns the page as UTF-8
  bytes beside the marks and channels, so the host splices at each offset. Text sentinels
  were rejected in three forms: a fixed comment can be typed by hand and becomes an
  undocumented API; a compile-time reservation check misses `raw()` at render time; a
  per-render nonce leaves text every host must search for and adds a render input.
- **An emit printed more than once is allowed.** The host resolves every mark.
- **Moving the page out of the JSON reply pays for itself.** Splicing at known offsets
  costs a median limulus.net page about 2 µs, against 11 µs to scan for text sentinels.
  Separately, a session that measured the boundary found that escaping and parsing the
  HTML inside JSON is more than half of a `render` call: the spike page drops from 108 µs
  to 49 µs without it. Per build that is a few milliseconds, so the design wins on
  simplicity and unforgeability, and the speed only confirms it doesn't lose.
- **Captures are the real cost, and they are bounded.** Every `lem-fn` block body is a
  capture (`crates/lemmon-vm/src/compile/mod.rs:381-392`), so marks must travel on `html`
  values. `html` is opaque at run time: only `EndCapture` (vm.rs:497-499), `Op::Emit`
  (vm.rs:485-488, the only run-time caller of `Value::emit`), `raw()`
  (builtins/values.rs:31-32), `readingTime` (builtins/text.rs:31), `equals`
  (value.rs:271), `ReceiverKind::of_value` (builtins/mod.rs:85), and a loaded document's
  `body` (load/validate.rs:400) touch it. No operator or builtin combines `html` values
  and string interpolation does not exist, so marks shift only at `Emit` and move only at
  `EndCapture`.
- **`Value` stays 24 bytes** (D38, datetime.rs:29): marked markup gets its own variant.
- **`scripts/wasm-artifact.ts:32` pins export names only**, so a new `render` return type
  leaves `npm run wasm:check` alone.
- **Whitespace is nearly free.** Any `BodyKind::Value` body gets D26's dedent
  (whitespace.rs:99-107); D27 is one `matches!` list (`renders_nothing`,
  whitespace.rs:236-254).

## Working agreements

- Two checkpoints, each leaving `npm run verify` green, plus `npm run rust:wasm:test` for
  checkpoint 2. After each: `/code-review medium --fix` on Opus, then stop; Eric reviews
  and commits. Open each with a plain-language preview.
- Docs change in the same pass as the semantics: `docs/language.md`,
  `docs/decisions/README.md` row and record, `docs/artifact-changes.md`,
  `docs/architecture.md`, `tests/corpus/README.md`.
- This plan takes decision D42: D40 is ∆BfR's (code highlighting) and D41 is ∆RGQ's (pages
  from front matter). Before writing it, check `docs/decisions/README.md`: another session
  may have taken D42 since.
- Line numbers below were checked when this plan was written; confirm them before editing.

## Design settled for the side-channels slice

- **Tags and heads** (invariant 4):
  - `<lem-style>…</lem-style>`, `<lem-head>…</lem-head>`: blocks with no head, using the
    `lem-else`/`lem-default` idiom `head(p, |p| p.skip_to_head_end("`lem-style` takes no
    head"))` (template.rs:458, 606). The body is a template, like a capture body.
  - `<lem-script src = expr />`: void. Head: the contextual name `src`, `=`, one expression
    checked as `asset` ("expected `asset`, got `string`" comes from `rules::expected`,
    rules.rs:37-45). `url` is not admitted now; admitting it later is additive. Renders
    nothing in place; collects `<script type="module" src="URL"></script>` into `scripts`,
    the URL escaped by `Value::emit` as `<lem.…>` escapes an `asset`.
  - `<lem-emit styles />` / `scripts` / `head`: void; the head is exactly one of the three
    names. Another name: "unknown channel `foo`: expected `styles`, `scripts`, or `head`"
    with a did-you-mean through `lemmon_syntax::nearest`, reported by the parser at the
    name.
- **Collection.** A channel is per render. A block's rendered body is one fragment;
  fragments keep first-seen order, and one equal to a fragment already in its channel is
  dropped. Collection happens when the block runs, wherever in the call tree. A channel
  block reached while the loader runs a default (`invoke`, vm.rs:97-118) is a load error
  at the block, since no page is collecting.
- **Marks.** `<lem-emit>` records `{channel, at}` at the current output buffer's byte
  length. A capture takes its buffer's marks onto the `html` value it makes; printing
  that value appends its marks shifted by the printing buffer's length. So a capture
  printed twice places twice, one never printed places nothing, and a loader default
  that returns marked `html` carries the mark to whichever page prints it. Consequences:
  - A fragment is text for the host, so a mark reaching a channel block (a `lem-fn` with
    an emit called inside `lem-style`) is a render error at the block: "a `lem-emit`
    cannot be collected into a side channel".
  - `==` on `html` compares marks as well as text: two values that would place
    differently are different.
  - Any future builtin that edits or escapes `html` (an Atom feed escaping a body for
    ∆ePP, an excerpt) must say what happens to marks. D42 records this.
- **Whitespace.** D26: a channel block body is a value body (`BodyKind::Value`). D27:
  `lem-style`, `lem-head`, `lem-script` take their line. `lem-emit` keeps it, so a mark on
  an indented line of its own lands after the indentation and before the newline, where
  the host's `<link>` belongs. Each record gets a Status clause; §Whitespace names the tags.
- **VM shape.**
  - `bytecode.rs`: `pub enum Channel { Styles, Scripts, Head }` (`Copy`, `name()`,
    `from_name`), `pub struct Mark { channel: Channel, at: u32 }`, two ops after
    `EndCapture`: `Place { channel: Channel }` and `Collect { channel: Channel }`. No
    constants are interned for either.
  - `Channels { styles, scripts, head: Vec<String> }` with `push(Channel, String)`;
    `Rendered { html: String, marks: Vec<Mark>, channels: Channels }`;
    `lemmon_vm::render` → `Result<Rendered, RuntimeError>`.
  - `value.rs`: `Value::Placed(Rc<Placed>)`, `struct Placed { text: Rc<str>, marks:
    Box<[Mark]> }`, made only by `EndCapture` when its buffer holds a mark, so plain markup
    stays `Html`. `type_name` "html"; `ReceiverKind::of_value` → `Html`; `readingTime`
    reads `text`; `raw()` passes it through; `equals` compares text and marks, and treats
    `Html(t)` as `Placed{t, []}`; `Value::emit` on `Placed` is `unreachable!` because the
    VM's `Emit` arm handles it first.
  - `vm.rs`: `out: Vec<Buffer>`, `struct Buffer { text: String, marks: Vec<Mark> }`;
    `channels: Option<Channels>` (`Some` in `render`, `None` in `invoke`).
- **Callers of `render`.** Outside the crate: `crates/lemmon-wasm/src/lib.rs:348` and
  `crates/lemmon-vm/tests/corpus.rs:165`. Inside the crate's unit tests, about twenty: the
  helpers `run` (vm.rs:785) and `run_files` (compile/mod.rs:961) map to `.html`; the
  direct `Ok` comparisons in vm.rs (881-1864), unit.rs (1269, 1292, 1323, 1418), and
  compile/items.rs:782 take `.html`. `Err` comparisons are unaffected.
- **Boundary shape.** `Core::render` returns `Vec<u8>` (a `Uint8Array` in JS, one copy out
  of wasm memory): a 4-byte little-endian length, that many bytes of metadata JSON, then
  the page's UTF-8 bytes. Metadata: `{marks: [{channel, at}], channels: {styles, scripts,
  head}, diagnostics}` on success; `{marks: null, channels: null, diagnostics}` with no
  HTML section on a failed render. `at` is a byte offset into the HTML section. The export
  name stays `core_render`, so no wasm-bindgen object needs freeing and the export
  allowlist is unchanged. Caller bugs still throw a `JsError`.
- **JS wrapper.** `Core.render(page)` returns `{html: Uint8Array | null, marks: Mark[],
  channels: Channels | null, diagnostics}`, with `html` a view past the metadata.
  `resolveChannels(result, resolvers): Uint8Array` in `packages/core/src/lib/channels.ts`:
  `resolvers.styles: (fragments: string[]) => string` is required (the shell writes the
  bundle and returns a `<link>`); `scripts` and `head` default to `fragments.join('\n')`.
  Each resolver is called at most once per page, only for a channel some mark names, and
  its result is encoded once with `TextEncoder`. The output is assembled into one
  `Uint8Array` (no Node `Buffer`, so the package stays browser-usable). A page with no
  marks returns `result.html` unchanged. The splice runs before any output transform:
  anything that needs a string (dprint, a playground) decodes once, afterwards.
- **Corpus `.html` format.** The harness prints each mark into the rendered text as
  `⟦lem-emit styles⟧` at its offset, and each collected fragment after the page, before
  the diagnostics trailer, as a `--- styles`, `--- scripts`, or `--- head` section.
  `trailer_start` takes the last `--- diagnostics` line (lemmon-corpus lib.rs:228-242), so
  sections above it pass through untouched. The notation exists only in expectations;
  `tests/corpus/README.md` documents it.
- **D42 "Side channels resolve in the host"**: row in `docs/decisions/README.md`, record
  `docs/decisions/D42-side-channels-resolve-in-the-host.md`.
  - Context: the architecture artifacts' `Channel`/`EmitChannel` sketch, "placeholders
    resolved at page end".
  - Decision: collection and dedup in the core, resolution in the host; an emit is a mark
    carried through captures; the page crosses as bytes outside the JSON; the splice
    precedes output transforms; repeated emits allowed; the two tables above.
  - Alternatives: the core splices `head`/`scripts` and leaves `styles`; the core inlines a
    `<style>`; a fixed text sentinel (hand-typeable, an undocumented API); a sentinel with
    a compile-time reservation check (misses `raw()` at render time, still a format every
    host hard-codes); a sentinel with a per-render nonce (unforgeable, but text every host
    must find, a new render input, and HTML still inside the JSON).
  - Consequences: builtins that edit `html` must decide about marks; the parity harness
    never sees an unresolved emit; `render`'s consumers decode bytes.

## Checkpoint 1 — the language: parser, compiler, VM, corpus, docs

### Parser (`crates/lemmon-syntax`)

1. `src/syntax_kind.rs`: four node kinds in the node section, documented like their
   neighbours: `StyleBlock`, `HeadBlock`, `ScriptVoid`, `EmitVoid`.
2. `src/parser/template.rs` `open_tag` (358-484): four arms before the `_` fallback.
   - `Style`/`Head`: the no-head idiom, then `block(p, m, SyntaxKind::StyleBlock, name,
     span, Body::Plain, stack)`.
   - `Script`: `head(p, script_head)`, `void_close(p)`, `m.complete(p,
     SyntaxKind::ScriptVoid)`. `script_head`, modelled on `let_void_head` (808-817):
     expect an `Ident` whose text is `src` ("expected `src`"), `skip_until(p,
     TokenKind::Eq, …)`, `p.expect(TokenKind::Eq, "`=`")`, `expr::expression(p)`.
   - `Emit`: `head(p, emit_head)`, `void_close`, `EmitVoid`. `emit_head`: one `Ident`
     ("expected a channel name: `styles`, `scripts`, or `head`" when missing) and the name
     check with did-you-mean, an unknown name wrapped in `Error` at its span.
   - Move `Style | Head | Script | Emit` into the `unreachable!` groups of `is_block` and
     `unsupported` (68-114). Refresh `head_has_eq`'s comment (~333): a `lem-script` head
     carries an `=`, but the check is per builtin.
   - Tests: retarget `knows_the_fixed_shape_of_every_opaque_builtin` and
     `names_the_item_each_opaque_builtin_waits_on` to `Import`, the one opaque builtin left;
     `tracks_the_nesting_of_an_unsupported_block_builtin` → `<lem-style><lem-head>a
     </lem-head></lem-style>` parses clean and a wrong close is reported;
     `still_reports_an_unclosed_block_after_an_error_at_its_opener` → `<lem-style x>hello`;
     `parser/mod.rs:622` `enters_the_error_state_only_for_a_diagnostic_about_the_current_token`
     needs a first diagnostic at an already-consumed opener, and no opaque block builtin
     remains. `<lem-style x />` does not work (the head skip leaves the error state at the
     `/>`, so the self-close error is dropped). Use `<lem-case 1 /></lem-case>` at top
     level and confirm both strings when writing it.
3. `src/parser/dump.rs` `is_boundary` (105-129): add the four kinds, or they vanish from
   `.tags`.
4. `src/ast/tags.rs`: views `StyleBlock { body() }`, `HeadBlock { body() }`, `ScriptVoid
   { src() -> Option<Expr> }` (via `head_child`), `EmitVoid { channel() ->
   Option<SyntaxToken> }` (via `head_token(Ident)`); `Item` variants `Style`, `Head`,
   `Script`, `Emit`; re-export in `src/ast/mod.rs:27-31`. Retarget
   `exposes_the_opener_of_unsupported_and_unknown_tags` (1064-1079) to `lem-import`; one
   view test per new node.
5. `tests/corpus.rs` `visit_items`: arms for the four (blocks visit their body; `Script`
   visits its expression), so the typed-view coverage gate holds.
6. Corpus, from the author's seat. `syntax/unsupported/builtins.lem` keeps only
   `lem-import` (so `UnsupportedTag` stays reached); `wrong-close.lem` moves to the new
   group. New `syntax/channels/`: `component-styles.lem` (a `lem-fn` with a `lem-style`
   whose CSS holds `>` and `{}`), `head-and-scripts.lem` (`lem-head` with a `<link>` and a
   `lem-if` inside, `lem-script src = asset("…")`, the three emits, one indented beside
   markup), `errors.lem` (`<lem-emit style />`, `<lem-emit />`, `<lem-script />`,
   `<lem-script href = x />`, `<lem-style class="x">`, `<lem-style />`, a `</lem-head>`
   closing a `lem-style`). Bless, read the diff.

### Compiler and VM (`crates/lemmon-vm`)

7. `src/bytecode.rs`: `Channel`, `Mark`, `Channels`, `Rendered` as in the design; ops
   `Place { channel }` ("Record a mark for `channel` at the current output's length") and
   `Collect { channel }` ("Pop an `html` value and add it to `channel` unless the channel
   already holds equal text; a value carrying a mark is an error"); bump
   `names_every_op_from_one_list` (303) to 45. Export the four types from `src/lib.rs:35`.
8. `src/compile/items.rs` `lower_item` (128-172), arms replacing the reach of the
   `unreachable!` at 166:
   - `Item::Style` / `Item::Head`: `Capture`, `lower_block(body.items(),
     BodyKind::Value)`, `EndCapture`, `Collect { channel }`, spanning `tag.span()`, which
     is `lower_let_block` (390-398) with the store replaced.
   - `Item::Script`: `Capture`; `EmitText` of `<script type="module" src="`;
     `self.check(&required(tag.src()), &Type::Asset)`; `Emit`; `EmitText` of
     `"></script>`; `EndCapture`; `Collect { channel: Scripts }`.
   - `Item::Emit`: `Place { channel }` from the token's text (the parser has validated it).
9. `src/compile/whitespace.rs` `renders_nothing` (236-254): add `Item::Style | Item::Head
   | Item::Script`; `Item::Emit` deliberately absent, with a comment. Test helper `body_of`
   (546-559) gains a `Style` arm; add a dedent test for a style body and a line-rule test
   for each of the four tags.
10. `src/value.rs`: `Value::Placed` and `Placed` as in the design; `type_name`, `equals`
    (text and marks, `Html` read as marks-empty), `emit` (`unreachable!` for `Placed`);
    `builtins/mod.rs:85`, `builtins/values.rs:31-32`, `builtins/text.rs:31` accept it.
    Confirm `size_of::<Value>()` is still 24 bytes before and after.
11. `src/vm.rs`: `Buffer` replaces `String` in `out`; `render` and `invoke` construct
    `vec![Buffer::default()]` and set `channels`.
    - `Op::Place`: push a mark at the top buffer's length.
    - `Op::EndCapture`: pop the buffer; `Value::html(text)` when it has no marks, else
      `Value::Placed`.
    - `Op::Emit`: a `Placed` value appends its marks shifted by the buffer's length, then
      its text; anything else goes to `Value::emit` as today.
    - `Op::Collect`: pop; a `Placed` value is `Fault::Message("a `lem-emit` cannot be
      collected into a side channel")`; `channels: None` is `Fault::Message("…ran while
      loading data: only a page render collects a side channel")`; otherwise `push`.
    - `Op::EmitText`, `Op::Capture`: adapt to `Buffer`.
    - `render` returns `Rendered` from the root buffer and `channels`.
    - Unit tests: a mark inside a capture printed twice gives two shifted marks, never
      printed gives none; nested captures shift through both levels; `raw()` keeps marks;
      `==` distinguishes marks; dedup keeps first-seen order; a marked value collected is
      the fault; `invoke` collecting is the fault; a second render starts with no marks
      or fragments (the twin of `counts_ids_from_zero_again_at_every_render`).
    - Update the ~20 test call sites listed in the design.
12. `tests/corpus.rs`: `rendered` (159-169) returns html, marks, and channels; `shown`
    (171-183) inserts `⟦lem-emit name⟧` at each mark (offsets are UTF-8 byte offsets,
    applied back to front) and writes the channel sections before the trailer; `html` and
    `typed_html` thread them through.
13. Corpus:
    - `render/channels/component-styles.lem`: two components with `lem-style`, one called
      three times (its fragment appears once); `<lem-emit styles />` on its own indented
      line in a layout `lem-fn`. Reaches `Collect` and `Place` for the every-op gate.
    - `render/channels/marks-through-captures.lem`: `lem-emit head` inside a `lem-let`
      capture printed twice (two marks), one never printed (none), and a mark inside a
      layout reached through two nested calls.
    - `render/channels/interpolated-style.lem`: a `lem-style` whose body prints a
      parameter, called with two values (two fragments: dedup is by content).
    - `render/errors/script-src-not-an-asset.lem`: `src = "x"`.
    - `render/errors/emit-collected-into-a-channel.lem`: a `lem-fn` with an emit called
      inside `lem-style`: the render error at the block.
    - `host/pages/scripts.md` (or a `.lem` under `host/`): `<lem-script src =
      asset("scripts/gallery.js") />` resolving through `ASSETS` (corpus.rs:62-69), reached
      twice: one `--- scripts` section with the hashed URL.
    - `document/blocks/style-block-in-prose.md`: a `lem-style` at document top level takes
      its line; the next prose chunk loses one newline (items.rs:24-32).
    - `render/canonical/article-card.lem`: add the `lem-style` block the language doc's
      canonical example carries, and a `<lem-emit styles />`.
    Bless, and read every diff line against what the source asked for.

### Docs (same checkpoint)

14. `docs/language.md`: Tags table rows 136-138 rewritten (block/void shapes, collection
    per render, dedup by content, the three channel names, an emit is a place the host
    fills, printing a capture places its emits each time, the collect-a-mark error);
    §Functions & components gains a paragraph: a component's styles live in a `lem-style`
    inside its `lem-fn`, collected once however often it is called, and the layout places
    the three emits; §Whitespace lists the tags under D26 and D27.
15. `docs/decisions/README.md`: D42 row and chronology sentence; D26 and D27 records get a
    Status clause ("(∆1Hp): channel blocks are value bodies / take their line"); write the
    D42 record.
16. `docs/artifact-changes.md`: "The Lemmon Language" — the side-channel and `lem-emit`
    rows. "Shell & Core" — the VM ops sketch (`Place`/`Collect`, no `EmitChannel`, marks
    carried on `html`), the `Value` list (marked markup), and Build pipeline steps 6-8
    (render returns bytes, marks, channels; the shell splices before post-processing).
17. `tests/corpus/README.md`: the `.html` row, the mark token, the channel sections.

Verification: `npm run rust`; `npm run corpus:bless`, then read the diff; `npm run
rust:wasm` (checkpoint 1 adapts `render_json` to `Rendered.html` and nothing more), then
`npm run verify`.

## Checkpoint 2 — the boundary, the wrapper, the scripts, the journal

18. `crates/lemmon-wasm/src/wire.rs`: replace `RenderResult` with `RenderMeta { marks:
    Option<Vec<MarkOut>>, channels: Option<ChannelsOut>, diagnostics }` and a
    `fn encode_render(meta, html: Option<&str>) -> Vec<u8>` writing the header, metadata,
    and page. `src/lib.rs`: `render` returns `Result<Vec<u8>, JsError>`, `render_json`
    becomes `render_bytes`; the shim doc (150-162) describes the layout. Host-target tests
    gain a `decode` helper splitting header, metadata, and HTML; `html_of` (450-458) and
    the tests reading `result["html"]` (872 and neighbours) use it; new tests: a page with
    a style and an emit returns one fragment and a mark at the right byte offset after
    non-ASCII text; a failed render has null marks and channels and no HTML section.
    `tests/boundary.rs`: decode in `compiles_loads_and_renders_through_the_abi` and
    `renders_blocks_nested_to_the_parser_cap_without_trapping` (it asserts on
    `"html":"a"` today).
19. `packages/core`:
    - `src/lib/boundary.ts`: `render(page: string): Uint8Array`.
    - `src/lib/types.ts`: `ChannelName`, `Mark`, `Channels`, `RenderResult { html:
      Uint8Array | null, marks, channels, diagnostics }`;
      rewrite the note at 180, which names ∆1Hp and ∆xBO.
    - `src/lib/core.ts` `render()`: read the header, decode the metadata, view the HTML.
    - New `src/lib/channels.ts` + `channels.spec.ts`, and `export * from
      './lib/channels.ts'` in `src/index.ts`. Spec every branch: no marks returns the same
      bytes; marks for each channel; a channel marked twice calls its resolver once;
      default joins; a resolver not called for an unmarked channel; offsets after
      multi-byte text; a mark at offset 0 and at the end.
    - `core.spec.ts` `render()`: a success-path test through the staged artifact
      (compile a template with a `lem-style` and `<lem-emit styles />`, render, check the
      mark and fragment, resolve, decode, compare); the failure test checks `html`,
      `marks`, `channels` are null.
    - `load.spec.ts:35` decodes.
20. `scripts/render-the-rebound.ts` (271-285) and `scripts/bench-wasm.ts` (211, 256, 285):
    decode `result.html` with one `TextDecoder`. Both are linted and typechecked, not
    covered.
21. `docs/architecture.md`: §WASM API class sketch (line 32) `render(page): {html (bytes),
    marks, channels, diagnostics}` with `deps` marked ∆xBO; the boundary rules (bulk data as
    JSON, except a render's page, which crosses as bytes beside its metadata, with the
    measurement); the bullet at 93; §VM ops sketch and value list; §Build pipeline steps
    6-8; the sentence at 241. `docs/artifact-changes.md` "Shell & Core": the WASM API
    entry.
22. Journal, once Eric accepts the work: `npx @limulus/delto@1 complete ∆1Hp
    docs/journal/∆1Hp-side-channels.md` while the item is still in `BACKLOG.md`; fill
    Planning from this file (link [[∆RGQ]] for the split; the three sentinel designs and
    why marks won, the boundary measurements), Refinement, Retrospective; then
    remove ∆1Hp from `BACKLOG.md` and delete this file.

Verification: `npm run rust:wasm`, `npm run verify` (100% coverage on the helper and the
wrapper), `npm run rust:wasm:test`, `npm run wasm:check`. Report which ran.

## Out of scope, noted for the follow-ons

- ∆7Xs decides the bundle's file name and hashing; the `styles` resolver is its seam, and
  it must splice before ∆F9l's `transformOutput`.
- ∆ePP's feed escapes rendered bodies: it must decide what an escaped body does with
  marks (D42 consequence).
- `lem-import` stays opaque under ∆PgF; `lem-page` is retired (D41).
- ∆5MM's grammar gets four boundary kinds from `.tags`.

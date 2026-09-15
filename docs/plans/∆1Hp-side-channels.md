# ∆1Hp Side channels and marks — plan

The plan for ∆1Hp, written by the session that split ∆RGQ and handed on to the session
that builds this item, and revised on 2026-09-13 by the session that settled D42, which
generalized the emit into the mark this plan now builds. Its decisions were settled with
Eric; they are not open unless he reopens them. When ∆1Hp's journal entry is written, its
Planning section absorbs this file and the file is deleted.

## Start here

1. Read `CLAUDE.md`, the ∆1Hp item in `BACKLOG.md`, and
   `docs/decisions/D42-marks-resolve-in-the-host.md`.
2. `npx @limulus/delto@1 claim ∆1Hp`.
3. If the checkout has no `target/` or `artifacts/wasm/`, the first `npm run rust:wasm`
   and `cargo test` are cold builds.
4. Open checkpoint 1 with a plain-language preview for Eric, then build it.

## Already done

**Checkpoint 0, the split** (∆RGQ, 2026-09-13): ∆RGQ was split into ∆1Hp, ∆c9S, ∆Qr2,
and ∆xBO and closed with a journal entry; `lem-page` was retired (D41); the parser's refusal
rows read "(∆1Hp)" for `lem-style`, `lem-head`, and `lem-script`.

**The revision** (D42, 2026-09-13): the decision record is written, the rows of D06, D08,
D20, D30, and D40 carry it, and `docs/architecture.md` and `docs/language.md` describe the
target with "not yet" placeholders naming this item and ∆7wK. `lem-emit` is retired
unbuilt: `Builtin::Emit` is gone from `crates/lemmon-syntax/src/parser/template.rs`, so
`<lem-emit styles />` reports as an unknown tag, and
`tests/corpus/syntax/unsupported/builtins.lem` no longer lists it. `BACKLOG.md` names ∆7wK
(code marks) after this item.

## What the revision changed, and why

The ∆RGQ plan had three fixed channels, an emit that recorded a point, and a `styles`
resolver the wrapper required. D42 keeps every mechanism it settled — marks carried on
`html` values through captures, the page as bytes beside its metadata, the splice before
any transform — and generalizes the shape so that ∆7wK can make a code fence a mark
without reopening anything this item blesses:

- **A mark is a range with default content, not a point.** An unclaimed name, a failed
  handler, or a host with no plugins leaves the page correct.
- **A mark has a name, not a channel.** The name is an interned constant, and the three
  the shell handles — `styles`, `scripts`, `head` — are claims on the host declaration
  like any other, checked at compile with a did-you-mean.
- **`mark()` places; `lem-emit` is retired.** An ambient constructor returning `html`
  replaces a tag, so placement needs no parser work and the same call carries a payload
  later (∆7wK).
- **The wrapper dispatches by name to producers and decorators**, one shape for the CSS
  bundler and for a highlighter.
- **Names that collided are renamed**: the plan's `Op::Place` and `Value::Placed` met the
  loader's `Place` position (`load/place.rs`) and `Fault::Placed`; this plan uses
  `Op::Collect`, `Value::Marked`, and `Mark`.

Everything else below is the ∆RGQ plan's text where it still holds.

## Why this design

- **Side channels resolve in the host.** The core collects each name's fragments per
  render and dedups by content. The CSS bundle's URL is the shell's to decide (D38: the
  core does no path arithmetic), and a core that resolved `head` and `scripts` but not
  `styles` would treat one name specially, which D30 removed.
- **A mark is a value, not text.** `mark("styles")` writes nothing into the page but its
  default, which is nothing here. It makes an `html` value carrying a named range, and
  `render` returns the page as UTF-8 bytes beside the marks, so the host splices at
  known ranges. Text sentinels were rejected in three forms: a fixed comment can be
  typed by hand and becomes an undocumented API; a compile-time reservation check misses
  `raw()` at render time; a per-render nonce leaves text every host must search for and
  adds a render input.
- **A mark printed more than once is allowed.** The host resolves every mark.
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
  (value.rs:271), `ReceiverKind::of_value` (builtins/mod.rs:85), a loaded document's
  `body` (load/validate.rs:400), and — found in the D42 review — `Value::emit`'s own
  `Html` arm (value.rs:246), `Value::type_name` (value.rs:212), the loader's type check
  (load/validate.rs:65, 85), and the compile-time placeholder (compile/annotations.rs:193)
  touch it. No operator or builtin combines `html` values and string interpolation does
  not exist, so marks shift only at `Emit` and move only at `EndCapture`; container
  builtins duplicate, drop, or reorder values without inspecting them, which is benign.
- **`Value` stays 24 bytes** (D38; datetime.rs:26-30 is prose — there is no `size_of`
  assertion, so add one): marked markup gets its own variant behind an `Rc`.
- **`scripts/wasm-artifact.ts:32` pins export names only**, so a new `render` return type
  leaves `npm run wasm:check` alone.
- **Whitespace is nearly free.** Any `BodyKind::Value` body gets D26's dedent
  (whitespace.rs:99-107); D27 is one `matches!` list (`renders_nothing`,
  whitespace.rs:236-254).

## Working agreements

- Two checkpoints, each leaving `npm run verify` green, plus `npm run rust:wasm:test` for
  checkpoint 2. After each: `/code-review medium --fix` on Opus, then stop; Eric reviews
  and commits. Open each with a plain-language preview.
- Docs change in the same pass as the semantics: `docs/language.md` § Marks and the
  placeholders naming ∆1Hp, `docs/decisions/D42-marks-resolve-in-the-host.md`'s Status
  line for anything the build changes, `docs/artifact-changes.md`,
  `docs/architecture.md`, `tests/corpus/README.md`.
- Line numbers below were checked when the ∆RGQ plan was written and rechecked on
  2026-09-13; confirm them before editing.

## Design settled for this item

- **Tags and heads** (invariant 4):
  - `<lem-style>…</lem-style>`, `<lem-head>…</lem-head>`: blocks with no head, using the
    `lem-else`/`lem-default` idiom `head(p, |p| p.skip_to_head_end("`lem-style` takes no
    head"))` (template.rs:458, 606). The body is a template, like a capture body.
  - `<lem-script src = expr />`: void. Head: the contextual name `src`, `=`, one expression
    checked as `asset` ("expected `asset`, got `string`" comes from `rules::expected`,
    rules.rs:37-45). `url` is not admitted now; admitting it later is additive. Renders
    nothing in place; collects the URL — escaped as `Value::emit` escapes an `asset` —
    as a fragment under `scripts`. The `<script type="module" src="…"></script>` markup
    is the producer's to write, not the core's (D42).
  - No placement tag. `mark()` is a builtin (below).
- **`mark(name: string, default: html = …)`**, an ambient function (D34) in
  `builtins/values.rs` beside `raw()`, with a rule beside the builtin table
  (`builtins/mod.rs`, `compile/rules.rs`): `name` must be a string literal ("`mark` takes
  a literal name", as `asset()` demands one, D38); the literal is checked against the
  names the host declaration claims — "no handler claims `stylez`: did you mean
  `styles`?" through `lemmon_syntax::nearest`, an error at the literal — and the claims
  read are a compile-time fact (below). `default` is named-only and optional; ∆7wK adds
  the positional `payload` between them. The call yields `Value::Marked` over the
  default's text (empty with none) with one mark spanning it. A default carrying marks is
  a render error at the call: "a mark cannot stand inside another mark's default".
- **The wrappers are how an author writes it.** `mark()` is the primitive; a host that
  claims a name ships an expression-bodied `lem-fn` wrapping it — the shell's bundler
  `<lem-fn styles(): html = mark("styles") />`, and likewise `scripts` and `head` — so a
  layout reads `<lem.styles()>` and a site author never types the primitive, as a
  plugin's component wraps its own mark (∆7wK's `code`, a diagram plugin's `diagram`).
  The core knows nothing of them: they are ordinary project or plugin functions (D23),
  so all this item owes them is `mark()` callable from an expression body, which is why
  the corpus writes one. ∆PgF ships the shell's three with the default plugin.
- **Claims on the declaration.** `HostDecl.marks?: {name, keys?, payload?}[]` (wire.rs,
  `HostInput`; `Host` in host.rs gains `marks: Vec<Claim>` and `claim_digest(name)`); this
  item reads names only, ∆7wK the keys and payload types. `Unit` gains `claim_refs:
  Vec<ClaimRef { name: u32, digest: Option<u64>, span }>` modelled on `GrammarRef`
  (unit.rs:226-238) and checked at link as `grammar_ref` is (unit.rs:383-407): a host
  that stops or starts claiming a name a unit read leaves it stale, named for
  recompiling. The corpus host (`crates/lemmon-vm/tests/corpus.rs:42-70`) claims
  `styles`, `scripts`, `head`, and one test name with a payload type for ∆7wK to use.
- **Collection.** A fragment is per render. A block's rendered body is one fragment;
  fragments keep first-seen order under their name, and one equal to a fragment already
  there is dropped. Collection happens when the block runs, wherever in the call tree. A
  collect block reached while the loader runs a default (`invoke`, vm.rs:97-118) is a
  load error at the block, since no page is collecting.
- **Marks.** A `Marked` value carries `{text, marks: [Mark { name, start, end, span }]}`;
  printing it appends its text and its marks shifted by the printing buffer's length. A
  capture takes its buffer's marks onto the `html` value it makes. So a capture printed
  twice places twice, one never printed places nothing, and a loader default that
  returns marked `html` carries the mark to whichever page prints it. Consequences:
  - A fragment is text for the host, so a mark reaching a collect block (a `lem-fn` that
    calls `mark()` inside `lem-style`) is a render error at the block: "a mark cannot be
    collected into a fragment".
  - `==` on `html` compares marks as well as text: two values that would place
    differently are different.
  - `readingTime` reads the text, default content included.
  - Any future builtin that edits or escapes `html` must say what happens to marks; D42
    settles the one case that mattered — a feed's body is a string the shell supplies.
- **Whitespace.** D26: a collect block body is a value body (`BodyKind::Value`). D27:
  `lem-style`, `lem-head`, `lem-script` take their line. `<lem.mark(…)>` is an access tag
  and keeps it, so a mark on an indented line of its own lands after the indentation and
  before the newline, where the host's `<link>` belongs. Each record gets a Status
  clause; §Whitespace names the tags.
- **VM shape.**
  - `bytecode.rs`: `pub struct Mark { name: u32, start: u32, end: u32, span: Span }` (the
    name a constant index); one op after `EndCapture`: `Collect { name: u32 }`. No
    `Place`: a mark is made by the builtin.
  - `Fragments(BTreeMap<u32, Vec<String>>)` keyed by name index with `push(name,
    String)`; `Rendered { html: String, marks: Vec<Mark>, fragments: Fragments }`;
    `lemmon_vm::render` → `Result<Rendered, RuntimeError>`.
  - `value.rs`: `Value::Marked(Rc<Marked>)`, `struct Marked { text: Rc<str>, marks:
    Box<[Mark]> }`, made by `mark()` and by `EndCapture` when its buffer holds a mark, so
    plain markup stays `Html`. `type_name` "html"; `ReceiverKind::of_value` → `Html`;
    `readingTime` reads `text`; `raw()` passes it through; `equals` compares text and
    marks, and treats `Html(t)` as `Marked{t, []}`; `Value::emit` on `Marked` is
    `unreachable!` because the VM's `Emit` arm handles it first; the loader's type check
    (validate.rs:65) admits it as `html`. Add `assert_eq!(size_of::<Value>(), 24)`.
  - `vm.rs`: `out: Vec<Buffer>`, `struct Buffer { text: String, marks: Vec<Mark> }`;
    `fragments: Option<Fragments>` (`Some` in `render`, `None` in `invoke`).
- **Callers of `render`.** Outside the crate: `crates/lemmon-wasm/src/lib.rs:348` and
  `crates/lemmon-vm/tests/corpus.rs:165`. Inside the crate's unit tests, about twenty: the
  helpers `run` (vm.rs:785) and `run_files` (compile/mod.rs:961) map to `.html`; the
  direct `Ok` comparisons in vm.rs (881-1864), unit.rs (1269, 1292, 1323, 1418), and
  compile/items.rs:782 take `.html`. `Err` comparisons are unaffected.
- **Boundary shape.** `Core::render` returns `Vec<u8>` (a `Uint8Array` in JS, one copy out
  of wasm memory): a 4-byte little-endian length, that many bytes of metadata JSON, then
  the page's UTF-8 bytes. Metadata: `{marks: [{name, start, end, span: {path, start, end,
  range}}], fragments: {[name]: string[]}, diagnostics}` on success; `{marks: null,
  fragments: null, diagnostics}` with no HTML section on a failed render. `start`/`end`
  are byte offsets into the HTML section; `span` is where the mark was made, so a
  handler's diagnostic can point at source (invariant 8). ∆7wK adds `payload` per mark
  and a payload table reported at compile. The export name stays `core_render`, so no
  wasm-bindgen object needs freeing and the export allowlist is unchanged. Caller bugs
  still throw a `JsError`.
- **JS wrapper.** `Core.render(page)` returns `{html: Uint8Array | null, marks: Mark[],
  fragments: Fragments | null, diagnostics}`, with `html` a view past the metadata.
  `resolveMarks(result, handlers): Resolved` in `packages/core/src/lib/marks.ts`, where
  `handlers` maps a name to `{produce?, decorate?}`: `produce({marks, fragments, page})`
  is called at most once per page per name, only for a name some mark carries, with every
  mark of that name and the fragments under it, and returns one replacement per mark
  (`string | Uint8Array`); each `decorate(mark, content)` runs after it, in the order the
  shell passes the handlers, over the previous content; a name with no handler leaves its
  default content and is reported in `Resolved.unclaimed`. Bytes are assembled into one
  `Uint8Array` (no Node `Buffer`, so the package stays browser-usable); a string result
  is encoded once with `TextEncoder`. A page with no marks returns `result.html`
  unchanged. Two producers for one name is a thrown caller bug here; the plugin-load
  error naming both plugins is ∆PgF's. The splice runs before any output transform:
  anything that needs a string (dprint, a playground) decodes once, afterwards.
- **Corpus `.html` format.** The harness prints each mark into the rendered text as
  `⟦mark styles⟧` at an empty range and `⟦mark toc⟧…⟦/mark⟧` around a default, and
  each collected fragment after the page, before the diagnostics trailer, as a `--- styles`,
  `--- scripts`, or `--- head` section (one per name that has fragments). `trailer_start`
  takes the last `--- diagnostics` line (lemmon-corpus lib.rs:228-242), so sections above
  it pass through untouched. The notation exists only in expectations;
  `tests/corpus/README.md` documents it.
- **D42 is written.** The build adds a Status clause to
  `docs/decisions/D42-marks-resolve-in-the-host.md` only for what it changes, and sweeps
  the placeholders naming ∆1Hp in `docs/language.md` § Marks and `docs/architecture.md`.

## Checkpoint 1 — the language: parser, compiler, VM, corpus, docs

### Parser (`crates/lemmon-syntax`)

1. `src/syntax_kind.rs`: three node kinds in the node section, documented like their
   neighbours: `StyleBlock`, `HeadBlock`, `ScriptVoid`.
2. `src/parser/template.rs` `open_tag` (358-484): three arms before the `_` fallback.
   - `Style`/`Head`: the no-head idiom, then `block(p, m, SyntaxKind::StyleBlock, name,
     span, Body::Plain, stack)`.
   - `Script`: `head(p, script_head)`, `void_close(p)`, `m.complete(p,
     SyntaxKind::ScriptVoid)`. `script_head`, modelled on `let_void_head` (808-817):
     expect an `Ident` whose text is `src` ("expected `src`"), `skip_until(p,
     TokenKind::Eq, …)`, `p.expect(TokenKind::Eq, "`=`")`, `expr::expression(p)`.
   - Move `Style | Head | Script` into the `unreachable!` groups of `is_block` and
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
3. `src/parser/dump.rs` `is_boundary` (105-129): add the three kinds, or they vanish from
   `.tags`.
4. `src/ast/tags.rs`: views `StyleBlock { body() }`, `HeadBlock { body() }`, `ScriptVoid
   { src() -> Option<Expr> }` (via `head_child`); `Item` variants `Style`, `Head`,
   `Script`; re-export in `src/ast/mod.rs:27-31`. Retarget
   `exposes_the_opener_of_unsupported_and_unknown_tags` (1064-1079) to `lem-import`; one
   view test per new node.
5. `tests/corpus.rs` `visit_items`: arms for the three (blocks visit their body; `Script`
   visits its expression), so the typed-view coverage gate holds.
6. Corpus, from the author's seat. `syntax/unsupported/builtins.lem` keeps only
   `lem-import` (so `UnsupportedTag` stays reached); `wrong-close.lem` moves to the new
   group. New `syntax/channels/`: `component-styles.lem` (a `lem-fn` with a `lem-style`
   whose CSS holds `>` and `{}`), `head-and-scripts.lem` (`lem-head` with a `<link>` and a
   `lem-if` inside, `lem-script src = asset("…")`, `<lem.mark("styles")>` indented beside
   markup), `errors.lem` (`<lem-script />`, `<lem-script href = x />`, `<lem-style
   class="x">`, `<lem-style />`, a `</lem-head>` closing a `lem-style`, and `<lem-emit
   styles />` as an unknown tag). Bless, read the diff.

### Compiler and VM (`crates/lemmon-vm`)

7. `src/bytecode.rs`: `Mark`, `Fragments`, `Rendered` as in the design; the op
   `Collect { name }` ("Pop an `html` value and add it to the fragments under `name`
   unless they already hold equal text; a value carrying a mark is an error"); bump
   `names_every_op_from_one_list` (303) to 44. Export the types from `src/lib.rs:35`.
8. `src/compile/items.rs` `lower_item` (128-172), arms replacing the reach of the
   `unreachable!` at 166:
   - `Item::Style` / `Item::Head`: `Capture`, `lower_block(body.items(),
     BodyKind::Value)`, `EndCapture`, `Collect { name }`, spanning `tag.span()`, which
     is `lower_let_block` (390-398) with the store replaced.
   - `Item::Script`: `Capture`; `self.check(&required(tag.src()), &Type::Asset)`; `Emit`;
     `EndCapture`; `Collect { name: scripts }`.
9. `mark()`: the builtin rule (literal name; the claims check with did-you-mean; named
   `default: html`), `Host::claims`, `ClaimRef` on the unit and the link check, the
   corpus host's claims.
10. `src/compile/whitespace.rs` `renders_nothing` (236-254): add `Item::Style | Item::Head
    | Item::Script`. Test helper `body_of` (546-559) gains a `Style` arm; add a dedent test
    for a style body and a line-rule test for each of the three tags and for `mark()`.
11. `src/value.rs`: `Value::Marked` and `Marked` as in the design; `type_name`, `equals`
    (text and marks, `Html` read as marks-empty), `emit` (`unreachable!` for `Marked`);
    `builtins/mod.rs:85`, `builtins/values.rs:31-32`, `builtins/text.rs:31`,
    `load/validate.rs:65` accept it. Add the 24-byte assertion.
12. `src/vm.rs`: `Buffer` replaces `String` in `out`; `render` and `invoke` construct
    `vec![Buffer::default()]` and set `fragments`.
    - `mark()`'s `CallBuiltin`: build `Marked` with one mark `{name, 0, text.len(),
      span}` from the pc's span.
    - `Op::EndCapture`: pop the buffer; `Value::html(text)` when it has no marks, else
      `Value::Marked`.
    - `Op::Emit`: a `Marked` value appends its marks shifted by the buffer's length, then
      its text; anything else goes to `Value::emit` as today.
    - `Op::Collect`: pop; a `Marked` value is `Fault::Message("a mark cannot be collected
      into a fragment")`; `fragments: None` is `Fault::Message("…ran while loading data:
      only a page render collects a fragment")`; otherwise `push`.
    - `Op::EmitText`, `Op::Capture`: adapt to `Buffer`.
    - `render` returns `Rendered` from the root buffer and `fragments`.
    - Unit tests: a mark inside a capture printed twice gives two shifted marks, never
      printed gives none; nested captures shift through both levels; `raw()` keeps marks;
      `==` distinguishes marks; a default's range covers its text; a marked default is
      the fault; dedup keeps first-seen order; a marked value collected is the fault;
      `invoke` collecting is the fault; a second render starts with no marks or
      fragments (the twin of `counts_ids_from_zero_again_at_every_render`).
    - Update the ~20 test call sites listed in the design.
13. `tests/corpus.rs`: `rendered` (159-169) returns html, marks, and fragments; `shown`
    (171-183) inserts the mark notation (offsets are UTF-8 byte offsets, applied back to
    front) and writes the fragment sections before the trailer; `html` and `typed_html`
    thread them through.
14. Corpus:
    - `render/channels/component-styles.lem`: two components with `lem-style`, one called
      three times (its fragment appears once); a wrapper `<lem-fn styles(): html =
      mark("styles") />` called as `<lem.styles()>` on its own indented line in a layout
      `lem-fn`, which pins the authoring form. Reaches `Collect` for the every-op gate.
    - `render/channels/marks-through-captures.lem`: `mark("head")` inside a `lem-let`
      capture printed twice (two marks), one never printed (none), and a mark inside a
      layout reached through two nested calls; `mark("toc", default: x)` with its range.
    - `render/channels/interpolated-style.lem`: a `lem-style` whose body prints a
      parameter, called with two values (two fragments: dedup is by content).
    - `render/errors/script-src-not-an-asset.lem`: `src = "x"`.
    - `render/errors/mark-collected-into-a-fragment.lem`: a `lem-fn` calling `mark()`
      inside `lem-style`: the render error at the block.
    - `render/errors/unclaimed-mark.lem`: `mark("stylez")`: the compile error with the
      did-you-mean; and `mark(name)` with a computed name.
    - `host/pages/scripts.md` (or a `.lem` under `host/`): `<lem-script src =
      asset("scripts/gallery.js") />` resolving through `ASSETS` (corpus.rs:62-69), reached
      twice: one `--- scripts` section with the hashed URL.
    - `document/blocks/style-block-in-prose.md`: a `lem-style` at document top level takes
      its line; the next prose chunk loses one newline (items.rs:24-32).
    - `render/canonical/article-card.lem`: add the `lem-style` block the language doc's
      canonical example carries.
    Bless, and read every diff line against what the source asked for.

### Docs (same checkpoint)

15. `docs/language.md`: § Marks loses its "not yet (∆1Hp)" placeholder, and its wrapper
    sentence is confirmed against the corpus case; the Tags table rows for the collect
    tags are confirmed against what was built; §Whitespace lists the
    tags under D26 and D27.
16. `docs/decisions/README.md`: D26 and D27 records get a Status clause ("(∆1Hp): collect
    blocks are value bodies / take their line"); D42's Status line gains a clause only if
    the build changed something it states.
17. `docs/artifact-changes.md`: confirm the D42 entries against what was built.
18. `tests/corpus/README.md`: the `.html` row, the mark notation, the fragment sections.

Verification: `npm run rust`; `npm run corpus:bless`, then read the diff; `npm run
rust:wasm` (checkpoint 1 adapts `render_json` to `Rendered.html` and nothing more), then
`npm run verify`.

## Checkpoint 2 — the boundary, the wrapper, the scripts, the journal

19. `crates/lemmon-wasm/src/wire.rs`: `HostInput.marks` and its `From` arm; replace
    `RenderResult` with `RenderMeta { marks: Option<Vec<MarkOut>>, fragments:
    Option<BTreeMap<String, Vec<String>>>, diagnostics }` and a `fn encode_render(meta,
    html: Option<&str>) -> Vec<u8>` writing the header, metadata, and page. `src/lib.rs`:
    `render` returns `Result<Vec<u8>, JsError>`, `render_json` becomes `render_bytes`; the
    shim doc (150-162) describes the layout. Host-target tests gain a `decode` helper
    splitting header, metadata, and HTML; `html_of` (450-458) and the tests reading
    `result["html"]` (872 and neighbours) use it; new tests: a page with a style and a
    mark returns one fragment and a mark at the right byte range after non-ASCII text,
    with its span; a failed render has null marks and fragments and no HTML section; a
    declaration that stops claiming a name stales the unit that read it.
    `tests/boundary.rs`: decode in `compiles_loads_and_renders_through_the_abi` and
    `renders_blocks_nested_to_the_parser_cap_without_trapping` (it asserts on
    `"html":"a"` today).
20. `packages/core`:
    - `src/lib/boundary.ts`: `render(page: string): Uint8Array`.
    - `src/lib/types.ts`: `Claim` on `HostDecl`, `Mark`, `Fragments`, `RenderResult {
      html: Uint8Array | null, marks, fragments, diagnostics }`; rewrite the note at 180,
      which names ∆1Hp and ∆xBO.
    - `src/lib/core.ts` `render()`: read the header, decode the metadata, view the HTML.
    - New `src/lib/marks.ts` + `marks.spec.ts`, and `export * from './lib/marks.ts'` in
      `src/index.ts`. Spec every branch: no marks returns the same bytes; a producer per
      name; a name marked twice calls its producer once; decorators in order over the
      producer's output and over default content when there is no producer; an unclaimed
      name left as its default and reported; offsets after multi-byte text; a mark at
      offset 0 and at the end; a range with content replaced; two producers for one name
      thrown.
    - `core.spec.ts` `render()`: a success-path test through the staged artifact
      (compile a template with a `lem-style` and `<lem.mark("styles")>`, render, check the
      mark and fragment, resolve, decode, compare); the failure test checks `html`,
      `marks`, `fragments` are null.
    - `load.spec.ts:35` decodes.
21. `scripts/render-the-rebound.ts` (271-285) and `scripts/bench-wasm.ts` (211, 256, 285):
    decode `result.html` with one `TextDecoder`. Both are linted and typechecked, not
    covered.
22. `docs/architecture.md`: the placeholders naming ∆1Hp; §WASM API's boundary rules
    (bulk data as JSON, except a render's page, which crosses as bytes beside its
    metadata, with the measurement). `docs/artifact-changes.md` "Shell & Core": the WASM
    API entry confirmed.
23. Journal, once Eric accepts the work: `npx @limulus/delto@1 complete ∆1Hp
    docs/journal/∆1Hp-side-channels.md` while the item is still in `BACKLOG.md`; fill
    Planning from this file (link [[∆RGQ]] for the split and the sentinel history; D42
    for the revision, its four angles, and the boundary measurements), Refinement,
    Retrospective; then remove ∆1Hp from `BACKLOG.md` and delete this file.

Verification: `npm run rust:wasm`, `npm run verify` (100% coverage on the helper and the
wrapper), `npm run rust:wasm:test`, `npm run wasm:check`. Report which ran.

## Out of scope, noted for the follow-ons

- ∆7wK makes a code block a `code` mark with a payload by id, gives `mark()` its
  positional payload typed by the claim's record, gives claims their keys, and removes
  syntect and the grammars; it builds on the `Mark`, `Marked`, wire, and wrapper shapes
  above without changing them.
- ∆7Xs decides the bundle's file name and hashing as the `styles` producer, and it must
  splice before ∆F9l's `transformOutput`; ∆PgF turns `handlers` into plugin
  registrations, the load-time collision error, and the default plugin with the three
  wrapper `lem-fn`s it ships.
- ∆ePP's feed reads a page's finished output as a string from the second render phase
  (D42); nothing here escapes `html`.
- `lem-import` stays opaque under ∆PgF; `lem-page` and `lem-emit` are retired.
- ∆5MM's grammar gets three boundary kinds from `.tags`.

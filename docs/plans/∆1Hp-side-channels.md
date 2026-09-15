# ∆1Hp Side channels and marks — plan

The plan for ∆1Hp. The session that split ∆RGQ wrote it for whichever session built the
item. The session that settled D42 revised it on 2026-09-13, generalizing the emit into a
mark. A verification pass on 2026-09-14 checked every claim against the code, corrected
what had drifted or was wrong, and split the build into three checkpoints. Its decisions
were settled with Eric and are not open unless he reopens them. When ∆1Hp's journal entry
is written, its Planning section absorbs this file and the file is deleted.

## Start here

1. Read `CLAUDE.md`, the ∆1Hp item in `BACKLOG.md`, and
   `docs/decisions/D42-marks-resolve-in-the-host.md`.
2. `npx @limulus/delto@1 claim ∆1Hp` (the claim ledger is per checkout).
3. If the checkout has no `target/` or `artifacts/wasm/`, the first `npm run rust:wasm`
   and `cargo test` are cold builds.
4. Open the next unbuilt checkpoint with a plain-language preview for Eric, then build it.

## Already done

**Checkpoint 0, the split** (∆RGQ, 2026-09-13): ∆RGQ was split into ∆1Hp, ∆c9S, ∆Qr2 and
∆xBO and closed with a journal entry. `lem-page` was retired (D41). The parser's refusal
rows read "(∆1Hp)" for `lem-style`, `lem-head` and `lem-script`.

**The revision** (D42, 2026-09-13): the decision record is written. The rows of D06, D08,
D20, D30 and D40 carry it, and `docs/architecture.md` and `docs/language.md` describe the
target with placeholders naming this item and ∆7wK. `lem-emit` is retired unbuilt:
`Builtin::Emit` is gone, `<lem-emit styles />` reports as an unknown tag, and
`tests/corpus/syntax/unsupported/builtins.lem` no longer lists it.

**The verification pass** (2026-09-14): four read-only reviews checked the parser, the
compiler and VM, and the boundary, plus an adversarial review against D42, the invariants,
and limulus.net's source. The corrections are folded in below. The ones that change the
design:

- **An unclaimed mark name is a warning, not an error.** D42 says so twice ("what makes an
  unclaimed mark a warning rather than a hole"; "`lem check` and the editor keep the
  warning at the word"), and `docs/language.md` § Code blocks makes an unclaimed language
  a warning. The revised plan had made it an error. That broke its own staleness fact:
  `Compiled::new` drops the unit when any diagnostic is an error
  (`compile/diagnostics.rs:176-186`), so no unit could record an unclaimed name, and "a
  host that starts claiming leaves it stale" could never fire. An error would also have
  failed every `mark()` compiled by a host that declares no claims, such as the
  playground, the LSP, or `@limulus/lemmon-core` before checkpoint 2. Following ∆BfR's
  grammars (`document/highlight/mod.rs:53-76, 131-157`): a host that declares no `marks`
  hears nothing; a host that declares some hears a warning for a name none claims.
- **Names travel as strings, not as constant indexes.** Each unit interns its own strings
  (`compile/mod.rs:881-888`), and link concatenates the pools with an offset
  (`unit.rs:553-569`). So `"styles"` interned by two component files has two indexes, and
  a `Fragments` map keyed by index would keep equal CSS from two files apart. The op
  operand stays an index, rebased at link; `Mark.name` and the fragment map key are
  `Rc<str>`. No single-file corpus case can see this, so it gets a two-file test.
- **`mark()` lowers to its own op, `Op::Mark { name }`.** It is intercepted by name in the
  compiler, as `asset()` is (`compile/calls.rs:219-224`). A builtin has no access to its
  call's file or span: `Machine` offers only `invoke` and `next_id`
  (`builtins/mod.rs:106-121`), and a position is read only by `Vm::fail` from the running
  function (`vm.rs:175-184`). An op gets both the way every fault does. Builtins also
  refuse named arguments before `call_builtin` is reached (`calls.rs:79-86`, `370-382`)
  and have a fixed arity. The intercept handles `default:` itself, so no general
  named-parameter machinery is built.
- **Marks and fragments carry a file as well as a span.** A mark made in a layout's
  `lem-fn` points into the layout, not the page rendered. Placing a span at the boundary
  needs its path (`wire.rs:576-584`). A fragment carries the file and span of the first
  block that collected it, for the same reason a mark carries one: a producer's
  diagnostic, such as a CSS parse error from ∆7Xs's bundler, points at source.
- **`==` on marked `html` compares text, and each mark's name and range**, not its file or
  span. Two values that would place the same are equal, wherever `mark()` ran.
- **No `unreachable!` in `Value::emit`.** A panic in the `.wasm` poisons the resident
  `Core` (invariant 8). `emit` writes into the output buffer, marks included, so there is
  one print path.
- **Claims on the wire are names only.** `HostInput` has `deny_unknown_fields`
  (`wire.rs:134-151`), and a field nothing reads is a comment. ∆7wK adds `mode`, `keys`
  and `payload` when it reads them.
- **The wrapper reports names with fragments but no mark** (`Resolved.unplaced`). Without
  that, a page that collects `lem-style` and never places `mark("styles")` loses its CSS
  with no report.
- **Three checkpoints instead of two.** The collect tags stand green without marks:
  `lem-style` reaches `Collect` for the every-op gate, and the 22 `render` call-site edits
  land once. So checkpoint 1 is split into 1a (collect tags) and 1b (marks).

## Why this design

- **Side channels resolve in the host.** The core collects each name's fragments per
  render and dedups them by content. The CSS bundle's URL is the shell's to decide (D38:
  the core does no path arithmetic). A core that resolved `head` and `scripts` but not
  `styles` would treat one name specially, which D30 removed.
- **A mark is a value, not text.** `mark("styles")` writes nothing into the page except
  its default, and here the default is empty. It makes an `html` value carrying a named
  range. `render` returns the page as UTF-8 bytes beside the marks, and the host splices
  at known ranges. Text sentinels were rejected in three forms:
  - A fixed comment can be typed by hand, and so becomes an undocumented API.
  - A compile-time reservation check misses `raw()` at render time.
  - A per-render nonce leaves text every host must search for, and adds a render input.
- **A mark printed more than once is allowed.** The host resolves every mark.
- **Moving the page out of the JSON reply pays for itself.** Splicing at known offsets
  costs a median limulus.net page about 2 µs; scanning for text sentinels costs 11 µs.
  Separately, escaping and parsing the HTML inside JSON is more than half of a `render`
  call: the spike page drops from 108 µs to 49 µs without it. Per build that is a few
  milliseconds. The design wins on simplicity and unforgeability, and the speed only
  confirms that it does not lose.
- **Captures are the real cost, and they are bounded.** Every `lem-fn` block body is a
  capture (`compile/mod.rs:381-392`), so marks must travel on `html` values. `html` is
  opaque at run time. These sites touch it:
  - `EndCapture` (`vm.rs:497-500`)
  - `Op::Emit` (`vm.rs:485-488`), the only non-test caller of `Value::emit`
    (`value.rs:243`)
  - `raw()` (`builtins/values.rs:31-32`)
  - `readingTime` (`builtins/text.rs:31`)
  - `equals` (`value.rs:271`)
  - `ReceiverKind::of_value` (`builtins/mod.rs:85`)
  - `Value::type_name` (`value.rs:212`)
  - the loader's type check (`load/validate.rs:65, 85`), which only a Rust host passing
    `html` in reaches, since JSON never becomes `html` (`wire.rs:599-620`)

  These go through a `_` fallback, where a forgotten `Marked` arm answers `false` or
  names the type rather than miscompiling:
  - `Value::order` (`value.rs:315-334`)
  - `as_condition` (`228-234`)
  - `in` (`vm.rs:629-636`)
  - `lists::contains` (`lists.rs:177-184`)

  A document's `body` (`load/validate.rs:400`) is prose the compiler rendered, with no
  block output, so it holds no marks. No operator combines `html`: `+` is string with
  string, at compile (`rules.rs:116`) and at run time (`vm.rs:636`). String interpolation
  is documented but unimplemented (`compile/calls.rs:298-303`), and when it lands it must
  refuse `html` or it drops marks. So marks shift only at `Emit` and move only at
  `EndCapture`. Container builtins duplicate, drop or reorder values without inspecting
  them, which is benign.
- **`Value` stays 24 bytes** (D38). Marked markup gets its own variant behind an `Rc`. A
  replica of the enum with the extra variant measured 24. The size is prose for `Value`
  (`datetime.rs:26-30`); `Datetime` has an assertion (`datetime.rs:433-438`) and `Value`
  has none. Adding one is insurance: nothing is broken, and it guards against a variant
  quietly widening every value.
- **The artifact check is unaffected.** `scripts/wasm-artifact.ts` compares export names
  (`:32`) and import names (`ALLOWED_IMPORT_NAMES`, `:64-71`), never signatures. A probe
  built with the pinned wasm-bindgen showed that a `Vec<u8>` return keeps the three
  imports the staged artifact has. Its glue copies the bytes out
  (`getArrayU8FromWasm0(…).slice()`) and frees them, so a view into the result survives
  the next call.
- **Whitespace is nearly free.** Any `BodyKind::Value` body gets D26's dedent
  (`whitespace.rs:99-107`). D27 is one `matches!` list (`renders_nothing`,
  `whitespace.rs:236-254`).

## Working agreements

- Three checkpoints, 1a, 1b and 2. Each leaves `npm run verify` green; checkpoint 2 also
  runs `npm run rust:wasm:test`, `npm run spike:the-rebound` and `npm run wasm:bench`.
  After each: `/code-review medium --fix` on Opus, then stop; Eric reviews and commits.
  Open each with a plain-language preview.
- Docs change in the same pass as the semantics: `docs/language.md` § Tags, § Whitespace,
  § Marks, the contextual keywords, and the placeholders naming ∆1Hp;
  `docs/decisions/D42-marks-resolve-in-the-host.md`'s Status line for anything the build
  changes; `docs/artifact-changes.md`; `docs/architecture.md`; `tests/corpus/README.md`.
- Line numbers were rechecked on 2026-09-14. Confirm them before editing.

## Design settled for this item

### Tags and heads (invariant 4)

- `<lem-style>…</lem-style>`, `<lem-head>…</lem-head>`: blocks with no head. They use the
  `lem-default` idiom `head(p, |p| p.skip_to_head_end("…"))` (`template.rs:452`), with the
  message per tag name ("`lem-style` takes no head"). The body is a template, as a capture
  body is.
- `<lem-script src = expr />`: void. The head is the contextual name `src`, then `=`, then
  one expression checked as `asset` ("expected `asset`, got `string`", from
  `rules::expected`, `rules.rs:37-45`). `url` is not admitted; admitting it later is
  additive. It renders nothing in place, and collects the URL as `Value::emit` prints an
  `asset` (attribute-escaped, `value.rs:258`) as a fragment under `scripts`. A fragment
  is always what printing would write, so a producer splices it into an attribute
  verbatim. The `<script>` markup and its attributes are one choice the producer makes
  for every script (limulus.net's would be `type="module" async`). Per-script attributes
  would be additive head syntax.
- No placement tag. `mark()` is an ambient function.

### `mark(name, default: html)`

- **Declared** as an ambient entry beside `raw()` and `asset()`, so name resolution and
  did-you-mean see it. Its run function refuses to run, as `asset`'s does
  (`builtins/values.rs:81-91`).
- **Intercepted by name** in `call`'s `Resolved::Builtin` arm, *before* `reject_named`
  (`calls.rs:79-86`), and lowered by `call_mark`, modelled on `call_asset`
  (`calls.rs:256-289`):
  - Exactly one positional argument, a string literal. A computed name is an error at the
    argument ("`mark()` needs a name written as a literal: …"), since the claims are read
    while the file compiles.
  - One optional named argument, `default`, checked as `html`. Any other name is an error
    at that argument. `default` is named-only, which keeps ∆7wK's positional `payload`
    additive: no existing call has a second positional argument. It is the language's
    first optional parameter and the first builtin with a named argument; say so in D42's
    Status clause and § Marks.
  - Lowering: the default, or a `Const` of empty `html` when there is none, then
    `Op::Mark { name }` with the name interned. The VM sees a fixed argument count. The
    type is `html`.
  - Unknown names do not wait for render. With the host declaring claims and none for the
    name: a warning at the literal, with did-you-mean through `lemmon_syntax::nearest`
    ("no handler claims `stylez`; did you mean `styles`?"). With no claims declared:
    nothing.
- **At run time**, `Op::Mark` pops the default. A `Marked` default is a fault at the call:
  "a mark cannot stand inside another mark's default". It pushes `Value::Marked` over the
  default's text with one mark `{name, start: 0, end: text.len(), file, span}`.

### The wrappers are how an author writes it

`mark()` is the primitive. A host that claims a name ships an expression-bodied `lem-fn`
wrapping it: the shell's bundler ships `<lem-fn styles(): html = mark("styles") />`, and
likewise `scripts` and `head`. So a layout reads `<lem.styles()>`, and a site author never
types the primitive, just as a plugin's component wraps its own mark (∆7wK's `code`, a
diagram plugin's `diagram`). The core knows nothing of them: they are ordinary project or
plugin functions (D23). All this item owes them is `mark()` callable from an expression
body, which is why the corpus writes one. ∆PgF ships the shell's three with the default
plugin.

### Claims on the declaration

- `Host` (`host.rs:80`) gains `marks: Option<BTreeSet<Rc<str>>>` and a `with_marks`
  builder beside `with_assets`/`with_grammars` (`host.rs:165, 199`). `None` means the
  host declares no claims.
- `Unit` gains `claim_refs: Vec<ClaimRef { name, state, span }>`, one per distinct name,
  at its first call, modelled on `GrammarRef` (`unit.rs:226-238`). `state` is one of
  three: the host declared no claims, declared claims without this name, or claims it.
  With names only, a claim carries nothing more; ∆7wK gives the claimed state a digest of
  the claim's keys and payload.
- **Link** compares each recorded state with the host's now and reports a change as stale
  at the call, with three messages modelled on `grammar_ref`'s (`unit.rs:383-407`).
- **The corpus hosts:** `untyped_host` (`tests/corpus.rs:44-50`), which the `render/`
  cases and the every-op gate compile against, claims `styles`, `scripts` and `head`.
  `declared_host` (`75-86`) declares none, so the `host/` group is silent about names.

### Collection

- A fragment is per render: `Fragment { text: Rc<str>, file: u32, span: Span }`, where the
  span is the block's tag. `Fragments` is `BTreeMap<Rc<str>, Vec<Fragment>>`: names in
  sorted order, and fragments under a name in first-seen order. A fragment with text
  equal to one already there is dropped, and the first one's file and span are kept.
- A block's rendered body is one fragment. An empty body collects nothing.
- Collection happens when the block runs, anywhere in the call tree: inside a capture that
  is never printed it still collects. This matches § Tags ("from every function actually
  called"). Placement differs: a mark is placed only when printed.
- A collect block inside another collect block collects its own fragment when it runs, and
  renders nothing in the outer body.
- A collect block reached while the loader runs a default (`invoke`, `vm.rs:97-118`) is a
  fault at the block ("only a page render collects fragments"), and the loader places it
  (`load/mod.rs:440-451`).
- A string printed inside a collect body is HTML-escaped, as anywhere in a template
  (invariant 7). That is also what a `<style>` element gets today, and limulus.net
  interpolates into none of its 19 style blocks. The corpus case prints a value that
  needs no escaping, and § Marks states the rule.

### Marks

- `Mark { name: Rc<str>, start: u32, end: u32, file: u32, span: Span }`. A `Marked` value
  is `{text: Rc<str>, marks: Box<[Mark]>}`, and always carries at least one mark.
- Printing a `Marked` value appends its marks, shifted by the buffer's length, then its
  text. Offsets are `u32` with checked arithmetic, a fault rather than a panic on
  overflow (`usize` is 32 bits in the `.wasm`). A capture takes its buffer's marks onto
  the `html` value it makes: plain `Html` when there are none, `Marked` otherwise.
- So marks come out in non-decreasing offset order, and marks at one offset (an empty
  `styles` mark beside an empty `head` mark) keep print order. The corpus notation and
  `resolveMarks` both rely on this.
- Consequences:
  - A capture printed twice places twice; one never printed places nothing. A loader
    default that returns marked `html` carries the mark to whichever page prints it.
  - A fragment is text for the host, so a `Marked` value reaching `Collect` is a fault at
    the block: "a mark cannot be collected into a fragment".
  - `==` compares text, and each mark's name and range. `Html` against `Marked` is
    `false` through the existing fallback, so no arm treats one as the other.
  - `readingTime` reads the text, default content included. `raw()` passes a `Marked`
    value through.
  - Any future builtin or interpolation that edits or escapes `html` must say what
    happens to marks. D42 settles the one case that mattered: a feed's body is a string
    the shell supplies.

### Whitespace

- D26: a collect block body is a value body (`BodyKind::Value`).
- D27: `lem-style`, `lem-head` and `lem-script` take their line.
- `<lem.mark(…)>` and `<lem.styles()>` are access tags and keep their line, so a mark on
  an indented line of its own lands after the indentation and before the newline, where
  the host's `<link>` belongs.
- Each record gets a Status clause, and § Whitespace names the tags.

### VM shape

- A new module, `src/marks.rs`: what a render collects beside its text — `Mark`,
  `Marked`, `Fragment`, `Fragments`, `Buffer { text: String, marks: Vec<Mark> }`, and
  `Rendered { html: String, marks: Vec<Mark>, fragments: Fragments }`. Its `//!` states
  that one responsibility. Export the public types from `src/lib.rs:35`.
- `bytecode.rs`: `Collect { name: u32 }` (1a) and `Mark { name: u32 }` (1b) after
  `EndCapture` (`:132`). `names_every_op_from_one_list` (`:303`) goes 43 → 44 → 45. Both
  join the name-operand arm of link's rebasing (`unit.rs:562-569`) and the test helper
  `run`'s shift (`vm.rs:768-782`).
- `value.rs`: `Value::Marked(Rc<Marked>)`. `Marked` derives what `Value` derives
  (`value.rs:126`), so the explicit `equals` arm, not the derive, is where the rule lives.
  `emit` becomes `pub(crate) fn emit(&self, out: &mut Buffer)`; its only callers are
  `vm.rs:487` and the test helper at `value.rs:391`. Add
  `assert_eq!(size_of::<Value>(), 24)`.
- `vm.rs`:
  - 1a: `out: Vec<String>` (`:153`), `fragments: Option<Fragments>` — `Some` in `render`,
    `None` in `invoke`.
  - 1b: `out` becomes `Vec<Buffer>`.
  - `render` returns `Result<Rendered, RuntimeError>`.

### Callers of `render`

22 in all, from 1a on:
- **Outside the crate:** `crates/lemmon-wasm/src/lib.rs:349` and
  `crates/lemmon-vm/tests/corpus.rs:191`.
- **`vm.rs`:** 13 — the `run` helper (`:785`); `Ok` comparisons at 881, 973, 1016 and
  1101; `Err` at 897, 911, 929, 1124, 1152, 1178 and 1218; and a `let _` at 1864. The
  `unwrap_err()` sites need `Rendered: Debug`.
- **`unit.rs`:** 1316, 1339, 1370 and 1465.
- **`compile/items.rs`:** 782.
- **`compile/mod.rs`:** `run_files` (`:962`) and 1883.

### Boundary shape (checkpoint 2)

- `Core::render` returns `Vec<u8>`, a `Uint8Array` in JS copied out of wasm memory. The
  layout is a 4-byte little-endian length, that many bytes of metadata JSON, then the
  page's UTF-8 bytes.
- **Metadata on success:**
  `{marks: [{name, start, end, path, span}], fragments: {[name]: [{text, path, span}]},
  diagnostics}`. `path` stands beside `span` as it does in a diagnostic (`wire.rs:376-417`,
  `SpanWire` at `413-417`). `start` and `end` are byte offsets into the HTML section.
- **On a failed render:** `{marks: null, fragments: null, diagnostics}`, with no HTML
  section. `html: null` comes from the metadata, never from the byte length, since an
  empty page also ends right after the metadata.
- ∆7wK adds `payload` per mark, and a payload table reported at compile.
- The export name stays `core_render`, so no wasm-bindgen object needs freeing and both
  allowlists are unchanged. Caller bugs still throw a `JsError` (`lib.rs:164`).

### JS wrapper (checkpoint 2)

- **`Core.render(page)`** returns
  `{html: Uint8Array | null, marks: Mark[] | null, fragments: Fragments | null,
  diagnostics}`. `html` is a subarray past the metadata, so anything reading `html.buffer`
  must honour `byteOffset`.
- **`resolveMarks(result, handlers): Resolved`** lives in `packages/core/src/lib/marks.ts`.
  `handlers` maps a name to `{produce?, decorate?}`.
  - `produce({marks, fragments, page})` runs at most once per page per name, only for a
    name some mark carries. It gets every mark of that name and the fragments under it,
    and returns one replacement per mark (`string | Uint8Array`).
  - Each `decorate(mark, content)` runs after it, in the order the shell passes the
    handlers, over the previous content.
  - A marked name with no handler keeps its default content and is listed in
    `Resolved.unclaimed`. A name with fragments and no mark is listed in
    `Resolved.unplaced`, with its first fragment's path and span.
  - Output bytes are assembled into one `Uint8Array`, and a string result is encoded once
    with `TextEncoder`. `marks.ts` imports nothing from Node, like `core.ts`.
  - A page with no marks returns `result.html` unchanged.
  - Two producers for one name is a caller bug, thrown here; the plugin-load error naming
    both plugins is ∆PgF's.
- The splice runs before any output transform. Anything that needs a string (dprint, a
  playground) decodes once, afterwards.

### Corpus `.html` format

- **Marks:** the harness prints each mark into the rendered text as `⟦mark styles⟧` for an
  empty range, and as `⟦mark toc⟧…⟦/mark⟧` around a default. Offsets are UTF-8 byte
  offsets, applied back to front, and ties keep print order.
- **Fragments:** each fragment follows the page as a `--- styles` (or `--- scripts`,
  `--- head`) header line, then its text, one header per fragment, in name order and then
  first-seen order. The sections come before the diagnostics trailer. `trailer_start`
  takes the last `--- diagnostics` line (`lemmon-corpus/src/lib.rs:232-242`), so the
  sections above it pass through.
- The notation exists only in expectations; `tests/corpus/README.md` documents it.

## Checkpoint 1a — the collect tags

### Parser (`crates/lemmon-syntax`)

1. `src/syntax_kind.rs`: `StyleBlock`, `HeadBlock` and `ScriptVoid` in the node section,
   documented like their neighbours, just before `UnsupportedTag` (`:113-114`).
2. `src/parser/template.rs` `open_tag` (`352-478`), with three arms before the `_`
   fallback:
   - `Style`/`Head`: the no-head idiom, then
     `block(p, m, SyntaxKind::StyleBlock, name, span, Body::Plain, stack)`
     (`block` at `508-516`).
   - `Script`: `head(p, script_head)`, `void_close(p)`, then
     `m.complete(p, SyntaxKind::ScriptVoid)`, as the `Enum` arm does (`423-427`).
     - `script_head` takes its shape from `enum_head` (`682-692`): name, `skip_until`
       `=`, `expect` `=`, `expr::expression(p)`.
     - Its keyword check is the contextual one: an `at_src` like `at_from` (`348-350`),
       and `error_expected("`src`")` as in `let_from_head` (`844-848`).
     - Expected messages: "expected `src` before `/>`", "expected `src`, found `href`",
       "expected `=` before `/>`" — one diagnostic each.
   - `Import` becomes the only opaque builtin, and it is void, so `is_block` (`66-84`)
     always answers false. Delete what that strands, rather than retargeting it:
     - `is_block`
     - the block branch of the `_` arm (`462-471`)
     - `UnsupportedTag::body()` (`ast/tags.rs:575-579`)
     - the `visit_block(tag.body())` arm in `tests/corpus.rs:314`
     - the "when the builtin is a block" doc on `UnsupportedTag`
     - `knows_the_fixed_shape_of_every_opaque_builtin` (`977-981`)

     Move `Style | Head | Script` into `unsupported`'s `unreachable!` group (`90-108`),
     and refresh `head_has_eq`'s comment (`323-326`).
   - Tests:
     - `names_the_item_each_opaque_builtin_waits_on` (`983-987`): retarget to `Import`.
     - `tracks_the_nesting_of_an_unsupported_block_builtin` (`1127-1136`): no
       unsupported block builtin remains. Rename it to a test that
       `<lem-style><lem-head>a</lem-head></lem-style>` parses clean. Drop its wrong-close
       half, which `never_adopts_a_wrong_close_tag` (`1156-1168`) already covers.
     - `still_reports_an_unclosed_block_after_an_error_at_its_opener` (`1345-1354`): use
       `<lem-default>hello`. It gives "`lem-default` outside `lem-match`" and "unclosed
       `lem-default`…", both at `0..12`, and the pass confirmed it fails when `error()`
       drops a same-start diagnostic. `<lem-style x>hello` does not: its errors do not
       share a start.
     - `parser/mod.rs:622-631`
       `enters_the_error_state_only_for_a_diagnostic_about_the_current_token`: use
       `<lem-default /></lem-default>`. It gives "`lem-default` outside `lem-match`" and
       "`lem-default` is a block tag and cannot be self-closed: expected `>`", and the
       second is dropped when `error()` enters the error state for every diagnostic.
       `<lem-case 1 /></lem-case>` does not test this: consuming the `1` clears the error
       state anyway.
     - Perturb both before trusting them.
3. `src/parser/dump.rs` `is_boundary` (`105-129`): add the three kinds, or they vanish
   from `.tags`. No gate catches that; only the blessed diff shows it.
4. `src/ast/tags.rs`: the views, the `Item` variants, and a view test per new node.
   - `StyleBlock { body() }` and `HeadBlock { body() }`, modelled on `DefaultTag::body()`
     (`507-519`).
   - `ScriptVoid { src() -> Option<Expr> }` via `head_child` (`652`).
   - `Item` variants `Style`, `Head` and `Script` (`22-59`). `Item::Head` sits beside the
     existing `Head` view (`649`); pick names that read clearly in `lower_item`.
   - Re-export them in `src/ast/mod.rs:27-31`.
   - Retarget `exposes_the_opener_of_unsupported_and_unknown_tags` (`1064-1079`) to
     `lem-import`.
5. `tests/corpus.rs` `visit_items` (`246-317`): `Style`/`Head` visit their body and
   `Script` visits its expression. The typed-view gate (`335-351`) walks only clean
   cases, so each new node must appear in one.
6. Corpus, from the author's seat:
   - `syntax/unsupported/builtins.lem` keeps only `lem-import`, so `UnsupportedTag` stays
     reached.
   - `wrong-close.{lem,cst,tags}` are deleted: `errors.lem` below covers a `</lem-head>`
     closing a `lem-style`, and the orphan gate (`lemmon-corpus/src/lib.rs:424`) fails on
     a stray expectation.
   - New `syntax/channels/`:
     - `component-styles.lem`: a `lem-fn` with a `lem-style` whose CSS holds `>` and
       `{}`.
     - `head-and-scripts.lem`: a `lem-head` with a `<link>` and a `lem-if` inside, and
       `lem-script src = asset("…")`.
     - `errors.lem`, an author's mistakes:
       - `<lem-script />`
       - `<lem-script href = x />`
       - `<lem-script src="/x.js" />`
       - `<lem-script>code</lem-script>`, a WebC habit
       - `<lem-style class="x">`
       - `<lem-style />`
       - a `</lem-head>` closing a `lem-style`
       - `<lem-emit styles />` as an unknown tag
   - `syntax/canonical/article-card.lem` gains the `lem-style` block the language doc's
     canonical example carries.
   - Bless, then read the diff.

### Compiler and VM (`crates/lemmon-vm`)

7. `src/marks.rs` with `Fragment`, `Fragments` and `Rendered { html, fragments }`;
   `bytecode.rs` `Collect { name }`, documented "Pop an `html` value and add its text to
   the fragments under the name, unless it is empty or they already hold equal text".
   Link rebasing and `run`'s shift gain the op; the op count goes to 44.
8. `src/compile/items.rs` `lower_item` (`128-172`), with arms replacing what reaches the
   `unreachable!` (`166-170`):
   - `Item::Style` / `Item::Head`: `Capture`, `lower_block(body.items(),
     BodyKind::Value)`, `EndCapture`, `Collect { name }`, spanning `tag.span()`. This is
     `lower_let_block` (`390-398`) with the store replaced.
   - `Item::Script`: `Capture`; `self.check(&required(tag.src()), &Type::Asset)`; `Emit`;
     `EndCapture`; `Collect { name: scripts }`.
9. `src/compile/whitespace.rs` `renders_nothing` (`236-254`): add the three items. The
   test helper `body_of` (`546-559`) gains a `Style` arm. Add a dedent test for a style
   body, and a line-rule test for each of the three tags.
10. `src/vm.rs`:
    - `fragments: Option<Fragments>`.
    - `Op::Collect`: pop, then read the name constant. `fragments: None` is a fault,
      "only a page render collects fragments". Otherwise push unless empty or equal.
      The file and span come from the running function and pc, as `fail` reads them.
    - `render` returns `Rendered`. Update the 22 call sites.
    - Unit tests:
      - dedup keeps first-seen order and the first span
      - an empty body collects nothing
      - a block inside an unprinted capture still collects
      - `invoke` reaching `Collect` is the fault
    - The "second render starts empty" test is dropped: a fresh `Vm` per render makes it
      pass by construction (`vm.rs:65-73`).
11. A two-file test (`compile/mod.rs` `run_files`): two files each collect equal CSS under
    `styles`, and one fragment results. Perturb it by keying on the operand to see it
    fail.
12. `tests/corpus.rs`: `rendered` (`186-195`) returns the html and fragments. `shown`
    (`200-207`) writes the fragment sections before the trailer, and `html` (`211-226`) and
    `typed_html` (`234-270`) thread them through. `untyped_host` claims its three names in
    1b, not here.
13. Corpus:
    - `render/channels/component-styles.lem`: two components with `lem-style`, one of
      them called three times (its fragment appears once). This reaches `Collect` for the
      every-op gate (`tests/corpus.rs:632-645`).
    - `render/channels/interpolated-style.lem`: a `lem-style` whose body prints a colour
      parameter. It is called with two values, giving two fragments, since dedup is by
      content.
    - `render/channels/collected-but-not-printed.lem`: a `lem-style` inside a function
      called only within a capture that is never printed. It still collects.
    - `render/errors/script-src-not-an-asset.lem`: `src = "x"`.
    - `host/pages/` gains a page with `<lem-script src = asset("scripts/gallery.js") />`
      reached twice. It resolves through `ASSETS` (`tests/corpus.rs:89-95`) and yields
      one `--- scripts` section with the hashed URL.
    - `document/blocks/style-block-in-prose.md`: a `lem-style` at document top level
      takes its line, and the next prose chunk loses one newline (`items.rs:24-32`).
    - `render/canonical/article-card.lem`: add the `lem-style` block.
    - Bless, and read every diff line against what the source asked for.

### Docs (1a)

14. `docs/language.md`:
    - The Tags rows for the collect tags are confirmed against what was built.
    - `src` is added to the contextual keywords (`:66-69`).
    - § Whitespace names the three tags under D26 and D27.
    - The collection rules (empty bodies, unprinted captures, escaping) go into § Tags or
      § Marks, whichever the row points to.
15. `docs/decisions/README.md` and the D26 and D27 records get a Status clause: "(∆1Hp)
    collect blocks are value bodies / take their line".
16. `tests/corpus/README.md`: the `.html` row and the fragment sections.

**Verification:** `npm run rust`, then `npm run corpus:bless` and a read of the diff.
Then `npm run rust:wasm`: `render_json` takes `Rendered.html` and drops the fragments,
which the checkpoint preview says. Then `npm run verify`.

## Checkpoint 1b — marks

17. `src/marks.rs`: `Mark`, `Marked`, `Buffer`; `Rendered` gains `marks`.
    `bytecode.rs`: `Mark { name }`, documented "Pop an `html` default and push it marked
    under the name, spanning its text". Link rebasing and `run`'s shift gain the op; the
    op count goes to 45.
18. `mark()`:
    - the ambient entry, which refuses to run
    - the intercept before `reject_named`, and `call_mark`
    - `Host::with_marks`
    - `ClaimRef` on the unit, and the link check
    - the warning with did-you-mean, silent without claims
    - `untyped_host` claims `styles`, `scripts` and `head`
19. `src/value.rs`:
    - `Value::Marked`, with `type_name` "html".
    - `equals`: text, then name and range per mark.
    - `emit(&self, out: &mut Buffer)`, where a `Marked` value shifts its marks, then
      appends its text.
    - `builtins/mod.rs:85` → `Html`; `builtins/values.rs:31-32` passes it through;
      `builtins/text.rs:31` reads its text; `load/validate.rs:65, 85` admit it as `html`.
    - The 24-byte assertion.
20. `src/vm.rs`:
    - `out: Vec<Buffer>`, and `render`/`invoke` construct `vec![Buffer::default()]`.
    - `EmitText` and `Capture` adapt.
    - `EndCapture` gives `Html` with no marks, `Marked` otherwise.
    - `Op::Mark` as designed, with checked offsets.
    - `Op::Collect` refuses a `Marked` value.
    - Unit tests:
      - a mark inside a capture printed twice gives two shifted marks, and never printed,
        none
      - nested captures shift through both levels
      - `raw()` keeps marks
      - `==` distinguishes a different name or range, and ignores the span
      - a default's range covers its text
      - a marked default is the fault
      - a marked value collected is the fault
      - two marks at one offset keep print order
21. `tests/corpus.rs`: `rendered` and `shown` add the marks and insert the notation.
22. Corpus:
    - `render/channels/component-styles.lem` gains a wrapper
      `<lem-fn styles(): html = mark("styles") />`, called as `<lem.styles()>` on its own
      indented line in a layout `lem-fn`, beside `<lem.head()>`. That pins the authoring
      form and coincident marks.
    - `render/channels/marks-through-captures.lem`:
      - `mark("head")` inside a `lem-let` capture printed twice (two marks), and one
        never printed (none)
      - a mark inside a layout reached through two nested calls
      - `mark("toc", default: x)` with its range. `untyped_host` does not claim `toc`, so
        this case also shows the warning.
    - `render/warnings/unclaimed-mark.lem`: `mark("stylez")`, the warning with
      did-you-mean.
    - `render/errors/mark-name-not-literal.lem`: `mark(name)` with a computed name, and
      `mark("styles", fallback: x)` with an unknown named argument.
    - `render/errors/mark-collected-into-a-fragment.lem`: a `lem-fn` calling `mark()`
      inside `lem-style`, which is the fault at the block.
    - `render/errors/mark-inside-a-default.lem`.
    - `syntax/channels/head-and-scripts.lem` gains `<lem.mark("styles")>` indented beside
      markup.
    - A host-level stale-claim test in `unit.rs`: a unit compiled against claims, linked
      against a host that drops one, and one compiled without claims, linked against a
      host that declares them.
    - Bless and read.

### Docs (1b)

23. `docs/language.md` § Marks:
    - Loses its "not yet (∆1Hp)" placeholder, keeping ∆7wK's.
    - States the named-only `default`, the warning, silence without claims, equality, and
      the escaping rule.
    - The wrapper sentence is confirmed against the corpus case.
    - The Calls row notes `mark`'s named argument.
24. `docs/decisions/D42-marks-resolve-in-the-host.md` Status clause (∆1Hp): `default` is
    named-only; the name is lowered to an op; marks and fragments carry a file.
    `docs/decisions/README.md`'s D42 row changes only if it states something the build
    changed.
25. `docs/artifact-changes.md`: amend the D42 entries against what was built — "The Lemmon
    Language" Marks entry (`:216-226`: named-only `default`, an unclaimed name a warning)
    and "Shell & Core" VM, ops and values (`:418-422`: the `Mark { name }` op beside
    `Collect`).
26. `tests/corpus/README.md`: the mark notation.

**Verification:** as in 1a.

## Checkpoint 2 — the boundary, the wrapper, the scripts, the journal

27. `crates/lemmon-wasm/src/wire.rs`:
    - `HostInput.marks: Option<Vec<ClaimInput { name }>>` with `deny_unknown_fields`, and
      one `with_marks` call in the `From` impl (`175-211`).
    - Replace `RenderResult` (`587-593`) with `RenderMeta`, as designed.
    - `fn encode_render(meta, html: Option<&str>) -> Vec<u8>`.
    - Rewrite the module doc (`:4-5`) and `RenderResult`'s doc (`:588`).
28. `crates/lemmon-wasm/src/lib.rs`:
    - `render` returns `Result<Vec<u8>, JsError>`, and `render_json` (`334-361`) becomes
      `render_bytes`.
    - The module doc (`:4-5`) and the shim doc (`154-162`) describe the layout.
    - Host-target tests gain a `decode` helper. `html_of` (`451-458`, 25 call sites), the
      runtime-failure test (`873-881`) and `drops_the_loaded_data_…` (`1052-1056`) use it.
    - New tests:
      - A page with a style and a mark after non-ASCII text returns one fragment with its
        path and span, and a mark at the right byte range with its path and span.
      - A failed render has null marks and fragments and no HTML section.
      - An empty page has an empty HTML section.
      - A declaration that stops claiming a name stales the unit that read it.
29. `crates/lemmon-wasm/tests/boundary.rs`: decode in all four tests that call
    `.contains` on the result (`36-39`, `86-92`, `116-119`, `161-164`). They also compile
    under `cargo test` and clippy, so `verify` breaks without this.
30. `packages/core`:
    - `src/lib/boundary.ts`: `render(page: string): Uint8Array`, and its doc (`:16-17`).
    - `src/lib/types.ts`: `Claim` on `HostDecl`, plus `Mark`, `Fragment`, `Fragments`,
      `RenderResult` as designed. Rewrite the note at `199-202`, which names `channels`
      (∆1Hp) and `deps` (∆xBO).
    - `src/lib/core.ts` `render()` (`81-83`): read the `u32` with a `DataView`, decode the
      metadata with a `TextDecoder`, and view the HTML. Update the docs at `12-14` and
      `78-80`.
    - New `src/lib/marks.ts` and `marks.spec.ts`, plus `export * from './lib/marks.ts'`
      in `src/index.ts`. Spec every branch:
      - no marks returns the same bytes
      - a producer per name
      - a name marked twice calls its producer once
      - decorators run in order over the producer's output, and over default content when
        there is no producer
      - an unclaimed name keeps its default and is reported
      - an unplaced name is reported with its span
      - offsets after multi-byte text
      - a mark at offset 0, at the end, and two at one offset
      - a range with content replaced
      - two producers for one name throws
    - `core.spec.ts`:
      - The nine `.html` string assertions (181, 226, 260, 267, 294, 303, 313, 314, 363)
        decode.
      - A success-path test through the staged artifact: compile a template with a
        `lem-style` and `<lem.mark("styles")>` against a host claiming `styles`, render,
        check the mark and fragment, resolve, decode, and compare.
      - The failure test checks that `html`, `marks` and `fragments` are null.
    - `load.spec.ts:35` and `:70` decode.
    - `packages/core/README.md:15`.
31. `scripts/render-the-rebound.ts`:
    - Pass 1's `bodyHtml` feeds `loadData` as `page.content` (`:277`), so decode once
      there.
    - Decode `:273` and `:285` outside the timed closure, so the 1 ms budget measures the
      render.
    - `:308` prints a byte length, which becomes `html.byteLength`.
32. `scripts/bench-wasm.ts`: `record()` computes `bytesOut` with `JSON.stringify` (`:71`),
    which counts a `Uint8Array` as `{"0":60,…}` and inflates silently. Count `byteLength`
    for a `Uint8Array`. Lines 213 and 258 pass the whole result.
33. `docs/architecture.md`:
    - The placeholders naming ∆1Hp at `:112-114` and `:277-279`, which do not use the
      gate's phrase, so only a grep finds them.
    - §WASM API's boundary rules (`:47-48`): bulk data crosses as JSON, except a render's
      page, which crosses as bytes beside its metadata, with the measurement.
    - `docs/artifact-changes.md` "Shell & Core": the WASM API entries (`383-394`)
      confirmed.
34. Journal, once Eric accepts the work:
    - `npx @limulus/delto@1 complete ∆1Hp docs/journal/∆1Hp-side-channels.md`, while the
      item is still in `BACKLOG.md`.
    - Fill Planning from this file: link [[∆RGQ]] for the split and the sentinel history,
      and D42 for the revision, its four angles, and the boundary measurements, plus the
      verification pass's corrections. Then fill Refinement and Retrospective.
    - Remove ∆1Hp from `BACKLOG.md`, and delete this file.

**Verification:** run each of these and report which ran:
- `npm run rust:wasm`
- `npm run verify`, with 100% coverage on the helper and the wrapper
- `npm run rust:wasm:test`
- `npm run wasm:check`
- `npm run spike:the-rebound`
- `npm run wasm:bench`

## Out of scope, noted for the follow-ons

- **∆7wK** makes a code block a `code` mark with a payload by id. It gives `mark()` its
  positional payload typed by the claim's record, gives claims their mode, keys and a
  digest, and removes syntect and the grammars. It builds on the `Mark`, `Marked`, wire
  and wrapper shapes above without changing them.
  - A mark's span is where `mark()` runs, which for a wrapper is inside the plugin's
    `lem-fn`. A handler diagnostic about `<lem.diagram(…)>` would point there rather than
    at the author's call.
- **∆7Xs** decides the bundle's file name and hashing as the `styles` producer, and must
  splice before ∆F9l's `transformOutput`. File-backed stylesheets have no form yet:
  - limulus.net's WebC inlines `normalize.css`, `main.css` and `hljs-github.css`
    (`base.webc:27-29`) and `mapbox-gl.css` (`map-box.webc:12`) into the bundle.
  - A `lem-style` body is inline text, and `@import` is invalid mid-bundle.
  - A void `<lem-style src = asset(…) />` would be additive, since the head decides the
    shape.
- **∆PgF** turns `handlers` into plugin registrations and the load-time collision error,
  and ships the default plugin with the three wrapper `lem-fn`s.
- **∆c9S** decides what a page is.
  - A non-page file's top-level output never renders. That includes a top-level
    `lem-style`, which is where a WebC port would put one: all 19 of limulus.net's
    `<style>` blocks sit at their file's top level. Once "is a page" is known, that can
    be a diagnostic.
  - Once the core calls a page's layout through its entry field, the page body reaches
    the layout as an `html` argument, a third place marks move.
- **∆ePP's feed** reads a page's finished output as a string from the second render phase
  (D42); nothing here escapes `html`.
- **`lem-script` is not needed for limulus.net parity.** Its three external scripts are
  kept in place (`base.webc:115`, `article-vod.webc:14`, `page.webc:71-76`) and nothing
  bundles JS. Its `head` bundle is (`getBundle('html', 'head')`, `base.webc:117`).
- **`lem-import`** stays opaque under ∆PgF; `lem-page` and `lem-emit` are retired.
- **∆5MM**'s grammar gets three boundary kinds from `.tags`.

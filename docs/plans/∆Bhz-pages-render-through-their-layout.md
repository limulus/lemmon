# ∆Bhz Pages render through their layout — plan

Written by the session that planned ∆c9S, split it into ∆Bhz and ∆99b, and settled D43
with Eric, and handed on to the session that builds this item. The design points below
were settled with Eric or follow from decisions he made; they are not open unless he
reopens them; the three at the end he answered on 2026-09-17. Re-verified that day
against `main` at `0319f60` by the session Eric asked to plan the item and to weigh
waiting for ∆7wK; that session split checkpoint 1 into a syntax half and a compiler
half, held its rest until ∆7wK landed, and refreshed the anchors below. When ∆Bhz's
journal entry is written, its Planning section absorbs this file and the file is
deleted.

## Start here

1. Read `CLAUDE.md`, the ∆Bhz item in `BACKLOG.md`,
   `docs/decisions/D43-generated-pages-and-neighbours.md`, and D41.
2. ∆1Hp has landed (`e1568cb` in the VM, `598d9f8` at the boundary). It reshaped
   `lemmon_vm::render`, the boundary's `render`, the corpus harness's `rendered`, and all
   24 `render` call sites, which now read `.html` off a `Rendered` — so this item leaves
   them alone. ∆7wK has landed too (`864c2bf`, journal written), so nothing below waits
   on it; every line number was rechecked against that head, and functions are named so
   they can be found again.
3. `npx @limulus/delto@1 claim ∆Bhz` — the ledger, `.delto-claims.local.jsonl`, is per
   checkout, so a claim made in a worktree is not seen from `main`.
4. If the checkout has no `target/` or `artifacts/wasm/`, the first `npm run rust:wasm`
   and `cargo test` are cold builds.
5. Checkpoints 1a (`bc2b3ab`) and 1b are built on the `claude/delto-plan-bhz-d39cc6`
   branch (2026-09-17); 1b awaits Eric's review. Open checkpoint 2 with a plain-language
   preview once it is committed.

## What ∆7wK left for this item (2026-09-17)

The item was planned while ∆7wK was in flight and the two touched the same compiler
files, so the plan held checkpoints 1b–3 until ∆7wK landed; it has (`864c2bf`), and the
hold is lifted. What it settled that this plan reads as given:

- `Mark { name, key, payload: Option<Payload>, start, end, file, span }` with
  `Payload::{Value(Value), Reported(Rc<str>)}`; `Mark`, `Marked`, and `Rendered` compare
  by `PartialEq` only. Checkpoint 2's ordering test compares `start`s and names, not
  whole marks, and `push_marked` must carry a body mark's payload into the layout's
  buffer unchanged — a fence in a document body is such a mark.
- `Unit.body` is a document's pieces' text with code markup included; `Module.payloads`
  sits beside `Module.documents`, and link pushes both per unit. A page template
  contributes no payloads.
- The corpus shows `⟦mark code bash Code {…}⟧` and the `.html` trailer gains a
  `--- payloads` section; `rendered` in the harness prints both, and a page rendered
  through its layout prints them the same way.
- `Core.compile` returns `CompileResult { diagnostics, payloads }`: every spec here reads
  `.diagnostics`, and `render-the-rebound.ts` and `bench-wasm.ts` already will.
- `HostInput` and `HostDecl` lose `grammars`; `url` goes beside `entry` and `marks`.
- Two cases it added change again under checkpoint 2: `host/tils/code-in-a-til.md`
  gains its layout, and `document/fences/fence-after-a-silent-tag.md` has front matter
  (1a re-blessed its tree).

## Already done

**The split** (∆c9S, 2026-09-14): ∆c9S became ∆Bhz (this item) and ∆99b (generators and
neighbours). D43 is written, with rows and Status clauses on D30, D36, D37, and D41;
`docs/language.md` § Ambient types, `docs/architecture.md` § WASM API, § Build pipeline
and § VM, and `docs/artifact-changes.md` describe the target, with placeholders naming
∆Bhz and ∆99b. `Page` in `language.md` no longer declares `prev`/`next`.

## What exists today, and why this item is larger than its old text

- **No page renders through its layout.** `lemmon_vm::render` (vm.rs:61-92) runs a file's
  entry function with no arguments and returns what it emitted — since ∆1Hp a
  `Rendered { html, marks, fragments }` (rendered.rs:208-216) — `.md` documents included.
  `Host::entry` has one consumer, `check_entry_field` (compile/schema.rs:213), which
  checks that a record whose base is the document type declares the field with the shape
  `(Self, html): html`; it runs only in the file that declares such a record, and nothing
  reads the field at render. `packages/core/src/lib/core.spec.ts:276`, `:289` and `:338`,
  and every `host/pages` and `host/tils` case that renders, show a document without its
  layout's `<h1>`.
- **A `.lem` file's front matter is text.** The template scanner has no front-matter rule,
  so `---\ntitle: x\n---` renders literally and a `<lem.` inside it is a tag. Only
  `parse_document` skips the block, with its private `front_matter`
  (lemmon-syntax/src/document.rs:160-176), and the block sits inside the document tree's
  first `TextRun` (the test at document.rs:313 asserts `Text@0..18`). The corpus harness
  has a second, LF-only detector (lemmon-vm/tests/corpus.rs:522, tested near 680).
- **A page template cannot be loaded.** `Module.documents` (bytecode.rs:239, its
  `Document` at 220-228) is `Some` only for `.md` files, built at link from `Unit.body`
  (unit.rs:550), so `check_envelope` (load/mod.rs:158) refuses a `.lem` glob
  member as "no compiled document". A file typed by `type:` alone, with no glob, compiles
  typed and has nowhere in `DataInput` (wire.rs:265) to send its front matter.
- **`page` in a template is the envelope's value.** `Store` keeps one record per document
  path (load/mod.rs:73-89); anything else binds the value the envelope carried, unchecked.
- **Nothing reports a url.** The harness makes one up (`/{stem}/`, corpus.rs:504).

## Working agreements

- Four checkpoints — 1a, 1b, 2, 3 — each leaving `npm run verify` green; checkpoint 3
  also runs `npm run rust:wasm:test`, `npm run wasm:check`, and `npm run
  spike:the-rebound`. After each: `/code-review medium --fix` on Opus, then stop; Eric
  reviews and commits. Open each with a plain-language preview. Commit a checkpoint
  before opening the next (∆1Hp's retrospective).
- Docs change in the same pass as the semantics: sweep the placeholders naming ∆Bhz in
  `docs/language.md` and `docs/architecture.md` as each lands, confirm the D43 entries in
  `docs/artifact-changes.md`, and update `tests/corpus/README.md`.
- Corpus cases are written from the author's seat; bless, then read every diff line
  against what the source asked for. Checkpoint 2 changes every `host/` case that renders
  a loaded page, and each must be read.

## Design settled for this item

### Front matter

- **One rule, one predicate.** `front_matter` moves out of `document.rs` into a public
  `lemmon_syntax::front_matter(src) -> Option<Span>`, unchanged: a first line of exactly
  `---` (CRLF allowed) through the next such line; with no closing line there is none.
  `parse` (template mode) and `parse_document` both call it. The harness's detector
  (corpus.rs:522) goes, its YAML tests staying; the harness takes the span and parses the
  YAML inside it. The tests at document.rs:309-327 exercise `parse_document`; the predicate
  gets tests of its own, and those stay where they are with the tree they assert updated.
- **Front matter is a token.** A new `SyntaxKind::FrontMatter` is the first child of `Root`
  in both modes, so the CST stays lossless and a `.md` tree no longer folds the block into
  its first `TextRun` (`Text@0..18` becomes `FrontMatter@0..13` and `Text@13..18`). The
  token section of `SyntaxKind` mirrors `TokenKind` one for one (syntax_kind.rs:9-11, the
  `From` at 208-260), so `FrontMatter` goes after `Error` (line 76) with a note saying it
  comes from no scanner token. `.tags` projects it as a boundary line, which is what a
  tree-sitter grammar has to agree with (invariant 9, ∆5MM). In template mode `parse` splits
  the file at the block's end, parses the rest, and splices the rest's children after the
  token, shifting diagnostics and their labels as document.rs:78-90 does. Nothing inside the
  YAML is ever scanned. The typed view iterates nodes only (ast/tags.rs:17-19), so it skips
  the token and the compiler emits nothing for it.
- **An unclosed block is an error in both modes** (Eric, 2026-09-17). The predicate is
  textual — a first line of exactly `---`, then the next line that is exactly `---` — so
  "unclosed" is the first holding and the second never holding, and the YAML is never
  read. `front_matter` returns a third state carrying the opening line's span, and
  `parse` and `parse_document` both report "front matter opened here has no closing
  `---`" there, with the block left in the tree as text so it stays lossless. The one
  markdown shape this refuses, a document opening with a thematic break and no second
  rule, has `***` and `___` as its spelling. `treats_an_unclosed_front_matter_as_prose`
  (document.rs:317) becomes the refusal, and the shell's reader (∆4tG) must refuse the
  same shape, so the two agree about which files are pages.

### Which files are pages

- **A page is a file compiled with a record that has front matter** — any `.md` document, or
  a `.lem` file that opens with a `---` block. It loads a record, `plan()` lists it, and
  `render` renders it through its entry. A `.md` with no record, or a `.lem` with front
  matter and no record, reads the document value unknown-typed and renders its own output,
  as an unmatched document does now. `Host::for_file` (host.rs:469-483) is keyed on
  `is_document(path)` today (line 479); it takes whether the file is page-shaped instead,
  and so does its caller in link (unit.rs:351), which has no source text and reads a flag
  the unit records.
- **A `.lem` file with no front matter takes no record.** It is no page, so a record would
  type `page` while the render binds the envelope's unchecked value (invariant 10). The
  compiler reports it at the start of the file: "a template with no front matter is no
  page, so it takes no record". A `.lem` with no front matter and no record is what it is
  today: `page` has the declared type and the envelope's value.
- **Top-level output in a file that is no page is a warning** (Eric, 2026-09-17). A
  `.lem` file with no front matter is a component or a layout, and its entry function is
  never rendered by a shell, so a top-level `lem-style` — where every one of limulus.net's
  nineteen `<style>` blocks sits in WebC — silently never runs. Under a host that
  declares a document type, the compiler warns once per such file, at the first
  top-level item that emits: a non-whitespace text run or any tag other than `lem-fn`,
  `lem-type`, `lem-let`, and an import — "this file has no front matter, so it is no page
  and its top-level output never renders". A host that declares no document type — the
  untyped corpus host, the spike, a playground — has no pages, so it says nothing, and
  the 112 `render/*.lem` cases and `spike:the-rebound` stay as they are. A warning, not
  an error: the harness and a playground do render such files.
- **`Module` carries a page source for every page-shaped file.** `Document.body` becomes
  optional — `None` for a page template, which has no prose — and link pushes a page
  source for a template with front matter as well as for every document. `Unit` records
  whether its file has front matter. `record_span` (compile/surfaces.rs:81-91) falls back
  to a template's front matter as it does a document's. Rename `Document` and
  `Module.documents` only if the word misleads and the diff measures small; recheck the
  loader's messages that say "document" either way.

### `body` and a page template

- **Where a default reads `body` is recorded.** The collector a default compiles under has
  no roots for `path` and `body`, and the
  resolver's `used` flag is a bool that `Implicit` bindings are exempt from, so neither
  gives the span. Record it where a name resolves to `Resolved::Local { binder }` and the
  binder is `body`'s — the one `compile_step` makes at `Span::new(span.end, span.end)`
  (compile/mod.rs:588, `path`'s at 587) — matching the binder, not the name, since a lambda
  parameter may be called `body`. Keep the first such span on `DefaultFn` (unit.rs:108).
- **Link refuses a page template whose record's plan runs such a default.** For each unit
  that is a page template with a record, walk that record's plan — every field's default
  from the nearest declaration that gives one, inherited ones included, so not through
  `record_plans`' `refuse` gate (unit.rs:695, the gate at 716), which skips them — and
  report one error at the template's record span: "`Til` cannot type a page template:
  its defaults read
  `body`, and a template has no body until it renders", labelled at each reading default.
  A document of the same record is not refused; both halves are tested.
- **The loader passes `Value::Null` for a page template's `body`.** Step results are not
  revalidated (validate.rs:333-339), so the link refusal is the only thing keeping a
  default such as `excerpt: html = body` from storing a null. Its negative test is a
  perturbation that shows that harm: disable the refusal, and watch the record carry null.
  Since ∆1Hp a `body`-derived default can also carry marks
  (`takes_marked_markup_where_a_host_supplies_html`, load/mod.rs:639), which is one
  more reason a template's null must never reach one.

### Loading

- **The envelope gains `pages`.** `Data.pages: Vec<Loaded>` (wire `DataInput.pages`, TS
  `DataInput.pages?: LoadedValue[]`) carries the front matter of every page no glob holds,
  path required. A glob member may now be a page template. The loader runs `pages` after
  every bindings plan, in envelope order, through the path `Loader::document`
  (load/mod.rs:482-519) takes, minus the element check.
- **What `check_envelope` refuses.** As caller bugs, after the refusals it makes today (so
  `refuses_sources_in_the_wrong_shape`, load/mod.rs:853, still passes): a `pages`
  entry with no path; one naming a file compiled as no page, or with no record; a page
  carried twice, in a glob and in `pages` or twice in either.
- **A page the envelope does not carry is not loaded.** It is not an error: the resident
  `Core` cannot remove a unit (the boundary only ever inserts), so a page the shell
  deleted, renamed, or left out as a draft (D19) stays compiled, and refusing it would
  make every later load throw. `plan()` omits it, and `render` of it throws a caller bug
  — "`x` is a page the last load did not carry" — since binding the envelope's value would
  type `page` as a record it is not. `skips_a_computed_binding_once_an_earlier_value_failed`
  (load/mod.rs:1180), which compiles `pages/a.md` and loads `posts: []`, keeps
  passing.
- **The store keeps pages in load order.** `Store.documents: BTreeMap<String, Value>`
  becomes an ordered list of loaded pages, each `{path, record, spans, record_span}`, with
  an index by path: collection members in the order the loader builds the collections —
  its steps' link order (unit.rs:788-796), which D43 and `architecture.md` call binding
  order and which can differ from declaration order — then `pages` in envelope order. A
  test holds the order where the two differ. `Store::globals` finds a page by path; ∆99b
  keys it by `{path, index}`. Callers to update: validate.rs:559, 1173, 1240, 1425;
  the loader's tests that read `store.documents`; bytecode.rs:406 builds a `Document`
  literally.

### The url field

- **The declaration names it** (D43). `Host` gains `url: Option<Rc<str>>` beside `entry`
  (wire `HostInput.url`, TS `HostDecl.url?`), installed as `entry` is.
- **It is checked at link, and so is the entry field.** The document type is declared in a
  host file with no base, which `check_declarations` (schema.rs:328) skips just inside, and
  neither name is a fact of any unit, so a compile that only changes the declaration
  relinks without recompiling (`compile`'s relink in lemmon-wasm/src/lib.rs) and a
  compile-time check would never see it. Link has the index and the host on every
  relink: it reports a
  document type that does not declare the url field as `url` at that type's declaration,
  and — now that `render` reads it — a page record without the entry field, in the same
  place `check_entry_field` would. Render and the url pass still return a spanned error
  rather than calling `expect` on either field (invariant 8).
- **Two loaded pages at one url is a load error** at the second page's `url` position — its
  spans table, then the page's own value span, then its `record_span` when neither exists,
  as for a url a default derived — labelled at the first. Tests cover a written repeat, a
  derived one, and one across a glob and `pages`. A host that names no url field gets no
  check and a `plan()` with no urls.

### Rendering and `plan()`

- **`render_page` renders a page through its entry, in one VM.** A new VM entry point
  takes the module, a handle on the store's page, the entry field's name, and the globals,
  and returns a `Rendered` as `render` does, so ∆99b adds the page table without changing
  the signature again and the 24 `render` call sites ∆1Hp reshaped stay as they are. It
  captures the file's entry function's output natively through `Buffer::into_value()`
  (rendered.rs:126), the one call `Op::EndCapture` (vm.rs:525-528) makes — plain
  `Html`, or `Marked` when the buffer holds marks — so marks in the body are not dropped.
  It then calls the record's entry field with the record and that body inside the same
  `Vm`, not through `vm::invoke` (whose `fragments` is `None`, vm.rs:123, so a `Collect`
  faults) or `Machine::invoke` (which counts `MAX_NATIVE_DEPTH`,
  vm.rs:686-691), and emits what it returns into the root buffer — a fallible step since
  ∆1Hp, because `Buffer::push_marked` (rendered.rs:98) refuses a page over four
  gigabytes. So `uniqueId` counts on from the body into the layout, the fragments of both
  land in one collection, and the marks of both arrive in one `Vec<Mark>` in
  non-decreasing `start` order, which is what `Rendered.marks` promises
  (rendered.rs:211-213). The capture adds no `Op`: the every-Op gate reads only the
  `render` group (`emits_every_op_from_some_case`, corpus.rs:736, `GROUP` at 37), where
  no page is rendered. A call that fails before running has no span; map it to the page's
  `record_span`. A layout's runtime error keeps the layout's span. A file that is no page
  renders as today, so `render/`, `document/`, and the spike are unchanged; a page with no
  host entry renders its own output, bound to its record.
- **`plan()` lists loaded pages.** `Store::plan(&Host) -> Vec<PlannedPage>` (not `Planned`,
  which `unit.rs:665` already names), each `{path, url?}` in store order, `url` the
  field's text when the host names one. At the boundary `Core::plan()` returns that JSON;
  a call before any load has succeeded, or after a compile dropped the store, throws, as
  `render` before a compile does; after a failed load it lists the stale store.
  `core_plan` joins `EXPECTED_EXPORTS` (scripts/wasm-artifact.ts:32). In TS, `plan():
  PlannedPage[]` with `interface PlannedPage extends PageRef { url?: string }`, and `render`
  sends only `PageRef`'s fields, since the wire's `PageRef` denies unknown fields.

## Checkpoint 1a — front matter in the syntax crate

Self-contained: nothing ∆7wK is building touches these files, so this half can go
before its second and third checkpoints land. After it, a `.lem` file that opens with
`---` carries the block as a `FrontMatter` token the compiler emits nothing for, so
the file renders without it and `page` is still the envelope's value; the record
refusal, the page source, and loading wait for 1b.

### Syntax (`crates/lemmon-syntax`)

1. `front_matter` public, with tests of its own; `parse_document` emits the token and
   starts its first text run after it; `SyntaxKind::FrontMatter` after `Error`;
   `parse`'s split and splice; `parser/dump.rs` `is_boundary` gains the kind.
2. Tests: spans after the block are file-absolute; a diagnostic and its labels after it are
   shifted; a scanner error right at the splice boundary; `<lem.x>` inside the YAML is no
   tag; CRLF; an unclosed block is refused at its opening line in both modes, with the
   text still in the tree; a `---` block not at the start is text; `--- ` with a
   trailing space and a blank line before `---` are no front matter.
3. Corpus: `syntax/front-matter/page-template.lem` (YAML holding `<lem.not-a-tag>` and a
   `>`, then markup and a tag), `syntax/front-matter/unclosed.lem` (the error), and
   `document/front-matter/unclosed.md` beside it. Every
   `document/**/*.cst` and `.tags` with front matter changes — two `document/` cases
   open with `---`; the thirty-odd `host/` documents carry no tree expectations.
3a. The harness's detector: `Document::of` (corpus.rs:464) takes the span from
   `lemmon_syntax::front_matter` and parses the YAML inside it, for both kinds of file;
   its own `front_matter` (corpus.rs:522) goes, its YAML tests staying. The `.lem`-by-glob
   typing and the url rule are 1b's (item 10). Rebase across ∆7wK's hunks in this file
   before review.
3b. Docs: `language.md` § Scanner gains the front-matter rule for both modes (the rest of
   item 13 is 1b's).

Verification: `npm run rust`; `npm run corpus:bless`, then read the diff; `npm run
verify`.

## Checkpoint 1b — page templates at compile and load

Built on the branch after 1a landed (`bc2b3ab`). Two departures from the items below:
the store keeps `{path, record}` per page and finds one by a scan, since nothing reads
a page's spans or `record_span` until checkpoint 2's url pass, which adds them with
their consumer; and the corpus's `.lem` cases that render output — the `errors/`,
`schemas/`, and `pages/` templates — were given front matter rather than a warning
line each, the shape Eric chose for the wrapper's fixture (item 20).

### Compiler (`crates/lemmon-vm`)

4. `compile/surfaces.rs`: `ParsedFile::Template` knows its front matter; `record_span`
   falls back to it; the no-front-matter record refusal. `Host::for_file` and its six
   tests and its link caller.
5. `unit.rs`, `bytecode.rs`: `Unit`'s front-matter flag; `Document.body` optional; link
   builds a page source for a document or a template with front matter.
6. `compile/mod.rs`: the `body` read span on `DefaultFn`. `unit.rs`: the page-template
   refusal at link.
6a. `compile/mod.rs` or `items.rs`, wherever the file's entry function is lowered: the
   top-level-output warning, keyed on the host declaring a document type and the unit
   having no front matter.
7. Unit tests: `for_file` for each kind of file; the record refusal on a template with no
   front matter; the `body` refusal at a template, silent at a document of the same
   record, found through an inherited default, not fooled by a lambda parameter named
   `body`, and the perturbation above; the warning at a text run, at a `lem-style`, and
   at a `<lem.x>`, once per file, silent for a file of declarations and whitespace, for
   a top-level `lem-let`, for a page template, and under a host with no document type.

### Loader

8. `load/mod.rs`: `Data.pages`; `check_envelope`'s new refusals and relaxed glob-member
   rule; `Loader` runs `pages` after the bindings; `Store` as settled and its callers;
   `globals` by path.
9. Unit tests: a template glob member loads and binds its own record; a `pages` entry loads
   after the bindings, its `by` index reading a site map; a `pages` entry that fails to
   load; each caller bug; an uncarried page is absent; store order where link order and
   declaration order differ.

### Corpus harness and cases (`tests/corpus.rs`, `tests/corpus/host/`)

10. The harness (already on `lemmon_syntax::front_matter` since 1a) types a `.lem` by
    glob or `type:` only when it has front matter, derives a `.lem` page's url without
    its extension, and sends a page no glob matches, typed by `type:`, in `pages`.
11. `host/environment/site.lem`: `SitePage extends Page` with `title`, `teaser: string?`,
    and a `layout` default naming a `sitePage` function (the host binds `page`, so a
    function of that name is refused at names.rs:424-429), and `<lem-let sections:
    [SitePage] from "sections/*.lem" />`. Bless `environment/site.html` and read it.
12. Cases:
    - `host/sections/index-page.lem`: a title and a teaser holding `<lem.x>`, a body
      reading `page.title` — binds its own record.
    - `host/sections/blank-line-after-front-matter.lem`: the leading newline a file body
      keeps (`compile/whitespace.rs`, the edge rule at 117-123 and 157-158 — ∆1Hp
      rewrote this file's trailing-edge handling, so read it before writing the case),
      as an author sees it.
    - `host/standalone/about.md` and `host/standalone/home.lem`: `type: SitePage`, no glob,
      loaded from `pages`.
    - `host/errors/template-typed-by-a-record-that-reads-body.lem`: `type: Til`, the link
      error with labels at `Til`'s readers of `body`.
    - `host/unmatched/front-matter-template.lem`: front matter, no record — `page`
      unknown-typed, the fixture's value.
    - A `host/sections` case with a load error in its front matter, at the YAML position.
    - `host/warnings/component-with-top-level-style.lem`: a `lem-fn` and a top-level
      `lem-style` after it, as a WebC port would write it — the warning at the style.
13. Docs: the envelope placeholder in `language.md` § Ambient types is swept;
    `tests/corpus/README.md` — whose § Marks in a render and § Fragments in a render
    (65-90) ∆1Hp and ∆7wK rewrote — describes `.lem` pages, `pages`, and `host/sections`
    and `host/standalone`. The corpus host (`declared_host`, corpus.rs:66) claims no
    mark names, so a layout that calls `mark()` warns rather
    than places: leave it that way, or claim a name deliberately.

Verification: `npm run rust`; `npm run corpus:bless`, then read the diff; `npm run
verify`.

## Checkpoint 2 — render through the entry, the url field, `plan()` in the VM

14. `host.rs`: `url`. `unit.rs` link: the url field check and the entry field check, with
    unit tests — missing, mistyped, a declaration-only change caught on relink, no host
    name.
15. `load/mod.rs`: the url pass after every page loads; `Store::plan`. Unit tests as
    settled above.
16. `vm.rs`: `render_page`. Unit tests: the body reaches the layout; `uniqueId` counts from
    body into layout; a layout's error keeps its span; a mark in the body and one in the
    layout arrive in one `Vec<Mark>` in non-decreasing `start` order; a `lem-style` in
    each half lands in one collection, which is the rule `vm::invoke` cannot satisfy; a
    page with no host entry renders its own output; `render` of an uncarried page is
    refused.
17. `tests/corpus.rs`: `declared_host` names `url`; `typed_html` renders a loaded page
    through `render_page`. The `host/` cases that render a loaded page gain their layout's
    output (about eleven more end in a load error and do not render). Add a case whose
    layout and body both call `uniqueId`, and read every blessed line.
18. Docs: the `language.md` placeholder for layouts, urls, and `body` is swept.

Verification: as checkpoint 1a.

## Checkpoint 3 — the boundary, the wrapper, the scripts, the journal

19. `crates/lemmon-wasm/src/wire.rs`: `HostInput.url` (beside `entry` and `marks`, since
    ∆7wK has removed `grammars`), `DataInput.pages`, the planned-page output;
    `PageRef`'s doc comment (wire.rs:411-412) says `index` waits on ∆99b. `src/lib.rs`:
    `plan`, and `render` through `render_page` — one swap at the `render(...)`
    inside `render_bytes` (lib.rs:360-373), since `RenderMeta::placed` and `encode_render`
    already place a mark by its own file — throwing for an uncarried page. Host-target
    tests: plan before a load, after a failed load, and after a compile that dropped the
    store; plan's order across a glob and `pages`; a page renders through its layout; a
    repeated url; the url field check on a declaration-only compile. `tests/boundary.rs`:
    `plan` across the ABI. Doc comments: the `compile` comment at lib.rs:105-118, the
    `render` comment at lib.rs:157-171, and host.rs:8-9, which says a host adds two
    names and now adds three.
20. `packages/core` (∆1Hp added `marks.ts`, `marks.spec.ts`, and a `RenderResult` of bytes,
    types.ts:294, so the coverage budget here is larger than it looks): `boundary.ts`
    `plan`; `types.ts` `HostDecl.url`, `DataInput.pages`, `PlannedPage`, the notes naming
    ∆Bhz (the document value) and `plan()` (`PageRef`, types.ts:183-185; the ∆Bhz note at
    types.ts:144) rewritten; `core.ts` `plan()`, `render` sending only `PageRef` fields, and
    its doc comment at 82-92, which ∆1Hp rewrote around the byte reply. Specs: `plan()`
    through the staged artifact; the expectations at `core.spec.ts:276`, `:289`, and `:338`
    — each now through the `page()` helper at `core.spec.ts:13` — gain their layout;
    `load.spec.ts` for `pages`. Every typed compile that asserts no diagnostics and
    renders a `.lem` with top-level output and no front matter (`page.lem` under
    `typedSite`, core.spec.ts:267) now warns: give the fixture front matter and send it
    in `pages`, which is the honest shape, rather than asserting the warning away.
21. `scripts/wasm-artifact.ts`: `core_plan`. `scripts/render-the-rebound.ts` and
    `scripts/bench-wasm.ts`: confirm the untyped spike renders unchanged; grep `scripts/`
    for other consumers of `render`'s shape.
22. Docs: `architecture.md`'s placeholders naming ∆Bhz; D36, D37, and D41's Status lines
    say what the build changed; `artifact-changes.md` "WASM API, pages" confirmed.
23. Journal, once Eric accepts the work: `npx @limulus/delto@1 complete ∆Bhz
    docs/journal/∆Bhz-pages-render-through-their-layout.md` while the item is still in
    `BACKLOG.md`; fill Planning from this file (link [[∆c9S]] for the split and D43),
    Refinement, Retrospective; then remove ∆Bhz from `BACKLOG.md` and delete this file.

Verification: `npm run rust:wasm`, `npm run verify` (100% coverage on the wrapper), `npm
run rust:wasm:test`, `npm run wasm:check`, `npm run spike:the-rebound`. Report which ran.

## Answered by Eric, 2026-09-17

- **A lost closing `---` is an error in both modes**, at the opening line. Today a `.md`
  whose front matter never closes renders the opening `---` as an `<hr>` and the YAML as
  prose, and the plan had given a `.lem` the same rule. Invariant 10 won; the design point
  under § Front matter says how.
- **Top-level output in a file that is no page warns**, under a host that declares a
  document type. From ∆1Hp's plan: a non-page `.lem` file's top-level output is never
  rendered, and that includes the top-level `lem-style` a WebC port writes. The design
  point under § Which files are pages says how; it lands in 1b with the page-shaped flag.
- **∆4tG gains `; needs: ∆Bhz`.** Its text calls `plan()` and compiles page templates with
  their record, neither of which exists before this item. Done in `BACKLOG.md` in the
  same pass as this note.

## Out of scope, noted for the follow-ons

- ∆99b: `for:`, expansion at load, neighbours and the page table, `index` on `PageRef`,
  and D43's load errors for a generator. "Carried twice" is keyed by path, the url pass
  runs after generators expand, and `render_page` gains the page table.
- ∆4tG: the shell's half — stripping `type:` (and, for ∆99b, `for:`), sending `pages`,
  deriving a `.lem` page's url, walking `plan()`, and what it does with drafts and deleted
  files, which stay compiled in the resident `Core`. Its YAML reader decides which files
  have front matter before compiling (D41), so it must apply exactly
  `lemmon_syntax::front_matter`'s rule, or the shell and the core disagree about which
  files are pages.
- ∆5MM: tree-sitter-lemmon's front-matter rule, held to `.tags` like any boundary.
- ∆xBO: a page render reads its layout's unit, which its deps must name.

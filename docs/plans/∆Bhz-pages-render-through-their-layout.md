# ∆Bhz Pages render through their layout — plan

Written by the session that planned ∆c9S, split it into ∆Bhz and ∆99b, and settled D43
with Eric, and handed on to the session that builds this item. The design points below
were settled with Eric or follow from decisions he made; they are not open unless he
reopens them. Two questions at the end are his to answer. When ∆Bhz's journal entry is
written, its Planning section absorbs this file and the file is deleted.

## Start here

1. Read `CLAUDE.md`, the ∆Bhz item in `BACKLOG.md`,
   `docs/decisions/D43-generated-pages-and-neighbours.md`, and D41.
2. This item needs ∆1Hp, which reshapes `lemmon_vm::render`, the boundary's `render`, the
   corpus harness's `rendered`, and about twenty-five `render` call sites. Every line
   number below was taken before ∆1Hp was built and checked by a read-only review on
   2026-09-14; functions are named so they can be found again. Confirm each before
   editing.
3. `npx @limulus/delto@1 claim ∆Bhz`.
4. If the checkout has no `target/` or `artifacts/wasm/`, the first `npm run rust:wasm`
   and `cargo test` are cold builds.
5. Open checkpoint 1 with a plain-language preview for Eric, then build it.

## Already done

**The split** (∆c9S, 2026-09-14): ∆c9S became ∆Bhz (this item) and ∆99b (generators and
neighbours). D43 is written, with rows and Status clauses on D30, D36, D37, and D41;
`docs/language.md` § Ambient types, `docs/architecture.md` § WASM API, § Build pipeline
and § VM, and `docs/artifact-changes.md` describe the target, with placeholders naming
∆Bhz and ∆99b. `Page` in `language.md` no longer declares `prev`/`next`.

## What exists today, and why this item is larger than its old text

- **No page renders through its layout.** `lemmon_vm::render` (vm.rs:57-82) runs a file's
  entry function with no arguments and returns its output, `.md` documents included.
  `Host::entry` has one consumer, `check_entry_field` (compile/schema.rs:143), which
  checks that a record whose base is the document type declares the field with the shape
  `(Self, html): html`; it runs only in the file that declares such a record, and nothing
  reads the field at render. `packages/core/src/lib/core.spec.ts:303`, `:314` and `:363`,
  and every `host/pages` and `host/tils` case that renders, show a document without its
  layout's `<h1>`.
- **A `.lem` file's front matter is text.** The template scanner has no front-matter rule,
  so `---\ntitle: x\n---` renders literally and a `<lem.` inside it is a tag. Only
  `parse_document` skips the block, with its private `front_matter`
  (lemmon-syntax/src/document.rs:160-176), and the block sits inside the document tree's
  first `TextRun` (the test at document.rs:313 asserts `Text@0..18`). The corpus harness
  has a second, LF-only detector (lemmon-vm/tests/corpus.rs:419-425, tested at 577-610).
- **A page template cannot be loaded.** `Module.documents[i]` (bytecode.rs:176-198) is
  `Some` only for `.md` files, built at link from `Unit.body` (unit.rs:546-552), so
  `check_envelope` (load/mod.rs:219-238) refuses a `.lem` glob member as "no compiled
  document". A file typed by `type:` alone, with no glob, compiles typed and has nowhere
  in `DataInput` (wire.rs:219-226) to send its front matter.
- **`page` in a template is the envelope's value.** `Store` keeps one record per document
  path (load/mod.rs:73-102); anything else binds the value the envelope carried, unchecked.
- **Nothing reports a url.** The harness makes one up (`/{stem}/`, corpus.rs:384-410).

## Working agreements

- Three checkpoints, each leaving `npm run verify` green; checkpoint 3 also runs `npm run
  rust:wasm:test`, `npm run wasm:check`, and `npm run spike:the-rebound`. After each:
  `/code-review medium --fix` on Opus, then stop; Eric reviews and commits. Open each with
  a plain-language preview.
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
  `parse` (template mode) and `parse_document` both call it. The harness's detector and
  its tests go; the harness takes the span and parses the YAML inside it. The tests at
  document.rs:309-327 exercise `parse_document`; the predicate gets tests of its own, and
  those stay where they are with the tree they assert updated.
- **Front matter is a token.** A new `SyntaxKind::FrontMatter` is the first child of `Root`
  in both modes, so the CST stays lossless and a `.md` tree no longer folds the block into
  its first `TextRun` (`Text@0..18` becomes `FrontMatter@0..13` and `Text@13..18`). The
  token section of `SyntaxKind` mirrors `TokenKind` one for one (syntax_kind.rs:9-11, the
  `From` at 202-253), so `FrontMatter` goes after `Error` with a note saying it comes from
  no scanner token. `.tags` projects it as a boundary line, which is what a tree-sitter
  grammar has to agree with (invariant 9, ∆5MM). In template mode `parse` splits the file
  at the block's end, parses the rest, and splices the rest's children after the token,
  shifting diagnostics and their labels as document.rs:78-91 does. Nothing inside the YAML
  is ever scanned. The typed view iterates nodes only (ast/tags.rs:17-19), so it skips the
  token and the compiler emits nothing for it.
- **An unclosed block stays text, as it does in documents.** See the questions for Eric.

### Which files are pages

- **A page is a file compiled with a record that has front matter** — any `.md` document,
  or a `.lem` file that opens with a `---` block. It loads a record, `plan()` lists it,
  and `render` renders it through its entry. A `.md` with no record, or a `.lem` with front
  matter and no record, reads the document value unknown-typed and renders its own output,
  as an unmatched document does now. `Host::for_file` (host.rs:305-319) is keyed on
  `is_document(path)` today; it takes whether the file is page-shaped instead, and so does
  its caller in link (unit.rs:339), which has no source text and reads a flag the unit
  records.
- **A `.lem` file with no front matter takes no record.** It is no page, so a record would
  type `page` while the render binds the envelope's unchecked value (invariant 10). The
  compiler reports it at the start of the file: "a template with no front matter is no
  page, so it takes no record". A `.lem` with no front matter and no record is what it is
  today: `page` has the declared type and the envelope's value.
- **`Module` carries a page source for every page-shaped file.** `Document.body` becomes
  optional — `None` for a page template, which has no prose — and link pushes a page
  source for a template with front matter as well as for every document. `Unit` records
  whether its file has front matter. `record_span` (compile/surfaces.rs:81-91) falls back
  to a template's front matter as it does a document's. Rename `Document` and
  `Module.documents` only if the word misleads and the diff measures small; recheck the
  loader's messages that say "document" either way.

### `body` and a page template

- **Where a default reads `body` is recorded.** The collector a default compiles under has
  no roots for `path` and `body` (compile/mod.rs:612 opens it with empty `params`), and
  the resolver's `used` flag is a bool that `Implicit` bindings are exempt from, so neither
  gives the span. Record it where a name resolves to `Resolved::Local { binder }` and the
  binder is `body`'s — the one `compile_step` makes at `Span::new(span.end, span.end)`
  (compile/mod.rs:585) — matching the binder, not the name, since a lambda parameter may
  be called `body`. Keep the first such span on `DefaultFn` (unit.rs:112).
- **Link refuses a page template whose record's plan runs such a default.** For each unit
  that is a page template with a record, walk that record's plan — every field's default
  from the nearest declaration that gives one, inherited ones included, so not through
  `record_plans`' `refuse` gate (unit.rs:688), which skips them — and report one error at
  the template's record span: "`Til` cannot type a page template: its defaults read
  `body`, and a template has no body until it renders", labelled at each reading default.
  A document of the same record is not refused; both halves are tested.
- **The loader passes `Value::Null` for a page template's `body`.** Step results are not
  revalidated (validate.rs:329-335), so the link refusal is the only thing keeping a
  default such as `excerpt: html = body` from storing a null. Its negative test is a
  perturbation that shows that harm: disable the refusal, and watch the record carry null.

### Loading

- **The envelope gains `pages`.** `Data.pages: Vec<Loaded>` (wire `DataInput.pages`, TS
  `DataInput.pages?: LoadedValue[]`) carries the front matter of every page no glob holds,
  path required. A glob member may now be a page template. The loader runs `pages` after
  every bindings plan, in envelope order, through the path `Loader::document`
  (load/mod.rs:482-519) takes, minus the element check.
- **What `check_envelope` refuses.** As caller bugs, after the refusals it makes today (so
  `refuses_sources_in_the_wrong_shape`, load/mod.rs:822-889, still passes): a `pages`
  entry with no path; one naming a file compiled as no page, or with no record; a page
  carried twice, in a glob and in `pages` or twice in either.
- **A page the envelope does not carry is not loaded.** It is not an error: the resident
  `Core` cannot remove a unit (the boundary only ever inserts), so a page the shell
  deleted, renamed, or left out as a draft (D19) stays compiled, and refusing it would
  make every later load throw. `plan()` omits it, and `render` of it throws a caller bug
  — "`x` is a page the last load did not carry" — since binding the envelope's value would
  type `page` as a record it is not. `skips_a_computed_binding_once_an_earlier_value_failed`
  (load/mod.rs:1146-1166), which compiles `pages/a.md` and loads `posts: []`, keeps
  passing.
- **The store keeps pages in load order.** `Store.documents: BTreeMap<String, Value>`
  becomes an ordered list of loaded pages, each `{path, record, spans, record_span}`, with
  an index by path: collection members in the order the loader builds the collections —
  its steps' link order (unit.rs:739-747), which D43 and `architecture.md` call binding
  order and which can differ from declaration order — then `pages` in envelope order. A
  test holds the order where the two differ. `Store::globals` finds a page by path; ∆99b
  keys it by `{path, index}`. Callers to update: validate.rs:559, 1173, 1240, 1425;
  load/mod.rs:783-815 and 972; bytecode.rs:356-379 builds a `Document` literally.

### The url field

- **The declaration names it** (D43). `Host` gains `url: Option<Rc<str>>` beside `entry`
  (wire `HostInput.url`, TS `HostDecl.url?`), installed as `entry` is.
- **It is checked at link, and so is the entry field.** The document type is declared in a
  host file with no base, which `check_declarations` skips (schema.rs:274-279), and
  neither name is a fact of any unit, so a compile that only changes the declaration
  relinks without recompiling (lemmon-wasm/src/lib.rs:228-262) and a compile-time check
  would never see it. Link has the index and the host on every relink: it reports a
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
  so ∆99b adds the page table without changing it again and the `render` call sites ∆1Hp
  touches stay as they are. It captures the file's entry function's output natively,
  converting the buffer exactly as ∆1Hp's `Op::EndCapture` does — plain `Html`, or
  `Marked` when the buffer holds marks — so marks in the body are not dropped. It then
  calls the record's entry field with the record and that body inside the same `Vm`, not
  through `vm::invoke` (which collects no fragments, so `Collect` would fault) or
  `Machine::invoke` (which counts `MAX_NATIVE_DEPTH`), and emits what it returns into the
  root buffer. So `uniqueId` counts on from the body into the layout, and the fragments of
  both land in one collection. The capture adds no `Op`: the every-Op gate reads only the
  `render` group (corpus.rs:620-640), where no page is rendered. A call that fails before
  running has no span (vm.rs:236-241); map it to the page's `record_span`. A layout's
  runtime error keeps the layout's span. A file that is no page renders as today, so
  `render/`, `document/`, and the spike are unchanged; a page with no host entry renders
  its own output, bound to its record.
- **`plan()` lists loaded pages.** `Store::plan(&Host) -> Vec<PlannedPage>` (not `Planned`,
  which `unit.rs:634` already names), each `{path, url?}` in store order, `url` the
  field's text when the host names one. At the boundary `Core::plan()` returns that JSON;
  a call before any load has succeeded, or after a compile dropped the store, throws, as
  `render` before a compile does; after a failed load it lists the stale store.
  `core_plan` joins `EXPECTED_EXPORTS` (scripts/wasm-artifact.ts:32). In TS, `plan():
  PlannedPage[]` with `interface PlannedPage extends PageRef { url?: string }`, and `render`
  sends only `PageRef`'s fields, since the wire's `PageRef` denies unknown fields.

## Checkpoint 1 — front matter, and page templates at load

### Syntax (`crates/lemmon-syntax`)

1. `front_matter` public, with tests of its own; `parse_document` emits the token and
   starts its first text run after it; `SyntaxKind::FrontMatter` after `Error`;
   `parse`'s split and splice; `parser/dump.rs` `is_boundary` gains the kind.
2. Tests: spans after the block are file-absolute; a diagnostic and its labels after it are
   shifted; a scanner error right at the splice boundary; `<lem.x>` inside the YAML is no
   tag; CRLF; unclosed is text and its tags parse; a `---` block not at the start is text;
   `--- ` with a trailing space and a blank line before `---` are no front matter.
3. Corpus: `syntax/front-matter/page-template.lem` (YAML holding `<lem.not-a-tag>` and a
   `>`, then markup and a tag), `syntax/front-matter/unclosed.lem`. Every
   `document/**/*.cst` and `.tags` with front matter changes: bless and read.

### Compiler (`crates/lemmon-vm`)

4. `compile/surfaces.rs`: `ParsedFile::Template` knows its front matter; `record_span`
   falls back to it; the no-front-matter record refusal. `Host::for_file` and its six
   tests (host.rs:490-527) and its link caller.
5. `unit.rs`, `bytecode.rs`: `Unit`'s front-matter flag; `Document.body` optional; link
   builds a page source for a document or a template with front matter.
6. `compile/mod.rs`: the `body` read span on `DefaultFn`. `unit.rs`: the page-template
   refusal at link.
7. Unit tests: `for_file` for each kind of file; the record refusal on a template with no
   front matter; the `body` refusal at a template, silent at a document of the same
   record, found through an inherited default, not fooled by a lambda parameter named
   `body`, and the perturbation above.

### Loader

8. `load/mod.rs`: `Data.pages`; `check_envelope`'s new refusals and relaxed glob-member
   rule; `Loader` runs `pages` after the bindings; `Store` as settled and its callers;
   `globals` by path.
9. Unit tests: a template glob member loads and binds its own record; a `pages` entry loads
   after the bindings, its `by` index reading a site map; a `pages` entry that fails to
   load; each caller bug; an uncarried page is absent; store order where link order and
   declaration order differ.

### Corpus harness and cases (`tests/corpus.rs`, `tests/corpus/host/`)

10. The harness takes front matter from `lemmon_syntax::front_matter` for both kinds of
    file, types a `.lem` by glob or `type:` only when it has front matter, derives a
    `.lem` page's url without its extension, and sends a page no glob matches, typed by
    `type:`, in `pages`.
11. `host/environment/site.lem`: `SitePage extends Page` with `title`, `teaser: string?`,
    and a `layout` default naming a `sitePage` function (the host binds `page`, so a
    function of that name is refused at names.rs:424-429), and `<lem-let sections:
    [SitePage] from "sections/*.lem" />`. Bless `environment/site.html` and read it.
12. Cases:
    - `host/sections/index-page.lem`: a title and a teaser holding `<lem.x>`, a body
      reading `page.title` — binds its own record.
    - `host/sections/blank-line-after-front-matter.lem`: the leading newline a file body
      keeps (whitespace.rs:36-40), as an author would see it.
    - `host/standalone/about.md` and `host/standalone/home.lem`: `type: SitePage`, no glob,
      loaded from `pages`.
    - `host/errors/template-typed-by-a-record-that-reads-body.lem`: `type: Til`, the link
      error with labels at `Til`'s readers of `body`.
    - `host/unmatched/front-matter-template.lem`: front matter, no record — `page`
      unknown-typed, the fixture's value.
    - A `host/sections` case with a load error in its front matter, at the YAML position.
13. Docs: `language.md` § Scanner gains the front-matter rule for both modes; the envelope
    placeholder in § Ambient types is swept; `tests/corpus/README.md` describes `.lem`
    pages, `pages`, and `host/sections` and `host/standalone`.

Verification: `npm run rust`; `npm run corpus:bless`, then read the diff; `npm run
verify`.

## Checkpoint 2 — render through the entry, the url field, `plan()` in the VM

14. `host.rs`: `url`. `unit.rs` link: the url field check and the entry field check, with
    unit tests — missing, mistyped, a declaration-only change caught on relink, no host
    name.
15. `load/mod.rs`: the url pass after every page loads; `Store::plan`. Unit tests as
    settled above.
16. `vm.rs`: `render_page`. Unit tests: the body reaches the layout; `uniqueId` counts from
    body into layout; a layout's error keeps its span; a mark in the body lands at its
    shifted place and a fragment from each half lands in one collection; a page with no
    host entry renders its own output; `render` of an uncarried page is refused.
17. `tests/corpus.rs`: `declared_host` names `url`; `typed_html` renders a loaded page
    through `render_page`. The `host/` cases that render a loaded page gain their layout's
    output (about eleven more end in a load error and do not render). Add a case whose
    layout and body both call `uniqueId`, and read every blessed line.
18. Docs: the `language.md` placeholder for layouts, urls, and `body` is swept.

Verification: as checkpoint 1.

## Checkpoint 3 — the boundary, the wrapper, the scripts, the journal

19. `crates/lemmon-wasm/src/wire.rs`: `HostInput.url`, `DataInput.pages`, the planned-page
    output; `PageRef`'s doc comment (wire.rs:365-366) says `index` waits on ∆99b. `src/
    lib.rs`: `plan`, and `render` through `render_page`, throwing for an uncarried page.
    Host-target tests: plan before a load, after a failed load, and after a compile that
    dropped the store; plan's order across a glob and `pages`; a page renders through its
    layout; a repeated url; the url field check on a declaration-only compile.
    `tests/boundary.rs`: `plan` across the ABI. Doc comments at wire.rs:123, lib.rs:112 and
    153-158, host.rs:8-9.
20. `packages/core`: `boundary.ts` `plan`; `types.ts` `HostDecl.url`, `DataInput.pages`,
    `PlannedPage`, the notes naming ∆Bhz (the document value) and `plan()` (`PageRef`,
    types.ts:135-136) rewritten; `core.ts` `plan()`, `render` sending only `PageRef`
    fields, and its doc comment at 73. Specs: `plan()` through the staged artifact; the
    expectations at `core.spec.ts:303`, `:314`, and `:363` gain their layout;
    `load.spec.ts` for `pages`.
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

## Questions for Eric, before checkpoint 1

- **A lost closing `---`.** Today a `.md` whose front matter has no closing line renders the
  opening `---` as an `<hr>` and the YAML as prose; this plan gives a `.lem` the same rule,
  so its YAML renders as text. Invariant 10 argues for an error in both modes, at the
  opening line. Changing it is small but moves `.md` behaviour, so it waits for his call.
- **Output from a file that is no page.** From ∆1Hp's plan: a non-page `.lem` file's
  top-level output is never rendered, and that includes a top-level `lem-style`, which is
  where a WebC port puts one (all nineteen of limulus.net's `<style>` blocks sit at their
  file's top level). Once a page is a known thing, top-level output other than whitespace
  in a file that is no page could be a warning at the first such item. Not built unless he
  asks for it.

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

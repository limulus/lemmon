# Decisions

Normative log. Each row is a settled choice: treat it as fixed unless Eric reopens it, and
cite the ID — in PRs, docs, and code comments. A row states *what* was chosen in a sentence
or two; it carries no rationale, no rejected alternatives, and no dates. Where the *why*
lives depends on when the decision was made:

- **D01–D24** were settled in the Aug 2026 architecture sessions; their rationale is in the
  three-artifact set ("Shell & Core", "The Lemmon Language", "From Eleventy to Lemmon"),
  owned by Eric.
- **D25 onward** were settled during implementation, and each has a record in this
  directory — `Dnn-slug.md`, linked from its ID — with context, the decision, alternatives
  considered, and consequences. The exact rule lives in `docs/language.md` or
  `docs/architecture.md`; a record cites the section rather than restating it.

Adding a decision: take the next ID, add a row, write the record. Revising one: update the
row and add a clause to the record's Status line. Never renumber — IDs are cited across the
docs, the journal, and the code.

| ID | Decision | Choice |
|---|---|---|
| D01 | Control-flow syntax | Dedicated prefixed tags; the tag head is an expression, not HTML attributes. |
| D02 | What is parsed | Only Lemmon tags. HTML is opaque text; no DOM, ever. |
| D03 | Interpolation | `<lem.expr>` — always void, never slashed; escaped by default; `raw()` is the only string→html cast. |
| D04 | Components | Typed functions returning markup, called like any function via `lem.`. |
| D05 | Type checking | Static, schema-first, declared in the language. JSON Schema is derived output (`lem schema export`), never the source. |
| D06 | Build-time logic | Maximal template language; no JS escape hatch at build time. Plugins cannot inject logic into the language or the core; what the shell does with a page after the core returns it — an output transform, a mark handler (D42) — is the shell's. |
| D07 | Components in markdown | Segmented documents: block-level lem tags split out before markdown, rendered by the VM, spliced via `<!--lem:N-->` HTML-comment sentinels. Block-level only in v1. A block is a template through its close, body included: markdown stops at the opening tag (D39). |
| D08 | Markdown engine | comrak built into the core (footnotes, heading ids, `{#id}` attrs, `render.unsafe`). Shell-side `markdown` override hook reserved, not built. A code block is a `code` mark the host's handlers resolve; the core embeds no highlighter (D42, superseding D40). |
| D09 | Client JS bundling | esbuild via its JS API; limulus.net's entry conventions (`**/index.ts`, `**/worker.ts`, `/lib/*` external, wasm loader) port 1:1. |
| D10 | Asset references | `asset("…")`/`[asset]` fields, compile-checked, in templates and front matter. Client TS keeps the `XXXXXXXX` hash-placeholder convention, resolved by the esbuild plugin. |
| D11 | Runtime split | Node/TS shell + Rust core compiled to WASM, npm-distributed. WASM-first; napi-rs native is an optional later perf path behind the same TS interface. |
| D12 | Capabilities as plugins | Photos, revisions, svg-sprite, dprint = TS plugins. Video prep stays in media-tools (`vod.id` convention). Media bytes stay on bunny.net. |
| D13 | Build artifact | Files on disk, uploadable anywhere. limulus.net's hash + `/assets/immutable/` conventions preserved so CloudFront config is untouched. |
| D14 | On-demand / DB | Headroom only, via the shell's `ContentSource` seam + serializable modules + record-level `upsertRecord`. Nothing request-time is built now. |
| D15 | Migration posture | Parity harness (old dist vs new dist, normalized, allowlisted) then a single cutover. No dual-serving. 11ty stays green on main until 0 unexpected diffs. |
| D16 | Editor tooling | Day one: `tree-sitter-lemmon` (highlighting + injections) and `@limulus/lemmon-lsp` (Node host over the same WASM core). Corpus agreement between the two grammars is CI-enforced. |
| D17 | Audience | Open source, usable by others. Public surfaces: the language, the plugin API, the WASM API. |
| D18 | Name & prefix | **Lemmon**; tag prefix `lem`; files `.lem`; CLI `lem`. Registry notes: crates.io `lemmon` free; npm unscoped `lemmon` squatted (dormant); npm packages publish under the existing `@limulus` scope as `@limulus/lemmon-*` (2026-08-16, superseding the unverified `@lemmon` scope idea); github.com/lemmon is an unrelated user → org needs a variant. |
| D19 | Drafts & scheduling | `draft: bool` + `--drafts` dev flag; `draft` is a field of the shell's `Page` declaration, not a reserved key (D30, 2026-08-28). No scheduled publishing in static mode (falls out of on-demand later). |
| D20 | CSS scoping | Convention, no rewriting: component root carries a class named after the function; styles nest under it with native CSS nesting. `lem-style` blocks → per-page hashed bundle, placed by `mark("styles")` (D42; `lem-emit` retired 2026-09-13, unbuilt). |
| D21 | Tag shapes | Kebab `lem-*` builtins (valid custom-element names; namespace reserved — unknown `lem-*` is a compile error). `lem.` accesses (always void); keyword heads with `= expr` are void, as is a `lem-let` head with `from` past its name (D30); heads with neither are blocks; voids canonically ` />`. All blocks close bare (`</lem-fn>`; the named close `</lem-fn.foo>` was dropped 2026-08-20). Heads never contain `>`: `lt lte gt gte`, `\|a\| expr`, `: T`. |
| D22 | Calls & slots | `<lem.f(args)>` is the only call form. Bodies pass capture-then-call (`<lem-let x>…</lem-let>` → `slot: x`). No call-with-body syntax; grammar stays head-decidable. One-shot sugar would be additive later. |
| D23 | Namespaces | One lexical value namespace: locals/params → project-flat (all site `.lem` files; duplicates = compile error; no site-file imports) → ambient (the values the host names — `site`/`page`/`build` in the shell, D30 — plus stdlib; shadowing stdlib warns). Plugins bind qualified via `lem-import` in `site.lem`, named promotion optional. Types are a namespace of their own, project-flat under the same duplicate rule; capitalization tells them apart by convention (D32). |
| D24 | Trailing commas | One optional trailing comma is allowed in every comma-separated list — `[a, b,]`, `{k: v,}`, `f(a, b,)`, `lem-fn f(a: int,)`, `\|a, b,\| …` — so multi-line literals and signatures stay one-line-per-item and diff-friendly, as in JS and Rust. Empty slots (`[a,,b]`, `f(,)`) remain errors (2026-08-22). |
| [D25](D25-separate-compilation.md) | Separate compilation | Each `.lem`/`.md` compiles to its own **unit**; a cheap link step joins units into the `Module` the VM runs. A unit is a function of the file's text plus the compile-time facts it consumed, which the compiler reports; no file's bytecode depends on another file's body (invariant 11). Reference-index ids are not a fact: a string literal in a template is a string, and a record named by id is read from its map at render (∆Hbw). |
| [D26](D26-body-whitespace.md) | Body whitespace | `lem-fn` block bodies and `lem-let` capture bodies are normalized at compile time: one newline trimmed at each edge, then dedented by the margin the lines share. A body hugging its own tag's line keeps its margin; flow bodies and interpolated values are never touched; no element awareness. |
| [D27](D27-statement-lines.md) | Statement lines | A `lem-let`/`lem-fn` tag, void or block, alone on its line takes the line with it. `<lem.…>`, `lem-if` and `lem-for` keep theirs. |
| [D28](D28-whitespace-is-a-source-concern.md) | Whitespace is a source concern | Lemmon optimizes how a `.lem` file reads, not how its HTML reads. The renderer never re-indents at a boundary, and whitespace HTML renders survives byte for byte; formatting the output is a downstream tool's job. Fidelity gates compare rendered HTML, not bytes. |
| [D29](D29-type-bodies.md) | Type bodies | A `lem-type` block body is scanned in *fields mode* — one `name: Type [by …] [= default]` per line, line ending as a token, until the first `<`. `extends` and `by` are contextual keywords. |
| [D30](D30-ambient-types.md) | Ambient types | The core knows no type or value by name: a host declares its ambient values and their types in Lemmon source compiled with the program. The shell ships `Page` and `Build`, binds `site` to `site.lem`'s top-level `lem-let … from …` bindings, and reads two names of its own, the `type:` key and the `layout` field; D43 adds the `for:` key, and its declaration names the url and neighbour fields. Content types `extends Page`; a document's type is that of the collection whose glob matched it, or a subtype a `type:` key names; a document no glob matches and no `type:` names is unknown-typed and renders. `lem-collection`/`lem-data` are retired; there is no `Layout`, `permalink`, or `Page.data`. A template's page fields are its front matter (D41). Environment values are a `from env("…")` source, the same shape as `plugin("…")` (∆1vS). |
| [D31](D31-check-while-lowering.md) | Check while lowering | The type checker is not a pass: the compiler's one walk types each expression as it lowers it (`expr(&Expr, expected) -> Type`), bindings carry their types in the one resolver, and narrowing is a scoped entry. No side table until something reads one (the LSP's hover, ∆5MM). |
| [D32](D32-records-and-enums.md) | Records and enums | Declared types are nominal, by name: one `Type::Named`, with the kind and shape in the index's type namespace, and the rules that need them take the declaration table as an argument. A subtype redeclares a field only with the same type (to change its default), which answers D30's `layout` question with no special rule. A variant carries a dot: `.hero` where the enum is expected, `Placement.hero` where nothing says which — so a bare name is only ever a name, and `Name.x` is the one place the type namespace answers first. An enum is its variant's name at run time and prints as it (provisional). A consumed declaration is a unit's compile-time fact (D25). |
| [D33](D33-diagnostic-positions.md) | Diagnostic positions | A span crosses the boundary as byte offsets *and* `range`, its two ends as LSP `Position`s — zero-based, `character` in UTF-16 code units — computed in the core, against the text that produced the unit the span belongs to. An offset past the end of a file or inside a character is a panic, not a rounded-off position. Byte spans stay in the `.cst` tree and all of `.tags`; the corpus trailers render `line:col` counting from one. |
| [D35](D35-runtime-type-tags.md) | Runtime type tags and narrowing | A record value carries its declared name, and the linked module the declaration table its ancestry is walked in; a map literal where a record is expected is that record, defaults not filled; a type test reads the tag, is `false` on `null`, and is a render error on anything else; narrowing is by path (a local — or, since D36, an ambient value — with fields off it), ending at a shadow, an operator, or `?.`; `lem-match` covers an enum (plus `null` for `T?`) or needs a default, on the provisional reading. |
| [D34](D34-builtin-methods.md) | Builtin methods | Every builtin type carries its own functions, reached as `xs.filter(…)` and resolved against the receiver's type, so `.` asks the receiver and never the enclosing scope. UFCS is retired; a record is data and takes `lem-fn`s. A stdlib entry whose first parameter is the value it works on is a method, one that constructs or casts is an ambient function (`raw()`, `asset()`, `url()`, `datetime()`, `uniqueId()`); method names occupy no namespace (revising D23). A builtin rule may shape the arguments after it, which is what types a lambda from the receiver's element type. |
| [D36](D36-host-declarations.md) | Host declarations | The host's declaration is `{ambient, document?, entry?}`, plus the url and neighbour fields since D43: each ambient name bound to a declared type, to a file's top-level bindings, or to nothing (named, untyped). It is resident at the boundary and a fact of every unit that read it, so rebinding a name leaves those units stale by name. `site`'s fields are a declaration synthesized from the bindings file and named by its path. Every surface one file offers another is read from heads alone — a `lem-fn`'s signature, a `lem-type`'s fields, and now the bindings file's top-level `lem-let`s, which declare their types — never inferred from a body (invariant 11). A file's `record` types the document value in it and is checked as a declared record extending that value's type; tagging loaded data stays with ∆iCW (revising D35). |
| [D37](D37-schemas-at-load.md) | Schemas at load | A field's default and the bindings file's computed bindings compile to functions of their own unit with the fields they read recorded, closed at link through the `lem-fn`s they call by name (∆HF0); a `by` index names one field of one ambient value, `by site.photos`, a pair the head declares and the loader indexes as loaded (∆eag); the link step orders each record's steps and the bindings — a `from` binding after every sibling the records it may hold index — and refuses a cycle. A default sees `self` as the declaring type, plus `path` and `body` on a page-backed record, and no ambient value, directly or through a call (∆eag). A `from` binding stands only in the bindings file, its source a string literal, `plugin("…")`, or `env("…")`; a sibling is read by its bare name there. The loader validates loaded data by declared type, tags records, fills omitted optionals, resolves a string by its expected type (enum variant, project function, `by` index), and rejects extra keys with no opt-out; positions come from the host as complete boundary spans keyed by field path; `render` binds the document value from the store. |
| [D38](D38-stdlib-values.md) | Stdlib values and the manifest at compile | `datetime` is an instant plus the offset it was written in, printing RFC 3339 in it and comparing by the instant, with no time-zone database and every refusal worded by the core; `duration` is whole seconds with no printed form; `url` is text percent-encoded once, idempotently, with `text()` to compose from; `asset` resolves against a manifest that rides on the **host declaration at `compile`** (correcting ∆d1N), its values finished URLs since the core does no path arithmetic — `asset("…")` takes a literal, folds to a constant, and records the entry as a compile-time fact the link step compares (D25, invariant 11), while `[asset]` fields resolve at load against the same map and share one refusal. `slugify` folds Latin marks rather than transliterating, carrying through a script it cannot fold; a character is a grapheme cluster throughout (UAX #29); `uniqueId(prefix)` counts per prefix per render and forgets between renders. No datetime arithmetic. |
| [D39](D39-markdown-stops-at-a-block.md) | Markdown stops at a block | In a document, a block is a template from its opening tag to its close, body included, and emits HTML; markdown, link definitions, footnotes, and fences stop at the opening tag (revising D07). Prose inside an element is written as raw HTML with a blank line on each side, and a block between those blank lines splices as anywhere else. A capture whose body is markdown was considered and deferred; both forms are additive. |
| [D40](D40-code-highlighting.md) | Code highlighting takes its grammars from the host | **Superseded by D42**, which takes highlighting out of the core. As accepted: the core embeds no grammars: a host declares them as `.sublime-syntax` text on the declaration, none declared meaning no highlighting, and a document whose code names a language records a digest of the set as a compile-time fact. A grammar is parsed the first time a block names its language; every lookup resolves against the declared order, the last declared winning, so output never depends on what an instance loaded before. The core writes comrak's `<pre><code class="language-x">` unchanged and owns only its inside, spans classed with scope atoms prefixed `syn-`; an unknown language, or a grammar that fails, leaves the block as comrak writes it, with a warning at the word. No theme is part of the core. |
| [D41](D41-pages-from-front-matter.md) | Pages from front matter | A `.lem` file is a page when it opens with front matter, typed as a document is, and its output is the body its layout receives; `lem-page` is retired. A generator is a page template whose front matter names an ambient value's list and the field each item lands in, one page per item (the `for:` key, D43); the list's filtering is a computed binding and the page's other fields are its type's defaults. |
| [D42](D42-marks-resolve-in-the-host.md) | Marks resolve in the host | A mark is a named byte range in the rendered page with its default content in place, recorded with an optional payload and a span; `render` returns the page as bytes beside its marks and the fragments the collect tags gathered, and the host's handlers replace the ranges before any output transform. A code block is a `code` mark keyed by its language word, its payload reported at compile by content hash; `mark(name, payload?, default?)` places one from a template, its literal name and key checked against the claims the host declares — producers with their keys, decorators with or without — and recorded as a compile-time fact; two producers on one key is a load error. A rendered body reaching a template is a string of finished output, rendered in a second phase; a mark inside a fragment, a payload, or a default is a render error. `lem-emit` is retired. |
| [D43](D43-generated-pages-and-neighbours.md) | Generated pages and their neighbours | A generator's front matter carries `for:` with one entry: a field its page type declares, and the `<ambient value>.<field>` list that fills it. The shell reads and strips the key like `type:`, the loader checks it, and the pages are built after the bindings, so no collection holds them. The host declaration names the url field; `plan()` reports each loaded page's reference, `{path, index?}`, with its url, and two pages at one url is a load error. It also names two neighbour fields, which a generated page's type declares as optionals of itself or an ancestor; the loader fills them in list order with references a field read resolves, and `Page` declares no `prev`/`next` (revising D30, D36). A record whose defaults read `body` cannot type a page template. |

## Deliberately deferred

Do not decide these ad hoc; use the provisional readings in `docs/language.md` and flag
in the PR when one is load-bearing:

- String interpolation delimiter: `{expr}` (provisional) vs `${expr}`.
- Number model: `int` + `float` (provisional) vs single `number`.
- Closed-union syntax; exact `lem-match` exhaustiveness rules. (How a variant is
  spelled is settled — D32, revised by ∆EN6.)
- Operator spelling consistency: `==`/`&&`/`||` (provisional) vs `eq`/`and`/`or`.
- Inline lem tags in prose: reserved for v2; document-mode design must stay compatible.
- Markdown inside a block: settled narrow — a block's body is a template (D39).
  Whether a capture may ever hold markdown is open; D39's alternatives record the two
  forms, both additive.
- Raw text in templates: a `: string` capture whose body is scanned as text, and
  call-with-body sugar (`lem-with`/`lem-raw`) lowering to capture-then-call. Both are
  additive and are taken up together once the port shows whether the two-tag idiom
  hurts (∆M5x); until then a multi-line source is a fence in a document (D42).

## Chronology

D01–D17, D19 settled 2026-08-14 (design interview); D08 revisited and D20–D21 settled
2026-08-15; named Lemmon with D18 closed, D21 finalized, D22–D23 added 2026-08-16; D18
npm scope fixed to `@limulus` later on 2026-08-16; D21 revised 2026-08-20 to close `lem-fn`
bare like every other block, to drop the `<lem-- -->` comment tag, and to drop the datetime
literal in favour of a `datetime("…")` stdlib constructor. D24 added 2026-08-22; D25
2026-08-23; D26 2026-08-23, implemented 2026-08-24 (∆2vO); D27 2026-08-24 (∆2vO); D28
2026-08-24, applied 2026-08-25 (∆Ma4); D29 2026-08-26 (∆up2); D30 2026-08-28,
revised the same day from well-known types to host-declared ambient types (revising D19,
D23 and D29 with it), its `from` clause parsed 2026-09-04 (∆1vS); D31 2026-09-06 (∆C8d);
D32 2026-09-06 (∆cea), its variant spelling revised 2026-09-07 (∆EN6); D33
2026-09-07 (∆cE6); D34 2026-09-07 (∆495), retiring the pipe operator from the
deferred list with it, its higher-order half the same day (∆987); D35 2026-09-07 (∆PCK),
revised the same day (∆Iau) for ambient-rooted narrowing and to leave tagging to the
loader; D36 2026-09-07 (∆Iau); D37 2026-09-08 (∆iCW), its recorded reads closed
through calls at link 2026-09-09 (∆HF0), `by` narrowed to a declared field and defaults
to the record 2026-09-10 (∆eag); D38 2026-09-11 (∆Q6L), implemented 2026-09-12 with
transliteration measured and cut from `slugify` on the way; D39 2026-09-12 (∆Hbw),
revising D07, with D25 revised the same day — reference-index ids are not a fact; D40
2026-09-13 (∆BfR), refining D08; D41 2026-09-13 (∆RGQ), revising D30, retiring `lem-page`
for front matter on templates; D42 2026-09-13, settled while planning ∆1Hp, superseding
D40 and retiring `lem-emit` before it was built; D43 2026-09-14, settled while planning
∆c9S, revising D30 and D36. From D25 on, a record's Status line carries the history
its row used to.

## Record template

```markdown
# Dnn — Title

- **Status:** Accepted YYYY-MM-DD; revised YYYY-MM-DD (∆xxx) to …
- **Backlog item:** ∆xxx
- **Normative text:** `docs/language.md` § Section

## Context
## Decision
## Alternatives considered
## Consequences
```

Context is what made the question come up. Decision is the choice in the form that
carries its reasoning, citing the normative section rather than restating the rule. Each
alternative opens bold and says why it was rejected. Consequences are what became easier or
harder, including hand-offs to backlog items.

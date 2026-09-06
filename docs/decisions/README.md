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
| D06 | Build-time logic | Maximal template language; no JS escape hatch at build time. Plugins cannot inject logic. |
| D07 | Components in markdown | Segmented documents: block-level lem tags split out before markdown, rendered by the VM, spliced via `<!--lem:N-->` HTML-comment sentinels. Block-level only in v1. |
| D08 | Markdown engine | comrak built into the core (footnotes, heading ids, `{#id}` attrs, syntect class-based highlighting, `render.unsafe`). Shell-side `markdown` override hook reserved, not built. |
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
| D20 | CSS scoping | Convention, no rewriting: component root carries a class named after the function; styles nest under it with native CSS nesting. `lem-style` blocks → per-page hashed bundle via `lem-emit styles`. |
| D21 | Tag shapes | Kebab `lem-*` builtins (valid custom-element names; namespace reserved — unknown `lem-*` is a compile error). `lem.` accesses (always void); keyword heads with `= expr` are void, as is a `lem-let` head with `from` past its name (D30); heads with neither are blocks; voids canonically ` />`. All blocks close bare (`</lem-fn>`; the named close `</lem-fn.foo>` was dropped 2026-08-20). Heads never contain `>`: `lt lte gt gte`, `\|a\| expr`, `: T`. |
| D22 | Calls & slots | `<lem.f(args)>` is the only call form. Bodies pass capture-then-call (`<lem-let x>…</lem-let>` → `slot: x`). No call-with-body syntax; grammar stays head-decidable. One-shot sugar would be additive later. |
| D23 | Namespaces | One lexical value namespace: locals/params → project-flat (all site `.lem` files; duplicates = compile error; no site-file imports) → ambient (the values the host names — `site`/`page`/`build` in the shell, D30 — plus stdlib; shadowing stdlib warns). Plugins bind qualified via `lem-import` in `site.lem`, named promotion optional. Types split from values by capitalization. |
| D24 | Trailing commas | One optional trailing comma is allowed in every comma-separated list — `[a, b,]`, `{k: v,}`, `f(a, b,)`, `lem-fn f(a: int,)`, `\|a, b,\| …` — so multi-line literals and signatures stay one-line-per-item and diff-friendly, as in JS and Rust. Empty slots (`[a,,b]`, `f(,)`) remain errors (2026-08-22). |
| [D25](D25-separate-compilation.md) | Separate compilation | Each `.lem`/`.md` compiles to its own **unit**; a cheap link step joins units into the `Module` the VM runs. A unit is a function of the file's text plus the compile-time facts it consumed, which the compiler reports; no file's bytecode depends on another file's body (invariant 11). |
| [D26](D26-body-whitespace.md) | Body whitespace | `lem-fn` block bodies and `lem-let` capture bodies are normalized at compile time: one newline trimmed at each edge, then dedented by the margin the lines share. A body hugging its own tag's line keeps its margin; flow bodies and interpolated values are never touched; no element awareness. |
| [D27](D27-statement-lines.md) | Statement lines | A `lem-let`/`lem-fn` tag, void or block, alone on its line takes the line with it. `<lem.…>`, `lem-if` and `lem-for` keep theirs. |
| [D28](D28-whitespace-is-a-source-concern.md) | Whitespace is a source concern | Lemmon optimizes how a `.lem` file reads, not how its HTML reads. The renderer never re-indents at a boundary, and whitespace HTML renders survives byte for byte; formatting the output is a downstream tool's job. Fidelity gates compare rendered HTML, not bytes. |
| [D29](D29-type-bodies.md) | Type bodies | A `lem-type` block body is scanned in *fields mode* — one `name: Type [by …] [= default]` per line, line ending as a token, until the first `<`. `extends` and `by` are contextual keywords. |
| [D30](D30-ambient-types.md) | Ambient types | The core knows no type or value by name: a host declares its ambient values and their types in Lemmon source compiled with the program. The shell ships `Page` and `Build`, binds `site` to `site.lem`'s top-level `lem-let … from …` bindings, and reads two names of its own — the `type:` key and the `layout` field. Content types `extends Page`; a document's type is that of the collection whose glob matched it, or a subtype a `type:` key names; a document no glob matches is unknown-typed and renders. `lem-collection`/`lem-data` are retired; there is no `Layout`, `permalink`, or `Page.data`. Environment values are a `from env("…")` source, the same shape as `plugin("…")` (∆1vS). |
| [D31](D31-check-while-lowering.md) | Check while lowering | The type checker is not a pass: the compiler's one walk types each expression as it lowers it (`expr(&Expr, expected) -> Type`), bindings carry their types in the one resolver, and narrowing is a scoped entry. No side table until something reads one (the LSP's hover, ∆5MM). |

## Deliberately deferred

Do not decide these ad hoc; use the provisional readings in `docs/language.md` and flag
in the PR when one is load-bearing:

- String interpolation delimiter: `{expr}` (provisional) vs `${expr}`.
- Number model: `int` + `float` (provisional) vs single `number`.
- Pipe operator `|>`: likely redundant given UFCS.
- Enum / closed-union syntax details; exact `lem-match` exhaustiveness rules.
- Operator spelling consistency: `==`/`&&`/`||` (provisional) vs `eq`/`and`/`or`.
- Inline lem tags in prose: reserved for v2; document-mode design must stay compatible.

## Chronology

D01–D17, D19 settled 2026-08-14 (design interview); D08 revisited and D20–D21 settled
2026-08-15; named Lemmon with D18 closed, D21 finalized, D22–D23 added 2026-08-16; D18
npm scope fixed to `@limulus` later on 2026-08-16; D21 revised 2026-08-20 to close `lem-fn`
bare like every other block, to drop the `<lem-- -->` comment tag, and to drop the datetime
literal in favour of a `datetime("…")` stdlib constructor. D24 added 2026-08-22; D25
2026-08-23; D26 2026-08-23, implemented 2026-08-24 (∆2vO); D27 2026-08-24 (∆2vO); D28
2026-08-24, applied 2026-08-25 (∆Ma4); D29 2026-08-26 (∆up2); D30 2026-08-28,
revised the same day from well-known types to host-declared ambient types (revising D19,
D23 and D29 with it), its `from` clause parsed 2026-09-04 (∆1vS); D31 2026-09-06 (∆C8d).
From D25 on, a record's Status line carries the history its row used to.

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

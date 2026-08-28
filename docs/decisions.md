# Decisions

Normative log, settled during the Aug 2026 architecture sessions (full rationale lives
in the three-artifact set: "Shell & Core", "The Lemmon Language", "From Eleventy to
Lemmon"). Treat these as fixed unless Eric reopens one; cite IDs in PRs when relevant.

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
| D19 | Drafts & scheduling | `draft: bool` + `--drafts` dev flag. No scheduled publishing in static mode (falls out of on-demand later). |
| D20 | CSS scoping | Convention, no rewriting: component root carries a class named after the function; styles nest under it with native CSS nesting. `lem-style` blocks → per-page hashed bundle via `lem-emit styles`. |
| D21 | Tag shapes | Kebab `lem-*` builtins (valid custom-element names; namespace reserved — unknown `lem-*` is a compile error). `lem.` accesses (always void); keyword heads with `= expr` are void, without are blocks; voids canonically ` />`. All blocks close bare (`</lem-fn>`; the named close `</lem-fn.foo>` was dropped 2026-08-20). Heads never contain `>`: `lt lte gt gte`, `\|a\| expr`, `: T`. |
| D22 | Calls & slots | `<lem.f(args)>` is the only call form. Bodies pass capture-then-call (`<lem-let x>…</lem-let>` → `slot: x`). No call-with-body syntax; grammar stays head-decidable. One-shot sugar would be additive later. |
| D23 | Namespaces | One lexical value namespace: locals/params → project-flat (all site `.lem` files; duplicates = compile error; no site-file imports) → ambient (`site`/`page`/`build` + stdlib; shadowing stdlib warns). Plugins bind qualified via `lem-import` in `site.lem`, named promotion optional. Types split from values by capitalization. |
| D24 | Trailing commas | One optional trailing comma is allowed in every comma-separated list — `[a, b,]`, `{k: v,}`, `f(a, b,)`, `lem-fn f(a: int,)`, `\|a, b,\| …` — so multi-line literals and signatures stay one-line-per-item and diff-friendly, as in JS and Rust. Empty slots (`[a,,b]`, `f(,)`) remain errors (2026-08-22). |
| D25 | Separate compilation | Each `.lem`/`.md` compiles to its own **unit** — file-local constant pool (document prose included), cross-file references symbolic — and a cheap link step joins the units into the `Module` the VM runs and `exportModule` serializes. A unit is a function of the file's text plus the *compile-time facts* it consumed (imported signatures and types, reference-index ids, asset-manifest entries), which the compiler reports so the cache key `hash(text) + hash(facts)` is built from evidence, not guessed. No file's bytecode may depend on another file's body; the checker stays whole-program but is not a compile-unit input (2026-08-23). |
| D26 | Body whitespace | Value-producing block bodies are normalized at compile time: a `lem-fn` block body and a `lem-let` capture body (D22's slots) lose the newline-and-indent at each edge — one newline per edge, so a deliberate blank line survives as one — and are dedented by the longest whitespace prefix the lines that remain share, so a definition's own indentation never reaches the output. A body that opens on its own tag's line (`<lem-fn f()><svg>` …) has no indentation on that first line, so the margin is empty and the body stays verbatim: hugging is how a definition emits a margin of its own. Flow constructs stay verbatim in that they take no edge trim and no dedent of their own — a loop body's leading newline is what separates iterations — but their text lies inside the definition around them, so it dedents with it and its lines set the margin like any others. A nested capture body does not: it is a root of its own, normalized on its own and opaque to the body around it. The rule is lexical, over the template's literal text only: it runs after D27 (a line that rule deletes must not set the margin), ignores blank lines, and never touches interpolated values, which arrive at run time. No element awareness (invariant 2): `<pre>`/`<textarea>` content dedents like everything else — only the shared margin goes, relative indentation survives (2026-08-23; the hugged, flow, and nested readings settled 2026-08-24 when it was implemented). |
| D27 | Statement lines | A tag that renders nothing where it stands takes its line with it when nothing else stands on that line: `lem-let` and `lem-fn`, in their void form and in their block form alike. So a file laid out one statement per line leaks no blank line per statement, and a file of component definitions leaks none per definition. `<lem.…>` renders in place and keeps the line it is written on; `lem-if`/`lem-for` render their bodies and keep theirs. Widened from the void-tag-only rule on 2026-08-24, when D26 landed and every block definition was still leaking a line into its own file's output. |
| D28 | Whitespace is a source concern | Lemmon optimizes how a `.lem` file reads, not how its HTML reads. Indentation and the blank lines between tags are not a product surface: a formatter or minifier downstream owns that (D12's dprint plugin), and a reader who wants the output legible has devtools. So the renderer never re-indents at a boundary — a definition loses its own margin (D26) and nothing else moves. A multi-line value keeps its own columns wherever it is substituted, so a slot's second line lands where its definition put it rather than at the call's column, and a flow body keeps its newlines; re-indenting either would make the output depend on a value's shape at run time. What this does *not* license is touching whitespace HTML renders — inline formatting contexts, `pre`/`textarea` — which is output semantics and survives byte for byte. Which is why a fidelity gate compares rendered HTML rather than raw bytes (D15's "normalized"; ∆Ma4 for the Phase 0 spike), and never by collapsing whitespace blindly (2026-08-24). |
| D29 | Type bodies | A `lem-type` block's body is not text. Its opener's name alone (invariant 4) puts the scanner into *fields mode* — the tag-mode vocabulary with a line ending as a token — until the first `<`, which rescans as text so the close tag lexes as every close does. One field per line, `name: Type [by index] [= default]`; a line ending inside open brackets is whitespace, so a default can be a multi-line literal (D24). `extends` and `by` are contextual keywords, plain names elsewhere. Rejected: a text body the parser re-lexes (the parser would become a second scanner, and the tree-sitter grammar a third) and a tag per field (three tags of noise per line, for a body that is never markup). One rule, at the scanner, that both grammars mirror (2026-08-26, ∆up2). |
| D30 | Well-known types | The core provides no complex types of its own. `Site`, `Page` and `Build` are *well-known*: the names are reserved, a site declares each as an ordinary `lem-type`, and the checker verifies the declaration against a core contract — `Page` needs `url: url`, `inputPath: string`, `body: html`, `prev: Page?`, `next: Page?`; `Build` needs `time: datetime`; `Site` needs nothing. Everything past the contract is the site's: `Page.data` is whatever record the site names, `Build.env` lists the environment values the site needs (a missing one is a load error at the declaration, not an `undefined` at render), and `Site`'s fields carry the collections and data bindings directly — `from "glob"` / `from plugin("…")` is a field clause beside `by`, so field order is `name: Type [by …] [from …] [= default]` and `lem-collection`/`lem-data` are retired. A program that declares none of the three keeps the checker's unknown type for that name: declaring is how a site buys the checking, not how it earns permission to render — which is what lets the Phase 0 spike and a first-day site render unannotated. There is no `layout` builtin. A string in data resolves by its expected type — record → its `by` index, enum → a variant, `asset` → the manifest, `datetime` → parse, function type → a project-namespace function of that type — so `layout: (Page, html): html = article` needs nothing special, and the void `<lem-type Layout = (Page, html): html />` alias (D21's `= expr` void marker applied to `lem-type`) is only a shorter way to write it. Rejected: keeping the four as builtins — a builtin has no span, so "expected because of this declaration" has nothing to point at, and `Site` was assembled from tags rather than defined anywhere; and generics on `Page`, a user-facing generic bought for one field. `site`, `page` and `build` stay ambient values that cannot be redefined (D23) (2026-08-28). |

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
literal in favour of a `datetime("…")` stdlib constructor. D24 added 2026-08-22; D25 added
2026-08-23 so that a content edit in `lem dev` never recompiles the layouts a document
depends on. D26 added 2026-08-23, after the Phase 0 spike showed a component's source
indentation leaking into the output bytes of every page that calls it; D26 implemented and
D27 added 2026-08-24 (∆2vO); D28 the same day, when the spike's byte gate was found to be
forcing exactly the templates D26 exists to abolish — and applied 2026-08-25 (∆Ma4), when
that gate became a rendered-HTML comparison and the spike's templates were rewritten as an
author would write them. D29 added 2026-08-26 (∆up2), when the parser first had to read a
`lem-type` body and the scanner's two modes turned out to be one short. D30 added
2026-08-28, when writing limulus.net's `site.lem` ahead of the checker found `layout` used
as a type that nothing declares — the only lowercase type name in the language — and `Site`
to be a type the checker would have had to assemble from tags anyway.

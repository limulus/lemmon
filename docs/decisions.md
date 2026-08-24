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
| D26 | Body whitespace | Value-producing block bodies are normalized at compile time: a `lem-fn` block body and a `lem-let` capture body (D22's slots) lose the newline-and-indent at each edge and are dedented by the common indentation of the lines that remain, so a definition's own indentation never reaches the output. Flow constructs stay verbatim — `lem-if`/`lem-for` bodies are untouched, since a loop body's leading newline is what separates iterations. The rule is lexical, over the template's literal text only: it runs after the void-line rule (a line that rule deletes must not set the margin), ignores blank lines, and never touches interpolated values, which arrive at run time. No element awareness (invariant 2): `<pre>`/`<textarea>` content dedents like everything else — only the shared margin goes, relative indentation survives (2026-08-23). |

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
indentation leaking into the output bytes of every page that calls it.

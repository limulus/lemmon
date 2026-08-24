# Lemmon architecture

Working distillation of the "Shell & Core" artifact for implementation sessions. Decision
IDs (D01…) refer to `docs/decisions.md`.

## Shape

Two halves, one contract:

- **Core** — Rust workspace compiled to WebAssembly: scanner, parser, type checker, bytecode
  compiler, VM, document-mode segmenter/splicer, and comrak for markdown. Pure:
  `(program, data, page) → (html, side channels, diagnostics)`. No I/O, clock, or env.
- **Shell** — Node/TypeScript: content walker, YAML front matter (with positions), plugin
  host, esbuild driver, worker pool, dev server (COOP/COEP headers, live reload), output
  writer (hashing conventions), CLI (`lem`). Everything site- or host-specific is a plugin:
  photos, revisions, svg-sprite, dprint. Markdown is _not_ a plugin (D08).

The same `.wasm` runs in the build, the LSP (Node process), a browser playground, and —
later, unchanged — a Lambda (`ContentSource` seam, D14).

## WASM API (`@limulus/lemmon-core`)

```ts
class Core {
  compile(files: {path, text}[]): Diagnostic[]        // changed files only; per-file units, cached (D25)
  loadData(input: DataInput): Diagnostic[]            // records, data files, asset manifest, build info
  upsertRecord(collection, record) / removeRecord(id) // dev server + future on-demand
  plan(): PageRef[]                                   // static pages + expanded generators
  render(page: PageRef): { html, channels, deps }
  exportModule(): Uint8Array / importModule(bytes)    // precompiled program (Lambda cold start)
  analyze(path, offset): { diagnostics, completions, hover, definition }  // LSP
  format(path): string
}
// reserved optional host import (D08): renderMarkdown?(text: string): string
```

Boundary rules: few large calls; program and data stay resident in the instance; bulk data
crosses as one UTF-8 JSON buffer (rmp-serde if profiling demands); one instance per worker
thread, nothing shared. Whole-site data ≪ 10 MB. comrak + syntect adds ~1 MB to the .wasm —
lazy-load in the playground, irrelevant in Node.

∆TNT built the `compile`/`loadData`/`render` subset of that class (plus `version`), and
settled the spike shapes the sketch left open — each chosen so the full form is additive,
not a break:

- `DataInput` is `{ambient: {site?, page?, build?}}`: an envelope object, so `records`,
  data files, and the manifest arrive as new fields. `loadData` validates nothing yet
  (schemas are ∆C8d) and always returns `[]`.
- `PageRef` is `{path}` until `plan()` exists to mint richer references.
- `render` returns `{html, diagnostics}` — `html: null` plus one diagnostic when the page
  fails at runtime. `channels` and `deps` are omitted entirely until ∆RGQ, not stubbed.
- A `Diagnostic` is `{path, span: {start, end}, message}` with byte offsets (invariant 8);
  `severity` and `code` arrive with the checker as optional fields.
- Errors split by kind: anything wrong with a template or document is a returned
  diagnostic; a thrown `JsError` is reserved for caller bugs — a malformed JSON envelope,
  rendering before a compile has succeeded, rendering a path the program does not have —
  which have no source span to point at.
- `compile` really does take changed files only: units stay resident in the `Core`, each
  call compiles what it is given against the resident signatures and relinks. A file that
  fails keeps its previous unit and the previous module — stale until fixed, failure
  reported. Deleting a file has no boundary call yet (nothing needs it before the shell
  exists); the link step's unresolved-import diagnostic is what makes adding one safe.
- A caller left stale by a signature it never saw change is a link diagnostic pointing at
  the caller, not a silent relink: named arguments are reordered against the parameter
  list at compile time, so recompiling only the callee would otherwise swap a caller's
  arguments with nothing reported. That check is what makes `Unit::facts` load-bearing
  rather than bookkeeping, and it names exactly the file the shell must recompile.

## Build pipeline

1. **Discover** (shell) — walk input, classify (document/template/data/passthrough), parse
   front matter with positions, load `lemmon.config.ts`.
2. **Providers** (shell) — plugin data hooks, document transforms (e.g. link-post reference
   appender), asset pipeline: esbuild → manifest (before compile, because `asset()` is
   compile-checked).
3. **Compile** (core) — every changed `.lem`/`.md` → its own bytecode unit, then a link step
   joins the units into the module (D25, §Separate compilation); document prose rendered by
   comrak here and stored in the document's unit (markdown never runs at render time).
4. **Load** (core) — validate records against schemas, evaluate computed fields, resolve
   reference fields and assets. Failures point at YAML positions.
5. **Plan** (core) — expand generators → full output list (url, entry, deps).
6. **Render × N** (core) — body → layout chain → html + channels (styles/scripts/head)
   - dependency set. Parallel via shell workers.
7. **Write** (shell) — HTML, hashed per-page CSS bundles (`name.<8hex>.ext`,
   `/assets/immutable/…` conventions preserved), passthrough.
8. **Post-process** (shell) — optional transforms (dprint), link/asset integrity check.

Dev incrementality comes from two reported sets. Render-reported deps: template edit →
recompile one file, re-render dependents; content edit → `upsertRecord`, re-render readers;
TS edit → esbuild incremental, re-render referencing pages. Compile-reported facts (D25):
the shell keys each file's unit on its text plus the facts it consumed, so a document edit
recompiles that document and nothing else — its layouts are unchanged units, not recompiled
ones.

## Separate compilation (D25)

The compile unit is the file. Each `.lem`/`.md` compiles to a **unit** with its own constant
pool (document prose included) and symbolic cross-file references — named functions, types,
pages — and a link step resolves those to indices when it joins units into the `Module` the
VM runs. Linking is a pass over plain data; it is what makes relinking the whole site after
a one-file edit cheaper than recompiling one file.

A unit depends on its text and on the **compile-time facts** it consumed, nothing else:

- signatures and types of the project-flat and plugin names it references (not bodies);
- the ids of any reference index it resolves a string literal through (language.md §Document
  mode item 6);
- the asset-manifest entries `asset()` was checked against (D10).

The compiler reports those facts per file, and the shell's cache key is
`hash(text) + hash(facts)`. That is the guarantee: a content edit re-runs comrak and codegen
for that one document; the layouts and components it depends on are cache hits. A new photo
id invalidates only documents whose facts named the photo index, never the layouts. The
checker remains whole-program (cheap: the program is small) but runs over units, so
re-checking never requires recompiling.

∆TNT made the split real: `lemmon-vm` compiles per file (`prepare` → `SignatureTable` →
`compile_unit`) into a `Unit` — file-local constant pool, unit-local function indices,
imports symbolic by name — and `link` concatenates pools with offsets into the same
`Module` shape the VM always ran, so the VM itself never changed. Cross-file duplicate
names are caught building the signature table (and again at link, for units compiled
apart); an import no unit exports is a spanned link diagnostic. The facts live on
`Unit::facts` — today only the imported-signatures kind exists, and invariant 11 is a
test: editing a callee's body leaves the caller's unit byte-identical. `∆RGQ` makes the
reported facts cross the boundary and the serialized form (`exportModule`) first-class.

## VM

Stack machine; serializable `Module { constants, functions, types, pages, spans }`, linked
from per-file units (above). Values are immutable and Rc'd:
`Null | Bool | Int | Float | Str | Html | DateTime | Duration | Url | Asset | List | Map | Record | Fn`.
Records store fields in declaration order (GetField is an index). Ops sketch:
`Const/Local/Global/Ambient`, `GetField/OptField/ Index`, `Call/CallBuiltin/CallValue`
(named args reordered at compile time; lambdas are closures over copied captures),
`Jump/JumpIfFalse` plus the short-circuit `JumpUnlessNull/JumpIfFalseKeep/JumpIfTrueKeep`,
`IterInit/IterNext`, `TypeTest`, `Emit` (type-directed) and `EmitText`, `Capture/EndCapture`
(slot bodies and `lem-let` blocks), `Channel/EmitChannel` (dedup by hash; placeholders
resolved at page end), `Track` (dependency recording), `Return`. Runtime errors carry spans
via the pc→span table.

What ∆c8h built is the untyped subset of that: `Value` without the stdlib types, maps in
place of records with `GetField` by name, and no `TypeTest`/`Channel`/`Track` — those arrive
with the checker (∆C8d), the stdlib (∆Q6L), and side channels (∆RGQ).

Calibration, not promise: whole site tens of ms inside WASM; single page well under 1 ms of
VM time (that number is the Phase 0 exit criterion).

## Crates (verified Aug 2026)

| Role           | Choice                                                   | Notes                                                                                                                                                                                                                    |
| -------------- | -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Scanner        | `logos` 0.16                                             | `morph()` = text↔tag mode switch                                                                                                                                                                                         |
| Parser         | hand-written RD + Pratt                                  | Chosen in the ∆7Ib bake-off over `lelwel` 0.10 (`docs/journal/∆7Ib-parser-bake-off.md`): LL(1) forced tag names into the token vocabulary and junk out of heads, costing recovery fidelity for ~700 fewer owned lines    |
| Syntax tree    | `rowan` 0.17 green/red CST                               | lossless + recovering; no incremental reparse needed (rust-analyzer's own conclusion). `lemmon_syntax::ast` is the typed view over it that the checker and codegen read; the CST stays for the formatter and diagnostics |
| Diagnostics    | structured JSON + `ariadne`/`annotate-snippets` renderer | spans first-class                                                                                                                                                                                                        |
| Markdown       | `comrak` 0.54 + `syntect` 5.3 + `two-face`               | in-core (D08); wasm32-clean. comrak is in (`default-features = false` + `attributes`, with a heading adapter since its renderer ignores `{#id}`); syntect is not yet                                                     |
| WASM           | `wasm-bindgen` 0.2.127, CLI direct (no wasm-pack)        | `wasm-bindgen --target web` runs in `scripts/stage-wasm.ts`: caller-supplied bytes keep loading injectable and browser-reusable, and the repo owns staging, so wasm-pack’s own package layout would only fight it (∆TNT)                                                                                                                                                                                   |
| Serde          | `serde_json` (rmp-serde optional)                        | avoid serde-wasm-bindgen for bulk data                                                                                                                                                                                   |
| Editor grammar | `tree-sitter` 0.26                                       | editor-only; do NOT link its C runtime into the core wasm; its `wasm` cargo feature means "host wasmtime", not "compile to wasm"                                                                                         |
| Native later   | `napi-rs` 3.x                                            | optional perf path behind the same TS interface (D11)                                                                                                                                                                    |
| Bundler        | `esbuild` 0.28 via JS API                                | config ports 1:1 from limulus.net                                                                                                                                                                                        |

## Shell plugin API (sketch)

```ts
interface Plugin {
  name: string
  contentSources?(ctx): ContentSource[] // the on-demand seam (D14)
  data?(ctx): Promise<Record<string, unknown>> // typed via site.lem declarations
  transformDocument?(doc, ctx): Document // pre-markdown text transform
  assets?(ctx): Promise<AssetManifest> // logicalPath → {url, hash, deps}
  markdown?: MarkdownRenderer // reserved override (D08)
  transformOutput?(file, ctx): OutputFile
  emit?(ctx): Promise<OutputFile[]>
  commands?: Command[] // e.g. `lem photos add`
  watch?(ctx): AsyncIterable<Change>
}
```

Plugins ship `.lem` files (functions + types) bound via `lem-import` in `site.lem` (D23);
they cannot inject build-time JS (D06). The shell also exposes a content API
(`loadSite(config).collection('articles')`) for tools like the revisions updater.

## Editor tooling (day one, D16)

- `tree-sitter-lemmon`: embedded-template grammar; HTML injected into text, Lemmon injected
  into markdown lem-blocks. Corpus agreement with the Rust parser is CI-enforced.
- `@limulus/lemmon-lsp`: Node process loading the same `@limulus/lemmon-core` .wasm;
  diagnostics as you type, schema-driven completion, hover, go-to-def, rename, formatting,
  front-matter validation at YAML positions. Precedent: bash-language-server, VS Code
  anycode, Oso's Polar (Node host + wasm core); native `lsp-server` binary remains an escape
  hatch.

## Testing

- Golden corpus `tests/corpus/`: one `.lem` input per case plus one expectation file per
  stage, grouped by what a case is for (`syntax/` today; `render/` and `document/` follow)
  and by language feature below that. `lemmon-syntax` owns `.cst` (the lossless tree with
  its diagnostics trailer) and `.tags` (the tag-boundary projection); the VM adds `.html`.
  Whether a case parses clean is data — an empty diagnostics section — not a directory.
  `crates/lemmon-corpus` does discovery, blessing (`npm run corpus:bless`), and failure
  reporting for every stage. Gates: no orphaned expectations, and every `SyntaxKind` reached
  by some case.
- `.tags` is the tree-sitter agreement target: tags, close tags, text runs, and item-level
  error nodes, with heads, expressions, and `Block` wrappers elided — the boundaries a
  context-free grammar can be held to, and no more.
- Scanner property tests: text without `<lem.`/`<lem-`/`</lem-` round-trips byte-for-byte;
  escaping never emits `<`.
- cargo-fuzz on parser and VM (the LSP feeds them half-typed garbage).
- wasm-bindgen-test through Node for the real boundary.
- The limulus.net parity harness (architecture doc 3) is the end-to-end acceptance bar.

# Lemmon architecture

Working distillation of the "Shell & Core" artifact for implementation sessions.
Decision IDs (D01…) refer to `docs/decisions.md`.

## Shape

Two halves, one contract:

- **Core** — Rust workspace compiled to WebAssembly: scanner, parser, type checker,
  bytecode compiler, VM, document-mode segmenter/splicer, and comrak for markdown. Pure:
  `(program, data, page) → (html, side channels, diagnostics)`. No I/O, clock, or env.
- **Shell** — Node/TypeScript: content walker, YAML front matter (with positions),
  plugin host, esbuild driver, worker pool, dev server (COOP/COEP headers, live reload),
  output writer (hashing conventions), CLI (`lem`). Everything site- or host-specific is
  a plugin: photos, revisions, svg-sprite, dprint. Markdown is *not* a plugin (D08).

The same `.wasm` runs in the build, the LSP (Node process), a browser playground, and —
later, unchanged — a Lambda (`ContentSource` seam, D14).

## WASM API (`@limulus/lemmon-core`)

```ts
class Core {
  compile(files: {path, text}[]): Diagnostic[]        // per-file cache by content hash
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

Boundary rules: few large calls; program and data stay resident in the instance; bulk
data crosses as one UTF-8 JSON buffer (rmp-serde if profiling demands); one instance per
worker thread, nothing shared. Whole-site data ≪ 10 MB. comrak + syntect adds ~1 MB to
the .wasm — lazy-load in the playground, irrelevant in Node.

## Build pipeline

1. **Discover** (shell) — walk input, classify (document/template/data/passthrough),
   parse front matter with positions, load `lemmon.config.ts`.
2. **Providers** (shell) — plugin data hooks, document transforms (e.g. link-post
   reference appender), asset pipeline: esbuild → manifest (before compile, because
   `asset()` is compile-checked).
3. **Compile** (core) — every `.lem`/`.md` → bytecode module; document prose rendered by
   comrak here and stored as constants (markdown never runs at render time).
4. **Load** (core) — validate records against schemas, evaluate computed fields, resolve
   reference fields and assets. Failures point at YAML positions.
5. **Plan** (core) — expand generators → full output list (url, entry, deps).
6. **Render × N** (core) — body → layout chain → html + channels (styles/scripts/head)
   + dependency set. Parallel via shell workers.
7. **Write** (shell) — HTML, hashed per-page CSS bundles (`name.<8hex>.ext`,
   `/assets/immutable/…` conventions preserved), passthrough.
8. **Post-process** (shell) — optional transforms (dprint), link/asset integrity check.

Dev incrementality comes from render-reported deps: template edit → recompile one file,
re-render dependents; content edit → `upsertRecord`, re-render readers; TS edit →
esbuild incremental, re-render referencing pages.

## VM

Stack machine; serializable `Module { constants, functions, types, pages, spans }`.
Values are immutable and Rc'd: `Null | Bool | Int | Float | Str | Html | DateTime |
Duration | Url | Asset | List | Map | Record | Fn`. Records store fields in declaration
order (GetField is an index). Ops sketch: `Const/Local/Global`, `GetField/OptField/
Index`, `Call/CallBuiltin/CallValue` (named args reordered at compile time),
`Jump/JumpIfFalse/JumpIfNull`, `IterInit/IterNext`, `TypeTest`,
`EmitText/EmitHtml/EmitTyped`, `Capture/EndCapture` (slot bodies and `lem-let` blocks),
`Channel/EmitChannel` (dedup by hash; placeholders resolved at page end), `Track`
(dependency recording), `Return`. Runtime errors carry spans via the pc→span table.

Calibration, not promise: whole site tens of ms inside WASM; single page well under
1 ms of VM time (that number is the Phase 0 exit criterion).

## Crates (verified Aug 2026)

| Role | Choice | Notes |
|---|---|---|
| Scanner | `logos` 0.16 | `morph()` = text↔tag mode switch |
| Parser | hand-written RD + Pratt | `lelwel` 0.10 is the one generator worth a spike (error-resilient, lossless CSTs, emits debuggable RD Rust); `chumsky` 0.13 the combinator fallback |
| Syntax tree | rowan-style green/red CST (rowan 0.17 or cstree 0.14) | lossless + recovering; no incremental reparse needed (rust-analyzer's own conclusion) |
| Diagnostics | structured JSON + `ariadne`/`annotate-snippets` renderer | spans first-class |
| Markdown | `comrak` 0.54 + `syntect` 5.3 + `two-face` | in-core (D08); wasm32-clean |
| WASM | `wasm-bindgen` 0.2.127 + `wasm-pack` 0.15 | both active under the wasm-bindgen org |
| Serde | `serde_json` (rmp-serde optional) | avoid serde-wasm-bindgen for bulk data |
| Editor grammar | `tree-sitter` 0.26 | editor-only; do NOT link its C runtime into the core wasm; its `wasm` cargo feature means "host wasmtime", not "compile to wasm" |
| Native later | `napi-rs` 3.x | optional perf path behind the same TS interface (D11) |
| Bundler | `esbuild` 0.28 via JS API | config ports 1:1 from limulus.net |

## Shell plugin API (sketch)

```ts
interface Plugin {
  name: string
  contentSources?(ctx): ContentSource[]        // the on-demand seam (D14)
  data?(ctx): Promise<Record<string, unknown>> // typed via site.lem declarations
  transformDocument?(doc, ctx): Document       // pre-markdown text transform
  assets?(ctx): Promise<AssetManifest>         // logicalPath → {url, hash, deps}
  markdown?: MarkdownRenderer                  // reserved override (D08)
  transformOutput?(file, ctx): OutputFile
  emit?(ctx): Promise<OutputFile[]>
  commands?: Command[]                         // e.g. `lem photos add`
  watch?(ctx): AsyncIterable<Change>
}
```

Plugins ship `.lem` files (functions + types) bound via `lem-import` in `site.lem`
(D23); they cannot inject build-time JS (D06). The shell also exposes a content API
(`loadSite(config).collection('articles')`) for tools like the revisions updater.

## Editor tooling (day one, D16)

- `tree-sitter-lemmon`: embedded-template grammar; HTML injected into text, Lemmon
  injected into markdown lem-blocks. Corpus agreement with the Rust parser is
  CI-enforced.
- `@limulus/lemmon-lsp`: Node process loading the same `@limulus/lemmon-core` .wasm;
  diagnostics as you type, schema-driven completion, hover, go-to-def, rename,
  formatting, front-matter validation at YAML positions. Precedent: bash-language-server,
  VS Code anycode, Oso's Polar (Node host + wasm core); native `lsp-server` binary remains
  an escape hatch.

## Testing

- Golden corpus `tests/corpus/`: input → expected HTML + expected diagnostics; shared
  with the tree-sitter agreement test.
- Scanner property tests: text without `<lem.`/`<lem-`/`</lem-` round-trips
  byte-for-byte; escaping never emits `<`.
- cargo-fuzz on parser and VM (the LSP feeds them half-typed garbage).
- wasm-bindgen-test through Node for the real boundary.
- The limulus.net parity harness (architecture doc 3) is the end-to-end acceptance bar.

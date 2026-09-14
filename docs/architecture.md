# Lemmon architecture

Working distillation of the "Shell & Core" artifact for implementation sessions. Decision
IDs (D01…) refer to `docs/decisions/README.md`.

## Shape

Two halves, one contract:

- **Core** — Rust workspace compiled to WebAssembly: scanner, parser, bytecode compiler
  (which is also the type checker, D31), VM, document-mode segmenter/splicer, and comrak
  for markdown. Pure:
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
  compile(files: {path, text, record?}[], host?: HostDecl): Diagnostic[]
      // changed files only; per-file units, cached (D25). `record` names a document's
      // schema; `host` names the ambient values and their types (D30)
  loadData(input: DataInput): Diagnostic[]            // records, data files, build info
  upsertRecord(collection, record) / removeRecord(id) // dev server + future on-demand
  plan(): PageRef[]                                   // static pages + expanded generators
  render(page: PageRef): { html, channels, deps }
  exportModule(): Uint8Array / importModule(bytes)    // precompiled program (Lambda cold start)
  analyze(path, offset): { diagnostics, completions, hover, definition }  // LSP
  format(path): string
}
// HostDecl = {ambient: {[name]: TypeName | {bindingsOf: path} | null},
//              document?: AmbientName, entry?: FieldName,
//              assets?: {[logicalPath]: url},   (D36, the manifest D38)
//              grammars?: {path, text}[]}       (.sublime-syntax, D40)
// reserved optional host import (D08): renderMarkdown?(text: string): string
```

Boundary rules: few large calls; program and data stay resident in the instance; bulk data
crosses as one UTF-8 JSON buffer (rmp-serde if profiling demands); one instance per worker
thread, nothing shared. Whole-site data ≪ 10 MB. syntect adds 1.97 MB to the .wasm with no
grammars in it (D40) — lazy-load in the playground, irrelevant in Node.

Ambient declarations (D30, D36): the core knows no type or value by name. The host passes
its environment with `compile` — the declarations as Lemmon source (the shell ships
`ambient.lem`, declaring `Page` and `Build`) and `host`, which binds each ambient value
name to a declared type, to the top-level bindings of a file (`site` → `site.lem`), or to
nothing at all, which names the value without typing it; `document` says which of them a
file's `record` retypes and `entry` names the field a page is rendered through
(`layout`). The declaration is resident like the units, so a call that changes nothing
about the environment omits it, and what each unit was compiled against is one of its
facts: a host that rebinds a name leaves the units that read it stale, named for
recompiling, as a changed signature does (D25). A core given no declaration has no
ambient values, and `page` in a template is an undefined name.

Each document carries its `record`: the element type of the `from` binding whose glob
matched it, or the type a `type:` key names, which the checker requires to extend that
element type. The unit depends on that type as a compile-time fact (D25). A document
without one compiles with `page` unknown-typed, which is how the spike renders today.

Code highlighting (D40) takes its grammars from the declaration too, as `.sublime-syntax`
text, because prose renders while compiling. The first code block that names a language
parses all of them into one set, linked in declaration order and never grown, so every
lookup is syntect's own and a block renders the same whatever an instance highlighted
before it. That parse is per worker: about 0.4 seconds for limulus.net's languages and
five seconds for bat's whole set, so a host declares the languages a site uses, or hands
its workers a precompiled set, which syntect loads in milliseconds and unpacks a
language at a time. What the core owns is only the inside of
`<code>`: comrak's `<pre><code class="language-x">` is written unchanged, so a host
that declares no grammars gets plain escaped blocks, and whatever colours them another
way — a post-render plugin, or a pure wasm highlighter the core calls (∆SDN) — starts
from markup that names the language. A highlighter choice would be a compile-time
input like the grammars are, and would ride the same fact.

∆TNT built the `compile`/`loadData`/`render` subset of that class (plus `version`), and
settled the spike shapes the sketch left open — each chosen so the full form is additive,
not a break:

- `DataInput` is `{ambient, sources?}` (D37, ∆iCW): `ambient` is bare JSON by any name
  the host bound — an untyped host's values pass through as sent — and `sources` fulfils
  each `from` binding by name: a list of `{path, value, spans}` for a glob, one such for
  a data file, `{value}` for a plugin or env source, where `spans` maps a field path to a
  complete boundary span the host computed from the YAML it parsed; the shell strips the
  `type:` key it consumed. The core validates every value against the program, runs the
  defaults and computed bindings, tags each record, and keeps the result resident — the
  store every render binds — returning the load diagnostics, each placed by the host's
  positions or at the declaration that expected the value. The store lives with the
  program it was loaded against: a load that reports an error leaves the previous data
  bound, stale until fixed, as a failed compile leaves the program, and a compile that
  installs a new program drops it — `loadData` again before `render`. A load runs
  against the program that linked, under the declaration it linked with; an envelope
  the program cannot be loaded from — a name the host does not bind, a source no `from`
  binding asked for, a document never compiled, a load before any compile has succeeded
  or after the declaration changed without one succeeding — is a thrown caller bug. The
  asset manifest rides on the declaration, not on the envelope: `asset("…")` is answered
  while compiling (D10, D38), so it has to be there by then, and a rebuild that re-hashes
  an asset is a change to the declaration. So do the grammars (D40), which a new
  declaration repeating them leaves parsed rather than parsed again; a label pointing
  into a grammar file arrives with positions the core computed, since no index it keeps
  covers a file it never compiled. A file passed to
  `compile` may carry `recordSpan`, the byte offsets of the `type:` key's value, so a
  diagnostic about the record points at the key; offsets that are not a span of the
  file's text are refused as a caller bug, since a line index panics on them.
- `PageRef` is `{path}` until `plan()` exists to mint richer references.
- `render` returns `{html, diagnostics}` — `html: null` plus one diagnostic when the page
  fails at runtime. `channels` and `deps` are omitted entirely until ∆1Hp and ∆xBO, not
  stubbed. A loaded document renders with the host's document value bound to its own
  record from the store; any other file — a template, a document no collection matched —
  reads the value the envelope carried, if it did, as sent (D37).
- A `Diagnostic` is `{path, span: {start, end, range}, severity, message, labels}`. The
  span carries byte offsets (invariant 8) *and* `range`, the same two points as
  `{line, character}` LSP positions — zero-based, `character` in UTF-16 code units. The
  core computes them because it holds the bytes; the shell and the language server would
  otherwise each re-encode the source to turn a byte offset into a UTF-16 column.
  `severity` is `error` or `warning` — a program is still produced
  when the only diagnostics are warnings — and `labels` are the secondary spans, each
  `{path, span, message}`, pointing at the same kind of thing the primary span does (a
  name, not a whole definition) (∆VNk). A `code` would be an additive field; nothing
  consumes one yet. `compile`'s diagnostics describe the files passed to that call, plus
  any resident unit the relink found stale; a resident file's warnings are reported once,
  by the call that compiled it, so a shell that shows warnings keeps them per path.
- Errors split by kind: anything wrong with a template or document is a returned
  diagnostic; a thrown `JsError` is reserved for caller bugs — a malformed JSON envelope,
  a `compile` naming the same file twice, rendering before a compile has succeeded,
  rendering a path the program does not have — which have no source span to point at.
- `compile` really does take changed files only: units stay resident in the `Core`, each
  call compiles what it is given against the resident export index and relinks. A file
  that fails keeps its previous unit and the previous module — stale until fixed, failure
  reported, and the index goes back to describing the units that are actually resident.
  Deleting a file has no boundary call yet (nothing needs it before the shell exists);
  the link step's unresolved-import diagnostic is what makes adding one safe.
- A definition of a name a resident file already exports is refused: the offering file is
  the one blamed, whatever its path sorts as, and it yields no unit — so the collision is
  reported once, by the call that introduced it, and the last good program still renders.
  A shell that shows errors keeps them per path, as it does warnings.
- A caller left stale by a signature it never saw change is a link diagnostic pointing at
  the caller, not a silent relink: named arguments are reordered against the parameter
  list at compile time, so recompiling only the callee would otherwise swap a caller's
  arguments with nothing reported. That check is what makes `Unit::facts` load-bearing
  rather than bookkeeping, and it names exactly the file the shell must recompile.

## Build pipeline

1. **Discover** (shell) — walk input, classify (document/template/data/passthrough), parse
   front matter with positions, load `lemmon.config.ts`.
2. **Providers** (shell) — plugin data hooks, document transforms (e.g. link-post reference
   appender), asset pipeline: esbuild → manifest, which is handed to `compile` on the
   host declaration because `asset()` is answered there (D10, D38) — its values the URLs
   assets shipped at, since the core does no path arithmetic.
3. **Compile** (core) — every changed `.lem`/`.md` → its own bytecode unit, then a link step
   joins the units into the module (D25, §Separate compilation); document prose rendered by
   comrak here and stored in the document's unit (markdown never runs at render time).
   Type checking is not a pass of its own: the walk that lowers an expression is the walk
   that types it (D31), so a type error is a compile diagnostic like any other and a file
   with one yields no unit.
   Templates without front matter compile first, `site.lem` among them: the shell matches
   documents and page templates — `.lem` files with front matter (D41) — against the
   `from` globs that `site.lem` reports, then compiles each with its `record` (D30).
4. **Load** (core) — validate records against schemas, run the defaults and computed
   bindings, resolve reference fields (D37) and `[asset]` fields, the latter against the
   manifest the declaration carried into step 3. Failures point at the YAML positions
   the shell sent with the data.
5. **Plan** (core) — expand generators → full output list (url, entry, deps); `entry` is
   the page's `layout`, the field the host named (D30).
6. **Render × N** (core) — body → the entry function → html + channels (styles/
   scripts/head) + dependency set. Parallel via shell workers.
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
- for a document, the record type the shell compiled it with (D30);
- what the host bound each ambient value it read to (D36);
- the asset-manifest entries `asset()` was checked against (D10);
- for a document whose code names a language, the grammars the host declared, as one
  digest of the whole set (D40).

The compiler reports those facts per file, and the shell's cache key is
`hash(text) + hash(facts)`. That is the guarantee: a content edit re-runs comrak and codegen
for that one document; the layouts and components it depends on are cache hits. A new photo
id changes no existing unit: a record named by id is read from its map at render, and no
unit's facts name an id (∆Hbw). Which pages then re-render — a render-time read of the
map, or a load-time `by` resolution — is not yet answered (∆xBO).
Checking is part of compiling a unit (D31): a file is re-checked exactly when it is
recompiled, which its text or its facts changing is what triggers.

∆TNT made the split real: `lemmon-vm` compiles per file (`prepare` → `ExportIndex` →
`compile_unit`) into a `Unit` — file-local constant pool, unit-local function indices,
imports symbolic by name — and `link` concatenates pools with offsets into the same
`Module` shape the VM always ran, so the VM itself never changed. The `ExportIndex` is
the project's one answer to which file defines each project-flat name (∆e5i): each file's
exports are offered to it in turn, a name another file already owns stays that file's,
and both `compile_unit` and `link` read that one answer rather than deriving their own.
A refused definition is reported where its file compiles, next to the stdlib-shadowing
warning that asks the index the same question; an import the index has no definition for
is a spanned link diagnostic. The index has a type namespace beside the functions
(∆cea): each file's `lem-type`/`lem-enum` declarations are offered under the same rule,
and annotations resolve to a name alone that the index gives a shape. Two kinds of fact
live on the unit — `Unit::facts`, the imported signatures (parameter names and types and
the return type a call was checked against, typed since ∆C8d), and `Unit::type_refs`,
the declarations of every foreign type the checking reached, each at the use that reached
it — so a callee whose type changed under a caller, or a type whose shape changed under a
consumer, is a link diagnostic naming the file to recompile, in that file's own order. Invariant 11 is a test:
editing a callee's body, or a declaration's default, leaves the consumer's unit
byte-identical. What the link step derives from every unit at once does change: since
∆HF0 it closes each unit's recorded reads through the calls they write, project-wide, so
a callee's body edit can reorder the loader's plans while changing no unit. `∆Qr2` makes the reported facts cross the boundary and the serialized
form (`exportModule`) first-class.

## VM

Stack machine; serializable
`Module { files, documents, constants, functions, declarations, exports, records, bindings }`
— the code, the project's type declarations, its project-flat functions by name, and
the loader's plans: each record's default functions and the bindings file's computed
bindings, in the order their recorded reads, closed through calls, dictate (D37,
∆HF0), and each record's `by` indexes, the ambient fields its strings resolve through
(∆eag) — linked from
per-file units (above). Values are immutable and Rc'd:
`Null | Bool | Int | Float | Str | Html | DateTime | Duration | Url | Asset | List | Map | Fn | Record`.
Records carry their declared name, and the module the declaration table that
`TypeTest` walks and the loader validates against, so the two cannot disagree about
what a name is; fields are read by name (by index is an optimisation nothing needs,
D35). Ops sketch:
`Const/Local/Global/Ambient`, `GetField/OptField/ Index`, `Call/CallBuiltin/CallValue`
(named args reordered at compile time; lambdas are closures over copied captures),
`Jump/JumpIfFalse` plus the short-circuit `JumpUnlessNull/JumpIfFalseKeep/JumpIfTrueKeep`,
`IterInit/IterNext`, `TypeTest`, `Emit` (type-directed) and `EmitText`, `Capture/EndCapture`
(slot bodies and `lem-let` blocks), `Channel/EmitChannel` (dedup by hash; placeholders
resolved at page end), `Track` (dependency recording), `Return`. Runtime errors carry spans
via the pc→span table. A builtin that takes a function (`map`, `filter`) runs it through a
re-entrant invoke inside its `CallBuiltin` step — the VM pushes the frame and runs its loop
again until that frame returns — so an error inside the closure keeps the closure's own
span (∆987). That re-entry costs native stack rather than heap, so it has a cap of its
own beside the frame count, sized against the megabyte of stack `crates/lemmon-wasm`'s
build script links the `.wasm` with; the parser's nesting cap is sized against the same
megabyte, and the boundary's ABI tests show both fit (D38, ∆vnI).

What ∆c8h built is the untyped subset of that: `Value` without the stdlib types, which
∆Q6L added, and no `Channel` or `Track`, which arrive with the side channels (∆1Hp) and
render-reported deps (∆xBO).
Records are checked by declaration since ∆cea, and since ∆PCK a record value is tagged
with its declared name (`Value::Record`, made by a map literal where a record is expected
and, later, by the loader at the boundary), which `TypeTest` reads; an enum value is its
variant's name as a string. The loader tags what it validates at `loadData` (D37, ∆iCW),
so a type test reaches loaded data; what still arrives untagged — an untyped host's
values, and `page` in a document no collection matched — is `any`, and a type test over
it is a render error (D35). The VM is the runtime for `any` (∆C8d): its type checks fire
only on values the checker let through as `any`, and stay for that reason.

Calibration, not promise: whole site tens of ms inside WASM; single page well under 1 ms of
VM time (that number is the Phase 0 exit criterion).

## Crates (verified Aug 2026)

| Role           | Choice                                                   | Notes                                                                                                                                                                                                                    |
| -------------- | -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Scanner        | `logos` 0.16                                             | `morph()` = text↔tag mode switch; a third, fields mode, lexes a `lem-type` body (D29)                                                                                                                                    |
| Parser         | hand-written RD + Pratt                                  | Chosen in the ∆7Ib bake-off over `lelwel` 0.10 (`docs/journal/∆7Ib-parser-bake-off.md`): LL(1) forced tag names into the token vocabulary and junk out of heads, costing recovery fidelity for ~700 fewer owned lines    |
| Syntax tree    | `rowan` 0.17 green/red CST                               | lossless + recovering; no incremental reparse needed (rust-analyzer's own conclusion). `lemmon_syntax::ast` is the typed view over it that the checker and codegen read; the CST stays for the formatter and diagnostics |
| Diagnostics    | structured JSON + `ariadne`/`annotate-snippets` renderer | spans first-class; `line-index` 0.1 (rust-analyzer's, sharing rowan's `text-size`) turns a byte offset into an LSP position at the boundary                                                                                                                                                                                                        |
| Markdown       | `comrak` 0.54 + `syntect` 5.3                            | in-core (D08); wasm32-clean. comrak with `default-features = false` + `attributes`, a heading adapter since its renderer ignores `{#id}`, and the core's own highlighter adapter — comrak's `SyntectAdapter` writes code unescaped when syntect errs and guesses a language from the first line. syntect with `regex-fancy` (Oniguruma is C) and `yaml-load`, no default grammars or themes: grammars come from the host (D40). +1.97 MB of staged `.wasm` (2.12 → 4.10 MB); embedding bat's grammars via `two-face` as well measured 0.7 MB more |
| WASM           | `wasm-bindgen` 0.2.127, CLI direct (no wasm-pack)        | `wasm-bindgen --target web` runs in `scripts/stage-wasm.ts`: caller-supplied bytes keep loading injectable and browser-reusable, and the repo owns staging, so wasm-pack’s own package layout would only fight it (∆TNT)                                                                                                                                                                                   |
| Dates          | `jiff` 0.2                                               | `default-features = false`, `alloc` only: no clock, no system zone, no tzdb, so invariant 1 is structural rather than reviewed. Parsing, the civil calendar, RFC 3339/2822 and strftime; a `datetime` is an instant and the offset it was written in (D38) |
| URL encoding   | `percent-encoding` 2.3                                   | one encoding pass behind `url()`. The WHATWG `url` crate was rejected: it refuses a relative reference like `/feed/` without a base, which is most of what a site composes, and pulls `idna`'s tables in (D38) |
| Latin folding  | `unicode-normalization` 0.1                              | canonical decomposition behind `slugify`, so a letter carrying a mark slugs as the letter. Transliteration was measured and rejected: `deunicode` cost 479 KB of the `.wasm` and `any_ascii` 582 KB, to answer a question no site in the corpus asks — and `@sindresorhus/slugify`'s answer applies German's `ü`→`ue` to every language's `ü` (D38) |
| Word counting  | `unicode-segmentation` 1.13                              | UAX #29 word boundaries behind `html.readingTime()`, so a script that does not space its words is counted rather than read as one long one. Splitting on whitespace would have made the core assume English |
| Suggestions    | `strsim` 0.11                                             | optimal string alignment behind `lemmon_syntax::nearest`, which owns the threshold deciding whether to suggest at all; shared so that every stage's did-you-mean reads the same, rather than each growing its own and drifting                                                                                |
| Corpus diffs   | `similar` 3.2                                             | the unified diff a failing corpus case reports, so what changed can be read from the failure instead of by blessing and diffing the result; dev-only, since `lemmon-corpus` is never anything but a dev-dependency                                                       |
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

Plugins ship `.lem` files (functions + types) bound via `lem-import` in `site.lem` (D23),
as the shell ships its own `ambient.lem` (D30); they cannot inject build-time JS (D06).
The shell also exposes a content API (`loadSite(config).collection('articles')`) for
tools like the revisions updater.

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
  Tree and projection carry byte ranges, since `.tags` is what a tree-sitter grammar is
  held to; the trailers render one-based `line:col` instead, so a blessed diff can be
  matched to the source by eye.
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
  It compares *rendered* HTML, never bytes (D28): `scripts/lib/rendered-html.ts` models
  the UA stylesheet's white-space processing so that everything D26 and D28 move is
  invisible to the comparison by construction, while a changed space inside `pre` or
  between two inline runs still fails (∆Ma4).

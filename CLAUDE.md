# Lemmon

Lemmon is a static site generator and a typed, tag-based template language. A
Node/TypeScript **shell** owns files, processes, plugins, and the dev server; a Rust
**core** (scanner → parser → checker → bytecode compiler → VM, plus comrak for markdown) is
compiled to WebAssembly and does everything deterministic. Distributed via npm; open source.
Its first production site is limulus.net, which it will take over from Eleventy + WebC once
a parity harness shows zero unexpected output diffs.

**Status:** early implementation, sequenced in `BACKLOG.md`; the current milestone is the
Phase 0 spike — one real page rendering end-to-end from the WASM core. Scanner → parser →
typed AST → compiler → VM run the spike subset untyped, `.md` goes through document mode,
and the checker (∆C8d) does not exist yet. Compilation is per-file: each file becomes a
`Unit`, and a link step joins units into the `Module` the VM runs (D25, invariant 11).
`crates/lemmon-wasm` exposes the resident `Core` (`compile`/`loadData`/`render`), and
`@limulus/lemmon-core` wraps the staged artifact — so `npm run rust:wasm` must have run
before `npm run verify`.

## Canonical references

Repo docs are the working distillation; read them before writing code:

- `docs/language.md` — the language: scanner, tags, expressions, types, namespaces, document
  mode. Keep open while working on the core.
- `docs/architecture.md` — core/shell split, WASM API, pipeline, VM, crate choices.
- `docs/decisions/` — the settled decisions: `README.md` is the index (one row per ID)
  and the deliberately deferred questions; every decision from D25 on also has a record,
  `Dnn-slug.md`, with its context, alternatives, and consequences. Do not re-litigate
  either ad hoc.

The full architecture record (with diagrams, migration plan, and rationale) lives in three
Claude Artifacts owned by Eric — "Shell & Core", "The Lemmon Language", and "From Eleventy
to Lemmon". Ask Eric for access if a repo doc seems to contradict itself; the artifacts win.

## Invariants

These hold for every change. A PR that breaks one is wrong even if its tests pass.

1. **The core is pure.** No filesystem, network, clock, randomness, or environment access
   inside the Rust core. Time, env values, and the asset manifest arrive as data from the
   shell. Same inputs → byte-identical output, always.
2. **Only Lemmon tags are parsed.** HTML is opaque text; the engine never builds a DOM. Text
   mode is interrupted only by `<lem.`, `<lem-`, `</lem-`.
3. **Heads never contain `>`.** A tag ends at the first `>` outside a string literal. That
   is why comparisons are `lt lte gt gte`, lambdas are `|a| expr`, and annotations are
   `: T`. Nothing may reintroduce `>` into a head.
4. **Tag shape is head-decidable.** Whether a tag is void or a block is determined by the
   tag name and its head alone (`= expr` marks the void form of `lem-let`/`lem-fn`, and
   `from` past the name marks `lem-let`'s void source form) — never by lookahead or name
   resolution. `<lem.…>` is always void.
5. **The `lem-` element namespace is reserved.** An unrecognized `lem-*` tag is a compile
   error with a did-you-mean, never pass-through.
6. **One call form.** `<lem.f(args)>` calls; bodies are passed capture-then-call via
   `<lem-let name>…</lem-let>`. No call-with-body syntax.
7. **Escaping is type-directed.** `<lem.…>` escapes strings, emits `html` verbatim; `raw()`
   is the only string→html cast. Nothing else may emit unescaped input.
8. **Every diagnostic carries a byte span.** Compile errors, load errors, and runtime errors
   all point at source. A throw without a span is a bug.
9. **Corpus discipline.** Any change to syntax or semantics lands with corpus coverage
   (`tests/corpus/`), and — once `tree-sitter-lemmon` exists — the Rust parser and the
   tree-sitter grammar must agree on tag boundaries for every corpus snippet (CI-enforced).
10. **Errors over guesses.** A forgotten close tag, a lost document-mode sentinel, an
    unknown name — all hard errors with precise spans. Never recover by guessing intent.
11. **Files compile separately.** A file's compiled unit is a function of its own text plus
    the compile-time facts it consumed (imported signatures, reference-index ids, asset
    manifest entries), which the compiler reports. Nothing may make one file's bytecode
    depend on another file's body — that is what lets a content edit leave its layouts'
    compiled units untouched (docs/decisions/README.md D25).

## Layout

```
crates/lemmon-syntax/   scanner, parser, CST/AST, diagnostics, document segmenter
crates/lemmon-corpus/   corpus discovery + blessing, used by every stage's tests
crates/lemmon-vm/       codegen + VM, document mode's markdown stage; checker to come
crates/lemmon-wasm/     wasm-bindgen boundary: the resident Core (lemmon-core's .wasm)
packages/core/          @limulus/lemmon-core — typed JS wrapper around the .wasm
packages/shell/         @limulus/lemmon-shell — CLI, walker, plugin host, dev server (stub)
tests/corpus/           golden inputs → expected trees + diagnostics; see its README
scripts/                repo tooling run by npm scripts (wasm staging + check, the spike);
                        scripts/lib/ is the logic they share, covered like a package
docs/                   language.md, architecture.md, decisions/, journal/
```

One npm workspace (`packages/*`) plus one Cargo workspace (`crates/*`), with a single shared
toolchain at the root: every script runs from the root, and packages carry no scripts of
their own. A new package copies `package.json` and both tsconfigs from an existing one, then
joins the root `tsconfig.build.json` references and the `packages` list in
`vitest.config.ts` (ESLint and `tscc` pick it up by glob). A new crate inherits
`[workspace.package]` via `field.workspace = true` and needs no other registration; pin
dependencies in the root `[workspace.dependencies]`. `rust-toolchain.toml` pins the
toolchain, components, and the `wasm32-unknown-unknown` target — rustup reconciles on every
invocation. Inside a package, `src/lib/` holds the functionality and `src/index.ts` is the
public API barrel; anything not re-exported there is internal.

Never check in a `.cargo/config.toml` setting `build.target-dir`: on a checkout shared
between machines of different platforms it collides their object files. `target/` placement
stays per-machine, via `CARGO_TARGET_DIR` in your own environment.

Published packages use the `@limulus` scope, named `@limulus/lemmon-<part>` (D18). Root and
packages are all `"private": true` and CI has no publish job: nothing is released yet, and
release tooling is undecided. Do not publish anything, or re-enable publishing, without
Eric.

## Commands

```bash
# The gate — exactly what pre-push runs: eslint, vitest with coverage, tscc, plus the
# Rust leg. Needs a Rust toolchain on PATH and a staged wasm artifact: run
# `npm run rust:wasm` first, and again after changing anything the boundary compiles in.
# CI runs the halves as separate jobs, so `verify` must stay defined as exactly
# `verify:js` + `rust` — extend those, or CI stops matching pre-push.
npm run verify
npm run verify:js      # the npm half: lint + test + tscc
npm run rust           # the Rust half: fmt --check, clippy -D warnings, cargo test
npm run lint:fix       # autofix lint — never concurrently with verify
npm run tscc           # typecheck root .ts config files + every package
npm run build          # emit packages/*/dist/{esm,types}
npm run clean:js       # dist, coverage, *.tsbuildinfo — what `prebuild` runs
npm run clean          # clean:js + artifacts/ + a literal ./target; it does NOT reach a
                       # redirected CARGO_TARGET_DIR — prefer `cargo clean -p <crate>` there

npx vitest run packages/core/src/lib/core.spec.ts    # single test file

# Rewrite every corpus expectation from what the code currently produces, then READ THE
# DIFF — blessing is a proposal, not a fix. Judge blessed output against what the source
# asked for, not for plausibility: wrong output has been blessed in twice. See
# tests/corpus/README.md.
npm run corpus:bless

npm run rust:cov       # Rust coverage ratchet — its own CI job (instrumentation needs its
                       # own RUSTFLAGS + target tree); needs `cargo install cargo-llvm-cov`
npm run rust:wasm      # cross-compile the boundary + stage into artifacts/wasm/. Needs
                       # wasm-bindgen-cli at the exact version pinned in Cargo.toml.
                       # Deliberately outside verify (second target tree), but verify's
                       # vitest leg consumes what it stages
npm run wasm:check     # compile the staged .wasm under this Node, assert exports/imports
npm run rust:wasm:test # exercise the boundary across the real ABI (wasm-bindgen-test)
npm run wasm:bench     # marshalling numbers only; correctness is the corpus gate's job
```

CI is build-once/consume-many: a `wasm` job stages and uploads the artifact, and the
`wasm-check`/`verify` matrices download it, so every supported Node is tested against the
exact bytes that would ship (accepting a `.wasm` depends on the Node major; building one
does not). `wasm:check`'s import allowlist (`scripts/wasm-artifact.ts`) is the mechanical
check on invariant 1 — a host capability pulled in by a dependency shows up there as a new
import.

Still aspirational; keep them true as they come to exist:

- `cargo fuzz run parser` — parser/VM fuzzing (never commit a panic repro as expected)
- `lem build | dev | check | fmt` — the CLI, once `@limulus/lemmon-shell` exists

## TypeScript setup

Node runs `.ts` files directly via type stripping, so relative imports name the real file
(`./x.ts`) and only erasable TypeScript survives — `erasableSyntaxOnly` enforces that. No
`enum` (use an `as const` object plus a `(typeof X)[keyof typeof X]` union), no parameter
properties, no runtime `namespace` or `import =`. `verbatimModuleSyntax` is also on: a name
used only as a type needs the inline `type` specifier
(`import { foo, type Bar } from './baz.ts'`).

Typechecking and building run the TypeScript 7 native compiler through the `typescript-7`
npm alias, invoked by explicit path because both TS packages ship a `tsc` bin and would
collide on `.bin/tsc`; typescript-eslint deliberately stays on stable `typescript` until
the stable native-compiler API in TS 7.1. Don't "simplify" this arrangement.

`tsconfig.base.json` holds the compiler options. Each package has `tsconfig.build.json`
(emits `dist/` from non-spec `src/`) and `tsconfig.json` (a single incremental noEmit
project over `src/**`, specs included — what `tscc`, editors, and typed lint use). Keep the
two independent: a reference from one to the other would make every typecheck emit `dist/`.
Diagnostics raised only during declaration emit surface in `npm run build`, not in `tscc`.

## Testing

- **BDD with 100% coverage** for `packages/*/src` and `scripts/lib` — no exceptions.
  Prefer assertions over defensive programming, and ask Eric before any
  `/* v8 ignore */`. The Rust crates gate on `cargo-llvm-cov` instead (`npm run
  rust:cov`, CI-only): a line ratchet, currently 97% — a floor to raise, never to lower
  to make a build pass. One carve-out: the scripts in `scripts/` themselves are linted
  and typechecked but not covered — logic worth testing goes in `scripts/lib/`
- **RSpec-style nesting** — `describe` for subject/context, `it('should …')` for
  assertions, no Given/When/Then. Rust mirrors it: `#[cfg(test)] mod tests` in the same
  file, one behaviour per `#[test]`, named as a sentence about the subject
- **Avoid `vi.mock()`** — exercise real dependencies; isolate from HTTP at the network
  layer (MSW)
- **vitest 4 constructs mock implementations with `new`**, and an arrow function is not
  constructable — supply the shape via `Mock.prototype`, or use a `function` expression
  when the implementation needs to throw
- Vitest runs one root config whose `projects` are listed inline with `extends: true` (a
  bare `packages/*` glob would silently drop the root `test` options); coverage thresholds
  are global across packages
- The golden corpus (`tests/corpus/`) is the shared truth across stages: `.lem`/`.md`
  inputs with one expectation file per stage (`.cst` and `.tags` from `lemmon-syntax`,
  `.html` from `lemmon-vm`). `crates/lemmon-corpus` owns discovery and blessing — add an
  extension there and one `check` call, never a second walker. The `.tags` projection is
  the tree-sitter agreement target (invariant 9), and an unreached `SyntaxKind` or
  unemitted `Op` fails the build — but treat that as a design question too: "nothing
  should reach this" is a legitimate answer, and deleting the unreached thing can be the
  honest fix
- Write corpus cases from the author's seat — the inputs an author would actually type (an
  escaped tag expected to survive as text, a typo'd close), breaking _early_ in a
  realistic file — not the shapes the implementation suggests
- A rule that suppresses diagnostics is tested on both halves: what is silenced _and_ what
  a neighbouring input must still report. Suppression keyed on a coincidence (same start
  byte, token adjacency) has hidden real errors more than once
- A check claimed to enforce an invariant is negative-tested first: feed it a real
  violation and watch it fail. Likewise a recorded fact: the test that matters is where
  consuming it changes an outcome — asserting its shape can pass forever while it is inert
- WASM: the boundary's logic is host-target unit tests in `crates/lemmon-wasm`;
  `npm run rust:wasm:test` re-exercises it across the real ABI, and `packages/core`'s
  specs cover the wrapper against the staged artifact. Never put ABI test files behind
  `#![cfg(target_arch = "wasm32")]` — the test macro already no-ops off-wasm32, and the
  cfg only hides the file from clippy and `cargo test`

## Code style

- ESM with the `node:` prefix for built-ins; relative imports include the `.ts` extension
- Strict typing: prefer interfaces for object types, explicit types on public APIs, typed
  errors with proper propagation; JSDoc where public API documentation helps
- Comments explain _why_ something unusual is done, never _what_ the code already shows
- Formatting is enforced (`@limulus/eslint-config` + prettier): no semicolons, single
  quotes, 92-char lines. rustfmt matches (`max_width = 92`) — take rustfmt's output rather
  than hand-tuning around it
- Naming: PascalCase classes, camelCase members, kebab-case files; acronyms stay upper
  case (`SSRFFetchOptions`, not `SsrfFetchOptions`)
- Don't re-invent the wheel — prefer a well-used npm package, or what a Rust dependency
  already ships, over your own implementation (rowan's `ast` module was re-implemented
  once before anyone looked)

## Backlog & journal (delto)

`BACKLOG.md` is delto-managed: items start with a `∆xxx` deltoid; `; needs:` edges are hard
prerequisites only. Use `npx @limulus/delto@1 surface` to find eligible work, `claim` before
starting, `complete` (then remove the item) when Eric accepts. Completed work gets a journal
entry in `docs/journal/`. Mint new deltoids with
`npx @limulus/delto@1 mint --journal-dir docs/journal` — never invent them.

## Working agreements

- The deferred language questions in `docs/decisions/README.md` stay deferred: when
  blocked on one, pick the provisional reading the docs already use, note it in the PR,
  move on.
- The eventual acceptance bar is the limulus.net parity harness (doc 3 of the architecture
  set); nothing in the core may assume that site's specifics to pass it.
- Fix a confirmed finding now when it is small and in scope of the crate being touched,
  even if it predates the diff; defer only what needs its own design. "Not in the diff"
  alone has been the wrong reason twice.
- A placeholder — a "not supported yet" or "not here yet" diagnostic in `crates/`, a
  "not yet" note in `docs/` — names the deltoid it waits on, and
  `crates/lemmon-corpus/tests/placeholders.rs` fails the build once that item has a
  journal entry. Landing a stage therefore includes sweeping the placeholders that name
  it; the gate is what notices when one is missed.
- A syntax or semantics change updates `docs/language.md` and `docs/decisions/` (the row,
  plus a record for any decision from D25 on) in the same pass, and records what the
  owning artifact must say in `docs/artifact-changes.md` — the artifacts themselves are
  reconciled once, at the end of Phase 1 (∆MlD). When a design point moves, sweep nearby
  prose for sentences the change just falsified — no gate ever reads the docs.

## Committing

Do not run `git commit` unless Eric has explicitly asked — he reviews diffs before they go
in, and landing work unasked denies him that step. Finishing a task means a working tree
clean of unintended changes and `npm run verify` passing; committing is separate and
user-initiated.

commitlint enforces `@commitlint/config-conventional` on every commit. Nothing is released
from this repo yet, but the history must stay release-grade so release tooling can derive
versions later: a breaking change bumps major (and must carry a `BREAKING CHANGE:` footer
describing the migration), `feat` bumps minor, `fix` bumps patch, other types don't bump.
Prefer the package or crate as the scope — `feat(core): add render()`,
`fix(shell): handle empty content dir`, `chore(deps): update dependencies`.

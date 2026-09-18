# Lemmon

Lemmon is a static site generator and a typed, tag-based template language. A
Node/TypeScript **shell** owns files, processes, plugins, and the dev server; a Rust
**core** (scanner → parser → bytecode compiler that checks as it lowers → VM, plus comrak
for markdown, its code blocks left as marks for the shell's highlighters) is compiled to
WebAssembly and does everything deterministic. Distributed via npm; open source.
Its first production site is limulus.net, which it will take over from Eleventy + WebC once
a parity harness shows zero unexpected output diffs.

**Status:** early implementation, sequenced in `BACKLOG.md`; Phase 0 (the spike — one real
page rendering end-to-end from the WASM core) is done and Phase 1 is under way. The
language runs end to end — scanner → parser → typed AST → compiler → VM, with `.md`
through document mode — and what each stage settled is the decision index
(`docs/decisions/README.md`, D01–D42), which is the status. The shape to hold in mind:
the checker is the compiler's own walk (D31); a host declares the ambient environment a
program compiles in, and `loadData` validates the host's data against the declared
schemas (D30, D36, D37); each file compiles to its own `Unit`, and a link step joins
units into the `Module` the VM runs (D25, invariant 11); `compile` returns its warnings
beside the module and keeps none on a unit. `crates/lemmon-wasm` exposes the resident
`Core` (`compile`/`loadData`/`plan`/`render`), and `@limulus/lemmon-core` wraps the staged
artifact — so `npm run rust:wasm` must have run before `npm run verify`.

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
   `: T` — and why there is no `|>`, `->`, or `=>` to propose. Nothing may reintroduce
   `>` into a head.
4. **Tag shape is head-decidable.** Whether a tag is void or a block is determined by the
   tag name and its head alone (`= expr` marks the void form of `lem-let`/`lem-fn`, and
   `from` past the name marks `lem-let`'s void source form) — never by lookahead or name
   resolution. `<lem.…>` is always void.
5. **The `lem-` element namespace is reserved.** An unrecognized `lem-*` tag is a compile
   error with a did-you-mean, never pass-through.
6. **One call form.** `<lem.f(args)>` calls a function and `<lem.xs.filter(…)>` a builtin
   method on its receiver (D34); bodies are passed capture-then-call via
   `<lem-let name>…</lem-let>`. No call-with-body syntax.
7. **Escaping is type-directed.** `<lem.…>` escapes strings, emits `html` verbatim; `raw()`
   is the only string→html cast. Nothing else may emit unescaped input.
8. **Every diagnostic carries a byte span.** Compile errors, load errors, and runtime errors
   all point at source. A throw without a span is a bug, and so is a panic: in the `.wasm`
   it is a trap that poisons the resident `Core`, so host-supplied offsets and paths are
   checked where the text is in hand, and a `debug_assert` guards nothing in release.
   Across the boundary a span also carries LSP `Position`s, computed in the core against
   the text that produced its unit (D33).
9. **Corpus discipline.** Any change to syntax or semantics lands with corpus coverage
   (`tests/corpus/`), and — once `tree-sitter-lemmon` exists — the Rust parser and the
   tree-sitter grammar must agree on tag boundaries for every corpus snippet (CI-enforced).
10. **Errors over guesses.** A forgotten close tag, a lost document-mode sentinel, an
    unknown name — all hard errors with precise spans. Never recover by guessing intent.
11. **Files compile separately.** A file's compiled unit is a function of its own text plus
    the compile-time facts it consumed (imported signatures and types, ambient bindings, a
    document's record, asset manifest entries), which the compiler reports. Nothing may
    make one file's bytecode depend on another file's body — that is what lets a content
    edit leave its layouts' compiled units untouched (docs/decisions/README.md D25).

## Layout

```
crates/lemmon-syntax/   scanner, parser, CST/AST, diagnostics, document segmenter
crates/lemmon-corpus/   corpus discovery + blessing + trailer display, used by every stage
crates/lemmon-vm/       types + checker-while-lowering + codegen + VM, document mode's
                        markdown and code-highlighting stages
crates/lemmon-wasm/     wasm-bindgen boundary: the resident Core (lemmon-core's .wasm);
                        build.rs pins the .wasm's stack
packages/core/          @limulus/lemmon-core — typed JS wrapper around the .wasm
packages/shell/         @limulus/lemmon-shell — CLI, walker, plugin host, dev server (stub)
tests/corpus/           golden inputs → expected trees + diagnostics; see its README
scripts/                repo tooling run by npm scripts (wasm staging + check, the spike);
                        scripts/lib/ is the logic they share, covered like a package
docs/                   language.md, architecture.md, decisions/, journal/, plans/
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

The `.wasm` is not a smaller native build: `usize` is 32 bits there, so arithmetic on a
length widens to `u64` before it multiplies (two builtins overflowed once each), and a trap
poisons the instance (invariant 8). The linked stack is pinned in
`crates/lemmon-wasm/build.rs` — never in `.cargo/config.toml` rustflags, which a `RUSTFLAGS`
variable replaces outright — and the parser's `MAX_DEPTH` and the VM's `MAX_NATIVE_DEPTH`
are sized to it, so raising either means re-running `npm run rust:wasm:test`, which
`verify` does not include.

Published packages use the `@limulus` scope, named `@limulus/lemmon-<part>` (D18). Root and
packages are all `"private": true` and CI has no publish job: nothing is released yet, and
release tooling is undecided, so a retired name or tag needs no alias or deprecation
message. Do not publish anything, or re-enable publishing, without Eric.

## Commands

```bash
# The gate — exactly what pre-push runs: eslint, vitest with coverage, tscc, plus the
# Rust leg. Needs a Rust toolchain on PATH and a staged wasm artifact: run
# `npm run rust:wasm` first, and again after changing anything the boundary compiles in.
# CI runs the halves as separate jobs, so `verify` must stay defined as exactly
# `verify:js` + `rust` — extend those, or CI stops matching pre-push.
npm run verify
npm run verify:js      # the npm half: lint + test + tscc
npm run rust           # the Rust half: fmt --check, clippy -D warnings, cargo test. Not
                       # rustdoc: after moving code between modules, run `cargo doc
                       # --workspace --no-deps` by hand for the links the move broke
npm run lint:fix       # autofix lint — never concurrently with verify
npm run tscc           # typecheck root .ts config files + every package
npm run build          # emit packages/*/dist/{esm,types}
npm run clean:js       # dist, coverage, *.tsbuildinfo — what `prebuild` runs
npm run clean          # clean:js + artifacts/ + a literal ./target; it does NOT reach a
                       # redirected CARGO_TARGET_DIR — prefer `cargo clean -p <crate>` there

npx vitest run packages/core/src/lib/core.spec.ts    # single test file

# Rewrite every corpus expectation from what the code currently produces, then READ THE
# DIFF — blessing is a proposal, not a fix. Judge blessed output, message text included,
# against what the source asked for, not for plausibility: wrong output has been blessed
# in twice. An empty diff for a change meant to alter a diagnostic means no case can see
# it; write that case. See tests/corpus/README.md.
npm run corpus:bless

npm run rust:cov       # Rust coverage ratchet — its own CI job (instrumentation needs its
                       # own RUSTFLAGS + target tree); needs `cargo install cargo-llvm-cov`.
                       # Never concurrently with verify; a ratchet failing for no visible
                       # reason wants `<target>/llvm-cov-target` deleted and a rerun
npm run rust:wasm      # cross-compile the boundary + stage into artifacts/wasm/. Needs
                       # wasm-bindgen-cli at the exact version pinned in Cargo.toml.
                       # Deliberately outside verify (second target tree), but verify's
                       # vitest leg consumes what it stages
npm run wasm:check     # compile the staged .wasm under this Node, assert exports/imports
npm run rust:wasm:test # exercise the boundary across the real ABI (wasm-bindgen-test), on
                       # the .wasm's own stack — outside verify, so a boundary or depth-cap
                       # change is not green until this has run
npm run wasm:bench     # marshalling numbers only; correctness is the corpus gate's job
npm run spike:the-rebound  # the Phase 0 fidelity gate: the-rebound through the staged
                       # .wasm against its 11ty build, compared as rendered HTML (D28);
                       # non-zero on an unexpected diff or a blown render budget
```

CI is build-once/consume-many: a `wasm` job stages and uploads the artifact, and the
`wasm-check`/`verify` matrices download it, so every supported Node is tested against the
exact bytes that would ship (accepting a `.wasm` depends on the Node major; building one
does not). `wasm:check`'s import allowlist (`scripts/wasm-artifact.ts`) is the mechanical
check on invariant 1 — a host capability pulled in by a dependency shows up there as a new
import.

`spike:the-rebound` and `wasm:bench` run in no gate, so when a boundary shape changes
meaning, grep `scripts/` for its consumers — they were the last holders of a retired
diagnostics contract once.

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
- **Avoid `vi.mock()`** — exercise real dependencies
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
- The corpus links one file per case except in `host/`, so a rule about the project — one
  diagnostic per binding, a per-unit operand rebased at link — needs a `host/` case or a
  two-file compile test to be seen breaking. A rule that can misfire twice shows its
  double diagnostic only in blessed output, since a unit test asserts one message
- Write corpus cases from the author's seat — the inputs an author would actually type (an
  escaped tag expected to survive as text, a typo'd close), breaking _early_ in a
  realistic file — not the shapes the implementation suggests; a green corpus proves
  nothing about a semantics change until the case an author would write is in it (every
  layout once read only its parameters). An author's mistakes are still well-formed
  tokens, though: a rule that counts tokens or positions owes the case where a scanner
  error sits in the counted slot — the recovery paths are where the bugs have been
- A diagnostic's span is asserted by the source text it covers, never by a counted
  offset: `at(source, needle)` in `crates/lemmon-vm/src/testing.rs` finds it,
  `all_of(source)` covers the whole text, and bracketing part of the needle with `«…»`
  picks one of several identical names (`at(src, "f(«a»: int")`). It places a host's
  positions too, so the span a fixture sends and the span a test asserts are written
  once. A span with no source behind it — a `Display` test, a `Names` unit test — stays
  a plain `Span::new`
- A rule that suppresses diagnostics is tested on both halves: what is silenced _and_ what
  a neighbouring input must still report. Suppression keyed on a coincidence (same start
  byte, token adjacency) has hidden real errors more than once. The same holds for a diff
  gate's allowlist and the normalization in front of it: test what it erases, not only
  what it selects
- A check claimed to enforce an invariant is negative-tested first: feed it a real
  violation and watch it fail. Likewise a recorded fact: the test that matters is where
  consuming it changes an outcome — asserting its shape can pass forever while it is
  inert, and coverage cannot tell, since an executed line counts with every conjunct on it
  inert. So perturb what a test claims to pin and watch it fail — above all a helper used
  on both sides of an assertion, or a test about to be deleted as redundant
- Undo a probe by reversing the probe, never with `git checkout -- <path>`: that restores
  the file from HEAD and discards every uncommitted edit in it, which has cost three
  sessions their work. Copy the file aside before a planned perturbation
- The host fixtures are shaped like `@limulus/lemmon-shell` (`Page`, `Build`, `site`,
  `layout`); a feature that exists for hosts this repo does not contain owes a test with
  a host that differs — both real bugs in the ambient types lived outside that shape
- WASM: the boundary's logic is host-target unit tests in `crates/lemmon-wasm`;
  `npm run rust:wasm:test` re-exercises it across the real ABI, and `packages/core`'s
  specs cover the wrapper against the staged artifact. Never put ABI test files behind
  `#![cfg(target_arch = "wasm32")]` — the test macro already no-ops off-wasm32, and the
  cfg only hides the file from clippy and `cargo test`. A boundary finding is reproduced
  against the staged artifact through `@limulus/lemmon-core` before it is a finding

## Code style

- ESM with the `node:` prefix for built-ins; relative imports include the `.ts` extension
- Strict typing: prefer interfaces for object types, explicit types on public APIs, typed
  errors with proper propagation; JSDoc where public API documentation helps
- Comments explain _why_ something unusual is done, never _what_ the code already shows
- Formatting is enforced (`@limulus/eslint-config` + prettier): no semicolons, single
  quotes, 92-char lines. rustfmt matches (`max_width = 92`) — take rustfmt's output rather
  than hand-tuning around it
- Naming: PascalCase classes, camelCase members, kebab-case files; an acronym is one
  word in a name on both sides (`JsonValue`, `Vm`, `Utf16`), as Rust's API guidelines
  have it and clippy's `upper_case_acronyms` already enforces for the all-caps form
- Don't re-invent the wheel — prefer a well-used npm package, or what a Rust dependency
  already ships, over your own implementation (rowan's `ast` module, syntect's serialized
  grammar set, and yaml-rust's loader were each re-implemented once before anyone
  looked). A policy lives once, too: did-you-mean goes through `lemmon_syntax::nearest`,
  never a per-crate `strsim` call with its own threshold
- A file is too big when its `//!` cannot state one responsibility — judged by the code
  above the inline test module, which is why there is no line-count lint
- A rule stated in two places gets one predicate and one sentence, called from both: the
  compiler owns a refusal's wording and the link step prefixes it with what reached it
- The compiler's walk is also the checker (D31), so an error arm that stops lowering a
  construct still lowers its operands (`discard` in `compile/calls.rs`), or the
  unused-binding rule warns about the names they read; the typing rules themselves are
  pure functions in `compile/rules.rs`, whose module doc says how one is written

## Backlog & journal (delto)

`BACKLOG.md` is delto-managed: items start with a `∆xxx` deltoid; `; needs:` edges are hard
prerequisites only. Use `npx @limulus/delto@1 surface` to find eligible work, `claim` before
starting, `complete` (then remove the item) when Eric accepts. Completed work gets a journal
entry in `docs/journal/`. Mint new deltoids with
`npx @limulus/delto@1 mint --journal-dir docs/journal` — never invent them. An item
planned by an earlier session names its plan in `docs/plans/`; start from that plan, and
fold it into the journal entry's Planning section and delete it when the item completes.

An item's _why_ is reliable and its _how_ is not: items are written at the end of other
work, from memory of the code. Verify its technical claims before planning around them,
and check the plan against the item's stated reason, not only its instruction. An item
that plans as several is split at plan time, each part minted; a session that has
reshaped the backlog hands off through `docs/plans/` rather than building.

## Working agreements

- The deferred language questions in `docs/decisions/README.md` stay deferred: when
  blocked on one, check the entry against the invariants (a deferred `|>` sat there
  asserting what invariant 3 forbids), pick the provisional reading the docs already use,
  note it in the PR, move on.
- Take a decision number against every checkout (`git worktree list`), not just this
  branch: another session's uncommitted record has held the next ID.
- The eventual acceptance bar is the limulus.net parity harness (doc 3 of the architecture
  set), which compares rendered HTML, not bytes (D28); nothing in the core may assume that
  site's specifics to pass it. When the question is whether something is needed, count
  that site's source first (its checkout sits beside this repo): six void component
  calls and zero bodied ones settled document mode's shape in minutes.
- Fix a confirmed finding now when it is small and in scope of the crate being touched,
  even if it predates the diff; defer only what needs its own design. "Not in the diff"
  alone has been the wrong reason twice. When a self-review turns up more work, say so
  rather than widening the diff, and a finding picked up inside another item lands as its
  own commit on top of that item's.
- A claimed simplification, a line count, or a dependency's cost is measured before it is
  offered — one `.wasm` build for the dependency, a diff split into production, test, and
  doc lines for the refactor — and reported as what is paid and when, not as a table.
- Review a plan before building it and the diff after; they find different things. A
  review agent gets "do not modify files" in every prompt, and a verifier that must
  mutate the tree gets its own worktree: one ran `git checkout --` on a file it probed.
- A placeholder names the deltoid it waits on, on the same line as the phrase the gate
  reads — in `crates/`, "not here yet", "not supported yet", or "not yet supported"; in
  `docs/`, "not yet" — and `crates/lemmon-corpus/tests/placeholders.rs` fails the build
  once that item has a journal entry. The gate reads only those trees and that phrasing:
  a promise in `packages/`, or a planned mechanism written in the present tense, rots
  unseen. Landing a stage therefore includes sweeping the placeholders that name it.
- A syntax or semantics change updates `docs/language.md` and `docs/decisions/` (the row,
  plus a record for any decision from D25 on) in the same pass, and records what the
  owning artifact must say in `docs/artifact-changes.md` — amending an earlier entry the
  change contradicts rather than appending — since the artifacts are reconciled once, at
  the end of Phase 1 (∆MlD). When a design point moves, or is decided against, grep for
  its nouns and sweep the prose the change just falsified, and write the diff's own doc
  sentences last: no gate ever reads the docs.

## Committing

Do not run `git commit` unless Eric has explicitly asked — he reviews diffs before they go
in, and landing work unasked denies him that step. Finishing a task means a working tree
clean of unintended changes and `npm run verify` passing — read `git diff` before saying
so, and name which gates ran, since `verify` covers neither the ABI suite nor the spike;
committing is separate and user-initiated.

commitlint enforces `@commitlint/config-conventional` on every commit. Nothing is released
from this repo yet, but the history must stay release-grade so release tooling can derive
versions later: a breaking change bumps major (and must carry a `BREAKING CHANGE:` footer
describing the migration), `feat` bumps minor, `fix` bumps patch, other types don't bump.
Prefer the package or crate as the scope — `feat(core): add render()`,
`fix(shell): handle empty content dir`, `chore(deps): update dependencies`.

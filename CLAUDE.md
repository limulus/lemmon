# Lemmon

Lemmon is a static site generator and a typed, tag-based template language. A
Node/TypeScript **shell** owns files, processes, plugins, and the dev server; a Rust
**core** (scanner → parser → checker → bytecode compiler → VM, plus comrak for markdown) is
compiled to WebAssembly and does everything deterministic. Distributed via npm; open source.
Its first production site is limulus.net, which it will take over from Eleventy + WebC once
a parity harness shows zero unexpected output diffs.

**Status:** early implementation. The work is sequenced in `BACKLOG.md` (delto-managed); the
current milestone is the Phase 0 spike — one real page rendering end-to-end from the WASM
core. Both halves of the repo scaffolding exist (see Layout); the scanner is landing with
∆FDQ, and nothing above it (parser, checker, VM) exists yet.

## Canonical references

Repo docs are the working distillation; read them before writing code:

- `docs/language.md` — the language: scanner, tags, expressions, types, namespaces, document
  mode. Keep open while working on the core.
- `docs/architecture.md` — core/shell split, WASM API, pipeline, VM, crate choices.
- `docs/decisions.md` — decisions D01–D23, **settled**; and the short list of deliberately
  deferred questions. Do not re-litigate either ad hoc.

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
   tag name and its head alone (`= expr` marks the void form of `lem-let`/`lem-fn`) — never
   by lookahead or name resolution. `<lem.…>` is always void.
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

## Layout

```
crates/lemmon-syntax/   scanner, parser, CST/AST, diagnostics           (scanner → typed AST)
crates/lemmon-corpus/   corpus discovery + blessing, used by every stage's tests
crates/lemmon-vm/       types, checker, codegen, VM, document mode      (skeleton)
crates/lemmon-wasm/     wasm-bindgen boundary (lemmon-core's .wasm)     (skeleton)
packages/core/          @limulus/lemmon-core — typed JS wrapper around the .wasm (stub)
packages/shell/         @limulus/lemmon-shell — CLI, walker, plugin host, dev server (stub)
tests/corpus/           golden inputs → expected trees + diagnostics; see its README
scripts/                repo tooling run by npm scripts (wasm staging + check)
docs/                   language.md, architecture.md, decisions.md, journal/
```

The repo is an npm workspace (`workspaces: ["packages/*"]`) with one shared toolchain at the
root — every script below runs from the root, and packages carry no scripts of their own.
Adding a package means: create `packages/<name>/` with `package.json`, `tsconfig.json`, and
`tsconfig.build.json` modelled on an existing one, then add it to the root
`tsconfig.build.json` references and the `packages` list in `vitest.config.ts`. ESLint and
`tscc` pick up `packages/*` by glob.

It is also a Cargo workspace (`members = ["crates/*"]`) sharing one `[workspace.package]`
block, so a new crate inherits version/edition/repository with `field.workspace = true` and
needs no registration anywhere. `rust-toolchain.toml` pins the exact toolchain and lists the
components and `wasm32-unknown-unknown` target; rustup reconciles an existing install
against it on every invocation, so nobody has to `rustup target add` by hand. Pin new
dependencies in the root `[workspace.dependencies]` and reference them as
`{ workspace = true }`.

There is deliberately **no checked-in `.cargo/config.toml` setting `build.target-dir`**, and
none should be added. Where a checkout is shared between machines of different platforms, a
committed target directory points both at the same path and their object files collide. Keep
`target/` placement a per-machine concern — set `CARGO_TARGET_DIR` in your own environment
if that applies to you.

Published packages use the `@limulus` npm scope, named `@limulus/lemmon-<part>` (see
docs/decisions.md D18). Both packages and the root are `"private": true` and CI has no
publish job (see `.github/workflows/ci.yaml`): nothing is released yet, and how the
monorepo's packages will be versioned and published (semantic-release or otherwise) is
undecided. Do not publish anything, and do not re-enable publishing, without Eric.

## Commands

```bash
# The gate: eslint, vitest with coverage, tsc, and the Rust leg (cargo fmt --check,
# clippy -D warnings, cargo test --workspace). Run it after making changes — it is what
# pre-push runs. It needs a Rust toolchain on PATH; rust-toolchain.toml supplies
# everything else on first invocation.
#
# CI runs its two halves as separate jobs rather than this one command, so that the
# host-target Rust checks run once instead of once per node version. `verify` is defined
# as exactly `verify:js` + `rust` — keep that true when adding a leg, or CI stops being
# equivalent to pre-push.
npm run verify
npm run verify:js   # the npm half alone: lint + test + tscc

# Autofix lint. Run it first, then re-run verify — never concurrently with it, since it
# rewrites the same files verify is reading.
npm run lint:fix

# Test (single file)
npx vitest run packages/core/src/lib/example.spec.ts

npm run build     # emit packages/*/dist/{esm,types} via the native compiler
npm run clean     # the clean:js set, plus artifacts/ and a literal ./target. It does
                  # NOT reach a redirected CARGO_TARGET_DIR — clean that yourself, and
                  # prefer `cargo clean -p <crate>` over a bare `cargo clean` if the
                  # directory is shared with other projects
npm run clean:js  # packages/*/dist, coverage, *.tsbuildinfo — what `prebuild` runs, so
                  # that building the TS never throws away cargo's cache or the staged wasm
npm run tscc      # typecheck: root .ts config files + every packages/*/tsconfig.json

# The Rust leg on its own — same three checks `verify` runs, for a tighter loop.
npm run rust
cargo test --workspace                       # core unit + golden corpus tests

# Rewrite every corpus expectation from what the code currently produces, then READ THE
# DIFF — blessing is a proposal, not a fix. See tests/corpus/README.md.
npm run corpus:bless

# Rust coverage. Needs `cargo install cargo-llvm-cov --locked` (rust-toolchain.toml
# supplies the llvm-tools-preview component). Kept out of `verify` and run as its own CI
# job because instrumentation needs different RUSTFLAGS and so a second target tree.
npm run rust:cov

# Cross-compile the boundary crate (release) and stage the .wasm into artifacts/wasm/.
# Kept out of `verify` because it populates a second target tree.
npm run rust:wasm

# Consume the staged .wasm: compile it under this Node, assert the boundary's exports
# and that it imports nothing outside the wasm-bindgen placeholders.
npm run wasm:check
```

CI is build-once/consume-many: a `wasm` job stages the artifact and uploads it, and a
`wasm-check` matrix downloads it so every supported Node is tested against the exact bytes
that would ship rather than its own rebuild. Building a `.wasm` never consults Node;
_accepting_ one does, since enabled WebAssembly proposals move between Node majors.
`wasm:check`'s import allowlist (`scripts/wasm-artifact.ts`) is a mechanical check on
invariant 1 — a host capability pulled in by some dependency shows up there as a new import
module.

Still aspirational; keep them true as they come to exist:

- `cargo fuzz run parser` — parser/VM fuzzing (never commit a panic repro as expected)
- `lem build | dev | check | fmt` — the CLI, once `@limulus/lemmon-shell` exists

## TypeScript Setup

Node runs `.ts` files directly via type stripping (v22.18+/v24), so imports name the real
file: `import x from './x.ts'`. Only the erasable subset of TypeScript survives that —
`erasableSyntaxOnly` is on to enforce it at compile time. It forbids:

- `enum` declarations — use an `as const` object plus a derived union type instead:

  ```ts
  export const Direction = { Up: 1, Right: 2, Down: 3, Left: 4 } as const
  export type Direction = (typeof Direction)[keyof typeof Direction]
  ```

- Parameter properties (`constructor(readonly foo: string) {}`) — declare the field and
  assign it in the constructor body
- `namespace` with runtime code, and `import =` aliases

`verbatimModuleSyntax` is also on, so any imported name used only as a type needs the `type`
keyword. Prefer inline specifiers over a separate statement:
`import { foo, type Bar } from './baz.ts'`.

### Two TypeScript packages

Typechecking and building run the TypeScript 7 native compiler through the `typescript-7`
npm alias, invoked by explicit path (`node node_modules/typescript-7/bin/tsc`) because both
packages ship a `tsc` bin and would collide on `.bin/tsc`. typescript-eslint deliberately
stays on the stable `typescript` package — it has no native-compiler support until the
stable API in TS 7.1. Don't "simplify" this arrangement.

### Project layout per package

`tsconfig.base.json` at the root holds the compiler options. Each package has
`tsconfig.build.json` (emits `dist/esm` + `dist/types` from non-spec `src/`) and
`tsconfig.json` (a single incremental noEmit project over `src/**`, specs included — what
`tscc`, editors, and typed lint use). Keep them independent: a project reference from
`tsconfig.json` to the build config would make every typecheck emit `dist/`. `tscc` passes
the root `tsconfig.json` (root `.ts` config files) plus `packages/*/tsconfig.json` by shell
glob to `tsc --build`, so a new package is typechecked without editing any references.
Diagnostics raised only during declaration emit surface in `npm run build` (CI runs it after
`verify`), not in `tscc`.

## Testing

- **BDD with 100% coverage** — no exceptions. Prefer assertions over defensive programming;
  if you think you need a `/* v8 ignore … */`, pause and confirm with the user first. This
  is the TypeScript rule for `packages/*/src`, which is what `vitest.config.ts` scopes
  coverage to. **The Rust crates gate on `cargo-llvm-cov` instead** (`npm run rust:cov`,
  CI-only), at a lower bar — a line ratchet set from the measured baseline, currently 97%.
  It is a floor to raise as the number rises, never to lower to make a build pass, and it
  counts the colocated `#[cfg(test)]` modules along with the code. One carve-out remains:
  **`scripts/` is linted and typechecked but not covered**, since it is build tooling
  exercised by running it. Weigh that before putting logic worth testing in `scripts/`;
  `scripts/check-wasm.ts` is already close to the line
- **RSpec-style nesting** — `describe` for subject/context, `it('should …')` for assertions.
  No Given/When/Then
- **Avoid `vi.mock()`** — prefer tests that exercise real dependencies, and mock at the
  network layer (MSW) when you need to isolate from HTTP
- **vitest 4 constructs mock implementations with `new`.** An arrow function is not
  constructable, so `vi.fn().mockImplementation(() => ({ … }))` throws when the subject
  calls `new`. Supply the shape via `Mock.prototype`, or use a `function` expression when
  the implementation needs to throw
- Vitest runs one root config whose `projects` are listed inline with `extends: true` (a
  bare `packages/*` glob would give each package a blank config and silently drop the root
  `test` options); coverage thresholds are global across all packages
- Rust: `#[cfg(test)] mod tests` in the same file as the code, one behaviour per `#[test]`
  with a name that reads as a sentence about the subject — the closest Rust gets to the
  `it('should …')` convention above
- The golden corpus in `tests/corpus/` is the shared truth across stages: a `.lem` input
  plus one expectation file per stage (`.cst` and `.tags` from `lemmon-syntax` today).
  `crates/lemmon-corpus` does discovery and blessing for all of them — add an extension
  there and one `check` call, never a second walker. Its `.tags` projection is the
  tree-sitter agreement target (invariant 9), and a `SyntaxKind` no case reaches fails the
  build, which is what makes "lands with corpus coverage" mechanical
- WASM (when it exists): wasm-bindgen-test via Node for the boundary

## Code Style Guidelines

- **ESM**: Use ES Modules (`import/export`, not `require()`)
  - Use the `node:` prefix for Node.js built-ins
- **File Extensions**: Always include `.ts` in relative imports
- **Typing**: Use strict TypeScript typing, prefer interfaces for object types
  - Prefer explicit types over inference for public APIs
- **Formatting**: no semicolons (rely on ASI), single quotes, 92-char lines — enforced by
  `@limulus/eslint-config` + prettier (same conventions as limulus.net)
- **Rust**: edition 2024, `cargo fmt` + `clippy -D warnings` (both in `npm run verify`).
  `rustfmt.toml` sets `max_width = 92` to match the TypeScript width; take rustfmt's output
  rather than hand-tuning around it
- **Naming**:
  - Classes: PascalCase
  - Methods/variables: camelCase
  - Files: kebab-case for modules
  - Acronyms stay upper case (i.e. use `SSRFFetchOptions`, not `SsrfFetchOptions`)
- **Error Handling**: Use typed errors and proper propagation
- **Comments**:
  - Only add comments to explain _why_ code is doing something unusual or non-obvious
  - Avoid comments that merely describe _what_ the code is doing (the code already shows
    that)
  - Use JSDoc for public API documentation where appropriate
- Don't re-invent the wheel — prefer a well-used npm package over your own implementation
  that may miss important edge cases

## Project Structure (per package)

- `src/lib/` — core functionality
- `src/index.ts` — the public API barrel; anything not re-exported here is internal

## Backlog & journal (delto)

`BACKLOG.md` is delto-managed: items start with a `∆xxx` deltoid; `; needs:` edges are hard
prerequisites only. Use `npx @limulus/delto@1 surface` to find eligible work, `claim` before
starting, `complete` (then remove the item) when Eric accepts. Completed work gets a journal
entry in `docs/journal/`. Mint new deltoids with
`npx @limulus/delto@1 mint --journal-dir docs/journal` — never invent them.

## Working agreements

- The deferred language questions in `docs/decisions.md` stay deferred: when blocked on one,
  pick the provisional reading the docs already use, note it in the PR, move on.
- The eventual acceptance bar is the limulus.net parity harness (doc 3 of the architecture
  set); nothing in the core may assume that site's specifics to pass it.

## Committing

Do not run `git commit` unless the user has explicitly asked you to. The user reviews diffs
before they go in — landing work without a review denies them that step. Finishing a task
means the working tree is clean of uncommitted _unintended_ changes and `npm run verify`
passes; staging and committing is a separate, user-initiated step.

## Commit Message Guidelines

Follow `conventionalcommits.org`, specifically the `@commitlint/config-conventional` format
(commitlint enforces it on every commit). Nothing is released from this repo yet, but the
history must stay release-grade so `semantic-release` — or whatever monorepo release tooling
is chosen — can derive versions from it later.

- `feat: msg`: features
- `fix: msg`: bug fixes
- `refactor: msg`: code improvements that do not affect functionality
- `test: msg`: changes in tests only, does not affect functionality
- `docs: msg`: changes to documentation
- `ci: msg`: build pipeline
- `chore: msg`: updating dependencies, miscellany
- Breaking changes to exposed API surfaces must be documented with a footer/trailer. For
  example:

  ```
  feat: remove the `POST /api/spline/reticulate` endpoint

  BREAKING CHANGE: Support for previously deprecated spline reticulation
    has been removed. Use `POST /api/spline/frobnicate` instead.
  ```

- Scopes may also be used — prefer the package or crate as the scope in this monorepo:
  - `feat(core): add render()`
  - `fix(shell): handle empty content dir`
  - `docs(readme): add frobnication section`
  - `chore(deps): update dependencies`

Only certain commit messages will trigger changes to the semantic version of the software:

- A breaking change will trigger a major version bump, regardless of the prefix
- The `feat` prefix bumps the minor version
- The `fix` prefix bumps the patch version
- All other commit messages have no effect on the version

# Lemmon

Lemmon is a static site generator and a typed, tag-based template language. A
Node/TypeScript **shell** owns files, processes, plugins, and the dev server; a Rust
**core** (scanner → parser → checker → bytecode compiler → VM, plus comrak for markdown)
is compiled to WebAssembly and does everything deterministic. Distributed via npm; open
source. Its first production site is limulus.net, which it will take over from
Eleventy + WebC once a parity harness shows zero unexpected output diffs.

**Status:** pre-implementation. The work is sequenced in `BACKLOG.md` (delto-managed);
the current milestone is the Phase 0 spike — one real page rendering end-to-end from the
WASM core. The npm half of the repo scaffolding exists (see Layout); the Rust half is
still backlog `∆059`.

## Canonical references

Repo docs are the working distillation; read them before writing code:

- `docs/language.md` — the language: scanner, tags, expressions, types, namespaces,
  document mode. Keep open while working on the core.
- `docs/architecture.md` — core/shell split, WASM API, pipeline, VM, crate choices.
- `docs/decisions.md` — decisions D01–D23, **settled**; and the short list of
  deliberately deferred questions. Do not re-litigate either ad hoc.

The full architecture record (with diagrams, migration plan, and rationale) lives in
three Claude Artifacts owned by Eric — "Shell & Core", "The Lemmon Language", and
"From Eleventy to Lemmon". Ask Eric for access if a repo doc seems to contradict itself;
the artifacts win.

## Invariants

These hold for every change. A PR that breaks one is wrong even if its tests pass.

1. **The core is pure.** No filesystem, network, clock, randomness, or environment
   access inside the Rust core. Time, env values, and the asset manifest arrive as data
   from the shell. Same inputs → byte-identical output, always.
2. **Only Lemmon tags are parsed.** HTML is opaque text; the engine never builds a DOM.
   Text mode is interrupted only by `<lem.`, `<lem-`, `</lem-`.
3. **Heads never contain `>`.** A tag ends at the first `>` outside a string literal.
   That is why comparisons are `lt lte gt gte`, lambdas are `|a| expr`, and annotations
   are `: T`. Nothing may reintroduce `>` into a head.
4. **Tag shape is head-decidable.** Whether a tag is void or a block is determined by
   the tag name and its head alone (`= expr` marks the void form of `lem-let`/`lem-fn`)
   — never by lookahead or name resolution. `<lem.…>` is always void.
5. **The `lem-` element namespace is reserved.** An unrecognized `lem-*` tag is a
   compile error with a did-you-mean, never pass-through.
6. **One call form.** `<lem.f(args)>` calls; bodies are passed capture-then-call via
   `<lem-let name>…</lem-let>`. No call-with-body syntax.
7. **Escaping is type-directed.** `<lem.…>` escapes strings, emits `html` verbatim;
   `raw()` is the only string→html cast. Nothing else may emit unescaped input.
8. **Every diagnostic carries a byte span.** Compile errors, load errors, and runtime
   errors all point at source. A throw without a span is a bug.
9. **Corpus discipline.** Any change to syntax or semantics lands with corpus coverage
   (`tests/corpus/`), and — once `tree-sitter-lemmon` exists — the Rust parser and the
   tree-sitter grammar must agree on tag boundaries for every corpus snippet (CI-enforced).
10. **Errors over guesses.** A forgotten close tag, a lost document-mode sentinel, an
    unknown name — all hard errors with precise spans. Never recover by guessing intent.

## Layout

```
crates/lemmon-syntax/   scanner, parser, CST/AST, diagnostics          (planned, ∆059)
crates/lemmon-vm/       types, checker, codegen, VM, document mode      (planned, ∆059)
crates/lemmon-wasm/     wasm-bindgen boundary (lemmon-core's .wasm)     (planned, ∆059)
packages/core/          @limulus/lemmon-core — typed JS wrapper around the .wasm (stub)
packages/shell/         @limulus/lemmon-shell — CLI, walker, plugin host, dev server (stub)
tests/corpus/           golden inputs → expected HTML + diagnostics     (planned, ∆TEJ)
docs/                   language.md, architecture.md, decisions.md, journal/
```

The repo is an npm workspace (`workspaces: ["packages/*"]`) with one shared toolchain at
the root — every script below runs from the root, and packages carry no scripts of their
own. Adding a package means: create `packages/<name>/` with `package.json`,
`tsconfig.json`, and `tsconfig.build.json` modelled on an existing one, then add it to
the root `tsconfig.build.json` references and the `packages` list in `vitest.config.ts`.
ESLint and `tscc` pick up `packages/*` by glob.

Published packages use the `@limulus` npm scope, named `@limulus/lemmon-<part>` (see
docs/decisions.md D18). Both packages and the root are `"private": true`
and CI has no publish job (see `.github/workflows/ci.yaml`): nothing is released yet, and
how the monorepo's packages will be versioned and published (semantic-release or
otherwise) is undecided. Do not publish anything, and do not re-enable publishing,
without Eric.

## Commands

```bash
# The gate: eslint, vitest with coverage, and tsc. Run it after making changes — it is
# exactly what pre-push and CI run, so green here means green there.
npm run verify

# Autofix lint. Run it first, then re-run verify — never concurrently with it, since it
# rewrites the same files verify is reading.
npm run lint:fix

# Test (single file)
npx vitest run packages/core/src/lib/example.spec.ts

npm run build     # emit packages/*/dist/{esm,types} via the native compiler
npm run clean     # remove packages/*/dist, coverage, *.tsbuildinfo
npm run tscc      # typecheck: root .ts config files + every packages/*/tsconfig.json
```

Until the Rust scaffolding lands (backlog `∆059`) these are aspirational; keep them true
as they come to exist:

- `cargo test --workspace` — core unit + corpus tests
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
`tsconfig.build.json` (emits `dist/esm` + `dist/types` from `src/`, excludes specs) and
`tsconfig.json` (noEmit; covers specs and references the build config). `tscc` passes
the root `tsconfig.json` (root config files) plus `packages/*/tsconfig.json` by shell glob
to `tsc --build`, so a new package is typechecked without editing any references. Note
that `tscc` also emits `packages/*/dist` as a side effect of the build reference — same as
the single-package template it derives from.

## Testing

- **BDD with 100% coverage** — no exceptions. Prefer assertions over defensive programming;
  if you think you need a `/* v8 ignore … */`, pause and confirm with the user first
- **RSpec-style nesting** — `describe` for subject/context, `it('should …')` for assertions.
  No Given/When/Then
- **Avoid `vi.mock()`** — prefer tests that exercise real dependencies, and mock at the
  network layer (MSW) when you need to isolate from HTTP
- **vitest 4 constructs mock implementations with `new`.** An arrow function is not
  constructable, so `vi.fn().mockImplementation(() => ({ … }))` throws when the subject
  calls `new`. Supply the shape via `Mock.prototype`, or use a `function` expression when
  the implementation needs to throw
- Vitest runs one root config whose `projects` are listed inline with `extends: true`
  (a bare `packages/*` glob would give each package a blank config and silently drop the
  root `test` options); coverage thresholds are global across all packages
- Rust/WASM (when it exists): wasm-bindgen-test via Node for the boundary; the golden
  corpus in `tests/corpus/` is the shared truth for the parser and the tree-sitter grammar

## Code Style Guidelines

- **ESM**: Use ES Modules (`import/export`, not `require()`)
  - Use the `node:` prefix for Node.js built-ins
- **File Extensions**: Always include `.ts` in relative imports
- **Typing**: Use strict TypeScript typing, prefer interfaces for object types
  - Prefer explicit types over inference for public APIs
- **Formatting**: no semicolons (rely on ASI), single quotes, 92-char lines — enforced by
  `@limulus/eslint-config` + prettier (same conventions as limulus.net)
- **Rust** (when it exists): 2021+ edition, `cargo fmt` + `clippy -D warnings`
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

`BACKLOG.md` is delto-managed: items start with a `∆xxx` deltoid; `; needs:` edges are
hard prerequisites only. Use `npx @limulus/delto@1 surface` to find eligible work,
`claim` before starting, `complete` (then remove the item) when Eric accepts. Completed
work gets a journal entry in `docs/journal/`. Mint new deltoids with
`npx @limulus/delto@1 mint --journal-dir docs/journal` — never invent them.

## Working agreements

- The deferred language questions in `docs/decisions.md` stay deferred: when blocked on
  one, pick the provisional reading the docs already use, note it in the PR, move on.
- The eventual acceptance bar is the limulus.net parity harness (doc 3 of the
  architecture set); nothing in the core may assume that site's specifics to pass it.

## Committing

Do not run `git commit` unless the user has explicitly asked you to. The user reviews diffs
before they go in — landing work without a review denies them that step. Finishing a task
means the working tree is clean of uncommitted _unintended_ changes and `npm run verify`
passes; staging and committing is a separate, user-initiated step.

## Commit Message Guidelines

Follow `conventionalcommits.org`, specifically the `@commitlint/config-conventional`
format (commitlint enforces it on every commit). Nothing is released from this repo yet,
but the history must stay release-grade so `semantic-release` — or whatever monorepo
release tooling is chosen — can derive versions from it later.

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

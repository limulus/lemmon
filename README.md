# Lemmon

A static site generator and a typed, tag-based template language.

A Node/TypeScript **shell** owns files, processes, plugins, and the dev server; a Rust
**core** (scanner → parser → checker → bytecode compiler → VM, plus comrak for markdown)
compiled to WebAssembly does everything deterministic.

**Status:** pre-implementation. Nothing is published yet. See `BACKLOG.md` for the plan
and `docs/` for the language, architecture, and decision record.

## Layout

| Path | What |
|---|---|
| `packages/core/` | `@limulus/lemmon-core` — typed JS wrapper around the WASM core (stub) |
| `packages/shell/` | `@limulus/lemmon-shell` — `lem` CLI, walker, plugin host, dev server |
| `crates/` | Rust core workspace — `lemmon-syntax`, `lemmon-vm`, `lemmon-wasm` (skeletons) |
| `docs/` | `language.md`, `architecture.md`, `decisions/`, `journal/` |

## Development

Requires Node (see `engines`) and a Rust toolchain — [rustup](https://rustup.rs) is
enough; `rust-toolchain.toml` pins the exact version and installs the components and the
`wasm32-unknown-unknown` target on the first `cargo` invocation.

```sh
npm install
npm run verify     # lint + test (100% coverage) + typecheck + the two wasm harnesses in
                   # one pass + cargo fmt/clippy/test
npm run build      # emit packages/*/dist
npm run rust:wasm  # cross-compile the boundary crate and stage the .wasm
npm run wasm:check # compile the staged .wasm under this Node and check its boundary
```

`npm run verify` is the whole gate for both halves of the repo and is what pre-push runs.
CI runs its parts as separate jobs so the Rust checks run once rather than once per Node
version, and so the `.wasm` is built once and then exercised on every supported Node.

Contributor guidance, invariants, and conventions live in `CLAUDE.md`.

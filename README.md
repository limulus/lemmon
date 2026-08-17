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
| `packages/core/` | `@lemmon/core` — typed JS wrapper around the WASM core (stub) |
| `packages/shell/` | `@lemmon/shell` — `lem` CLI, walker, plugin host, dev server (stub) |
| `crates/` | Rust core workspace (planned) |
| `docs/` | `language.md`, `architecture.md`, `decisions.md`, `journal/` |

## Development

```sh
npm install
npm run verify   # lint + test (100% coverage) + typecheck — what pre-push and CI run
npm run build    # emit packages/*/dist
```

Contributor guidance, invariants, and conventions live in `CLAUDE.md`.

# @limulus/lemmon-core

Typed JS wrapper around the Lemmon Rust core compiled to WebAssembly (see
`docs/architecture.md` §WASM API). What exists today is the ∆TNT subset — `compile`,
`loadData`, `render` on a resident `Core` — with `plan` and the LSP-facing
`analyze`/`format` arriving with their backlog items.

```ts
import { loadCore } from '@limulus/lemmon-core'

const core = await loadCore() // the staged artifact in the repo's artifacts/wasm/
core.compile([{ path: 'page.lem', text: 'Hi <lem.site.name>' }]) // [] = clean
core.loadData({ ambient: { site: { name: 'limulus.net' } } })
core.render({ path: 'page.lem' }) // { html, diagnostics }
```

`loadCore()` reads the wasm-bindgen output `npm run rust:wasm` stages into
`artifacts/wasm/` (pass a directory to read from somewhere else); `createCore(glue,
bytes)` is the injectable seam under it for callers that bring their own loading — the
glue is `--target web` output, so the same files will serve a browser playground. The
artifact deliberately has no home inside this package yet: nothing publishes, and where
it lands in the published layout is decided when something does.

A boundary throw is a caller bug (malformed payload, rendering an unknown path);
everything wrong with a template or document comes back as `Diagnostic[]` with byte
spans. The package's specs run against the real staged artifact — stage it before
`npm run verify`.

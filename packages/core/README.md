# @limulus/lemmon-core

Typed JS wrapper around the Lemmon Rust core compiled to WebAssembly (see
`docs/architecture.md` §WASM API). What exists today is the ∆TNT subset — `compile`,
`loadData`, `render` on a resident `Core` — with `plan` and the LSP-facing
`analyze`/`format` arriving with their backlog items.

```ts
import { loadCore } from '@limulus/lemmon-core'

const core = await loadCore() // the staged artifact in the repo's artifacts/wasm/
const host = { ambient: { site: null } } // names `site`, types nothing (D30)
core.compile([{ path: 'page.lem', text: 'Hi <lem.site.name>' }], host) // [] = clean
core.loadData({ ambient: { site: { name: 'limulus.net' } } }) // [] = loaded
core.render({ path: 'page.lem' }) // { html (bytes), marks, fragments, diagnostics }
```

`loadCore()` reads the wasm-bindgen output `npm run rust:wasm` stages into
`artifacts/wasm/` (pass a directory to read from somewhere else); `createCore(glue,
bytes)` is the injectable seam under it for callers that bring their own loading — the
glue is `--target web` output, so the same files will serve a browser playground. The
artifact deliberately has no home inside this package yet: nothing publishes, and where
it lands in the published layout is decided when something does.

A boundary throw is a caller bug — a malformed payload, rendering an unknown path, an
envelope the program cannot be loaded from, a load or render before any compile has
succeeded; everything wrong with a template, document, or data file comes back as
`Diagnostic[]` with byte spans. The package's specs run against the real staged
artifact — stage it before `npm run verify`.

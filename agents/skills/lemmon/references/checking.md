# Checking what you wrote

`lem check` is not yet built (∆1rD), and `lem build` is not yet here (∆muD); it will
supersede this page: once it exists, a build reports every diagnostic, load errors included.
Until then, this script compiles files through the staged core with the host declaration the
standard shell uses, and prints each diagnostic as `path:line:col: severity: message`.

Save it as `check-lem.mjs` in a scratch directory — it is a stopgap, not part of any site —
and run it from the directory `site.lem`'s globs are relative to, naming every file in the
site:

```sh
LEMMON=/path/to/lemmon node /path/to/check-lem.mjs site.lem layouts/*.lem feed/*/index.md
```

```js
import { matchesGlob } from 'node:path'
import { readFileSync } from 'node:fs'
import { pathToFileURL } from 'node:url'

const src = `${process.env.LEMMON ?? '../lemmon'}/packages/core/src/index.ts`
const { loadCore } = await import(pathToFileURL(src).href)
const ambient = `<lem-type Page>\n  url: url\n  inputPath: string\n  draft: bool = false
</lem-type>\n<lem-type Build>\n  time: datetime\n  generator: string\n</lem-type>
<lem-type Code>\n  language: string?\n  info: string\n  code: string\n</lem-type>\n`
const read = (path) => ({ path, text: readFileSync(path, 'utf8') })
const files = process.argv.slice(2).map(read)
if (!files.some((f) => f.path.endsWith('ambient.lem'))) {
  files.unshift({ path: 'ambient.lem', text: ambient })
}
const site = files.find((f) => f.path.endsWith('site.lem'))
const globs = [...(site?.text ?? '').matchAll(/(\w+)\]\s+from\s+"([^"]+)"/g)]
for (const f of files) {
  const front = /^---\n([^]*?)\n?---/.exec(f.text)
  if (!front && !f.path.endsWith('.md')) continue
  f.record = /^type: *(\w+)/m.exec(front?.[1] ?? '')?.[1]
  f.record ??= globs.find(([, , glob]) => matchesGlob(f.path, glob))?.[1]
}
const host = {
  ambient: { site: site ? { bindingsOf: site.path } : null, page: 'Page', build: 'Build' },
  document: 'page',
  entry: 'layout',
  url: 'url',
}
const { diagnostics } = (await loadCore()).compile(files, host)
for (const { path, span, severity, message } of diagnostics) {
  const { line, character } = span.range.start
  console.log(`${path}:${line + 1}:${character + 1}: ${severity}: ${message}`)
}
process.exitCode = diagnostics.some((d) => d.severity === 'error') ? 1 : 0
```

No output and exit 0 means clean. A document is typed the way the shell types it: by its
`type:` key, or else by the `from` glob in `site.lem` that matches its path — so pass paths
as the globs spell them. A document nothing types compiles with `page` as `any` and its
field reads go unchecked; that is the page-without-a-record case, not a pass.

What it does not cover:

- **Load errors.** It compiles; it never loads. A misspelled or missing front-matter key, a
  `by` reference its map lacks, a bad date in YAML, and a data file with keys its type does
  not declare are found by `loadData`, which `lem build` runs. The
  [load mistakes](mistakes.md#while-loading-front-matter) are the common ones.
- **Assets.** The declaration carries no manifest, so every `asset("…")` reports that the
  host declares none. That is expected while the esbuild pipeline is not yet built (∆d1N)
  and hands the shell no manifest.
- **Marks.** It declares no mark handlers, so a misspelled `mark("stylez")` passes silently
  here; the shell's handlers, not yet built (∆F4y), will warn at it.
- **Render errors.** An `any` read that misses, a map key that is absent, a list index out
  of range: these surface only when a page renders.

The core is staged at `artifacts/wasm/` in the lemmon checkout (`npm run rust:wasm` there
rebuilds it); `loadCore()` finds it by walking up from `packages/core/`.

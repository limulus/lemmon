/**
 * Measures the cost of crossing the WASM boundary (`npm run wasm:bench`), against the
 * staged artifact — run `npm run rust:wasm` first. Numbers only: correctness of the
 * corpus fixtures is the Rust corpus gate's job and byte-parity is ∆xWs's, so this
 * harness never diffs output — but it does exit non-zero on any unexpected diagnostic
 * or throw, so it cannot silently benchmark garbage.
 *
 * Scenarios:
 *  - corpus compile / render — every non-`errors` corpus fixture as its own one-file
 *    program: the per-call floor on real content.
 *  - site big call vs site call-per-file — the same synthetic N-page site compiled in
 *    one `compile` call and in N+1 single-file calls: the few-large-calls validation.
 *  - incremental recompile — one page of the resident site recompiled repeatedly:
 *    what a dev-server edit costs (compile one unit + relink all).
 *  - loadData floor — a near-empty payload, repeated: per-call boundary overhead.
 *  - typed loadData — N documents' front matter with positions, validated against
 *    their schema and tagged (D37): what a real load costs per site.
 */

import { readdirSync, readFileSync } from 'node:fs'
import { join, relative } from 'node:path'

import {
  loadCore,
  type DataInput,
  type Diagnostic,
  type HostDecl,
  type LoadedValue,
  type Position,
  type SourceFileInput,
  type Span,
} from '../packages/core/src/index.ts'

const CORPUS = 'tests/corpus'
const SITE_PAGES = 100
const INCREMENTAL_RUNS = 50
const RENDER_RUNS = 200
const LOAD_DATA_RUNS = 200
const TYPED_LOAD_RUNS = 50

// The corpus render globals (crates/lemmon-vm/tests/corpus.rs), so document fixtures
// that read `page`/`site` resolve.
const AMBIENT = {
  page: { url: '/feed/the-rebound/', title: 'The Rebound' },
  site: { name: 'limulus.net' },
}

interface Sample {
  micros: number
  bytesIn: number
  bytesOut: number
}

interface Scenario {
  name: string
  samples: Sample[]
}

const scenarios: Scenario[] = []

function record(name: string): (fn: () => { in: unknown; out: unknown }) => void {
  const scenario: Scenario = { name, samples: [] }
  scenarios.push(scenario)
  return (fn) => {
    const start = process.hrtime.bigint()
    const payloads = fn()
    const micros = Number(process.hrtime.bigint() - start) / 1000
    scenario.samples.push({
      micros,
      bytesIn: Buffer.byteLength(JSON.stringify(payloads.in)),
      bytesOut: Buffer.byteLength(JSON.stringify(payloads.out)),
    })
  }
}

function expectClean(diagnostics: Diagnostic[], doing: string): void {
  // Warnings do not fail a compile, and they are not this harness's concern.
  const errors = diagnostics.filter((d) => d.severity === 'error')
  if (errors.length > 0) {
    throw new Error(`unexpected diagnostics ${doing}: ${JSON.stringify(errors)}`)
  }
}

// --- Fixtures ---------------------------------------------------------------------

function* walk(dir: string): Generator<string> {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name)
    if (entry.isDirectory()) yield* walk(path)
    else yield path
  }
}

/** Every render/document corpus input except the deliberate-error cases. */
function fixtures(): SourceFileInput[] {
  const files: SourceFileInput[] = []
  for (const group of ['render', 'document']) {
    for (const path of walk(join(CORPUS, group))) {
      if (!/\.(lem|md)$/.test(path) || path.includes('/errors/')) continue
      files.push({
        path: relative(CORPUS, path),
        text: readFileSync(path, 'utf8'),
      })
    }
  }
  return files
}

/** A synthetic site: one shared lib and N pages that call it. */
function site(): SourceFileInput[] {
  const lib = {
    path: 'lib.lem',
    text:
      '<lem-fn card(title: string)><article><h2><lem.title></h2>' +
      '<p>On <lem.site.name></p></article></lem-fn>',
  }
  const pages = Array.from({ length: SITE_PAGES }, (_, i) => ({
    path: `page-${i}.lem`,
    text: `<h1>Page ${i}</h1>\n<lem.card("Card ${i}")>\n<lem.page.title>\n`,
  }))
  return [lib, ...pages]
}

/** The host these fixtures compile against: the ambient names, none of them typed. */
const HOST: HostDecl = { ambient: { site: null, page: null, build: null } }

/** A span from one position to another, as a shell computes one from parsed YAML. */
function span(start: number, end: number, from: Position, to: Position): Span {
  return { start, end, range: { start: from, end: to } }
}

/**
 * A typed site: N `Post` documents collected by a glob, each with the front matter
 * and positions a shell would send — what `loadData` validates and tags (D37). The
 * spans are what they would be for the front matter as written — each key through its
 * value, and the whole value across both lines — so the payload is the real one, not
 * a stand-in.
 */
function typedSite(): { host: HostDecl; files: SourceFileInput[]; data: DataInput } {
  const host: HostDecl = {
    ambient: { page: 'Page', site: { bindingsOf: 'site.lem' } },
    document: 'page',
    entry: 'layout',
  }
  const files: SourceFileInput[] = [
    { path: 'ambient.lem', text: '<lem-type Page>\n  url: url\n</lem-type>' },
    {
      path: 'site.lem',
      text:
        '<lem-type Post extends Page>\n  title: string\n  slug: string = path\n' +
        '  layout: (Post, html): html = post\n</lem-type>\n' +
        '<lem-fn post(p: Post, body: html)><h1><lem.p.title></h1><lem.body></lem-fn>\n' +
        '<lem-let posts: [Post] from "posts/*.md" />',
    },
  ]
  const posts: LoadedValue[] = []
  for (let i = 0; i < SITE_PAGES; i += 1) {
    const path = `posts/post-${i}.md`
    const title = `title: Post ${i}`
    const url = `url: /posts/${i}/`
    const frontMatter = `${title}\n${url}\n`
    files.push({ path, text: `---\n${frontMatter}---\n<lem.page.title>\n`, record: 'Post' })
    // Line 0 is the opening `---`; `title:` is line 1 and `url:` line 2.
    const titleStart = 4
    const urlStart = titleStart + title.length + 1
    const urlEnd = urlStart + url.length
    posts.push({
      path,
      value: { title: `Post ${i}`, url: `/posts/${i}/` },
      spans: {
        '': span(
          titleStart,
          urlEnd,
          { line: 1, character: 0 },
          { line: 2, character: url.length }
        ),
        title: span(
          titleStart,
          titleStart + title.length,
          { line: 1, character: 0 },
          { line: 1, character: title.length }
        ),
        url: span(
          urlStart,
          urlEnd,
          { line: 2, character: 0 },
          { line: 2, character: url.length }
        ),
      },
    })
  }
  return { host, files, data: { ambient: {}, sources: { posts } } }
}

// --- Scenarios --------------------------------------------------------------------

async function corpusScenarios(): Promise<void> {
  const inputs = fixtures()
  const compileOne = record('corpus compile (1 file/call, fresh core)')
  const renderOne = record('corpus render (1 page/call)')
  for (const input of inputs) {
    const core = await loadCore()
    let diagnostics: Diagnostic[] = []
    compileOne(() => {
      diagnostics = core.compile([input], HOST)
      return { in: [input], out: diagnostics }
    })
    expectClean(diagnostics, `compiling ${input.path}`)
    core.loadData({ ambient: AMBIENT })
    renderOne(() => {
      const result = core.render({ path: input.path })
      expectClean(result.diagnostics, `rendering ${input.path}`)
      return { in: { path: input.path }, out: result }
    })
  }
  console.log(`corpus: ${inputs.length} fixtures (error cases excluded)`)
}

async function siteScenarios(): Promise<void> {
  const files = site()

  const bigCall = record(`site compile (${files.length} files, 1 call)`)
  const bigCore = await loadCore()
  bigCall(() => {
    const diagnostics = bigCore.compile(files, HOST)
    expectClean(diagnostics, 'compiling the site in one call')
    return { in: files, out: diagnostics }
  })

  const perFile = record(`site compile (${files.length} files, 1 file/call)`)
  const smallCore = await loadCore()
  for (const file of files) {
    perFile(() => {
      const diagnostics = smallCore.compile([file], HOST)
      expectClean(diagnostics, `compiling ${file.path} incrementally`)
      return { in: [file], out: diagnostics }
    })
  }

  const incremental = record('incremental recompile (1 page + relink)')
  const edited = files.at(-1)
  if (!edited) throw new Error('the site has pages')
  for (let i = 0; i < INCREMENTAL_RUNS; i += 1) {
    incremental(() => {
      const diagnostics = bigCore.compile([edited], HOST)
      expectClean(diagnostics, 'recompiling one page')
      return { in: [edited], out: diagnostics }
    })
  }

  bigCore.loadData({ ambient: AMBIENT })
  const renders = record('site render (1 page/call, repeated)')
  for (let i = 0; i < RENDER_RUNS; i += 1) {
    const path = `page-${i % SITE_PAGES}.lem`
    renders(() => {
      const result = bigCore.render({ path })
      expectClean(result.diagnostics, `rendering ${path}`)
      return { in: { path }, out: result }
    })
  }

  const floor = record('loadData floor (near-empty payload)')
  for (let i = 0; i < LOAD_DATA_RUNS; i += 1) {
    floor(() => {
      const input = { ambient: {} }
      return { in: input, out: bigCore.loadData(input) }
    })
  }
}

async function typedLoadScenarios(): Promise<void> {
  const { host, files, data } = typedSite()
  const core = await loadCore()
  expectClean(core.compile(files, host), 'compiling the typed site')
  const typed = record(`typed loadData (${SITE_PAGES} documents, positions included)`)
  for (let i = 0; i < TYPED_LOAD_RUNS; i += 1) {
    typed(() => {
      const diagnostics = core.loadData(data)
      expectClean(diagnostics, 'loading the typed site')
      return { in: data, out: diagnostics }
    })
  }
  // One render proves the load bound something: an unbound `page` would be a
  // diagnostic here. What it rendered is the corpus gate's business, not this one's.
  expectClean(core.render({ path: 'posts/post-0.md' }).diagnostics, 'rendering a post')
}

// --- Report -----------------------------------------------------------------------

function percentile(sorted: number[], p: number): number {
  const at = Math.min(sorted.length - 1, Math.ceil((p / 100) * sorted.length) - 1)
  return sorted[Math.max(0, at)]
}

function report(): void {
  // An empty scenario means the fixtures went missing, not that the boundary is fast:
  // say so rather than dying in the arithmetic below.
  const empty = scenarios.filter(({ samples }) => samples.length === 0)
  if (empty.length > 0) {
    throw new Error(
      `no samples for ${empty.map(({ name }) => name).join(', ')} — ` +
        `are the corpus fixtures still under ${CORPUS}?`
    )
  }
  const header = ['scenario', 'calls', 'in B/call', 'out B/call', 'mean µs', 'p50', 'p99']
  const rows = scenarios.map(({ name, samples }) => {
    const sorted = samples.map((s) => s.micros).sort((a, b) => a - b)
    const total = sorted.reduce((a, b) => a + b, 0)
    const per = (of: (s: Sample) => number): number =>
      Math.round(samples.reduce((a, s) => a + of(s), 0) / samples.length)
    return [
      name,
      String(samples.length),
      String(per((s) => s.bytesIn)),
      String(per((s) => s.bytesOut)),
      (total / samples.length).toFixed(1),
      percentile(sorted, 50).toFixed(1),
      percentile(sorted, 99).toFixed(1),
    ]
  })
  const widths = header.map((h, i) =>
    Math.max(h.length, ...rows.map((row) => row[i].length))
  )
  const line = (row: string[]): string =>
    row.map((cell, i) => cell.padStart(widths[i]).padEnd(widths[i])).join('  ')
  console.log()
  console.log(line(header))
  console.log(widths.map((w) => '-'.repeat(w)).join('  '))
  for (const row of rows) console.log(line(row))
}

await corpusScenarios()
await siteScenarios()
await typedLoadScenarios()
report()

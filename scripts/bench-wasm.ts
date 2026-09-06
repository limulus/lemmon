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
 */

import { readdirSync, readFileSync } from 'node:fs'
import { join, relative } from 'node:path'

import {
  loadCore,
  type Diagnostic,
  type SourceFileInput,
} from '../packages/core/src/index.ts'

const CORPUS = 'tests/corpus'
const SITE_PAGES = 100
const INCREMENTAL_RUNS = 50
const RENDER_RUNS = 200
const LOAD_DATA_RUNS = 200

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

// --- Scenarios --------------------------------------------------------------------

async function corpusScenarios(): Promise<void> {
  const inputs = fixtures()
  const compileOne = record('corpus compile (1 file/call, fresh core)')
  const renderOne = record('corpus render (1 page/call)')
  for (const input of inputs) {
    const core = await loadCore()
    let diagnostics: Diagnostic[] = []
    compileOne(() => {
      diagnostics = core.compile([input])
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
    const diagnostics = bigCore.compile(files)
    expectClean(diagnostics, 'compiling the site in one call')
    return { in: files, out: diagnostics }
  })

  const perFile = record(`site compile (${files.length} files, 1 file/call)`)
  const smallCore = await loadCore()
  for (const file of files) {
    perFile(() => {
      const diagnostics = smallCore.compile([file])
      expectClean(diagnostics, `compiling ${file.path} incrementally`)
      return { in: [file], out: diagnostics }
    })
  }

  const incremental = record('incremental recompile (1 page + relink)')
  const edited = files.at(-1)
  if (!edited) throw new Error('the site has pages')
  for (let i = 0; i < INCREMENTAL_RUNS; i += 1) {
    incremental(() => {
      const diagnostics = bigCore.compile([edited])
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
report()

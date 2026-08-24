/**
 * The ∆xWs proof (`npm run spike:the-rebound`): renders limulus.net's `the-rebound`
 * post end to end through the WASM core — document mode for the post body, then a
 * base→page→article layout chain — and holds the result against the 11ty-built
 * reference in spike/the-rebound/fixtures/.
 *
 * Both sides are normalized with the same dprint markup formatter the 11ty build uses
 * (lineWidth 92), then diffed line-by-line. Every hunk must match an allowlisted
 * difference class (markdown-engine deltas the spike accepts); anything else — or a
 * page render time at or over the 1 ms exit criterion — exits non-zero.
 */

import { createFromBuffer } from '@dprint/formatter'
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { parse } from 'yaml'

import { loadCore, type SourceFileInput } from '../packages/core/src/index.ts'

const SPIKE = 'spike/the-rebound'
const PAGE_PATH = 'feed/the-rebound/index.md'
const RENDER_RUNS = 200
const BUDGET_MICROS = 1000

// --- Fixtures ---------------------------------------------------------------------

const LAYOUT_FILES = [
  'sprite.lem',
  'components.lem',
  'article.lem',
  'page.lem',
  'base.lem',
  'driver.lem',
]

function sourceFiles(): SourceFileInput[] {
  const post = {
    path: PAGE_PATH,
    text: readFileSync(join(SPIKE, 'fixtures/index.md'), 'utf8'),
  }
  const layouts = LAYOUT_FILES.map((name) => ({
    path: name,
    text: readFileSync(join(SPIKE, 'site', name), 'utf8'),
  }))
  return [post, ...layouts]
}

type JsonRecord = Record<string, unknown>

/**
 * The core skips a document's front matter without reading it (that is the shell's
 * job), so the harness parses it here and folds it into the ambient `page` record.
 *
 * `data.json` is hand-transcribed from a build of the post, so it can drift from the
 * vendored source silently. The two share no key names — the front matter carries
 * `date`/`hero`, the fixture carries what 11ty *derived* from them — so agreement has
 * to be checked on derived values, not on same-named keys: a check over the key
 * intersection is vacuously true and would pass forever.
 */
function ambientData(postText: string): JsonRecord {
  const data = JSON.parse(readFileSync(join(SPIKE, 'fixtures/data.json'), 'utf8')) as {
    site: JsonRecord
    page: JsonRecord
    build: JsonRecord
  }
  const match = /^---\n([\s\S]*?)\n---\n/.exec(postText)
  if (!match) throw new Error('fixtures/index.md has no front matter')
  const frontMatter = parse(match[1]) as JsonRecord

  const agree = (what: string, derived: string, recorded: unknown): void => {
    if (derived !== recorded) {
      throw new Error(
        `fixtures/data.json disagrees with the post's front matter on ${what}: ` +
          `${JSON.stringify(recorded)}, but the source's date derives ${JSON.stringify(derived)}`
      )
    }
  }
  const published = new Date(String(frontMatter.date))
  agree('page.publishedIso', published.toISOString(), data.page.publishedIso)
  agree('page.publishedUtc', published.toUTCString(), data.page.publishedUtc)
  const photos = data.site.photos as JsonRecord
  if (!(String(frontMatter.hero) in photos)) {
    throw new Error(`the post's hero (${String(frontMatter.hero)}) is not in site.photos`)
  }

  return { site: data.site, page: { ...data.page, ...frontMatter }, build: data.build }
}

// --- Timing (the bench-wasm idiom) ------------------------------------------------

interface Scenario {
  name: string
  micros: number[]
}

const scenarios: Scenario[] = []

function record(name: string): (fn: () => void) => void {
  const scenario: Scenario = { name, micros: [] }
  scenarios.push(scenario)
  return (fn) => {
    const start = process.hrtime.bigint()
    fn()
    scenario.micros.push(Number(process.hrtime.bigint() - start) / 1000)
  }
}

function mean(samples: number[]): number {
  return samples.reduce((a, b) => a + b, 0) / samples.length
}

function percentile(samples: number[], p: number): number {
  const sorted = [...samples].sort((a, b) => a - b)
  const at = Math.min(sorted.length - 1, Math.ceil((p / 100) * sorted.length) - 1)
  return sorted[Math.max(0, at)]
}

function expectClean(diagnostics: unknown[], doing: string): void {
  if (diagnostics.length > 0) {
    throw new Error(`unexpected diagnostics ${doing}: ${JSON.stringify(diagnostics)}`)
  }
}

// --- dprint (mirrors limulus-dot-net's 11ty transform) ----------------------------

function makeFormatter(): (html: string) => string {
  const wasmPath = fileURLToPath(import.meta.resolve('dprint-plugin-markup/plugin.wasm'))
  const formatter = createFromBuffer(new Uint8Array(readFileSync(wasmPath)))
  formatter.setConfig({ lineWidth: 92 }, {})
  return (html) => formatter.formatText({ filePath: 'index.html', fileText: html })
}

// --- Line diff (LCS) --------------------------------------------------------------

interface Hunk {
  refStart: number
  ours: string[]
  theirs: string[]
}

/** Hunks of a line diff: `theirs` (reference) replaced by `ours` (lemmon output). */
function diffLines(ours: string[], theirs: string[]): Hunk[] {
  const n = ours.length
  const m = theirs.length
  // LCS lengths, bottom-up; the pages are ~1k lines so O(n·m) is fine.
  const lcs: Uint32Array[] = Array.from({ length: n + 1 }, () => new Uint32Array(m + 1))
  for (let i = n - 1; i >= 0; i -= 1) {
    for (let j = m - 1; j >= 0; j -= 1) {
      lcs[i][j] =
        ours[i] === theirs[j]
          ? lcs[i + 1][j + 1] + 1
          : Math.max(lcs[i + 1][j], lcs[i][j + 1])
    }
  }
  const hunks: Hunk[] = []
  let i = 0
  let j = 0
  let open: Hunk | null = null
  while (i < n || j < m) {
    if (i < n && j < m && ours[i] === theirs[j]) {
      open = null
      i += 1
      j += 1
    } else {
      if (!open) {
        open = { refStart: j + 1, ours: [], theirs: [] }
        hunks.push(open)
      }
      if (j >= m || (i < n && lcs[i + 1][j] >= lcs[i][j + 1])) {
        open.ours.push(ours[i])
        i += 1
      } else {
        open.theirs.push(theirs[j])
        j += 1
      }
    }
  }
  return hunks
}

// --- Hunk classification ----------------------------------------------------------

/**
 * A difference class is a normalization under which the hunk's two sides agree —
 * D15's "normalized, allowlisted" in miniature. Each class carries a `applies` guard so
 * it can only claim a hunk it actually explains: a normalization that collapses
 * whitespace will otherwise absorb every unrelated indentation change as well, which is
 * how a suppression starts hiding real differences.
 *
 *  - whitespace: blank-line-only differences (every line on both sides is empty).
 *    Indentation deltas on content lines are deliberately NOT covered — they were
 *    driven to zero by matching WebC's emitted whitespace, and must fail if they
 *    return.
 *  - heading-id: markdown-it-anchor percent-encodes heading ids and adds
 *    tabindex="-1"; comrak emits plain slugs and no tabindex.
 *  - md-photo-wrap: markdown-it re-wraps the tail of each in-content
 *    <article-photo> block in a spurious <p>; lemmon's sentinel splice emits the
 *    markup unwrapped. Only the two <p> tags that mangle actually produces are
 *    elided, and comparison is per-line-trimmed rather than fully collapsed: a
 *    normalization that erased every <p> and all whitespace in the hunk would also
 *    absorb a genuinely dropped paragraph wrapper sitting beside the photo.
 *  - attr-escaping: lemmon escapes `"` inside attribute values where WebC emitted
 *    the raw character.
 */
interface DiffClass {
  name: string
  /** Whether this class is even about the difference in this hunk. */
  applies: (ours: string, theirs: string) => boolean
  normalize: (side: string) => string
}

/** Line structure kept, indentation and blank lines dropped. */
const trimLines = (s: string): string =>
  s
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line !== '')
    .join('\n')
const blank = (s: string): boolean => /^\s*$/.test(s)

const DIFF_CLASSES: DiffClass[] = [
  {
    name: 'whitespace',
    applies: (ours, theirs) => blank(ours) && blank(theirs),
    normalize: () => '',
  },
  {
    name: 'heading-id',
    applies: (ours, theirs) => ours.includes('<h2 id=') && theirs.includes('<h2 id='),
    normalize: (side) => side.replace(/<h2 id="[^"]*"( tabindex="-1")?>/g, '<h2>'),
  },
  {
    name: 'md-photo-wrap',
    applies: (ours, theirs) =>
      ours.includes('pho-picture') && theirs.includes('pho-picture'),
    normalize: (side) =>
      trimLines(
        side.replace(/<p>(?=<\/pho-picture>)/g, '').replace(/(?<=<\/figcaption>)<\/p>/g, '')
      ),
  },
  {
    name: 'attr-escaping',
    applies: (ours) => ours.includes('&quot;'),
    normalize: (side) => side.replaceAll('&quot;', '"'),
  },
]

function classify(hunk: Hunk): string | null {
  const ours = hunk.ours.join('\n')
  const theirs = hunk.theirs.join('\n')
  for (const { name, applies, normalize } of DIFF_CLASSES) {
    if (applies(ours, theirs) && normalize(ours) === normalize(theirs)) return name
  }
  return null
}

// --- Main -------------------------------------------------------------------------

const files = sourceFiles()
const ambient = ambientData(files[0].text)
const core = await loadCore()

const compile = record(`compile (${files.length} files, 1 call)`)
compile(() => {
  expectClean(core.compile(files), 'compiling the spike')
})

const loadAmbient = record('loadData (full ambient)')
const loadContent = record('loadData (ambient + content)')
const renderPost = record('render pass 1 (index.md)')
const renderPage = record('render pass 2 (driver.lem)')

let bodyHtml = ''
let pageHtml = ''
for (let run = 0; run < RENDER_RUNS; run += 1) {
  loadAmbient(() => {
    expectClean(core.loadData({ ambient }), 'loading ambient data')
  })
  renderPost(() => {
    const result = core.render({ path: PAGE_PATH })
    expectClean(result.diagnostics, `rendering ${PAGE_PATH}`)
    bodyHtml = result.html ?? ''
  })
  const withContent = {
    ...ambient,
    page: { ...(ambient.page as JsonRecord), content: bodyHtml },
  }
  loadContent(() => {
    expectClean(core.loadData({ ambient: withContent }), 'loading page content')
  })
  renderPage(() => {
    const result = core.render({ path: 'driver.lem' })
    expectClean(result.diagnostics, 'rendering driver.lem')
    pageHtml = result.html ?? ''
  })
}
core.free()

// --- Normalize + diff -------------------------------------------------------------

// Mirror the 11ty build's dprint transform, fallback included: it swallows formatter
// errors and ships the page raw — and this page IS such a page, because markdown-it's
// re-wrap mangle (`<p></pho-picture>`) is unparseable markup. So when the reference
// cannot be formatted, both sides are compared raw; the `whitespace` diff class then
// absorbs indentation-only differences.
const format = makeFormatter()
const reference = readFileSync(join(SPIKE, 'fixtures/reference.html'), 'utf8')
let referenceFormatted = reference
let formatted = true
try {
  referenceFormatted = format(reference)
} catch (error) {
  console.log(`note: dprint cannot parse the reference (${(error as Error).message});`)
  console.log('      the 11ty build ships it unformatted too — comparing raw output')
  formatted = false
}
let oursFormatted = pageHtml
if (formatted) {
  try {
    oursFormatted = format(pageHtml)
  } catch (error) {
    // Asymmetric formatting would diff apples against oranges, so drop both sides to
    // raw — and say so loudly, since unparseable output is news about the renderer.
    console.log(`note: dprint cannot parse LEMMON's output (${(error as Error).message});`)
    console.log('      comparing both sides raw — check the renderer, not the reference')
    referenceFormatted = reference
    formatted = false
  }
}
const idempotent = !formatted || referenceFormatted === reference

const outDir = join(tmpdir(), 'lemmon-the-rebound')
mkdirSync(outDir, { recursive: true })
writeFileSync(join(outDir, 'lemmon.html'), oursFormatted)
writeFileSync(join(outDir, 'reference.html'), referenceFormatted)

const oursLines = oursFormatted.split('\n')
const refLines = referenceFormatted.split('\n')
const hunks = diffLines(oursLines, refLines)
const counts = new Map<string, number>()
const unexpected: Hunk[] = []
for (const hunk of hunks) {
  const cls = classify(hunk)
  if (cls) counts.set(cls, (counts.get(cls) ?? 0) + 1)
  else unexpected.push(hunk)
}

// --- Report -----------------------------------------------------------------------

console.log(`normalized output written to ${outDir}`)
console.log()
const how = formatted ? 'both dprint-formatted' : 'both raw, unformatted'
console.log(`diff summary (lemmon vs 11ty reference, ${how}):`)
console.log(`  bytes: ${oursFormatted.length} lemmon, ${referenceFormatted.length} 11ty`)
const changed = hunks.reduce((a, h) => a + Math.max(h.ours.length, h.theirs.length), 0)
const identical = (100 * (refLines.length - changed)) / refLines.length
console.log(`  lines identical: ${Math.max(0, identical).toFixed(1)}%`)
for (const [name, count] of counts) {
  console.log(`  expected ${name}: ${count} hunk${count === 1 ? '' : 's'}`)
}
console.log(`  unexpected: ${unexpected.length}`)
for (const hunk of unexpected) {
  console.log()
  console.log(`--- reference.html:${hunk.refStart}`)
  for (const line of hunk.theirs) console.log(`-${line}`)
  for (const line of hunk.ours) console.log(`+${line}`)
}

console.log()
const header = ['scenario', 'calls', 'mean µs', 'p50', 'p99']
const rows = scenarios.map(({ name, micros }) => [
  name,
  String(micros.length),
  mean(micros).toFixed(1),
  percentile(micros, 50).toFixed(1),
  percentile(micros, 99).toFixed(1),
])
const widths = header.map((h, i) => Math.max(h.length, ...rows.map((row) => row[i].length)))
const line = (row: string[]): string =>
  row.map((cell, i) => cell.padStart(widths[i]).padEnd(widths[i])).join('  ')
console.log(line(header))
console.log(widths.map((w) => '-'.repeat(w)).join('  '))
for (const row of rows) console.log(line(row))

const pageMicros =
  mean(scenarios.find((s) => s.name.startsWith('render pass 1'))?.micros ?? [NaN]) +
  mean(scenarios.find((s) => s.name.startsWith('render pass 2'))?.micros ?? [NaN])
const loadMicros = scenarios
  .filter((s) => s.name.startsWith('loadData'))
  .reduce((total, s) => total + mean(s.micros), 0)
const pct = ((100 * pageMicros) / BUDGET_MICROS).toFixed(1)
const timeOk = pageMicros < BUDGET_MICROS
console.log()
console.log(
  `page render (pass 1 + pass 2 mean, marshalling included): ${pageMicros.toFixed(1)} µs ` +
    `(${pct}% of the 1 ms budget) — ${timeOk ? 'PASS' : 'FAIL'}`
)
console.log(
  `  excludes ${loadMicros.toFixed(1)} µs of loadData per page: the criterion gates render ` +
    `time, and\n  a real build loads ambient data once per site, not once per page`
)

let failed = false
if (!idempotent) {
  failed = true
  console.error(
    'FAIL: dprint(reference.html) differs from reference.html — the reference was ' +
      'not built with this formatter version/config, so the diff cannot be trusted'
  )
}
if (unexpected.length > 0) {
  failed = true
  console.error(`FAIL: ${unexpected.length} unexpected diff hunk(s), listed above`)
}
if (!timeOk) {
  failed = true
  console.error('FAIL: page render time is at or over the 1 ms/page exit criterion')
}
process.exitCode = failed ? 1 : 0

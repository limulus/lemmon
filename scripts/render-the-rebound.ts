/**
 * The ∆xWs proof: renders limulus.net's `the-rebound` post end to end through the WASM
 * core — document mode for the post body, then a base→page→article layout chain — and
 * holds the result against the 11ty-built reference in tests/fidelity/the-rebound/.
 *
 * The two pages are compared as what a browser would render, not as bytes (D28,
 * ∆Ma4): parse5 trees, attributes by value, text after white-space processing. Every
 * remaining difference must be one of the forgiveness classes below (markdown-engine
 * and WebC deltas the harness accepts), and each must fire exactly as often as
 * `EXPECTED_FORGIVEN` says; anything else exits non-zero.
 *
 * Two modes. `npm run test:fidelity` renders once and compares, and is a leg of
 * `verify:js`. `npm run test:fidelity -- --time` adds the timing loop, the table, and
 * the render time held against the 1 ms exit criterion — a verdict about the machine it
 * runs on, which is why the gate does not apply it.
 */

import { readFileSync, writeFileSync, mkdirSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { parse } from 'yaml'

import { expectClean } from './lib/diagnostics.ts'
import {
  loadCore,
  type JsonValue,
  type RenderResult,
  type SourceFileInput,
} from '../packages/core/src/index.ts'
import {
  compareRendered,
  renderedPage,
  type Display,
  type Forgiveness,
  type RenderedElement,
} from '../packages/shell/src/index.ts'

const FIXTURES = 'tests/fidelity/the-rebound'
const PAGE_PATH = 'feed/the-rebound/index.md'
const BUDGET_MICROS = 1000

// `--time` is the only argument. An unrecognized one fails instead of being ignored: a
// typo like `--timing` would otherwise record one render as a timing run.
const args = process.argv.slice(2)
const timing = args.includes('--time')
const unknown = args.filter((arg) => arg !== '--time')
if (unknown.length > 0) {
  console.error(`unknown argument: ${unknown.join(' ')}`)
  console.error('usage: node scripts/render-the-rebound.ts [--time]')
  process.exit(2)
}

// The comparison reads the last iteration's output, so one pass compares as faithfully
// as two hundred; the rest of the loop is there to collect samples.
const RENDER_RUNS = timing ? 200 : 1

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
    text: readFileSync(join(FIXTURES, 'fixtures/index.md'), 'utf8'),
  }
  const layouts = LAYOUT_FILES.map((name) => ({
    path: name,
    text: readFileSync(join(FIXTURES, 'site', name), 'utf8'),
  }))
  return [post, ...layouts]
}

type JsonRecord = Record<string, JsonValue>

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
  const data = JSON.parse(readFileSync(join(FIXTURES, 'fixtures/data.json'), 'utf8')) as {
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

// --- What the site's CSS does to whitespace -----------------------------------------

/**
 * The comparator knows the UA stylesheet only; these are the custom elements whose
 * whitespace limulus.net's own CSS decides. Each is verified against the component
 * source, not assumed: `article-header.webc` makes `article-photo` a block, and
 * `data-chip.webc` makes its host a flex container (whose items' whitespace is never
 * rendered). Declaring one here that the CSS does not back would hide a real space.
 */
const SITE_DISPLAY: Record<string, Display> = {
  'article-photo': 'block',
  'data-chip': 'flex',
}

// --- Forgiveness classes ----------------------------------------------------------

/**
 * Each class is D15's "allowlisted" in miniature: a difference the harness accepts,
 * stated narrowly enough that it cannot absorb one it does not explain.
 *
 *  - heading-id: markdown-it-anchor percent-encodes heading ids and adds
 *    tabindex="-1"; comrak emits plain slugs and no tabindex. Only `id` and `tabindex`
 *    are set aside; every other attribute still has to agree.
 *  - md-photo-wrap: markdown-it wraps the tail of each in-content <article-photo>
 *    block in a spurious <p>, which the HTML parser's adoption agency turns into a
 *    ghost `<p><a></a></p>` and an empty `<p>` inside the figure. Lemmon's sentinel
 *    splice emits the markup unwrapped. Only those two shapes are forgiven, and only
 *    in a figure holding a pho-picture: a `p` with content beside the photo is a
 *    dropped paragraph and still fails.
 *  - attr-escaping: WebC emitted a raw `"` inside a double-quoted attribute value;
 *    lemmon escapes it. Parsed, the reference's value stops at the quote and the rest
 *    becomes junk attribute names. Forgiven only when the reference's value is ours
 *    cut at the first quote and every junk name is a token of what was cut off.
 */

function contains(element: RenderedElement, tag: string): boolean {
  return element.children.some(
    (child) => child.kind === 'element' && (child.tag === tag || contains(child, tag))
  )
}

/** The attribute ours wrote with a `"` in it that the reference has cut at the quote. */
function cutAt(
  ours: RenderedElement,
  theirs: RenderedElement
): [string, string] | undefined {
  return [...ours.attrs].find(
    ([name, value]) =>
      value.includes('"') && theirs.attrs.get(name) === value.slice(0, value.indexOf('"'))
  )
}

const FORGIVENESS: Forgiveness[] = [
  {
    name: 'heading-id',
    attr: (name, ours, theirs, at) =>
      at.ours.tag === 'h2' &&
      at.ours.attrs.has('id') &&
      at.theirs.attrs.get('tabindex') === '-1' &&
      ((name === 'id' && theirs !== undefined) ||
        (name === 'tabindex' && ours === undefined)),
  },
  {
    name: 'md-photo-wrap',
    theirsOnly: (theirs, at) => {
      if (
        at.theirs.tag !== 'figure' ||
        theirs.tag !== 'p' ||
        !contains(at.theirs, 'pho-picture')
      ) {
        return false
      }
      const [only, ...rest] = theirs.children
      return (
        only === undefined ||
        (rest.length === 0 &&
          only.kind === 'element' &&
          only.tag === 'a' &&
          !only.children.length)
      )
    },
  },
  {
    name: 'attr-escaping',
    attr: (name, ours, theirs, at) => {
      const cut = cutAt(at.ours, at.theirs)
      if (cut === undefined) return false
      // The attribute that was cut, or one of the junk names the rest of it became.
      const [cutName, value] = cut
      if (name === cutName) return true
      if (ours !== undefined || theirs !== '') return false
      // Attribute names come back lowercased, so match the tokens the same way.
      const tokens = value
        .slice(value.indexOf('"') + 1)
        .toLowerCase()
        .split(/\s+/)
      return tokens.includes(name) || tokens.includes(name.replace(/"$/, ''))
    },
  },
]

/**
 * How often each class is expected to fire, checked in both modes. A class that forgives
 * fewer differences than this already fails, because the one it stopped absorbing
 * becomes unexpected; a class that starts absorbing more than it explains is silent
 * without these numbers, which is the half ∆xWs's retrospective says nothing made it
 * look at. A name on one side only is a mismatch too: a new class that forgave
 * something, or one of these that forgave nothing.
 */
const EXPECTED_FORGIVEN: Record<string, number> = {
  'heading-id': 6,
  'md-photo-wrap': 6,
  'attr-escaping': 1,
}

// --- Main -------------------------------------------------------------------------

const files = sourceFiles()
const ambient = ambientData(files[0].text)
const core = await loadCore()

// The harness's host names the three ambient values and types none of them, which is
// what leaves its ad hoc fields unknown-typed and renders them unannotated (D30) —
// and what lets `loadData` bind them as sent, validating nothing (D37).
const host = { ambient: { site: null, page: null, build: null } }

const compile = record(`compile (${files.length} files, 1 call)`)
compile(() => {
  expectClean(core.compile(files, host).diagnostics, 'compiling the fidelity fixtures')
})

const loadAmbient = record('loadData (full ambient)')
const loadContent = record('loadData (ambient + content)')
const renderPost = record('render pass 1 (index.md)')
const renderPage = record('render pass 2 (driver.lem)')

// A render returns the page as bytes beside its marks (D42). The harness runs no
// handlers, so each pass decodes outside the timed closure: what the budget is about is
// the render, and a real shell decodes once, after splicing.
const decoder = new TextDecoder()
const last: { body: RenderResult['html']; page: RenderResult['html'] } = {
  body: null,
  page: null,
}
let bodyHtml = ''
let pageHtml = ''
for (let run = 0; run < RENDER_RUNS; run += 1) {
  loadAmbient(() => {
    expectClean(core.loadData({ ambient }), 'loading ambient data')
  })
  renderPost(() => {
    const result = core.render({ path: PAGE_PATH })
    expectClean(result.diagnostics, `rendering ${PAGE_PATH}`)
    last.body = result.html
  })
  bodyHtml = last.body === null ? '' : decoder.decode(last.body)
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
    last.page = result.html
  })
  pageHtml = last.page === null ? '' : decoder.decode(last.page)
}
core.free()

// --- Compare ----------------------------------------------------------------------

const outDir = join(tmpdir(), 'lemmon-the-rebound')
mkdirSync(outDir, { recursive: true })
writeFileSync(join(outDir, 'lemmon.html'), pageHtml)

const reference = readFileSync(join(FIXTURES, 'fixtures/reference.html'), 'utf8')
const comparison = compareRendered(
  renderedPage(pageHtml, { display: SITE_DISPLAY }),
  renderedPage(reference, { display: SITE_DISPLAY }),
  FORGIVENESS
)

// --- Report -----------------------------------------------------------------------

console.log(`lemmon output written to ${outDir}/lemmon.html`)
console.log()
console.log('rendered comparison (lemmon vs 11ty reference):')
console.log(`  bytes: ${last.page?.length ?? 0} lemmon, ${reference.length} 11ty`)
console.log(`  nodes matched: ${comparison.matched}`)
for (const [name, count] of comparison.forgiven) {
  console.log(`  forgiven ${name}: ${count}`)
}
const unexpected = comparison.differences
console.log(`  unexpected: ${unexpected.length}`)
for (const { path, ours, theirs } of unexpected) {
  console.log()
  console.log(`--- ${path}`)
  console.log(`-${theirs ?? '(nothing)'}`)
  console.log(`+${ours ?? '(nothing)'}`)
}

const miscounted = [
  ...new Set([...Object.keys(EXPECTED_FORGIVEN), ...comparison.forgiven.keys()]),
]
  .map((name) => ({
    name,
    forgave: comparison.forgiven.get(name) ?? 0,
    expected: EXPECTED_FORGIVEN[name] ?? 0,
  }))
  .filter(({ forgave, expected }) => forgave !== expected)

let timeOk = true
if (timing) {
  console.log()
  const header = ['scenario', 'calls', 'mean µs', 'p50', 'p99']
  const rows = scenarios.map(({ name, micros }) => [
    name,
    String(micros.length),
    mean(micros).toFixed(1),
    percentile(micros, 50).toFixed(1),
    percentile(micros, 99).toFixed(1),
  ])
  const widths = header.map((h, i) =>
    Math.max(h.length, ...rows.map((row) => row[i].length))
  )
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
  timeOk = pageMicros < BUDGET_MICROS
  console.log()
  console.log(
    `page render (pass 1 + pass 2 mean, marshalling included): ${pageMicros.toFixed(1)} µs ` +
      `(${pct}% of the 1 ms budget) — ${timeOk ? 'PASS' : 'FAIL'}`
  )
  console.log(
    `  excludes ${loadMicros.toFixed(1)} µs of loadData per page: the criterion gates render ` +
      `time, and\n  a real build loads ambient data once per site, not once per page`
  )
}

let failed = false
if (unexpected.length > 0) {
  failed = true
  console.error(
    `FAIL: ${unexpected.length} unexpected rendered difference(s), listed above`
  )
}
if (miscounted.length > 0) {
  failed = true
  console.error('FAIL: the forgiveness classes did not fire as often as expected:')
  for (const { name, forgave, expected } of miscounted) {
    console.error(`  ${name}: forgave ${forgave}, expected ${expected}`)
  }
}
if (!timeOk) {
  failed = true
  console.error('FAIL: page render time is at or over the 1 ms/page exit criterion')
}
if (!failed) {
  console.log()
  console.log(`PASS: ${comparison.matched} nodes matched, nothing unexpected`)
}
process.exitCode = failed ? 1 : 0

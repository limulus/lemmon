/**
 * The ∆xWs proof (`npm run spike:the-rebound`): renders limulus.net's `the-rebound`
 * post end to end through the WASM core — document mode for the post body, then a
 * base→page→article layout chain — and holds the result against the 11ty-built
 * reference in spike/the-rebound/fixtures/.
 *
 * The two pages are compared as what a browser would render, not as bytes (D28,
 * ∆Ma4): parse5 trees, attributes by value, text after white-space processing. Every
 * remaining difference must be one of the forgiveness classes below (markdown-engine
 * and WebC deltas the spike accepts); anything else — or a page render time at or over
 * the 1 ms exit criterion — exits non-zero.
 */

import { readFileSync, writeFileSync, mkdirSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { parse } from 'yaml'

import {
  compareRendered,
  renderedPage,
  sameAttrs,
  type Display,
  type Forgiveness,
  type RenderedElement,
} from './lib/rendered-html.ts'
import {
  loadCore,
  type Diagnostic,
  type SourceFileInput,
} from '../packages/core/src/index.ts'

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

function expectClean(diagnostics: Diagnostic[], doing: string): void {
  // Warnings do not fail a compile, and they are not this harness's concern.
  const errors = diagnostics.filter((d) => d.severity === 'error')
  if (errors.length > 0) {
    throw new Error(`unexpected diagnostics ${doing}: ${JSON.stringify(errors)}`)
  }
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
 * Each class is D15's "allowlisted" in miniature: a difference the spike accepts,
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

/** `attrs` without the named keys. */
function without(attrs: Map<string, string>, names: string[]): Map<string, string> {
  return new Map([...attrs].filter(([name]) => !names.includes(name)))
}

function contains(element: RenderedElement, tag: string): boolean {
  return element.children.some(
    (child) => child.kind === 'element' && (child.tag === tag || contains(child, tag))
  )
}

const FORGIVENESS: Forgiveness[] = [
  {
    name: 'heading-id',
    attrs: (ours, theirs) =>
      ours.tag === 'h2' &&
      ours.attrs.has('id') &&
      theirs.attrs.get('tabindex') === '-1' &&
      sameAttrs(without(ours.attrs, ['id']), without(theirs.attrs, ['id', 'tabindex'])),
  },
  {
    name: 'md-photo-wrap',
    extra: (theirs, parent) => {
      if (
        parent.tag !== 'figure' ||
        theirs.tag !== 'p' ||
        !contains(parent, 'pho-picture')
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
    attrs: (ours, theirs) => {
      const cut = [...ours.attrs].find(([, value]) => value.includes('"'))
      if (!cut) return false
      const [name, value] = cut
      const quote = value.indexOf('"')
      if (theirs.attrs.get(name) !== value.slice(0, quote)) return false
      // Attribute names come back lowercased, so match the tokens the same way.
      const tokens = value
        .slice(quote + 1)
        .toLowerCase()
        .split(/\s+/)
      const junk = [...theirs.attrs].filter(([junkName]) => !ours.attrs.has(junkName))
      const junkExplained = junk.every(
        ([junkName, junkValue]) =>
          junkValue === '' &&
          (tokens.includes(junkName) || tokens.includes(junkName.replace(/"$/, '')))
      )
      const junkNames = junk.map(([junkName]) => junkName)
      return (
        junkExplained &&
        sameAttrs(without(ours.attrs, [name]), without(theirs.attrs, [name, ...junkNames]))
      )
    },
  },
]

// --- Main -------------------------------------------------------------------------

const files = sourceFiles()
const ambient = ambientData(files[0].text)
const core = await loadCore()

// The spike's host names the three ambient values and types none of them, which is
// what leaves its ad hoc fields unknown-typed and renders them unannotated (D30).
const host = { ambient: { site: null, page: null, build: null } }

const compile = record(`compile (${files.length} files, 1 call)`)
compile(() => {
  expectClean(core.compile(files, host), 'compiling the spike')
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

// --- Compare ----------------------------------------------------------------------

const outDir = join(tmpdir(), 'lemmon-the-rebound')
mkdirSync(outDir, { recursive: true })
writeFileSync(join(outDir, 'lemmon.html'), pageHtml)

const reference = readFileSync(join(SPIKE, 'fixtures/reference.html'), 'utf8')
const comparison = compareRendered(
  renderedPage(pageHtml, { display: SITE_DISPLAY }),
  renderedPage(reference, { display: SITE_DISPLAY }),
  FORGIVENESS
)

// --- Report -----------------------------------------------------------------------

console.log(`lemmon output written to ${outDir}/lemmon.html`)
console.log()
console.log('rendered comparison (lemmon vs 11ty reference):')
console.log(`  bytes: ${pageHtml.length} lemmon, ${reference.length} 11ty`)
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
if (unexpected.length > 0) {
  failed = true
  console.error(
    `FAIL: ${unexpected.length} unexpected rendered difference(s), listed above`
  )
}
if (!timeOk) {
  failed = true
  console.error('FAIL: page render time is at or over the 1 ms/page exit criterion')
}
process.exitCode = failed ? 1 : 0

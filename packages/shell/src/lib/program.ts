import type {
  CodePayload,
  Core,
  Diagnostic,
  FromBinding,
  HostDecl,
  SourceFileInput,
  Span,
} from '@limulus/lemmon-core'
import { existsSync } from 'node:fs'
import { readFile } from 'node:fs/promises'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import picomatch from 'picomatch'

import { hasError } from './diagnostics.ts'
import type { Page } from './front-matter.ts'
import { claimsOf, type Plugin } from './plugins.ts'
import type { Site } from './site.ts'

/**
 * The file whose top-level bindings are `site` (D30, D36): every collection, data file
 * and environment value a site loads is declared there, relative to the input.
 */
export const BINDINGS_FILE = 'site.lem'

/**
 * The environment the standard shell declares (D30): `site` the bindings of
 * {@link BINDINGS_FILE}, `page` a `Page` and `build` a `Build` — both declared by
 * {@link AMBIENT_FILE} — every page rendered through its `layout` field and placed by
 * its `url`, and a generated page's neighbours filled into its `prev` and `next` where
 * its type declares them (D43). It is the ambient part alone: the mark claims and the
 * asset manifest are a site's plugins', which {@link hostFor} adds.
 */
export const HOST: HostDecl = {
  ambient: { site: { bindingsOf: BINDINGS_FILE }, page: 'Page', build: 'Build' },
  document: 'page',
  entry: 'layout',
  url: 'url',
  neighbours: ['prev', 'next'],
}

/**
 * The declaration a site whose config lists `plugins` compiles under: {@link HOST} with
 * what their handlers claim (D42), and `assets`, the manifest their asset hooks
 * returned (D38, D44). The claims are sent even when there are none, so a site listing
 * no plugins is warned at every mark, which nothing will fill: only a host with no
 * handlers at all — a playground, an editor — omits them, and hears nothing.
 */
export function hostFor(
  plugins: readonly Plugin[],
  assets: Record<string, string> = {}
): HostDecl {
  return { ...HOST, assets, marks: claimsOf(plugins) }
}

/**
 * The shell's own declarations, `Page`, `Build` and `Code` (docs/language.md §Ambient
 * types), compiled with every program. A file rather than a string so that a
 * diagnostic labelled at one of them — a page type redeclaring `url` as a `string` —
 * names a place an author can open.
 *
 * Its path is also the path the core knows it by: absolute, which no file the walk
 * finds can be, so a site's own `ambient.lem` is a template like any other rather than
 * a second file at one path.
 */
export const AMBIENT_FILE = fileURLToPath(new URL('ambient.lem', packageRoot()))

/**
 * This package's directory: the nearest one at or above this module that holds a
 * `package.json`. Searched rather than counted in `../`s because this module runs
 * from `src/lib/` under the source condition and from `dist/esm/lib/` once built.
 */
function packageRoot(): URL {
  let dir = new URL('./', import.meta.url)
  while (!existsSync(new URL('package.json', dir))) dir = new URL('../', dir)
  return dir
}

/**
 * A site that cannot be compiled for a reason no source file can carry a span for,
 * naming the file to blame.
 */
export class SiteError extends Error {
  readonly file: string

  constructor(file: string, message: string) {
    super(`${file}: ${message}`)
    this.name = 'SiteError'
    this.file = file
  }
}

/** A page as it was compiled: the record it is, and the glob that holds it. */
export interface CompiledPage {
  page: Page
  /**
   * The record the page was compiled as: the one its own `type:` named, else the one
   * the glob that holds it collects. A page with neither has no schema, renders its
   * own output with `page` unknown-typed (D30), and is loaded nowhere.
   */
  record?: string
  /** The `from` binding whose glob holds the page, if one does. */
  binding?: string
}

/** A site's program, compiled and resident in the core it was compiled in. */
export interface Program {
  /** Every `from` binding the program declares, as the core reported them. */
  sources: FromBinding[]
  /** Every page, by path, in path order. */
  pages: Map<string, CompiledPage>
  /**
   * Every code block's payload, by the id a render's mark names it with (D42): what a
   * handler is given beside the id. The second compile call reports them all, since a
   * code block is markdown and documents compile only there.
   */
  payloads: Map<string, CodePayload>
}

/** What compiling a site found: its program, unless an error stopped it, and why. */
export interface Compiled {
  program?: Program
  diagnostics: Diagnostic[]
}

/**
 * Compiles `site` into `core`, in the two calls a page's record needs (step 3 of the
 * build pipeline). The first compiles {@link AMBIENT_FILE} and every template — those
 * without front matter, {@link BINDINGS_FILE} among them, and the page templates too,
 * with no record, so that what a page template defines is in the index a layout is
 * compiled against — under the declaration {@link hostFor} makes of the config's
 * plugins and `assets`, the manifest their asset hooks returned. It reports the `from`
 * bindings and the record each glob collects. The second compiles every page —
 * documents and page templates — matched against the collecting globs, as its record.
 * An error in the first call stops the second, since no page can be typed without the
 * globs; a warning does not. A page whose front matter has `for:` and that no `type:`
 * and no glob types is an error at the key, and is not compiled as a page (D43).
 *
 * Documents compile only in the second call, so what a document (`.md`) defines is
 * invisible to every template, with front matter or without: the first call is where
 * a template is checked, and no document is in it.
 *
 * The text of {@link AMBIENT_FILE}, which this reads, is recorded in `texts` by its
 * path, as a build records every text its diagnostics may be placed in.
 *
 * @throws {SiteError} when the input has no {@link BINDINGS_FILE}: the declaration
 *   names it whatever the site holds, and without it no glob matches anything, so
 *   every document would render as it is, without its layout, far from the cause.
 */
export async function compileSite(
  core: Core,
  site: Site,
  texts: Map<string, string>,
  assets: Record<string, string> = {}
): Promise<Compiled> {
  const templates: SourceFileInput[] = []
  const pageTemplates = new Set<string>()
  for (const entry of site.entries) {
    if (entry.kind !== 'template') continue
    templates.push({ path: entry.path, text: entry.text })
    if (entry.page) pageTemplates.add(entry.path)
  }
  if (!templates.some((t) => t.path === BINDINGS_FILE && !pageTemplates.has(t.path))) {
    throw new SiteError(
      join(site.config.input, BINDINGS_FILE),
      'no bindings file: a site declares its collections and data in a `site.lem` ' +
        'without front matter here, and an empty one declares none'
    )
  }
  const ambient = { path: AMBIENT_FILE, text: await readFile(AMBIENT_FILE, 'utf8') }
  texts.set(ambient.path, ambient.text)
  const first = core.compile([ambient, ...templates], hostFor(site.config.plugins, assets))
  if (hasError(first.diagnostics)) return { diagnostics: first.diagnostics }
  // A page template compiles again in the second call, as its record, and reports its
  // warnings there; its first, recordless compile would report them twice.
  const diagnostics = first.diagnostics.filter((d) => !pageTemplates.has(d.path))
  const globs = first.sources
    .filter((source) => source.collects !== undefined)
    .map((source) => ({
      source,
      // The walk matches passthrough globs case-insensitively, as Eleventy does, and a
      // case-insensitive file system cannot tell `Posts/` from `posts/`: a collection
      // glob matches the same way, or one spelling of a directory would collect and
      // the other render untyped.
      matches: picomatch(source.literal, { dot: true, nocase: true }),
    }))
  const pages = new Map<string, CompiledPage>()
  const files: SourceFileInput[] = []
  for (const entry of site.entries) {
    if (entry.kind !== 'document' && entry.kind !== 'template') continue
    const page = site.pages.get(entry.path)
    if (page === undefined) continue
    const holders = globs.filter(({ matches }) => matches(entry.path))
    if (holders.length > 1) {
      diagnostics.push(
        heldTwice(
          entry.path,
          page,
          holders.map(({ source }) => source)
        )
      )
      continue
    }
    const binding = holders.at(0)?.source
    const record = page.record?.name ?? binding?.collects
    // A generator with a record goes to the core, which checks its `for:` key at load
    // and refuses one a glob holds; one with none has no type to make its pages of, and
    // no record to compile it as, so the core would never see it (D43).
    if (page.for !== undefined && record === undefined) {
      diagnostics.push(untyped(entry.path, entry.kind, page.for.spans['']))
      continue
    }
    pages.set(entry.path, { page, record, binding: binding?.name })
    const file: SourceFileInput = { path: entry.path, text: entry.text }
    if (record !== undefined) file.record = record
    if (page.record !== undefined) {
      const { start, end } = page.record.span
      file.recordSpan = { start, end }
    }
    files.push(file)
  }
  if (hasError(diagnostics)) return { diagnostics }
  const second = core.compile(files)
  diagnostics.push(...second.diagnostics)
  if (hasError(diagnostics)) return { diagnostics }
  const payloads = new Map(second.payloads.map((payload) => [payload.id, payload]))
  return { program: { sources: first.sources, pages, payloads }, diagnostics }
}

/**
 * A page two collecting globs match. The core loads a page once, since one path is one
 * render, so the shell refuses it at the page — at its `type:` where it has one, else
 * at its front matter — with a label at each binding.
 */
function heldTwice(path: string, page: Page, holders: FromBinding[]): Diagnostic {
  const names = LIST.format(holders.map((source) => `\`${source.name}\``))
  return {
    path,
    span: page.record?.span ?? pageSpan(page),
    severity: 'error',
    message:
      `the globs of ${names} each match this page, ` +
      'and a page belongs to one collection',
    labels: holders.map((source) => ({
      path: source.path,
      span: source.span,
      message: `\`${source.literal}\` matches it`,
    })),
  }
}

/** How a message lists names: `a and b`, `a, b, and c`. */
export const LIST = new Intl.ListFormat('en', { type: 'conjunction' })

/**
 * The refusal of a `for:` key on a page no `type:` and no glob types: a template's
 * generator has to name its pages' type, and a document is no generator at all — said
 * in the core's words for a document it is given with the key, since the mistake is the
 * same whether or not the page has a type (D43).
 */
function untyped(path: string, kind: 'document' | 'template', span: Span): Diagnostic {
  return {
    path,
    span,
    severity: 'error',
    message:
      kind === 'document'
        ? 'a document is one page, of its own prose, so it takes no `for:` key: a ' +
          'generator is a page template'
        : '`for:` makes this page a generator, and a generator names the type of its ' +
          'pages with `type:`',
    labels: [],
  }
}

/**
 * Where a diagnostic about a page as a whole points: its front matter, or the start of
 * the file where it has none.
 */
export function pageSpan(page: Page): Span {
  return page.loaded.spans[''] ?? FILE_START
}

/** The start of a file, as an empty span: where a file with nothing to point at is placed. */
export const FILE_START: Span = {
  start: 0,
  end: 0,
  range: { start: { line: 0, character: 0 }, end: { line: 0, character: 0 } },
}

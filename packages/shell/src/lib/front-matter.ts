import type { Diagnostic, JsonValue, LoadedValue, Span } from '@limulus/lemmon-core'
import { isMap, isScalar, type Pair, type ParsedNode } from 'yaml'

import { SourceText } from './source-text.ts'
import { YamlWalk, type Spans } from './yaml-values.ts'

export type { Spans } from './yaml-values.ts'

/**
 * Where a file's front matter stands, as string indexes into its text: the block runs
 * from the start of the file to {@link end}, and the YAML is what stands between its
 * two `---` lines.
 */
export interface Block {
  /** One past the closing line's line ending, which is where the body starts. */
  end: number
  /** The YAML between the opening and closing lines. */
  inner: { start: number; end: number }
}

/**
 * Finds the front matter `text` opens with, by the rule the core's scanner applies
 * (`lemmon_syntax::front_matter`), so the two never disagree about which files have it:
 * a first line that is exactly `---` opens a block that runs through the next line that
 * is exactly `---`, a carriage return before either newline allowed (D41).
 *
 * An opening line that nothing closes finds nothing. That is an error, but the core's
 * to report when it compiles the file, at the opening line; the shell parses no YAML
 * for it.
 */
export function findFrontMatter(text: string): Block | undefined {
  let opened: number | undefined
  let pos = 0
  while (pos < text.length) {
    const lineEnd = endOfLine(text, pos)
    const isRule = ruleAt(text, pos, lineEnd)
    if (opened === undefined) {
      if (!isRule) return undefined
      opened = lineEnd
    } else if (isRule) {
      return { end: lineEnd, inner: { start: opened, end: pos } }
    }
    pos = lineEnd
  }
  return undefined
}

/**
 * Whether `text` opens with the line that opens front matter by {@link findFrontMatter}'s
 * rule, whether or not a line closes it — a block nothing closes is still front matter
 * to the core, which reports it.
 */
export function opensFrontMatter(text: string): boolean {
  return ruleAt(text, 0, endOfLine(text, 0))
}

/** One past the newline ending the line that starts at `pos`, or the end of the text. */
function endOfLine(text: string, pos: number): number {
  const newline = text.indexOf('\n', pos)
  return newline === -1 ? text.length : newline + 1
}

/** Whether the line from `pos` to `lineEnd` is exactly `---`, line ending aside. */
const ruleAt = (text: string, pos: number, lineEnd: number): boolean =>
  text.slice(pos, lineEnd).replace(/[\r\n]+$/, '') === '---'

/**
 * A page — a document, or a template that opens with front matter (D41) — as the shell
 * reads it before anything has a type: its front matter as the value the core loads,
 * and the two keys the shell reads for itself (D30).
 */
export interface Page {
  /**
   * The front matter as the core loads it: `type:` and `for:` stripped, `url` and
   * `inputPath` derived where it gives neither, and a span for each field path. With
   * no front matter the value holds only what was derived.
   */
  loaded: LoadedValue & { path: string; value: { [key: string]: JsonValue }; spans: Spans }
  /**
   * The record the `type:` key named, with the span of its value. A source file's
   * `recordSpan` is `{ start, end }` of that span: the core refuses the `range` with it,
   * and TypeScript lets the whole span through where it is not a literal.
   */
  record?: { name: string; span: Span }
  /** A generator's `for:` key (D43), read and stripped as `type:` is. */
  for?: ForKey
  /** The byte offset the body starts at: past the closing `---` line, or 0. */
  body: number
}

/**
 * A generator's `for:` key as the shell reads it and the core takes it (D43): its value
 * as parsed, which the core checks; spans keyed from that value — `''` for the whole of
 * it, `topic` for its entry from key through value — and each entry's value spanned
 * alone, by its key, so that an error about the list underlines the list.
 */
export interface ForKey {
  value: JsonValue
  spans: Spans
  valueSpans: Spans
}

/**
 * The one entry of a `for:` key the core accepted: the list it names — `site.gallery` —
 * and where the entry stands, which a diagnostic about one of the generator's pages
 * points at. The core refuses a malformed key before it makes any page from it, so a
 * generator with a planned page has one.
 */
export function forEntry(key: ForKey): { list: string; span: Span } {
  const [[field, list]] = Object.entries(key.value as Record<string, JsonValue>)
  return { list: String(list), span: key.spans[field] }
}

/** What reading a page found: the page, unless an error stopped it, and why. */
export interface PageRead {
  page?: Page
  diagnostics: Diagnostic[]
}

/** How {@link readPage} places a page in the project. */
export interface ReadOptions {
  /**
   * The input directory's path from the directory holding `lemmon.config.ts`, with `/`
   * separators — `www` — which a derived `inputPath` starts with. `''`, the default,
   * makes the input path the core's path.
   */
  inputDir?: string
}

/**
 * Reads the page at `path`, which names it as the core does — relative to the input
 * directory, with `/` separators. Malformed YAML, a duplicate key, front matter that is
 * not a map, and a value JSON cannot carry are errors with the span they stand at, and
 * leave no page; an unresolved tag is a warning, and its value reads as a string.
 */
export function readPage(
  path: string,
  text: string,
  { inputDir = '' }: ReadOptions = {}
): PageRead {
  const where = { path, inputPath: inputDir === '' ? path : `${inputDir}/${path}` }
  const block = findFrontMatter(text)
  if (block === undefined) {
    return { page: { loaded: derived(where, {}, {}, false), body: 0 }, diagnostics: [] }
  }
  return new Reader(where, new SourceText(text), block).read()
}

/**
 * The url a page at `path` is written to when its front matter gives none: the path
 * without its extension, as a directory, with an `index` file standing for its
 * directory — `feed/the-rebound/index.md` at `/feed/the-rebound/`, `tils/git/foo.md` at
 * `/tils/git/foo/`, `index.lem` at `/`. A `#`, `?` or `%` in the path is
 * percent-encoded, since each would end or escape the path; spaces and non-ASCII are
 * left to the core's `url` type, which encodes them.
 */
export function derivedUrl(path: string): string {
  const stem = path.replace(/\.[^./]*$/, '').replace(/[#?%]/g, encodeURIComponent)
  if (stem === 'index') return '/'
  if (stem.endsWith('/index')) return `/${stem.slice(0, -'index'.length)}`
  return `/${stem}/`
}

/** The page's path as the core knows it, and as the project does. */
interface Where {
  path: string
  inputPath: string
}

/**
 * The value the core loads for a page: its front matter, then what the shell derives
 * where the front matter gave none. A generator's front matter derives no `url`, since
 * every page of one file would get the same one; its type's default gives each its own
 * (D43).
 */
function derived(
  { path, inputPath }: Where,
  value: { [key: string]: JsonValue },
  spans: Spans,
  generator: boolean
): Page['loaded'] {
  if (!generator && !Object.hasOwn(value, 'url')) value.url = derivedUrl(path)
  if (!Object.hasOwn(value, 'inputPath')) value.inputPath = inputPath
  return { value, path, spans }
}

/** One front matter block's YAML, walked into a page placed in the file. */
class Reader {
  readonly #where: Where
  readonly #source: SourceText
  readonly #block: Block
  readonly #walk: YamlWalk

  constructor(where: Where, source: SourceText, block: Block) {
    this.#where = where
    this.#source = source
    this.#block = block
    this.#walk = new YamlWalk(where.path, source, block.inner, {
      subject: 'front matter',
      key: 'a front-matter key',
    })
  }

  read(): PageRead {
    const root = this.#walk.parse()
    const page = root === undefined ? undefined : this.#page(root)
    const diagnostics = this.#walk.diagnostics
    return { page: this.#walk.failed ? undefined : page, diagnostics }
  }

  #page(root: ParsedNode | null): Page | undefined {
    const body = this.#source.offset(this.#block.end)
    if (root === null) {
      // Empty, or only comments: the whole block stands for the value.
      const spans = { '': this.#walk.fileSpan(0, this.#block.end) }
      return { loaded: derived(this.#where, {}, spans, false), body }
    }
    if (!isMap(root)) {
      this.#walk.error(root.range, 'front matter must be a map of fields')
      return undefined
    }
    const spans: Spans = { '': this.#walk.nodeSpan(root.range) }
    const page: Omit<Page, 'loaded'> = { body }
    const entries: [string, JsonValue][] = []
    for (const [name, pair] of this.#walk.fields(root, '')) {
      if (name === 'type') page.record = this.#record(pair)
      else if (name === 'for') page.for = this.#for(pair)
      else entries.push([name, this.#walk.entry(pair, name, spans)])
    }
    const value = Object.fromEntries(entries)
    return { ...page, loaded: derived(this.#where, value, spans, page.for !== undefined) }
  }

  /** The `type:` key's record name, which must be a string. */
  #record(pair: Pair<ParsedNode, ParsedNode | null>): Page['record'] {
    const node = pair.value
    if (node !== null && isScalar(node) && typeof node.value === 'string') {
      return { name: node.value, span: this.#walk.nodeSpan(node.range) }
    }
    const end = (node ?? pair.key).range[1]
    const message = '`type:` names a record, so its value must be a string'
    this.#walk.report('error', pair.key.range[0], end, message, [])
    return undefined
  }

  /** The `for:` key's value, with tables of its own. */
  #for(pair: Pair<ParsedNode, ParsedNode | null>): ForKey {
    const spans: Spans = {}
    const valueSpans: Spans = {}
    const node = pair.value ?? pair.key
    spans[''] = this.#walk.nodeSpan(node.range)
    const value = pair.value === null ? null : this.#walk.value(pair.value, '', spans)
    // An entry with no value has nothing to span alone, and one whose key is no string
    // was reported, leaving no page; neither is the core's to see.
    if (pair.value !== null && isMap(pair.value)) {
      for (const item of pair.value.items) {
        const key = isScalar(item.key) ? item.key.value : undefined
        const range = item.value?.range
        if (typeof key === 'string' && range !== undefined && range[1] > range[0]) {
          valueSpans[key] = this.#walk.nodeSpan(range)
        }
      }
    }
    return { value, spans, valueSpans }
  }
}

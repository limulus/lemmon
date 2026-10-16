import type { Diagnostic, JsonValue, Label, LoadedValue, Span } from '@limulus/lemmon-core'
import {
  isAlias,
  isMap,
  isScalar,
  parseDocument,
  type Pair,
  type ParsedNode,
  type Range,
  type YAMLMap,
} from 'yaml'

import { SourceText, splitsSurrogatePair } from './source-text.ts'

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
    const newline = text.indexOf('\n', pos)
    const lineEnd = newline === -1 ? text.length : newline + 1
    const isRule = text.slice(pos, lineEnd).replace(/[\r\n]+$/, '') === '---'
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

/** A table of spans by field path, as {@link LoadedValue.spans} keys it. */
export type Spans = Record<string, Span>

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
  /**
   * A generator's `for:` key (D43), read and stripped as `type:` is: its value, with
   * spans keyed from that value — `''` for the whole of it, `topic` for its entry.
   */
  for?: { value: JsonValue; spans: Spans }
  /** The byte offset the body starts at: past the closing `---` line, or 0. */
  body: number
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

/** One front matter block's YAML, walked into values and spans placed in the file. */
class Reader {
  readonly #where: Where
  readonly #path: string
  readonly #source: SourceText
  readonly #block: Block
  readonly #diagnostics: Diagnostic[] = []

  constructor(where: Where, source: SourceText, block: Block) {
    this.#where = where
    this.#path = where.path
    this.#source = source
    this.#block = block
  }

  read(): PageRead {
    const { inner } = this.#block
    const yaml = this.#source.text.slice(inner.start, inner.end)
    // `resolveKnownTags: false` keeps `!!timestamp`, `!!binary` and `!!set` from
    // producing a Date, bytes, or a Set: each is an unresolved tag, warned about, and
    // its value is read as the core-schema value it is written as. A duplicate key is
    // found by the walk instead of the parser, which points at one character of it.
    const doc = parseDocument(yaml, {
      prettyErrors: false,
      resolveKnownTags: false,
      uniqueKeys: false,
    })
    for (const error of doc.errors) {
      const message = `front matter is not valid YAML: ${error.message}`
      this.#report('error', ...this.#whole(error.pos), message, [])
    }
    for (const warning of doc.warnings) {
      this.#report('warning', ...this.#whole(warning.pos), warning.message, [])
    }
    const page = doc.errors.length === 0 ? this.#page(doc.contents) : undefined
    const failed = this.#diagnostics.some((d) => d.severity === 'error')
    // The parser's findings come first and the walk's after; a reader wants them in the
    // order they stand in the file.
    const diagnostics = this.#diagnostics.sort((a, b) => a.span.start - b.span.start)
    return { page: failed ? undefined : page, diagnostics }
  }

  #page(root: ParsedNode | null): Page | undefined {
    const body = this.#source.offset(this.#block.end)
    if (root === null) {
      // Empty, or only comments: the whole block stands for the value.
      const spans = {
        '': this.#source.span(0, trimmed(this.#source.text, 0, this.#block.end)),
      }
      return { loaded: derived(this.#where, {}, spans, false), body }
    }
    if (!isMap(root)) {
      this.#error(root.range, 'front matter must be a map of fields')
      return undefined
    }
    const spans: Spans = { '': this.#nodeSpan(root.range) }
    const page: Omit<Page, 'loaded'> = { body }
    const entries: [string, JsonValue][] = []
    for (const [name, pair] of this.#fields(root, '')) {
      if (name === 'type') page.record = this.#record(pair)
      else if (name === 'for') page.for = this.#for(pair)
      else entries.push([name, this.#entry(pair, name, spans)])
    }
    const value = Object.fromEntries(entries)
    return { ...page, loaded: derived(this.#where, value, spans, page.for !== undefined) }
  }

  /** The `type:` key's record name, which must be a string. */
  #record(pair: Pair<ParsedNode, ParsedNode | null>): Page['record'] {
    const node = pair.value
    if (node !== null && isScalar(node) && typeof node.value === 'string') {
      return { name: node.value, span: this.#nodeSpan(node.range) }
    }
    const end = (node ?? pair.key).range[1]
    const message = '`type:` names a record, so its value must be a string'
    this.#report('error', pair.key.range[0], end, message, [])
    return undefined
  }

  /** The `for:` key's value, with a table of its own. */
  #for(pair: Pair<ParsedNode, ParsedNode | null>): Page['for'] {
    const spans: Spans = {}
    const node = pair.value ?? pair.key
    spans[''] = this.#nodeSpan(node.range)
    const value = pair.value === null ? null : this.#value(pair.value, '', spans)
    return { value, spans }
  }

  /** A map entry's value, with its span — the key through the value — under `field`. */
  #entry(
    pair: Pair<ParsedNode, ParsedNode | null>,
    field: string,
    spans: Spans
  ): JsonValue {
    const end = (pair.value ?? pair.key).range[1]
    spans[field] = this.#span(pair.key.range[0], end)
    return pair.value === null ? null : this.#value(pair.value, field, spans)
  }

  /**
   * A map's entries by their keys' names. A key that is not a string, or that an
   * earlier entry of the map already gave, is reported and its entry skipped.
   */
  *#fields(
    map: YAMLMap.Parsed,
    path: string
  ): Generator<[string, Pair<ParsedNode, ParsedNode | null>]> {
    const seen = new Map<string, Range>()
    for (const pair of map.items) {
      const { key } = pair
      if (!isScalar(key) || typeof key.value !== 'string') {
        this.#error(key.range, 'a front-matter key must be a string')
        continue
      }
      const first = seen.get(key.value)
      if (first !== undefined) {
        const label = {
          path: this.#path,
          span: this.#nodeSpan(first),
          message: 'first given here',
        }
        this.#error(key.range, `\`${fieldPath(path, key.value)}\` is already given`, [
          label,
        ])
        continue
      }
      seen.set(key.value, key.range)
      yield [key.value, pair]
    }
  }

  #value(node: ParsedNode, path: string, spans: Spans): JsonValue {
    if (isScalar(node)) {
      // The core schema with known tags left unresolved yields nothing else.
      const value = node.value as string | number | boolean | null
      if (typeof value === 'number') {
        // A float literal is approximate by nature; an integer literal is not, and one
        // past 2^53 would load as a neighbouring integer without a word.
        if (!Number.isFinite(value)) {
          this.#error(node.range, `front matter cannot carry ${value}: not a finite number`)
        } else if (INTEGER.test(node.source) && !Number.isSafeInteger(value)) {
          const message = `front matter cannot carry ${node.source}: an integer this large`
          this.#error(node.range, `${message} loses precision`)
        }
      }
      return value
    }
    if (isAlias(node)) {
      this.#error(node.range, 'front matter cannot use an alias; write the value out')
      return null
    }
    if (isMap(node)) {
      const entries: [string, JsonValue][] = []
      for (const [name, pair] of this.#fields(node, path)) {
        entries.push([name, this.#entry(pair, fieldPath(path, name), spans)])
      }
      return Object.fromEntries(entries)
    }
    return node.items.map((item, index) => {
      const field = fieldPath(path, String(index))
      spans[field] = this.#nodeSpan(item.range)
      return this.#value(item, field, spans)
    })
  }

  /**
   * A position the parser reported, widened to whole characters. The parser counts
   * UTF-16 code units, and an error it places one unit long — `[offset, offset + 1]` —
   * ends inside an emoji standing at `offset`.
   */
  #whole([start, end]: [number, number]): [number, number] {
    const base = this.#block.inner.start
    const text = this.#source.text
    return [
      start - Number(splitsSurrogatePair(text, base + start)),
      end + Number(splitsSurrogatePair(text, base + end)),
    ]
  }

  #nodeSpan(range: Range): Span {
    return this.#span(range[0], range[1])
  }

  /** The span of `start..end`, string indexes into the YAML, placed in the file. */
  #span(start: number, end: number): Span {
    const base = this.#block.inner.start
    return this.#source.span(
      base + start,
      trimmed(this.#source.text, base + start, base + end)
    )
  }

  #error(range: Range, message: string, labels: Label[] = []): void {
    this.#report('error', range[0], range[1], message, labels)
  }

  #report(
    severity: Diagnostic['severity'],
    start: number,
    end: number,
    message: string,
    labels: Label[]
  ): void {
    this.#diagnostics.push({
      path: this.#path,
      span: this.#span(start, end),
      severity,
      message,
      labels,
    })
  }
}

/** An integer literal as the YAML 1.2 core schema reads one: decimal, octal, or hex. */
const INTEGER = /^[-+]?(?:[0-9]+|0o[0-7]+|0x[0-9a-fA-F]+)$/

/**
 * The field path of `key` under `prefix`, as a spans table keys it: `hero` at the top,
 * `hero.caption` under `hero`, `tags.2` for an element. The core spells it the same way
 * (`field_path` in `crates/lemmon-vm/src/load/place.rs`).
 */
function fieldPath(prefix: string, key: string): string {
  return prefix === '' ? key : `${prefix}.${key}`
}

/**
 * `end` moved back past the blanks and line breaks that end `start..end` in `text`: a
 * block scalar or collection, and the block as a whole, end past their last line
 * break, which a span stops short of.
 */
function trimmed(text: string, start: number, end: number): number {
  let last = end
  while (last > start && ' \t\r\n'.includes(text[last - 1])) last--
  return last
}

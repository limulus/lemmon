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

/** A table of spans by field path, as {@link LoadedValue.spans} keys it. */
export type Spans = Record<string, Span>

/** How a walk's messages name what it reads. */
export interface Wording {
  /** The YAML, as a message's subject: `front matter`, `this data file`. */
  subject: string
  /** One of its keys: `a front-matter key`, `a key in this data file`. */
  key: string
}

/** A part of a file's text, as string indexes into it. */
export interface Region {
  start: number
  end: number
}

/**
 * One region of a file's text — a front matter block's YAML, or a whole data file —
 * parsed with the `yaml` package and walked into the JSON value the core loads, with a
 * span for every field path placed in the file, in bytes and UTF-16 positions as the
 * core counts them. What JSON cannot carry — a key that is not a string, a key given
 * twice, an alias, a number past what a double holds exactly — is an error at the span
 * it stands at, and bad YAML is too: a walk reports, and never throws.
 *
 * The walk is the generic half of reading front matter and data alike; what a page
 * makes of its keys is `readPage`'s.
 */
export class YamlWalk {
  readonly #path: string
  readonly #source: SourceText
  readonly #region: Region
  readonly #wording: Wording
  readonly #diagnostics: Diagnostic[] = []

  constructor(path: string, source: SourceText, region: Region, wording: Wording) {
    this.#path = path
    this.#source = source
    this.#region = region
    this.#wording = wording
  }

  /**
   * The region's root node: `null` where it holds nothing but blanks and comments, and
   * `undefined` where the YAML has an error, which is reported.
   */
  parse(): ParsedNode | null | undefined {
    const yaml = this.#source.text.slice(this.#region.start, this.#region.end)
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
      const message = `${this.#wording.subject} is not valid YAML: ${error.message}`
      this.report('error', ...this.#whole(error.pos), message, [])
    }
    for (const warning of doc.warnings) {
      this.report('warning', ...this.#whole(warning.pos), warning.message, [])
    }
    return doc.errors.length === 0 ? doc.contents : undefined
  }

  /** Whether the walk has reported an error. */
  get failed(): boolean {
    return this.#diagnostics.some((d) => d.severity === 'error')
  }

  /**
   * What the walk reported, in the order it stands in the file: the parser's findings
   * come first and the walk's after, and a reader wants neither order.
   */
  get diagnostics(): Diagnostic[] {
    return this.#diagnostics.sort((a, b) => a.span.start - b.span.start)
  }

  /**
   * `node`'s value at the field path `path`, with a span in `spans` for every field
   * path under it — each element's, and each entry's from its key through its value.
   */
  value(node: ParsedNode, path: string, spans: Spans): JsonValue {
    if (isScalar(node)) {
      // The core schema with known tags left unresolved yields nothing else.
      const value = node.value as string | number | boolean | null
      if (typeof value === 'number') {
        const { subject } = this.#wording
        // A float literal is approximate by nature; an integer literal is not, and one
        // past 2^53 would load as a neighbouring integer without a word.
        if (!Number.isFinite(value)) {
          this.error(node.range, `${subject} cannot carry ${value}: not a finite number`)
        } else if (INTEGER.test(node.source) && !Number.isSafeInteger(value)) {
          const message = `${subject} cannot carry ${node.source}: an integer this large`
          this.error(node.range, `${message} loses precision`)
        }
      }
      return value
    }
    if (isAlias(node)) {
      const message = `${this.#wording.subject} cannot use an alias; write the value out`
      this.error(node.range, message)
      return null
    }
    if (isMap(node)) {
      const entries: [string, JsonValue][] = []
      for (const [name, pair] of this.fields(node, path)) {
        entries.push([name, this.entry(pair, fieldPath(path, name), spans)])
      }
      return Object.fromEntries(entries)
    }
    return node.items.map((item, index) => {
      const field = fieldPath(path, String(index))
      spans[field] = this.nodeSpan(item.range)
      return this.value(item, field, spans)
    })
  }

  /** A map entry's value, with its span — the key through the value — under `field`. */
  entry(pair: Pair<ParsedNode, ParsedNode | null>, field: string, spans: Spans): JsonValue {
    const end = (pair.value ?? pair.key).range[1]
    spans[field] = this.span(pair.key.range[0], end)
    return pair.value === null ? null : this.value(pair.value, field, spans)
  }

  /**
   * A map's entries by their keys' names, the map standing at the field path `path`.
   * A key that is not a string, or that an earlier entry of the map already gave, is
   * reported and its entry skipped.
   */
  *fields(
    map: YAMLMap.Parsed,
    path: string
  ): Generator<[string, Pair<ParsedNode, ParsedNode | null>]> {
    const seen = new Map<string, Range>()
    for (const pair of map.items) {
      const { key } = pair
      if (!isScalar(key) || typeof key.value !== 'string') {
        this.error(key.range, `${this.#wording.key} must be a string`)
        continue
      }
      const first = seen.get(key.value)
      if (first !== undefined) {
        const label = {
          path: this.#path,
          span: this.nodeSpan(first),
          message: 'first given here',
        }
        this.error(key.range, `\`${fieldPath(path, key.value)}\` is already given`, [label])
        continue
      }
      seen.set(key.value, key.range)
      yield [key.value, pair]
    }
  }

  /** The span of a node's range. */
  nodeSpan(range: Range): Span {
    return this.span(range[0], range[1])
  }

  /** The span of `start..end`, string indexes into the YAML, placed in the file. */
  span(start: number, end: number): Span {
    const base = this.#region.start
    return this.fileSpan(base + start, base + end)
  }

  /**
   * The span of `start..end`, string indexes into the file, stopping short of the
   * blanks and line breaks that end it: a block scalar or collection, and a block or
   * a file as a whole, end past their last line break.
   */
  fileSpan(start: number, end: number): Span {
    const text = this.#source.text
    let last = end
    while (last > start && ' \t\r\n'.includes(text[last - 1])) last--
    return this.#source.span(start, last)
  }

  /** Reports an error at a node's range. */
  error(range: Range, message: string, labels: Label[] = []): void {
    this.report('error', range[0], range[1], message, labels)
  }

  /** Reports a diagnostic at `start..end`, string indexes into the YAML. */
  report(
    severity: Diagnostic['severity'],
    start: number,
    end: number,
    message: string,
    labels: Label[]
  ): void {
    this.#diagnostics.push({
      path: this.#path,
      span: this.span(start, end),
      severity,
      message,
      labels,
    })
  }

  /**
   * A position the parser reported, widened to whole characters. The parser counts
   * UTF-16 code units, and an error it places one unit long — `[offset, offset + 1]` —
   * ends inside an emoji standing at `offset`.
   */
  #whole([start, end]: [number, number]): [number, number] {
    const base = this.#region.start
    const text = this.#source.text
    return [
      start - Number(splitsSurrogatePair(text, base + start)),
      end + Number(splitsSurrogatePair(text, base + end)),
    ]
  }
}

/** What reading a data file found: its value, unless an error stopped it, and why. */
export interface DataRead {
  loaded?: LoadedValue & { path: string; spans: Spans }
  diagnostics: Diagnostic[]
}

/**
 * Reads the data file at `path` — JSON, or YAML, of which JSON is a part — as the one
 * value a `from` binding loads, with a span for the value as a whole and for every
 * field path in it. Malformed YAML and a value JSON cannot carry are errors with the
 * span they stand at, and leave no value; a file of nothing but comments is `null`.
 */
export function readData(path: string, text: string): DataRead {
  const walk = new YamlWalk(
    path,
    new SourceText(text),
    { start: 0, end: text.length },
    {
      subject: 'this data file',
      key: 'a key in this data file',
    }
  )
  const root = walk.parse()
  if (root === undefined) return { diagnostics: walk.diagnostics }
  const spans: Spans = {}
  let value: JsonValue = null
  if (root === null) {
    spans[''] = walk.fileSpan(0, text.length)
  } else {
    spans[''] = walk.nodeSpan(root.range)
    value = walk.value(root, '', spans)
  }
  const diagnostics = walk.diagnostics
  if (walk.failed) return { diagnostics }
  return { loaded: { value, path, spans }, diagnostics }
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

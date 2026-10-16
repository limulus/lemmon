import type { Position, Span } from '@limulus/lemmon-core'

/**
 * A file's text, able to place a range of it as the core does: byte offsets into its
 * UTF-8 encoding, and LSP positions whose `character` counts UTF-16 code units from the
 * start of the line. JavaScript indexes a string in UTF-16 code units, so a position
 * falls out of the index directly and the byte offset is the one that is computed.
 *
 * Only `\n` starts a line, as in the core's line index: a `\r` belongs to the line it
 * ends, so a CRLF file's columns agree with its byte offsets.
 */
export class SourceText {
  readonly text: string
  /** The string index each line starts at. */
  readonly #lineStarts: number[] = [0]

  constructor(text: string) {
    this.text = text
    for (let i = text.indexOf('\n'); i !== -1; i = text.indexOf('\n', i + 1)) {
      this.#lineStarts.push(i + 1)
    }
  }

  /** The span of the string indexes `start..end`. */
  span(start: number, end: number): Span {
    return {
      start: this.offset(start),
      end: this.offset(end),
      range: { start: this.#position(start), end: this.#position(end) },
    }
  }

  /** The byte offset of the string index `index`. */
  offset(index: number): number {
    // The core traps on a span that splits a character (invariant 8), so an index
    // outside the text or inside a surrogate pair is refused here rather than sent. A
    // parser can report one — the `yaml` package's one-unit error spans stop inside an
    // emoji — so its positions are widened with `splitsSurrogatePair` before they get
    // here, and this is the backstop for anything that was not.
    if (!Number.isInteger(index) || index < 0 || index > this.text.length) {
      throw new RangeError(`index ${index} is outside a text of length ${this.text.length}`)
    }
    if (splitsSurrogatePair(this.text, index)) {
      throw new RangeError(`index ${index} splits a surrogate pair`)
    }
    return Buffer.byteLength(this.text.slice(0, index), 'utf8')
  }

  #position(index: number): Position {
    // The last line start at or before `index`.
    let low = 0
    let high = this.#lineStarts.length - 1
    while (low < high) {
      const mid = (low + high + 1) >> 1
      if (this.#lineStarts[mid] <= index) low = mid
      else high = mid - 1
    }
    return { line: low, character: index - this.#lineStarts[low] }
  }
}

/** Whether the string index `index` falls between the two halves of a surrogate pair. */
export function splitsSurrogatePair(text: string, index: number): boolean {
  return (
    isLowSurrogate(text.charCodeAt(index)) && isHighSurrogate(text.charCodeAt(index - 1))
  )
}

const isHighSurrogate = (unit: number): boolean => unit >= 0xd800 && unit <= 0xdbff
const isLowSurrogate = (unit: number): boolean => unit >= 0xdc00 && unit <= 0xdfff

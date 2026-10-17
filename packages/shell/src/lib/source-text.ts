import type { Position, Range, Span } from '@limulus/lemmon-core'

/**
 * A file's text, able to place a range of it as the core does — byte offsets into its
 * UTF-8 encoding, and LSP positions whose `character` counts UTF-16 code units from the
 * start of the line — and to show one. JavaScript indexes a string in UTF-16 code units,
 * so a position falls out of the index directly and the byte offset is the one that is
 * computed.
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

  /**
   * The lines that show `range` beneath a diagnostic: each line it covers, marked `>`
   * with its number from 1, and under each a marker line of `^` beneath what the range
   * covers there — from the start's column on its first line, to the end's column on
   * its last, and the whole of each line between. A range ending at the start of a line
   * covers nothing on it, so that line is not shown, and an empty range is one `^` at
   * its place. The marker line keeps the tabs before its carets, so a caret stands under
   * the character it marks after a tab; inside the range a tab is one caret, like any
   * other column. A `\r` ending a line is neither shown nor counted as a column; every
   * other control character and a byte order mark are shown as U+FFFD, a column each.
   *
   * Columns are the positions' own UTF-16 code units. An astral emoji is two units and
   * two columns wide, so the carets after it stay aligned; but a BMP emoji such as ☕ is
   * one unit and two columns wide, and so is each CJK character, so the carets drift a
   * column to the left for each one before them, and a combining mark, a unit drawn in
   * no column of its own, drifts them a column to the right.
   *
   * A line longer than {@link WINDOW} units is shown as that many of them, with `…`
   * where it is cut: the range's first line from {@link LEAD} units before the range
   * starts, and every later line from its own start. A range covering more than five
   * lines shows its first three and its last, with `…` between: its opening lines say
   * what it is — a block's head, a front matter's first keys — and its last where it
   * ends, and the lines between would push the next diagnostic off the screen. A range
   * that does not fall in the text, past its last line or a line's end, shows nothing.
   */
  frame({ start, end }: Range): string[] {
    if (!this.#holds(start) || !this.#holds(end)) return []
    const last = end.line > start.line && end.character === 0 ? end.line - 1 : end.line
    // `undefined` stands for the lines left out.
    const lines: (number | undefined)[] = []
    for (let line = start.line; line <= last; line++) lines.push(line)
    if (lines.length > 5) lines.splice(3, lines.length - 4, undefined)
    const width = String(last + 1).length
    const bar = (gutter: string, text: string) =>
      `${gutter} |${text === '' ? '' : ` ${text}`}`
    return lines.flatMap((line) => {
      if (line === undefined) return [`  ${'…'.padStart(width)} |`]
      const text = this.#line(line)
      const from = line === start.line ? start.character : 0
      const to = line === end.line ? Math.min(end.character, text.length) : text.length
      const cut = Math.max(0, Math.min(from - LEAD, text.length - WINDOW))
      const stop = Math.min(cut + WINDOW, text.length)
      const head = cut > 0 ? '…' : ''
      const tail = stop < text.length ? '…' : ''
      // The first line marks where the range starts even where it covers no column of
      // it: an empty range, or one that starts at the line's end.
      const carets = Math.max(Math.min(to, stop) - from, line === start.line ? 1 : 0)
      const shown = head + visible(text.slice(cut, stop)) + tail
      const before = ' '.repeat(head.length) + text.slice(cut, from).replace(/[^\t]/g, ' ')
      return [
        bar(`> ${String(line + 1).padStart(width)}`, shown),
        bar(`  ${' '.repeat(width)}`, before + '^'.repeat(carets)),
      ]
    })
  }

  /** Whether `position` is in the text: on one of its lines, not past that line's end. */
  #holds({ line, character }: Position): boolean {
    return (
      line < this.#lineStarts.length &&
      character <= this.#end(line) - this.#lineStarts[line]
    )
  }

  /** The text of line `line`, without the `\n` that ends it or a `\r` before that. */
  #line(line: number): string {
    return this.text.slice(this.#lineStarts[line], this.#end(line)).replace(/\r$/, '')
  }

  /** The string index line `line` ends at: its `\n`, or the end of the text. */
  #end(line: number): number {
    return line + 1 < this.#lineStarts.length
      ? this.#lineStarts[line + 1] - 1
      : this.text.length
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

/**
 * How many units of a line a frame shows: a line up to this long — what source wrapped at
 * an editor's width runs to — is shown whole, and a longer one, generated or minified
 * on one line, as a window of it, which with the gutter fits a terminal 120 columns
 * wide.
 */
const WINDOW = 100

/** How many units before a range's start a window of a long line begins. */
const LEAD = 20

/**
 * `text` with each character a terminal would not draw as one column of its own — a
 * control character but the tab, and a byte order mark — as U+FFFD, one unit for one,
 * so the carets beneath still count the line's units.
 */
const visible = (text: string): string =>
  text.replace(/[\x00-\x08\x0b-\x1f\x7f-\x9f\ufeff]/g, '\ufffd')

/** Whether the string index `index` falls between the two halves of a surrogate pair. */
export function splitsSurrogatePair(text: string, index: number): boolean {
  return (
    isLowSurrogate(text.charCodeAt(index)) && isHighSurrogate(text.charCodeAt(index - 1))
  )
}

const isHighSurrogate = (unit: number): boolean => unit >= 0xd800 && unit <= 0xdbff
const isLowSurrogate = (unit: number): boolean => unit >= 0xdc00 && unit <= 0xdfff

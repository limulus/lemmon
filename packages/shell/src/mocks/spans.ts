import type { Position, Span } from '@limulus/lemmon-core'

/**
 * The span of `needle`'s one occurrence in `source`, computed apart from the code under
 * test: byte offsets by encoding the text before each end, positions by counting the
 * `\n`s before it and the UTF-16 code units since the last. Bracketing part of the
 * needle with `«…»` spans that part alone — `at(src, 'title: a\n«title»')` is the
 * second `title`. Mirrors `at` in `crates/lemmon-syntax/src/spans.rs`.
 */
export function at(source: string, needle: string): Span {
  const plain = needle.replace('«', '').replace('»', '')
  const found = source.indexOf(plain)
  if (found === -1) throw new Error(`${JSON.stringify(plain)} is not in the source`)
  if (source.includes(plain, found + 1)) {
    throw new Error(`${JSON.stringify(plain)} is in the source more than once`)
  }
  const open = needle.indexOf('«')
  const [start, end] =
    open === -1
      ? [found, found + plain.length]
      : [found + open, found + needle.indexOf('»') - 1]
  return {
    start: bytes(source.slice(0, start)),
    end: bytes(source.slice(0, end)),
    range: { start: position(source, start), end: position(source, end) },
  }
}

/** The bytes of `source` that `span` covers, decoded: what a span points at. */
export function covered(source: string, span: Span): string {
  return Buffer.from(source, 'utf8').subarray(span.start, span.end).toString('utf8')
}

const bytes = (text: string): number => new TextEncoder().encode(text).length

function position(source: string, index: number): Position {
  const before = source.slice(0, index)
  return {
    line: before.split('\n').length - 1,
    character: index - (before.lastIndexOf('\n') + 1),
  }
}

import { describe, expect, it } from 'vitest'

import { SourceText } from './source-text.ts'
import { at } from '../mocks/spans.ts'

/** The frame of `needle`'s span in `text`, as {@link at} places it. */
const framed = (text: string, needle: string): string[] =>
  new SourceText(text).frame(at(text, needle).range)

describe('SourceText', () => {
  describe('span()', () => {
    it('should give byte offsets and positions for an all-ASCII text', () => {
      const text = 'one\ntwo\nthree'
      const start = text.indexOf('two')

      expect(new SourceText(text).span(start, start + 3)).toEqual(at(text, 'two'))
    })

    it('should count bytes for offsets and UTF-16 code units for columns', () => {
      // `é` is two bytes and one code unit, `’` three bytes and one code unit, and `😀`
      // four bytes and two code units.
      const text = 'a\né’😀 x\n'
      const start = text.indexOf('x')

      expect(new SourceText(text).span(start, start + 1)).toEqual({
        start: 12,
        end: 13,
        range: {
          start: { line: 1, character: 5 },
          end: { line: 1, character: 6 },
        },
      })
      expect(new SourceText(text).span(start, start + 1)).toEqual(at(text, 'x'))
    })

    it('should start a line only at a newline, keeping a carriage return', () => {
      const text = 'a\r\nb\rc'
      const start = text.indexOf('c')

      expect(new SourceText(text).span(start, start + 1).range.start).toEqual({
        line: 1,
        character: 2,
      })
    })

    it('should place the end of the text, and a line that starts it', () => {
      const text = 'a\n'
      const source = new SourceText(text)

      expect(source.span(2, 2).range.start).toEqual({ line: 1, character: 0 })
      expect(source.span(0, 2).end).toBe(2)
    })
  })

  describe('offset()', () => {
    it('should refuse an index outside the text', () => {
      const source = new SourceText('abc')

      expect(() => source.offset(-1)).toThrow('index -1 is outside a text of length 3')
      expect(() => source.offset(4)).toThrow('index 4 is outside a text of length 3')
      expect(() => source.offset(1.5)).toThrow('index 1.5 is outside')
    })

    it('should refuse an index that splits a surrogate pair', () => {
      const source = new SourceText('😀')

      expect(() => source.offset(1)).toThrow('index 1 splits a surrogate pair')
      expect(source.offset(2)).toBe(4)
    })

    it('should take a lone low surrogate as a character of its own', () => {
      // Encoded as U+FFFD, as the core's boundary encodes it: three bytes.
      const source = new SourceText('a\udc00b')

      expect(source.offset(1)).toBe(1)
      expect(source.offset(2)).toBe(4)
    })
  })

  describe('frame()', () => {
    it('should show the line a span is on, with carets under what it covers', () => {
      expect(framed('one\ntwo three\nfour\n', 'three')).toEqual([
        '> 2 | two three',
        '    |     ^^^^^',
      ])
    })

    it('should show an empty span as one caret at its place', () => {
      expect(framed('one\ntwo three\n', 'two «»three')).toEqual([
        '> 2 | two three',
        '    |     ^',
      ])
    })

    it('should mark a span over two lines from its start and to its end', () => {
      expect(framed('one\ntwo three\nfour five\n', 'three\nfour')).toEqual([
        '> 2 | two three',
        '    |     ^^^^^',
        '> 3 | four five',
        '    | ^^^^',
      ])
    })

    it('should mark the whole of each line between, an empty one with no caret', () => {
      expect(framed('a\nbc\n\nd\n', 'a\nbc\n\nd')).toEqual([
        '> 1 | a',
        '    | ^',
        '> 2 | bc',
        '    | ^^',
        '> 3 |',
        '    |',
        '> 4 | d',
        '    | ^',
      ])
    })

    it('should leave out the line a span ends at the start of', () => {
      // The shape of a block's span that ends past its last line break.
      expect(framed('a\nb\nc\nd\ne\n', 'b\nc\nd\n')).toEqual([
        '> 2 | b',
        '    | ^',
        '> 3 | c',
        '    | ^',
        '> 4 | d',
        '    | ^',
      ])
    })

    it('should mark a span starting at a line’s end there, though it covers no column', () => {
      expect(framed('ab\ncd\n', 'ab«\n»cd')).toEqual(['> 1 | ab', '    |   ^'])
    })

    it('should keep the tabs before a span, and give a tab inside one a caret', () => {
      expect(framed('\tx = 1\n', 'x')).toEqual(['> 1 | \tx = 1', '    | \t^'])
      expect(framed('a\tb\n', 'a\tb')).toEqual(['> 1 | a\tb', '    | ^^^'])
    })

    it('should count columns in UTF-16 code units, as positions do', () => {
      // `é` is one code unit and the emoji two, as many columns as a terminal draws.
      expect(framed('é😀 x\n', 'x')).toEqual(['> 1 | é😀 x', '    |     ^'])
    })

    it('should neither show nor mark the carriage return of a CRLF line', () => {
      expect(framed('one\r\ntwo\r\n', 'one\r\ntwo')).toEqual([
        '> 1 | one',
        '    | ^^^',
        '> 2 | two',
        '    | ^^^',
      ])
      expect(framed('one\r\ntwo\r\n', 'two\r')).toEqual(['> 2 | two', '    | ^^^'])
    })

    it('should show the last line when no newline ends it, and the end of the text', () => {
      expect(framed('a\nlast', 'last')).toEqual(['> 2 | last', '    | ^^^^'])
      expect(new SourceText('a\n').frame(at('a\n', 'a\n«»').range)).toEqual([
        '> 2 |',
        '    | ^',
      ])
    })

    it('should pad the gutter to the widest line number shown', () => {
      const text = '1\n2\n3\n4\n5\n6\n7\n8\nnine\nten\n'

      expect(framed(text, 'nine\nten')).toEqual([
        '>  9 | nine',
        '     | ^^^^',
        '> 10 | ten',
        '     | ^^^',
      ])
    })

    it('should show a span over more than five lines as its first three and its last', () => {
      const text = 'l1\nl2\nl3\nl4\nl5\nl6\n'

      expect(framed(text, text)).toEqual([
        '> 1 | l1',
        '    | ^^',
        '> 2 | l2',
        '    | ^^',
        '> 3 | l3',
        '    | ^^',
        '  … |',
        '> 6 | l6',
        '    | ^^',
      ])
      // Five lines are shown whole, so that `…` never stands for one line alone.
      expect(framed(text, 'l1\nl2\nl3\nl4\nl5')).toHaveLength(10)
    })

    it('should pad the gutter to the widest line number shown, not the line a span ends at', () => {
      const text = '1\n2\n3\n4\n5\n6\n7\neight\nnine\nten\n'

      expect(framed(text, 'eight\nnine\n')).toEqual([
        '> 8 | eight',
        '    | ^^^^^',
        '> 9 | nine',
        '    | ^^^^',
      ])
    })

    it('should pad the `…` of a long span to the gutter', () => {
      const text = Array.from({ length: 12 }, (_, i) => `l${i + 1}`).join('\n')

      expect(framed(text, 'l5\nl6\nl7\nl8\nl9\nl10')).toEqual([
        '>  5 | l5',
        '     | ^^',
        '>  6 | l6',
        '     | ^^',
        '>  7 | l7',
        '     | ^^',
        '   … |',
        '> 10 | l10',
        '     | ^^^',
      ])
    })

    describe('a long line', () => {
      it('should show it from a little before the span, cut at the start', () => {
        expect(framed(`${'a'.repeat(120)} needle\n`, 'needle')).toEqual([
          `> 1 | …${'a'.repeat(93)} needle`,
          `    | ${' '.repeat(95)}^^^^^^`,
        ])
      })

      it('should cut it at the end where it runs on past the window', () => {
        expect(framed(`needle ${'a'.repeat(120)}\n`, 'needle')).toEqual([
          `> 1 | needle ${'a'.repeat(93)}…`,
          '    | ^^^^^^',
        ])
      })

      it('should cut it at both ends', () => {
        expect(framed(`${'a'.repeat(60)} needle ${'a'.repeat(100)}\n`, 'needle')).toEqual([
          `> 1 | …${'a'.repeat(19)} needle ${'a'.repeat(73)}…`,
          `    | ${' '.repeat(21)}^^^^^^`,
        ])
      })

      it('should keep a tab inside the window and drop one before it', () => {
        expect(framed(`\t${'a'.repeat(110)}\tneedle\n`, 'needle')).toEqual([
          `> 1 | …${'a'.repeat(93)}\tneedle`,
          `    | ${' '.repeat(94)}\t^^^^^^`,
        ])
      })

      it('should mark a span longer than the window to the cut', () => {
        expect(framed(`x ${'b'.repeat(150)} y\n`, 'b'.repeat(150))).toEqual([
          `> 1 | x ${'b'.repeat(98)}…`,
          `    |   ${'^'.repeat(98)}`,
        ])
      })

      it('should show a line a span covers whole from its start', () => {
        const text = `start\n${'m'.repeat(120)}\nend\n`

        expect(framed(text, text.trimEnd())).toEqual([
          '> 1 | start',
          '    | ^^^^^',
          `> 2 | ${'m'.repeat(100)}…`,
          `    | ${'^'.repeat(100)}`,
          '> 3 | end',
          '    | ^^^',
        ])
      })

      it('should show a line of the window’s length whole, and cut one a unit longer', () => {
        expect(framed(`${'a'.repeat(93)} needle\n`, 'needle')).toEqual([
          `> 1 | ${'a'.repeat(93)} needle`,
          `    | ${' '.repeat(94)}^^^^^^`,
        ])
        expect(framed(`${'a'.repeat(94)} needle\n`, 'needle')).toEqual([
          `> 1 | …${'a'.repeat(93)} needle`,
          `    | ${' '.repeat(95)}^^^^^^`,
        ])
      })
    })

    describe('a character a terminal does not show', () => {
      it('should show a byte order mark as U+FFFD, one column', () => {
        expect(framed('\ufeff<lem.nope>\n', 'nope')).toEqual([
          '> 1 | \ufffd<lem.nope>',
          '    |       ^^^^',
        ])
      })

      it('should show a carriage return inside a line as U+FFFD, one column', () => {
        expect(framed('a\rb c\n', 'c')).toEqual(['> 1 | a\ufffdb c', '    |     ^'])
      })

      it('should show every other control character so, and keep the tab', () => {
        expect(framed('\x00\x08\x0b\x1b\x1f\x7f\x85\x9f\tx\n', 'x')).toEqual([
          `> 1 | ${'\ufffd'.repeat(8)}\tx`,
          `    | ${' '.repeat(8)}\t^`,
        ])
      })
    })

    it('should show nothing for a range that does not fall in the text', () => {
      const source = new SourceText('a\nb')
      const point = (line: number, character: number) => ({ line, character })

      expect(source.frame({ start: point(2, 0), end: point(2, 0) })).toEqual([])
      expect(source.frame({ start: point(0, 2), end: point(1, 0) })).toEqual([])
      expect(source.frame({ start: point(0, 0), end: point(1, 2) })).toEqual([])
      expect(source.frame({ start: point(0, 1), end: point(1, 1) })).toHaveLength(4)
    })
  })
})

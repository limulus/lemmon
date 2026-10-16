import { describe, expect, it } from 'vitest'

import { SourceText } from './source-text.ts'
import { at } from '../mocks/spans.ts'

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
})

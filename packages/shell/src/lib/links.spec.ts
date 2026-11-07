import { describe, expect, it } from 'vitest'

import { srcset, stripped, tokens, type Located } from './links.ts'

/** What each of `located` covers in `value`, which its url should be. */
const sliced = (value: string, located: Located[]) =>
  located.map(({ start, end }) => value.slice(start, end))

describe('srcset()', () => {
  it('should read each candidate’s url, at offsets that slice back to it', () => {
    const value = ' /a.jpg 1x,/b.jpg  2x ,\n/c.jpg 640w 480h'
    const urls = ['/a.jpg', '/b.jpg', '/c.jpg']

    expect(srcset(value)!.map(({ url }) => url)).toEqual(urls)
    expect(sliced(value, srcset(value)!)).toEqual(urls)
  })

  it('should keep the comma inside a `data:` url, and take a trailing one off a url', () => {
    const value = 'data:image/png;base64,AAAA 1x, /b.jpg,, /c.jpg'
    const urls = ['data:image/png;base64,AAAA', '/b.jpg', '/c.jpg']

    expect(srcset(value)!.map(({ url }) => url)).toEqual(urls)
    expect(sliced(value, srcset(value)!)).toEqual(urls)
  })

  it('should read no candidate in a value of only whitespace and commas', () => {
    for (const value of ['', ' , ', '\t,\n,']) expect(srcset(value), value).toEqual([])
  })

  it('should refuse the whole value where one candidate’s descriptors do not parse', () => {
    for (const value of [
      '/a.jpg 1x, /b.jpg 2q',
      '/a.jpg 0w',
      '/a.jpg -1x',
      '/a.jpg 1x 2x',
    ]) {
      expect(srcset(value), value).toBeUndefined()
    }
  })
})

describe('tokens()', () => {
  it('should read the urls between ASCII whitespace, at offsets that slice back to each', () => {
    const value = ' /p1\t/p2\n/p3\f/p4\r/p5,/p6 '
    const urls = ['/p1', '/p2', '/p3', '/p4', '/p5,/p6']

    expect(tokens(value).map(({ url }) => url)).toEqual(urls)
    expect(sliced(value, tokens(value))).toEqual(urls)
  })

  it('should read no url where there is only whitespace', () => {
    expect(tokens(' \t\n')).toEqual([])
  })
})

describe('stripped()', () => {
  it('should take ASCII whitespace off either end, and nothing else', () => {
    expect(stripped(' \t\n\f\r/a b\r\f\n\t ')).toBe('/a b')
    expect(stripped('\u00a0/a\u00a0')).toBe('\u00a0/a\u00a0')
  })
})

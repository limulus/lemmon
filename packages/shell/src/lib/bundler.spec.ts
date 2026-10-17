import type { Fragment, Mark } from '@limulus/lemmon-core'
import { describe, expect, it } from 'vitest'

import { bundler } from './bundler.ts'
import { claimsOf, validatePlugins, type ProducerContext } from './plugins.ts'
import { FILE_START } from './program.ts'

/**
 * What the bundler's producer for `name` makes of a page holding `marks` marks of that
 * name, which collected `texts` under it — one replacement per mark — and each file it
 * wrote, `[url, content]`.
 */
function produced(name: string, texts: string[], marks = 1) {
  const handler = bundler().marks?.find((candidate) => candidate.name === name)
  if (handler?.produce === undefined) throw new Error(`no producer for \`${name}\``)
  const mark: Mark = { name, start: 0, end: 0, path: 'base.lem', span: FILE_START }
  const fragments: Fragment[] = texts.map((text) => ({
    text,
    path: 'card.lem',
    span: FILE_START,
  }))
  const written: [string, string | Uint8Array][] = []
  const context: ProducerContext = {
    name,
    marks: Array.from({ length: marks }, () => mark),
    fragments,
    page: new Uint8Array(),
    write: (url, content) => {
      written.push([url, content])
    },
  }
  return { made: handler.produce(context), written }
}

describe('bundler()', () => {
  it('should claim `styles`, `scripts` and `head`, each without keys or payload', () => {
    const plugin = bundler()

    expect(validatePlugins([plugin])).toEqual([plugin])
    expect(plugin.name).toBe('bundler')
    expect(claimsOf([plugin])).toStrictEqual(
      ['styles', 'scripts', 'head'].map((name) => ({
        name,
        keys: [],
        payload: undefined,
        keyedBy: undefined,
      }))
    )
  })

  describe('styles', () => {
    const BUNDLES = '/assets/immutable/bundle/'

    it('should name the bundle by ten base64url characters of its SHA-256', () => {
      // Each name computed apart from the code, with node:crypto by hand.
      const cases: [string[], string][] = [
        [['.a {}'], 'FhBjQrPqBk'],
        [['.a {}', '.b {}'], 'Fe6UY9uAQj'],
        // In base64 this one is `7/tsgIH2xI`: a directory, and a file inside it.
        [['.b {}', '.a {}'], '7_tsgIH2xI'],
        // And this one `MVE+pSoPd3`.
        [['.a4 {}'], 'MVE-pSoPd3'],
        // Hashed as UTF-8: its Latin-1 bytes would name it `wfTMELClx0`.
        [['.a::after { content: "→"; }'], 'tTRZitUN7y'],
      ]

      for (const [texts, name] of cases) {
        const url = `${BUNDLES}${name}.css`
        expect(produced('styles', texts)).toEqual({
          made: [`<link rel="stylesheet" href="${url}">`],
          written: [[url, texts.join('\n')]],
        })
      }
    })

    it('should join the fragments with newlines, in collection order', () => {
      const { written } = produced('styles', ['.b {\n  color: red;\n}', '.a {}'])

      expect(written).toEqual([[expect.any(String), '.b {\n  color: red;\n}\n.a {}']])
    })

    it('should place the same link at every `styles` mark, writing the bundle once', () => {
      const link = `<link rel="stylesheet" href="${BUNDLES}FhBjQrPqBk.css">`

      expect(produced('styles', ['.a {}'], 2)).toEqual({
        made: [link, link],
        written: [[`${BUNDLES}FhBjQrPqBk.css`, '.a {}']],
      })
    })

    it('should place and write nothing for a page with no styles', () => {
      expect(produced('styles', [], 2)).toEqual({ made: ['', ''], written: [] })
    })
  })

  describe('scripts', () => {
    it('should load each collected script as a module, at every `scripts` mark', () => {
      // A `lem-script` collects its URL as `<lem.…>` prints one, so it is escaped for an
      // attribute already, and its `&amp;` is not escaped again.
      const { made, written } = produced(
        'scripts',
        ['/lib/a.js', '/lib/b.js?v=1&amp;w=2'],
        2
      )
      const tags =
        '<script type="module" src="/lib/a.js"></script>\n' +
        '<script type="module" src="/lib/b.js?v=1&amp;w=2"></script>'

      expect(made).toEqual([tags, tags])
      expect(written).toEqual([])
    })

    it('should place nothing for a page that collected no scripts', () => {
      expect(produced('scripts', []).made).toEqual([''])
    })
  })

  describe('head', () => {
    it('should place the page’s `lem-head` fragments at each `head` mark', () => {
      const texts = ['<meta name="a" content="1">', '<link rel="me" href="/me/">']
      const { made, written } = produced('head', texts, 2)
      const html = texts.join('\n')

      expect(made).toEqual([html, html])
      expect(written).toEqual([])
    })

    it('should place nothing for a page that collected no head', () => {
      expect(produced('head', []).made).toEqual([''])
    })
  })
})

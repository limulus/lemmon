import { describe, expect, it } from 'vitest'

import { resolveMarks } from './marks.ts'
import type { MarkHandler } from './marks.ts'
import type { Fragment, Mark, RenderResult, Span } from './types.ts'

const encoder = new TextEncoder()
const decoder = new TextDecoder()

const span: Span = {
  start: 0,
  end: 1,
  range: { start: { line: 0, character: 0 }, end: { line: 0, character: 1 } },
}

/** A mark of `name` over the range `[start, end)` of the page. */
const mark = (name: string, start: number, end = start): Mark => ({
  name,
  start,
  end,
  path: 'layout.lem',
  span,
})

/** A mark of `name` keyed by `key`, as a code block is by its language. */
const keyed = (name: string, key: string, start: number, end = start): Mark => ({
  ...mark(name, start, end),
  key,
})

const fragment = (text: string): Fragment => ({ text, path: 'card.lem', span })

/** A render as the core returns it: the page as bytes, its marks, its fragments. */
const rendered = (
  html: string,
  marks: Mark[],
  fragments: Record<string, Fragment[]> = {}
): RenderResult => ({
  html: encoder.encode(html),
  marks,
  fragments,
  diagnostics: [],
})

const resolved = (result: RenderResult, handlers: MarkHandler[] = []) => {
  const { html, ...rest } = resolveMarks(result, handlers)
  return { html: decoder.decode(html), ...rest }
}

describe('resolveMarks()', () => {
  it('should return the page unchanged when it holds no marks', () => {
    const result = rendered('<p>a</p>', [])
    expect(resolveMarks(result, []).html).toBe(result.html)
    expect(resolved(result)).toEqual({
      html: '<p>a</p>',
      unclaimed: [],
      unplaced: [],
    })
  })

  it('should replace each mark with what its producer made', () => {
    const result = rendered('<head></head>', [mark('styles', 6)], {
      styles: [fragment('.a {}'), fragment('.b {}')],
    })
    const styles: MarkHandler = {
      name: 'styles',
      produce: ({ name, marks, fragments, page }) => {
        expect([name, marks.length, decoder.decode(page)]).toEqual([
          'styles',
          1,
          '<head></head>',
        ])
        return [`<style>${fragments.map((f) => f.text).join('')}</style>`]
      },
    }
    expect(resolved(result, [styles]).html).toBe('<head><style>.a {}.b {}</style></head>')
  })

  it('should call a producer once for a name the page marks twice', () => {
    let calls = 0
    const result = rendered('a|b|', [mark('head', 2), mark('head', 4)])
    const handler: MarkHandler = {
      name: 'head',
      produce: ({ marks }) => {
        calls += 1
        return marks.map((_, at) => `<${at}>`)
      },
    }
    expect(resolved(result, [handler]).html).toBe('a|<0>b|<1>')
    expect(calls).toBe(1)
  })

  it('should take a producer’s bytes without re-encoding them', () => {
    const result = rendered('><', [mark('styles', 1)])
    const handler: MarkHandler = {
      name: 'styles',
      produce: () => [encoder.encode('café')],
    }
    expect(resolved(result, [handler]).html).toBe('>café<')
  })

  it('should run decorators in the order given, over the content so far', () => {
    const result = rendered('[]', [mark('code', 1)])
    const handlers: MarkHandler[] = [
      { name: 'code', produce: () => ['a'] },
      { name: 'code', decorate: (_, content) => `<i>${decoder.decode(content)}</i>` },
      { name: 'code', decorate: (_, content) => `<b>${decoder.decode(content)}</b>` },
    ]
    expect(resolved(result, handlers).html).toBe('[<b><i>a</i></b>]')
  })

  it('should decorate the default content where no producer claims the name', () => {
    // A copy button over a block nothing highlighted: the page already holds what the
    // core wrote, and a decorator alone is enough to claim the name.
    const result = rendered('<pre>code</pre>', [mark('code', 0, 15)])
    const handler: MarkHandler = {
      name: 'code',
      decorate: (mark, content) =>
        `<figure data-name="${mark.name}">${decoder.decode(content)}</figure>`,
    }
    expect(resolved(result, [handler])).toEqual({
      html: '<figure data-name="code"><pre>code</pre></figure>',
      unclaimed: [],
      unplaced: [],
    })
  })

  it('should leave an unclaimed name its default content and report it once', () => {
    const result = rendered('<p>x</p>', [mark('toc', 3, 4), mark('toc', 8)], {})
    expect(resolved(result, [{ name: 'other', produce: () => [] }])).toEqual({
      html: '<p>x</p>',
      unclaimed: [{ name: 'toc' }],
      unplaced: [],
    })
  })

  it('should choose a producer by the key a mark carries', () => {
    // One producer for the languages it lists, and nothing for a block with no
    // language: there is no wildcard producer (D42). Each producer runs once, over its
    // own marks in page order.
    const result = rendered('[a][b][c][d]', [
      keyed('code', 'ts', 0, 3),
      keyed('code', 'rust', 3, 6),
      mark('code', 6, 9),
      keyed('code', 'ts', 9, 12),
    ])
    const seen: string[][] = []
    const highlighter: MarkHandler = {
      name: 'code',
      keys: ['ts', 'rust'],
      produce: ({ marks }) => {
        seen.push(marks.map((m) => m.key!))
        return marks.map((m) => `<${m.key}>`)
      },
    }
    expect(resolved(result, [highlighter])).toEqual({
      html: '<ts><rust>[c]<ts>',
      unclaimed: [{ name: 'code' }],
      unplaced: [],
    })
    expect(seen).toEqual([['ts', 'rust', 'ts']])
  })

  it('should give a keyless producer only the marks that carry no key', () => {
    // The shell's `styles` bundler lists no keys and answers the marks that have none;
    // a keyed mark of the same name is no mark it knows how to make.
    const result = rendered('ab', [mark('styles', 1), keyed('styles', 'print', 2)])
    const bundler: MarkHandler = {
      name: 'styles',
      produce: ({ marks }) => marks.map(() => 'S'),
    }
    expect(resolved(result, [bundler])).toEqual({
      html: 'aSb',
      unclaimed: [{ name: 'styles', key: 'print' }],
      unplaced: [],
    })
  })

  it('should decorate only the keys a decorator lists, and every mark where it lists none', () => {
    const result = rendered('[a][b]', [
      keyed('code', 'ts', 0, 3),
      keyed('code', 'sh', 3, 6),
    ])
    const handlers: MarkHandler[] = [
      { name: 'code', keys: ['sh'], decorate: (_, c) => `<sh>${decoder.decode(c)}</sh>` },
      { name: 'code', decorate: (_, c) => `<copy>${decoder.decode(c)}</copy>` },
    ]
    // A decorator is enough to answer a mark, so nothing here goes unclaimed.
    expect(resolved(result, handlers)).toEqual({
      html: '<copy>[a]</copy><copy><sh>[b]</sh></copy>',
      unclaimed: [],
      unplaced: [],
    })
  })

  it('should report each name and key nothing answered once, in page order', () => {
    const result = rendered('abc', [
      keyed('code', 'lem', 1),
      keyed('code', 'bsah', 2),
      keyed('code', 'lem', 3),
      mark('code', 3),
    ])
    const highlighter: MarkHandler = { name: 'code', keys: ['sh'], produce: () => [] }
    expect(resolved(result, [highlighter]).unclaimed).toEqual([
      { name: 'code', key: 'lem' },
      { name: 'code', key: 'bsah' },
      { name: 'code' },
    ])
  })

  it('should report a name whose fragments nothing placed', () => {
    // A layout that forgot its `<lem.styles()>`: the CSS was collected and has nowhere
    // to go, which is worth saying rather than dropping.
    const result = rendered('<p>a</p>', [], { styles: [fragment('.a {}')] })
    expect(resolved(result).unplaced).toEqual(['styles'])
  })

  it('should place marks at the page’s edges and two at one offset', () => {
    const result = rendered('mid', [mark('a', 0), mark('b', 0), mark('c', 3)])
    const handlers: MarkHandler[] = ['a', 'b', 'c'].map((name) => ({
      name,
      produce: ({ marks }) => marks.map(() => name.toUpperCase()),
    }))
    expect(resolved(result, handlers).html).toBe('ABmidC')
  })

  it('should give a producer no fragments for a name that collected none', () => {
    // Read as a plain property, `constructor` would answer from the prototype rather
    // than falling back to nothing, and the handler would be handed a function.
    const result = rendered('a', [mark('constructor', 1)])
    let seen: readonly Fragment[] | undefined
    const handler: MarkHandler = {
      name: 'constructor',
      produce: ({ fragments }) => {
        seen = fragments
        return ['x']
      },
    }
    expect(resolved(result, [handler]).html).toBe('ax')
    expect(seen).toEqual([])
  })

  it('should throw when a render that failed is resolved', () => {
    const failed: RenderResult = {
      html: null,
      marks: null,
      fragments: null,
      diagnostics: [],
    }
    expect(() => resolveMarks(failed)).toThrow(/no page to resolve/)
  })

  it('should throw when two handlers produce one name', () => {
    const result = rendered('a', [mark('styles', 1)])
    const handlers: MarkHandler[] = [
      { name: 'styles', produce: () => ['x'] },
      { name: 'styles', produce: () => ['y'] },
    ]
    expect(() => resolveMarks(result, handlers)).toThrow(/two handlers produce `styles`;/)
  })

  it('should throw when two producers list one key, and not when their keys are apart', () => {
    // Checked over the handlers rather than the page, so a plugin list that cannot work
    // fails on a page that marks nothing.
    const page = rendered('a', [])
    const ts: MarkHandler = { name: 'code', keys: ['ts', 'js'], produce: () => [] }
    const js: MarkHandler = { name: 'code', keys: ['js'], produce: () => [] }
    const rust: MarkHandler = { name: 'code', keys: ['rust'], produce: () => [] }
    expect(() => resolveMarks(page, [ts, js])).toThrow(
      /two handlers produce `code` for `js`/
    )
    expect(() => resolveMarks(page, [ts, rust])).not.toThrow()
    // A producer listing a key twice — a language and its alias — is one producer.
    const aliased: MarkHandler = {
      name: 'code',
      keys: ['js', 'ts', 'js'],
      produce: () => [],
    }
    expect(() => resolveMarks(page, [aliased])).not.toThrow()
  })

  it('should throw when a producer answers with the wrong number of replacements', () => {
    const result = rendered('ab', [mark('styles', 1), mark('styles', 2)])
    const handler: MarkHandler = { name: 'styles', produce: () => ['only one'] }
    expect(() => resolveMarks(result, [handler])).toThrow(/1 replacements for 2 marks/)
  })

  it('should throw for marks that do not stand in order', () => {
    const overlapping = rendered('abcdef', [mark('a', 4, 6), mark('b', 1, 2)])
    expect(() => resolveMarks(overlapping)).toThrow(/does not stand in order/)
    const backwards = rendered('abcdef', [mark('a', 4, 2)])
    expect(() => resolveMarks(backwards)).toThrow(/does not stand in order/)
  })
})

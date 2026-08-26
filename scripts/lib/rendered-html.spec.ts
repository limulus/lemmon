import { describe, expect, it } from 'vitest'

import {
  compareRendered,
  renderedPage,
  sameAttrs,
  show,
  type Forgiveness,
  type RenderedElement,
  type RenderedNode,
  type RenderOptions,
} from './rendered-html.ts'

/** The rendered `body` of a page built from `inner`. */
function body(inner: string, options?: RenderOptions): RenderedElement {
  const root = renderedPage(`<!doctype html><html><body>${inner}</body></html>`, options)
  const found = root.root.children.find((n) => n.kind === 'element' && n.tag === 'body')
  return found as RenderedElement
}

/** A node as a compact literal: text as a string, an element as `[tag, ...children]`. */
function literal(node: RenderedNode): unknown {
  if (node.kind === 'text') return node.text
  return [node.tag, ...node.children.map(literal)]
}

const children = (element: RenderedElement): unknown[] => element.children.map(literal)

describe('renderedPage()', () => {
  describe('white-space processing', () => {
    it('should collapse runs of whitespace and trim at block edges', () => {
      expect(children(body('<p>\n  a \t b\n</p>'))).toEqual([['p', 'a b']])
    })

    it('should keep one space between two inline runs', () => {
      expect(children(body('<p><b>a</b> <i>b</i></p>'))).toEqual([
        ['p', ['b', 'a'], ' ', ['i', 'b']],
      ])
    })

    it('should drop a space that follows a space in another element', () => {
      expect(children(body('<p><b>a </b> <i>b</i></p>'))).toEqual([
        ['p', ['b', 'a '], ['i', 'b']],
      ])
    })

    it('should drop whitespace at a line edge even inside an inline element', () => {
      expect(children(body('<p><a>\n<b>x</b>\n</a></p>'))).toEqual([
        ['p', ['a', ['b', 'x']]],
      ])
    })

    it('should treat a replaced element as a character on the line', () => {
      expect(children(body('<p><img> x <img></p>'))).toEqual([
        ['p', ['img'], ' x ', ['img']],
      ])
    })

    it('should break the line at a br', () => {
      expect(children(body('<p>a <br> b</p>'))).toEqual([['p', 'a', ['br'], 'b']])
    })

    it('should collapse across an element that is not rendered', () => {
      expect(children(body('<p>a <script>x</script> b</p>'))).toEqual([
        ['p', 'a ', ['script', 'x'], 'b'],
      ])
    })

    it('should drop comments and collapse the runs they separated as one', () => {
      expect(children(body('<p>a <!-- note --> b</p>'))).toEqual([['p', 'a b']])
    })

    it('should not render whitespace between svg elements, and keep the space after the svg', () => {
      const svg = '<svg>\n  <desc>\n   an icon \n  </desc>\n  <use href="#i"></use>\n</svg>'
      expect(children(body(`<p>${svg} after</p>`))).toEqual([
        ['p', ['svg', ['desc', 'an icon'], ['use']], ' after'],
      ])
    })

    it('should keep pre content byte for byte', () => {
      expect(children(body('<pre>\n  a\n   b </pre>'))).toEqual([['pre', '  a\n   b ']])
    })

    it('should keep pre content exact through the elements nested in it', () => {
      expect(children(body('<pre><code>  a\n b </code>\n</pre>'))).toEqual([
        ['pre', ['code', '  a\n b '], '\n'],
      ])
    })

    it('should keep textarea content byte for byte', () => {
      expect(children(body('<textarea>  a\n b </textarea>'))).toEqual([
        ['textarea', '  a\n b '],
      ])
    })

    it('should compare script text with its indentation removed', () => {
      const script = '<script>\n    if (x) {\n      y()\n    }\n  </script>'
      expect(children(body(script))).toEqual([['script', 'if (x) {\ny()\n}']])
    })

    it('should collapse and trim the title', () => {
      const page = renderedPage(
        '<!doctype html><html><head><title>\n  T \n</title></head></html>'
      )
      expect(literal(page.root)).toEqual(['html', ['head', ['title', 'T']], ['body']])
    })
  })

  describe('display declarations', () => {
    it('should leave an unknown element inline by default', () => {
      expect(children(body('<span>a <x-b>b</x-b></span>'))).toEqual([
        ['span', 'a ', ['x-b', 'b']],
      ])
    })

    it('should break the line at the edges of an element declared block', () => {
      const options = { display: { 'x-b': 'block' } } as const
      expect(children(body('<span>a <x-b>b</x-b></span>', options))).toEqual([
        ['span', 'a', ['x-b', 'b']],
      ])
    })

    it('should drop whitespace between the items of an element declared flex', () => {
      const html = '<x-f> <i>a</i> <i>b</i> t </x-f>'
      expect(children(body(html))).toEqual([['x-f', ['i', 'a'], ' ', ['i', 'b'], ' t']])
      const options = { display: { 'x-f': 'flex' } } as const
      expect(children(body(html, options))).toEqual([['x-f', ['i', 'a'], ['i', 'b'], 't']])
    })
  })

  describe('the document', () => {
    it('should record the doctype name, or null when there is none', () => {
      expect(renderedPage('<!doctype html><p>x</p>').doctype).toBe('html')
      expect(renderedPage('<p>x</p>').doctype).toBeNull()
    })

    it('should keep attributes by name and value', () => {
      const [p] = body('<p class="a" hidden>x</p>').children as RenderedElement[]
      expect([...p.attrs]).toEqual([
        ['class', 'a'],
        ['hidden', ''],
      ])
    })
  })
})

describe('compareRendered()', () => {
  const page = (inner: string): ReturnType<typeof renderedPage> =>
    renderedPage(`<!doctype html><html><body>${inner}</body></html>`)

  it('should report no differences for pages that render the same', () => {
    const result = compareRendered(
      page('<p>\n  a <b>b</b>\n</p>'),
      page('<p>a <b>b</b></p>')
    )
    expect(result.differences).toEqual([])
    expect(result.matched).toBe(7)
    expect(result.forgiven.size).toBe(0)
  })

  it('should report a changed attribute value with its path', () => {
    const result = compareRendered(page('<p class="a">x</p>'), page('<p class="b">x</p>'))
    expect(result.differences).toEqual([
      { path: 'html > body[0] > p[0]', ours: '<p class="a">', theirs: '<p class="b">' },
    ])
  })

  it('should report changed text', () => {
    const result = compareRendered(page('<p>x</p>'), page('<p>y</p>'))
    expect(result.differences).toEqual([
      { path: 'html > body[0] > p[0] > child 0', ours: '"x"', theirs: '"y"' },
    ])
  })

  it('should report a different element, and a text where an element was', () => {
    expect(
      compareRendered(page('<p><b>x</b></p>'), page('<p><i>x</i></p>')).differences
    ).toEqual([{ path: 'html > body[0] > p[0] > child 0', ours: '<b>', theirs: '<i>' }])
    expect(compareRendered(page('<p><b>x</b></p>'), page('<p>x</p>')).differences).toEqual([
      { path: 'html > body[0] > p[0] > child 0', ours: '<b>', theirs: '"x"' },
    ])
  })

  it('should report a child only one side has', () => {
    expect(compareRendered(page('<p></p>'), page('<p><b>x</b></p>')).differences).toEqual([
      { path: 'html > body[0] > p[0] > child 0', ours: null, theirs: '<b>' },
    ])
    expect(compareRendered(page('<p><b>x</b></p>'), page('<p></p>')).differences).toEqual([
      { path: 'html > body[0] > p[0] > child 0', ours: '<b>', theirs: null },
    ])
  })

  it('should report a doctype difference', () => {
    const result = compareRendered(renderedPage('<p>x</p>'), page('<p>x</p>'))
    expect(result.differences).toEqual([{ path: 'doctype', ours: null, theirs: 'html' }])
  })

  it('should stop at the first difference within a parent but go on in other parents', () => {
    const result = compareRendered(
      page('<p>a<br>b</p><p>c</p>'),
      page('<p>x<br>y</p><p>d</p>')
    )
    expect(result.differences.map((d) => d.path)).toEqual([
      'html > body[0] > p[0] > child 0',
      'html > body[0] > p[1] > child 0',
    ])
  })

  it('should count the matches it made', () => {
    expect(compareRendered(page('<p>x</p>'), page('<p>y</p>')).matched).toBe(4)
  })

  describe('with forgiveness classes', () => {
    const emptyB: Forgiveness = {
      name: 'empty-b',
      extra: (theirs, parent) =>
        parent.tag === 'p' && theirs.tag === 'b' && theirs.children.length === 0,
    }
    const anyId: Forgiveness = {
      name: 'any-id',
      attrs: (ours, theirs) => ours.attrs.has('id') && theirs.attrs.has('id'),
    }

    it('should skip an extra reference element a class accounts for, and count it', () => {
      const result = compareRendered(page('<p>x</p>'), page('<p>x<b></b></p>'), [emptyB])
      expect(result.differences).toEqual([])
      expect([...result.forgiven]).toEqual([['empty-b', 1]])
    })

    it('should not skip an extra element no class accounts for', () => {
      const result = compareRendered(page('<p>x</p>'), page('<p>x<b>y</b></p>'), [emptyB])
      expect(result.differences).toHaveLength(1)
      expect(result.forgiven.size).toBe(0)
    })

    it('should not consult the classes about an extra text node', () => {
      const result = compareRendered(page('<p><b>x</b></p>'), page('<p>t<b>x</b></p>'), [
        emptyB,
      ])
      expect(result.differences).toHaveLength(1)
    })

    it('should accept an attribute difference a class accounts for, and count it', () => {
      const result = compareRendered(page('<p id="a">x</p>'), page('<p id="b">x</p>'), [
        anyId,
      ])
      expect(result.differences).toEqual([])
      expect([...result.forgiven]).toEqual([['any-id', 1]])
    })

    it('should not accept an attribute difference no class accounts for', () => {
      const result = compareRendered(page('<p id="a">x</p>'), page('<p id="b" x>x</p>'), [
        emptyB,
      ])
      expect(result.differences).toHaveLength(1)
    })
  })
})

describe('sameAttrs()', () => {
  it('should ignore order and see a missing or changed attribute', () => {
    const a = new Map([
      ['x', '1'],
      ['y', '2'],
    ])
    expect(sameAttrs(a, new Map([...a].reverse()))).toBe(true)
    expect(sameAttrs(a, new Map([['x', '1']]))).toBe(false)
    expect(
      sameAttrs(
        a,
        new Map([
          ['x', '1'],
          ['y', '3'],
        ])
      )
    ).toBe(false)
  })
})

describe('show()', () => {
  it('should truncate a long open tag', () => {
    const [p] = body(`<p title="${'x'.repeat(200)}">t</p>`).children as RenderedElement[]
    expect(show(p)).toHaveLength(160)
    expect(show(p).endsWith('…')).toBe(true)
  })
})

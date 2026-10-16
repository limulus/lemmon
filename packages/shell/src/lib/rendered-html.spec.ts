import { describe, expect, it } from 'vitest'

import {
  compareRendered,
  renderedPage,
  show,
  type Forgiveness,
  type RenderedElement,
  type RenderedNode,
  type RenderedPage,
  type RenderOptions,
  type Scope,
} from './rendered-html.ts'

/** A doctype named `html` that still puts the page in quirks mode. */
const QUIRKS_HTML = '<!DOCTYPE html PUBLIC "-//W3C//DTD HTML 3.2 Final//EN"><p>x</p>'

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

    it('should record the document mode the doctype puts the page in', () => {
      expect(renderedPage('<!doctype html><p>x</p>').mode).toBe('no-quirks')
      expect(renderedPage('<p>x</p>').mode).toBe('quirks')
      expect(renderedPage(QUIRKS_HTML).mode).toBe('quirks')
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
  const page = (inner: string): RenderedPage =>
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

  it('should report a changed attribute value with its path and name', () => {
    const result = compareRendered(page('<p class="a">x</p>'), page('<p class="b">x</p>'))
    expect(result.differences).toEqual([
      {
        path: 'html > body[0] > p[0]',
        ours: '<p class="a">',
        theirs: '<p class="b">',
        note: 'attributes differ: class',
      },
    ])
  })

  it('should name every attribute that differs, whichever side has it', () => {
    const result = compareRendered(page('<p a="1" b>x</p>'), page('<p a="2" c>x</p>'))
    expect(result.differences.map((d) => d.note)).toEqual(['attributes differ: a, b, c'])
  })

  it('should not compare the children of an element whose attributes differ', () => {
    const result = compareRendered(page('<p id="a">x</p>'), page('<p id="b">y</p>'))
    expect(result.differences).toHaveLength(1)
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

  it('should report a doctype difference, and the mode it changes', () => {
    const result = compareRendered(renderedPage('<p>x</p>'), page('<p>x</p>'))
    expect(result.differences).toEqual([
      { path: 'doctype', ours: null, theirs: 'html' },
      { path: 'document mode', ours: 'quirks', theirs: 'no-quirks' },
    ])
  })

  it('should report a doctype of the same name that puts the page in quirks mode', () => {
    const result = compareRendered(
      renderedPage(QUIRKS_HTML),
      renderedPage('<!doctype html><p>x</p>')
    )
    expect(result.differences).toEqual([
      { path: 'document mode', ours: 'quirks', theirs: 'no-quirks' },
    ])
  })

  it('should go on past a pair whose attributes differ to compare its siblings', () => {
    const result = compareRendered(
      page('<p class="a">x</p><p>y</p>'),
      page('<p class="b">x</p><p>z</p>')
    )
    expect(result.differences.map((d) => d.path)).toEqual([
      'html > body[0] > p[0]',
      'html > body[0] > p[1] > child 0',
    ])
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

  describe('with attribute classes', () => {
    const anyId: Forgiveness = {
      name: 'any-id',
      attr: (name, ours, theirs) => name === 'id' && ours !== undefined && !!theirs,
    }
    const extraTabindex: Forgiveness = {
      name: 'extra-tabindex',
      attr: (name, ours, theirs) =>
        name === 'tabindex' && ours === undefined && theirs === '-1',
    }

    it('should accept an attribute a class explains, and count it', () => {
      const result = compareRendered(page('<p id="a">x</p>'), page('<p id="b">x</p>'), [
        anyId,
      ])
      expect(result.differences).toEqual([])
      expect([...result.forgiven]).toEqual([['any-id', 1]])
    })

    it('should let two classes each explain one attribute of an element', () => {
      const result = compareRendered(
        page('<h2 id="a">x</h2>'),
        page('<h2 id="b" tabindex="-1">x</h2>'),
        [anyId, extraTabindex]
      )
      expect(result.differences).toEqual([])
      expect([...result.forgiven]).toEqual([
        ['any-id', 1],
        ['extra-tabindex', 1],
      ])
    })

    it('should count a class once per element however many attributes it explains', () => {
      const anyAttr: Forgiveness = { name: 'any', attr: () => true }
      const result = compareRendered(page('<p a="1" b="1">x</p>'), page('<p a b>x</p>'), [
        anyAttr,
      ])
      expect([...result.forgiven]).toEqual([['any', 1]])
    })

    it('should count only the first class that explains an attribute', () => {
      const result = compareRendered(page('<p id="a">x</p>'), page('<p id="b">x</p>'), [
        anyId,
        { ...anyId, name: 'also-any-id' },
      ])
      expect([...result.forgiven]).toEqual([['any-id', 1]])
    })

    it('should report the element, and count nothing, when one attribute is unexplained', () => {
      const result = compareRendered(
        page('<p id="a">x</p>'),
        page('<p id="b" tabindex="0">x</p>'),
        [anyId, extraTabindex]
      )
      expect(result.differences.map((d) => d.note)).toEqual(['attributes differ: tabindex'])
      expect(result.forgiven.size).toBe(0)
    })

    it('should offer only the attributes that differ, with the pair and its ancestors', () => {
      const seen: { name: string; tag: string; ancestors: string[] }[] = []
      const spy: Forgiveness = {
        name: 'spy',
        attr: (name, _ours, _theirs, at) => {
          seen.push({
            name,
            tag: at.theirs.tag,
            ancestors: at.ancestors.map((pair) => pair.ours.tag),
          })
          return true
        },
      }
      compareRendered(page('<p a="1" b="2">x</p>'), page('<p a="1" b="3">x</p>'), [spy])
      expect(seen).toEqual([{ name: 'b', tag: 'p', ancestors: ['html', 'body'] }])
    })
  })

  describe('with text classes', () => {
    const bracketed: Forgiveness = {
      name: 'bracketed',
      text: (ours, theirs, at) => at.ours.tag === 'sup' && theirs === `[${ours}]`,
    }

    it('should accept a text difference a class explains, and count it', () => {
      const result = compareRendered(page('<sup>1</sup>'), page('<sup>[1]</sup>'), [
        bracketed,
      ])
      expect(result.differences).toEqual([])
      expect([...result.forgiven]).toEqual([['bracketed', 1]])
      // html, head, body and sup: a forgiven run is not also a match.
      expect(result.matched).toBe(4)
    })

    it('should not accept a text difference no class explains', () => {
      const result = compareRendered(page('<sup>1</sup>'), page('<sup>[2]</sup>'), [
        bracketed,
      ])
      expect(result.differences).toHaveLength(1)
      expect(result.forgiven.size).toBe(0)
    })

    it('should count only the first class that explains a text run', () => {
      const result = compareRendered(page('<sup>1</sup>'), page('<sup>[1]</sup>'), [
        bracketed,
        { ...bracketed, name: 'also-bracketed' },
      ])
      expect([...result.forgiven]).toEqual([['bracketed', 1]])
    })

    it('should not offer text that matches', () => {
      const result = compareRendered(page('<p>x</p>'), page('<p>x</p>'), [
        { name: 'any-text', text: () => true },
      ])
      expect(result.forgiven.size).toBe(0)
    })
  })

  describe('with classes for an element only one side has', () => {
    const emptyB: Forgiveness = {
      name: 'empty-b',
      theirsOnly: (theirs, at) =>
        at.theirs.tag === 'p' && theirs.tag === 'b' && theirs.children.length === 0,
    }
    const oursLink: Forgiveness = {
      name: 'ours-link',
      oursOnly: (ours, at) => at.ours.tag === 'head' && ours.tag === 'link',
    }

    it('should skip a reference element a class accounts for, and count it', () => {
      const result = compareRendered(page('<p>x</p>'), page('<p>x<b></b></p>'), [emptyB])
      expect(result.differences).toEqual([])
      expect([...result.forgiven]).toEqual([['empty-b', 1]])
    })

    it('should not skip a reference element no class accounts for', () => {
      const result = compareRendered(page('<p>x</p>'), page('<p>x<b>y</b></p>'), [emptyB])
      expect(result.differences).toHaveLength(1)
      expect(result.forgiven.size).toBe(0)
    })

    it('should skip an element only ours has that a class accounts for, and count it', () => {
      const result = compareRendered(
        renderedPage('<!doctype html><head><link rel="a"><title>t</title></head>'),
        renderedPage('<!doctype html><head><title>t</title></head>'),
        [oursLink]
      )
      expect(result.differences).toEqual([])
      expect([...result.forgiven]).toEqual([['ours-link', 1]])
    })

    it('should ask about the reference element before ours where both are unpaired', () => {
      const result = compareRendered(page('<p><i></i>x</p>'), page('<p><b></b>x</p>'), [
        { name: 'ours-i', oursOnly: (ours) => ours.tag === 'i' },
        { name: 'theirs-b', theirsOnly: (theirs) => theirs.tag === 'b' },
      ])
      expect(result.differences).toEqual([])
      expect([...result.forgiven]).toEqual([
        ['theirs-b', 1],
        ['ours-i', 1],
      ])
    })

    it('should not consult the classes about a text node only one side has', () => {
      const anything: Forgiveness = {
        name: 'anything',
        theirsOnly: () => true,
        oursOnly: () => true,
      }
      expect(
        compareRendered(page('<p><b>x</b></p>'), page('<p>t<b>x</b></p>'), [anything])
          .differences
      ).toHaveLength(1)
      expect(
        compareRendered(page('<p>t<b>x</b></p>'), page('<p><b>x</b></p>'), [anything])
          .differences
      ).toHaveLength(1)
    })
  })

  describe('with subtree classes', () => {
    /** Takes over every `ul`, comparing only the first `li` of each side. */
    const firstItem = (more: Forgiveness[] = []): Forgiveness => ({
      name: 'first-item',
      subtree: (at: Scope, sub) => {
        if (at.ours.tag !== 'ul') return null
        const [o] = at.ours.children as RenderedElement[]
        const [t] = at.theirs.children as RenderedElement[]
        sub.compare(o, t, 'first', more)
        return at.theirs.children.length - 1
      },
    })

    it('should hand a matched pair to the class that takes it, and count what it says', () => {
      const result = compareRendered(
        page('<ul><li>a</li></ul>'),
        page('<ul><li>a</li><li>b</li><li>c</li></ul>'),
        [firstItem()]
      )
      expect(result.differences).toEqual([])
      expect([...result.forgiven]).toEqual([['first-item', 2]])
      expect(result.matched).toBe(6)
    })

    it('should record nothing for a subtree that forgave nothing', () => {
      const result = compareRendered(
        page('<ul><li>a</li></ul>'),
        page('<ul><li>a</li></ul>'),
        [firstItem()]
      )
      expect(result.forgiven.size).toBe(0)
    })

    it('should compare what the class hands back, under its label', () => {
      const result = compareRendered(
        page('<ul><li>a</li></ul>'),
        page('<ul><li>b</li></ul>'),
        [firstItem()]
      )
      expect(result.differences.map((d) => d.path)).toEqual([
        'html > body[0] > ul[0] > first > child 0',
      ])
    })

    it('should put the classes the subtree adds in effect only inside it', () => {
      const anyText: Forgiveness = { name: 'any-text', text: () => true }
      const result = compareRendered(
        page('<ul><li>a</li></ul><p>x</p>'),
        page('<ul><li>b</li></ul><p>y</p>'),
        [firstItem([anyText])]
      )
      expect(result.differences.map((d) => d.path)).toEqual([
        'html > body[0] > p[0] > child 0',
      ])
      expect([...result.forgiven]).toEqual([['any-text', 1]])
    })

    it('should give a compared pair the subtree and its ancestors as ancestors', () => {
      const tags: string[][] = []
      const spy: Forgiveness = {
        name: 'spy',
        text: (_ours, _theirs, at) => {
          tags.push([...at.ancestors.map((pair) => pair.theirs.tag), at.theirs.tag])
          return true
        },
      }
      compareRendered(page('<ul><li>a</li></ul>'), page('<ul><li>b</li></ul>'), [
        firstItem([spy]),
      ])
      expect(tags).toEqual([['html', 'body', 'ul', 'li']])
    })

    it('should report a subtree the class refuses, with its reason', () => {
      const refuse: Forgiveness = {
        name: 'refuse',
        subtree: (at, sub) => {
          if (at.ours.tag !== 'ul') return null
          sub.differ('too few items')
          return 0
        },
      }
      const result = compareRendered(page('<ul></ul>'), page('<ul><li>a</li></ul>'), [
        refuse,
      ])
      expect(result.differences).toEqual([
        {
          path: 'html > body[0] > ul[0]',
          ours: '<ul>',
          theirs: '<ul>',
          note: 'too few items',
        },
      ])
    })

    it('should refuse a class that uses the subtree and then leaves it', () => {
      const [ours, theirs] = [page('<p><b>x</b></p>'), page('<p><b>x</b></p>')]
      const compares: Forgiveness = {
        name: 'compares',
        subtree: (at, sub) => {
          if (at.ours.tag !== 'p') return null
          const [o] = at.ours.children as RenderedElement[]
          const [t] = at.theirs.children as RenderedElement[]
          sub.compare(o, t, 'b')
          return null
        },
      }
      expect(() => compareRendered(ours, theirs, [compares])).toThrow(
        'compares used the subtree at html > body[0] > p[0], then left it to the comparison'
      )
      const differs: Forgiveness = {
        name: 'differs',
        subtree: (at, sub) => {
          if (at.ours.tag !== 'p') return null
          sub.differ('why')
          return null
        },
      }
      expect(() => compareRendered(ours, theirs, [differs])).toThrow(
        'differs used the subtree at html > body[0] > p[0]'
      )
    })

    it('should compare the children itself when no class takes the pair', () => {
      const result = compareRendered(
        page('<ol><li>a</li></ol>'),
        page('<ol><li>b</li></ol>'),
        [firstItem(), { name: 'no-hook' }]
      )
      expect(result.differences.map((d) => d.path)).toEqual([
        'html > body[0] > ol[0] > li[0] > child 0',
      ])
    })

    it('should not offer a pair whose attributes differ', () => {
      const result = compareRendered(
        page('<ul class="a"><li>a</li></ul>'),
        page('<ul class="b"><li>a</li><li>b</li></ul>'),
        [firstItem()]
      )
      expect(result.differences.map((d) => d.note)).toEqual(['attributes differ: class'])
      expect(result.forgiven.size).toBe(0)
    })
  })
})

describe('show()', () => {
  it('should truncate a long open tag', () => {
    const [p] = body(`<p title="${'x'.repeat(200)}">t</p>`).children as RenderedElement[]
    expect(show(p)).toHaveLength(160)
    expect(show(p).endsWith('…')).toBe(true)
  })
})

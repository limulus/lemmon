import { describe, expect, it } from 'vitest'

import { loadCore } from './load.ts'
import type { HostDecl, SourceFileInput } from './types.ts'

/**
 * The declaration most of these compile against: the three names the standard shell
 * binds, none of them typed. A core given no declaration has no ambient values at all
 * (D30), so every test that reads one says so.
 */
const untyped: HostDecl = { ambient: { site: null, page: null, build: null } }

/** A host that declares what it renders: `Page` in a file it compiles with the program. */
const declared: HostDecl = {
  ambient: { page: 'Page' },
  document: 'page',
  entry: 'layout',
}

const schema: SourceFileInput[] = [
  { path: 'ambient.lem', text: '<lem-type Page>\n  url: url\n</lem-type>' },
  {
    path: 'schema.lem',
    text:
      '<lem-type Post extends Page>\n  title: string\n' +
      '  layout: (Post, html): html\n</lem-type>',
  },
]

const layoutChain = [
  {
    path: 'base.lem',
    text: '<lem-fn base(body: html)><html><lem.body></html></lem-fn>',
  },
  {
    path: 'layout.lem',
    text:
      '<lem-fn layout(body: html)><lem-let inner><main><lem.body></main></lem-let>' +
      '<lem.base(inner)></lem-fn>',
  },
  {
    path: 'article.lem',
    text: '<lem-let content>Hi <lem.page.title></lem-let><lem.layout(content)>',
  },
]

describe('Core', () => {
  describe('compile()', () => {
    it('should report success on a clean program as empty diagnostics', async () => {
      const core = await loadCore()
      expect(core.compile(layoutChain, untyped)).toEqual([])
    })

    it('should report problems as diagnostics with byte spans and positions', async () => {
      const core = await loadCore()
      expect(core.compile([{ path: 'bad.lem', text: '<lem.nope>' }])).toEqual([
        {
          path: 'bad.lem',
          span: {
            start: 5,
            end: 9,
            range: { start: { line: 0, character: 5 }, end: { line: 0, character: 9 } },
          },
          severity: 'error',
          message: 'undefined name `nope`',
          labels: [],
        },
      ])
    })

    it('should count a position in UTF-16 code units, not bytes', async () => {
      const core = await loadCore()
      // `’` is three bytes and one code unit, so the byte offset and the column part
      // company on this line — which is the whole reason the core computes the position.
      const [diagnostic] = core.compile([
        { path: 'bad.lem', text: '<p>ok</p>\nI’m <lem.nope>' },
      ])
      expect(diagnostic.span.start).toBe(21)
      expect(diagnostic.span.range.start).toEqual({ line: 1, character: 9 })
    })

    it('should carry a secondary span as a path-qualified label', async () => {
      const core = await loadCore()
      core.compile([{ path: 'a.lem', text: '<lem-fn f(): int = 1 />' }])
      const [collision] = core.compile([{ path: 'b.lem', text: '<lem-fn f(): int = 2 />' }])
      expect(collision.message).toBe('`f` is already defined')
      expect(collision.labels).toEqual([
        {
          path: 'a.lem',
          span: {
            start: 8,
            end: 9,
            range: { start: { line: 0, character: 8 }, end: { line: 0, character: 9 } },
          },
          message: 'first defined here',
        },
      ])
    })

    it('should report a warning and still replace the program', async () => {
      const core = await loadCore()
      core.compile([{ path: 'page.lem', text: 'old' }], untyped)
      core.loadData({ ambient: {} })
      const diagnostics = core.compile([
        { path: 'page.lem', text: '<lem-let raw = 1 />new<lem.raw>' },
      ])
      expect(diagnostics.map((d) => d.severity)).toEqual(['warning'])
      expect(core.render({ path: 'page.lem' }).html).toBe('new1')
    })

    it('should leave an ambient name undefined when no host declares one', async () => {
      const core = await loadCore()
      const [undeclared] = core.compile([{ path: 'page.lem', text: '<lem.page.title>' }])
      expect(undeclared.message).toBe('undefined name `page`')
    })

    it('should type an ambient value from the declaration it compiles with', async () => {
      const core = await loadCore()
      expect(
        core.compile([...schema, { path: 'page.lem', text: '<lem.page.url>' }], declared)
      ).toEqual([])
      const [wrong] = core.compile(
        [{ path: 'page.lem', text: '<lem.page.nope>' }],
        declared
      )
      expect(wrong.message).toBe('`Page` has no field `nope`')
    })

    it('should compile a document as the record the host named', async () => {
      const core = await loadCore()
      const post = { path: 'post.md', text: '<lem.page.title>', record: 'Post' }
      expect(core.compile([...schema, post], declared)).toEqual([])
      const [wrong] = core.compile([{ ...post, text: '<lem.page.byline>' }], declared)
      expect(wrong.message).toBe('`Post` has no field `byline`')
    })

    it('should keep the declaration for calls that carry none', async () => {
      const core = await loadCore()
      core.compile(schema, declared)
      const [wrong] = core.compile([{ path: 'page.lem', text: '<lem.page.nope>' }])
      expect(wrong.message).toBe('`Page` has no field `nope`')
    })

    it('should recompile one changed file against the resident rest', async () => {
      const core = await loadCore()
      core.compile(layoutChain, untyped)
      core.loadData({ ambient: { page: { title: 'T' } } })
      expect(core.render({ path: 'article.lem' }).html).toBe(
        '<html><main>Hi T</main></html>'
      )
      const edited = layoutChain[1].text.replaceAll('main>', 'article>')
      expect(core.compile([{ path: 'layout.lem', text: edited }])).toEqual([])
      expect(core.render({ path: 'article.lem' }).html).toBe(
        '<html><article>Hi T</article></html>'
      )
    })
  })

  describe('loadData()', () => {
    it('should bind the ambient records every render reads', async () => {
      const core = await loadCore()
      core.compile(
        [{ path: 'page.lem', text: '<lem.page.title> on <lem.site.name>' }],
        untyped
      )
      expect(
        core.loadData({ ambient: { page: { title: 'T' }, site: { name: 's' } } })
      ).toEqual([])
      expect(core.render({ path: 'page.lem' }).html).toBe('T on s')
    })
  })

  describe('render()', () => {
    it('should return the runtime diagnostic instead of HTML when the page fails', async () => {
      const core = await loadCore()
      core.compile([{ path: 'page.lem', text: '<lem.page.absent>' }], untyped)
      core.loadData({ ambient: { page: {} } })
      const result = core.render({ path: 'page.lem' })
      expect(result.html).toBeNull()
      expect(result.diagnostics).toHaveLength(1)
      expect(result.diagnostics[0].path).toBe('page.lem')
      expect(result.diagnostics[0].span.start).toBeGreaterThan(0)
    })

    it('should throw for a path the program has no file for', async () => {
      const core = await loadCore()
      core.compile([{ path: 'page.lem', text: 'ok' }])
      expect(() => core.render({ path: 'other.lem' })).toThrow(/no compiled file/)
    })

    it('should throw before any compile has succeeded', async () => {
      const core = await loadCore()
      expect(() => core.render({ path: 'page.lem' })).toThrow(/`compile` first/)
    })
  })

  describe('free()', () => {
    it('should release the core so that using it afterwards throws', async () => {
      const core = await loadCore()
      core.compile([{ path: 'page.lem', text: 'hi' }])
      core.free()
      expect(() => core.render({ path: 'page.lem' })).toThrow()
    })
  })
})

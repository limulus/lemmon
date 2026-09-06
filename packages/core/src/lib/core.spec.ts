import { describe, expect, it } from 'vitest'

import { loadCore } from './load.ts'

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
      expect(core.compile(layoutChain)).toEqual([])
    })

    it('should report problems as diagnostics with byte spans', async () => {
      const core = await loadCore()
      expect(core.compile([{ path: 'bad.lem', text: '<lem.nope>' }])).toEqual([
        {
          path: 'bad.lem',
          span: { start: 5, end: 9 },
          severity: 'error',
          message: 'undefined name `nope`',
          labels: [],
        },
      ])
    })

    it('should carry a secondary span as a path-qualified label', async () => {
      const core = await loadCore()
      core.compile([{ path: 'a.lem', text: '<lem-fn f(): int = 1 />' }])
      const [collision] = core.compile([{ path: 'b.lem', text: '<lem-fn f(): int = 2 />' }])
      expect(collision.message).toBe('`f` is already defined')
      expect(collision.labels).toEqual([
        { path: 'a.lem', span: { start: 8, end: 9 }, message: 'first defined here' },
      ])
    })

    it('should report a warning and still replace the program', async () => {
      const core = await loadCore()
      core.compile([{ path: 'page.lem', text: 'old' }])
      core.loadData({ ambient: {} })
      const diagnostics = core.compile([
        { path: 'page.lem', text: '<lem-let raw = 1 />new' },
      ])
      expect(diagnostics.map((d) => d.severity)).toEqual(['warning'])
      expect(core.render({ path: 'page.lem' }).html).toBe('new')
    })

    it('should recompile one changed file against the resident rest', async () => {
      const core = await loadCore()
      core.compile(layoutChain)
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
      core.compile([{ path: 'page.lem', text: '<lem.page.title> on <lem.site.name>' }])
      expect(
        core.loadData({ ambient: { page: { title: 'T' }, site: { name: 's' } } })
      ).toEqual([])
      expect(core.render({ path: 'page.lem' }).html).toBe('T on s')
    })
  })

  describe('render()', () => {
    it('should return the runtime diagnostic instead of HTML when the page fails', async () => {
      const core = await loadCore()
      core.compile([{ path: 'page.lem', text: '<lem.page.absent>' }])
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

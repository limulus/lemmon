import { describe, expect, it } from 'vitest'

import { loadCore } from './load.ts'
import type { DataInput, HostDecl, SourceFileInput, Span } from './types.ts'

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

/**
 * A typed site: the host reads `site` from `site.lem`, whose bindings collect the
 * documents of `posts/*.md` as `Post`s, load a data file, and take an environment
 * value; `posts/a.md` is one such document.
 */
const typedSite: HostDecl = {
  ambient: { page: 'Page', site: { bindingsOf: 'site.lem' } },
  document: 'page',
  entry: 'layout',
}

const site: SourceFileInput[] = [
  { path: 'ambient.lem', text: '<lem-type Page>\n  url: url\n</lem-type>' },
  {
    path: 'site.lem',
    text: [
      '<lem-type Photo>',
      '  caption: string',
      '</lem-type>',
      '<lem-type Post extends Page>',
      '  title: string',
      '  slug: string = path',
      '  layout: (Post, html): html = post',
      '</lem-type>',
      '<lem-fn post(p: Post, body: html)><h1><lem.p.title></h1><lem.body></lem-fn>',
      '<lem-let posts: [Post] from "posts/*.md" />',
      '<lem-let photos: {string: Photo} from "_data/photos.json" />',
      '<lem-let motto: string from env("MOTTO") />',
    ].join('\n'),
  },
  {
    path: 'posts/a.md',
    text: '---\ntitle: A\nurl: /a/\n---\n<lem.page.title>\n<lem.page.slug>\n<lem.site.motto>\n',
    record: 'Post',
  },
]

/** The envelope that loads the typed site cleanly. */
const loaded: DataInput = {
  ambient: {},
  sources: {
    posts: [{ path: 'posts/a.md', value: { title: 'A', url: '/a/' } }],
    photos: { path: '_data/photos.json', value: {} },
    motto: { value: 'hi' },
  },
}

/** Where `x.caption` stands in the data file, as a shell computes it from the JSON. */
const captionSpan: Span = {
  start: 20,
  end: 32,
  range: { start: { line: 2, character: 4 }, end: { line: 2, character: 16 } },
}

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

    it('should resolve an asset against the manifest the declaration carries', async () => {
      const core = await loadCore()
      const page = { path: 'page.lem', text: '<lem.asset("styles/site.css")>' }
      const built = { ...untyped, assets: { 'styles/site.css': '/site.abc123.css' } }
      expect(core.compile([page], built)).toEqual([])
      const [missing] = core.compile([page], untyped)
      expect(missing.message).toBe(
        'the manifest has no asset `styles/site.css`; the host declares none'
      )
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
      // The new program binds nothing until data is loaded against it.
      core.loadData({ ambient: { page: { title: 'T' } } })
      expect(core.render({ path: 'article.lem' }).html).toBe(
        '<html><article>Hi T</article></html>'
      )
    })

    it('should drop the loaded data when it installs a new program', async () => {
      const core = await loadCore()
      core.compile(layoutChain, untyped)
      core.loadData({ ambient: { page: { title: 'T' } } })
      expect(core.compile([{ path: 'article.lem', text: 'Hi <lem.page.title>' }])).toEqual(
        []
      )
      const [stale] = core.render({ path: 'article.lem' }).diagnostics
      expect(stale.message).toBe('`page` is not bound')
    })
  })

  describe('loadData()', () => {
    it('should bind the ambient values an untyped host sends as they are', async () => {
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

    it('should load a collection and bind each document at render as its own record', async () => {
      const core = await loadCore()
      expect(core.compile(site, typedSite)).toEqual([])
      expect(core.loadData(loaded)).toEqual([])
      // `title` was validated, `slug`'s default ran over the document's path, and
      // `motto` came through `site`.
      expect(core.render({ path: 'posts/a.md' }).html).toBe('A\nposts/a.md\nhi\n')
    })

    it("should bind the envelope's document value only where no loaded document stands", async () => {
      const core = await loadCore()
      expect(core.compile(site, typedSite)).toEqual([])
      expect(core.compile([{ path: 'loose.md', text: '<lem.page.title>' }])).toEqual([])
      expect(core.loadData({ ...loaded, ambient: { page: { title: 'Loose' } } })).toEqual(
        []
      )
      expect(core.render({ path: 'loose.md' }).html).toBe('Loose\n')
      expect(core.render({ path: 'posts/a.md' }).html).toBe('A\nposts/a.md\nhi\n')
    })

    it('should place a load diagnostic by the positions the host sent', async () => {
      const core = await loadCore()
      expect(core.compile(site, typedSite)).toEqual([])
      // The data file was never compiled, so only the carried positions can place it.
      const [diagnostic] = core.loadData({
        ...loaded,
        sources: {
          ...loaded.sources,
          photos: {
            path: '_data/photos.json',
            value: { x: { caption: 3 } },
            spans: { 'x.caption': captionSpan },
          },
        },
      })
      expect(diagnostic.path).toBe('_data/photos.json')
      expect(diagnostic.severity).toBe('error')
      expect(diagnostic.message).toMatch(/string/)
      expect(diagnostic.span).toEqual(captionSpan)
    })

    it('should place a load diagnostic at the declaration when the host sent no positions', async () => {
      const core = await loadCore()
      expect(core.compile(site, typedSite)).toEqual([])
      // An environment value has no file behind it: the error is at the `from`
      // binding, the last line of site.lem.
      const [diagnostic] = core.loadData({
        ...loaded,
        sources: { ...loaded.sources, motto: { value: 3 } },
      })
      expect(diagnostic.path).toBe('site.lem')
      expect(diagnostic.span.range.start.line).toBe(11)
    })

    it('should keep the last good data when a load reports an error', async () => {
      const core = await loadCore()
      expect(core.compile(site, typedSite)).toEqual([])
      expect(core.loadData(loaded)).toEqual([])
      const diagnostics = core.loadData({
        ...loaded,
        sources: {
          ...loaded.sources,
          posts: [{ path: 'posts/a.md', value: { title: 3, url: '/a/' } }],
        },
      })
      expect(diagnostics).toHaveLength(1)
      expect(core.render({ path: 'posts/a.md' }).html).toBe('A\nposts/a.md\nhi\n')
    })

    it('should throw for an envelope the program cannot be loaded from', async () => {
      const core = await loadCore()
      expect(core.compile(site, typedSite)).toEqual([])
      expect(() => core.loadData({ ambient: { weather: 1 } })).toThrow(
        /the host binds no `weather`/
      )
      expect(() =>
        core.loadData({
          ...loaded,
          sources: { ...loaded.sources, extra: { value: 1 } },
        })
      ).toThrow(/no `from` binding is named `extra`/)
    })

    it('should throw before any compile has succeeded', async () => {
      const core = await loadCore()
      expect(() => core.loadData({})).toThrow(/`compile` first/)
    })

    it('should throw after the declaration changed without a compile succeeding', async () => {
      const core = await loadCore()
      expect(core.compile(site, typedSite)).toEqual([])
      const changed: HostDecl = {
        ...typedSite,
        ambient: { ...typedSite.ambient, build: 'Build' },
      }
      const [typo] = core.compile(
        [{ path: 'build.lem', text: '<lem-type Build>\n  generator: strng\n</lem-type>' }],
        changed
      )
      expect(typo.severity).toBe('error')
      expect(() => core.loadData(loaded)).toThrow(/declaration changed/)
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

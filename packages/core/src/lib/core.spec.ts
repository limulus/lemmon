import { describe, expect, it } from 'vitest'

import { loadCore } from './load.ts'
import { resolveMarks } from './marks.ts'
import type { DataInput, HostDecl, RenderResult, SourceFileInput, Span } from './types.ts'

const decoder = new TextDecoder()

/**
 * A render's page as text. What the core returns is bytes, since a mark's offsets name
 * them and the host splices there before anything needs a string (D42).
 */
const page = (result: RenderResult): string | null =>
  result.html === null ? null : decoder.decode(result.html)

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
  url: 'url',
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
  url: 'url',
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

/** A component file: what a `.lem` that is no page holds, its output inside a `lem-fn`. */
const card = (body: string): string => `<lem-fn card()>${body}</lem-fn>`

/** A page template no glob holds, typed by its own key as a `Post` (D41). */
const home: SourceFileInput = {
  path: 'home.lem',
  text: '---\ntype: Post\ntitle: Home\n---\n<p><lem.page.title></p>',
  record: 'Post',
}

/** `home` as the shell sends it in `pages`, with the url it derived. */
const homeLoaded = (url: string): DataInput => ({
  ...loaded,
  pages: [{ path: 'home.lem', value: { title: 'Home', url } }],
})

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
      expect(core.compile(layoutChain, untyped).diagnostics).toEqual([])
    })

    it('should report problems as diagnostics with byte spans and positions', async () => {
      const core = await loadCore()
      expect(core.compile([{ path: 'bad.lem', text: '<lem.nope>' }]).diagnostics).toEqual([
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
      ]).diagnostics
      expect(diagnostic.span.start).toBe(21)
      expect(diagnostic.span.range.start).toEqual({ line: 1, character: 9 })
    })

    it('should carry a secondary span as a path-qualified label', async () => {
      const core = await loadCore()
      core.compile([{ path: 'a.lem', text: '<lem-fn f(): int = 1 />' }])
      const [collision] = core.compile([
        { path: 'b.lem', text: '<lem-fn f(): int = 2 />' },
      ]).diagnostics
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
      ]).diagnostics
      expect(diagnostics.map((d) => d.severity)).toEqual(['warning'])
      expect(page(core.render({ path: 'page.lem' }))).toBe('new1')
    })

    it('should leave an ambient name undefined when no host declares one', async () => {
      const core = await loadCore()
      const [undeclared] = core.compile([
        { path: 'page.lem', text: '<lem.page.title>' },
      ]).diagnostics
      expect(undeclared.message).toBe('undefined name `page`')
    })

    it('should type an ambient value from the declaration it compiles with', async () => {
      const core = await loadCore()
      // A component reads the value as declared; a template with no front matter is
      // no page, so output at its top level would only draw the warning for it.
      expect(
        core.compile(
          [...schema, { path: 'card.lem', text: card('<lem.page.url>') }],
          declared
        ).diagnostics
      ).toEqual([])
      const [wrong] = core.compile(
        [{ path: 'card.lem', text: card('<lem.page.nope>') }],
        declared
      ).diagnostics
      expect(wrong.message).toBe('`Page` has no field `nope`')
    })

    it('should compile a document as the record the host named', async () => {
      const core = await loadCore()
      const post = { path: 'post.md', text: '<lem.page.title>', record: 'Post' }
      expect(core.compile([...schema, post], declared).diagnostics).toEqual([])
      const [wrong] = core.compile(
        [{ ...post, text: '<lem.page.byline>' }],
        declared
      ).diagnostics
      expect(wrong.message).toBe('`Post` has no field `byline`')
    })

    it('should report the `from` bindings a host fulfils, and what each glob collects', async () => {
      const core = await loadCore()
      const { diagnostics, sources } = core.compile(site, typedSite)
      expect(diagnostics).toEqual([])
      // `posts` is a glob: its documents compile as `Post` and load as a list. The
      // other two are one value each, so they collect nothing.
      expect(
        sources.map(({ name, kind, literal, collects }) => [name, kind, literal, collects])
      ).toEqual([
        ['posts', 'file', 'posts/*.md', 'Post'],
        ['photos', 'file', '_data/photos.json', undefined],
        ['motto', 'env', 'MOTTO', undefined],
      ])
      const motto = sources[2]
      expect(motto).not.toHaveProperty('collects')
      expect(motto.path).toBe('site.lem')
      const siteText = site[1].text
      expect(
        Buffer.from(siteText).subarray(motto.span.start, motto.span.end).toString()
      ).toBe('motto')
      expect(motto.span.range.start).toEqual({ line: 11, character: 9 })
    })

    it('should report no sources from a compile that installs no program', async () => {
      const core = await loadCore()
      expect(core.compile(site, typedSite).sources).toHaveLength(3)
      const failed = core.compile(
        [{ path: 'posts/a.md', text: '<lem-if x>', record: 'Post' }],
        typedSite
      )
      expect(failed.diagnostics[0].severity).toBe('error')
      expect(failed.sources).toEqual([])
    })

    it('should resolve an asset against the manifest the declaration carries', async () => {
      const core = await loadCore()
      const page = { path: 'page.lem', text: '<lem.asset("styles/site.css")>' }
      const built = { ...untyped, assets: { 'styles/site.css': '/site.abc123.css' } }
      expect(core.compile([page], built).diagnostics).toEqual([])
      const [missing] = core.compile([page], untyped).diagnostics
      expect(missing.message).toBe(
        'the manifest has no asset `styles/site.css`; the host declares none'
      )
    })

    it('should keep the declaration for calls that carry none', async () => {
      const core = await loadCore()
      core.compile(schema, declared)
      const [wrong] = core.compile([
        { path: 'card.lem', text: card('<lem.page.nope>') },
      ]).diagnostics
      expect(wrong.message).toBe('`Page` has no field `nope`')
    })

    it('should recompile one changed file against the resident rest', async () => {
      const core = await loadCore()
      core.compile(layoutChain, untyped)
      core.loadData({ ambient: { page: { title: 'T' } } })
      expect(page(core.render({ path: 'article.lem' }))).toBe(
        '<html><main>Hi T</main></html>'
      )
      const edited = layoutChain[1].text.replaceAll('main>', 'article>')
      expect(core.compile([{ path: 'layout.lem', text: edited }]).diagnostics).toEqual([])
      // The new program binds nothing until data is loaded against it.
      core.loadData({ ambient: { page: { title: 'T' } } })
      expect(page(core.render({ path: 'article.lem' }))).toBe(
        '<html><article>Hi T</article></html>'
      )
    })

    it('should drop the loaded data when it installs a new program', async () => {
      const core = await loadCore()
      core.compile(layoutChain, untyped)
      core.loadData({ ambient: { page: { title: 'T' } } })
      expect(
        core.compile([{ path: 'article.lem', text: 'Hi <lem.page.title>' }]).diagnostics
      ).toEqual([])
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
      expect(page(core.render({ path: 'page.lem' }))).toBe('T on s')
    })

    it('should load a collection and bind each document at render as its own record', async () => {
      const core = await loadCore()
      expect(core.compile(site, typedSite).diagnostics).toEqual([])
      expect(core.loadData(loaded)).toEqual([])
      // `title` was validated, `slug`'s default ran over the document's path, and
      // `motto` came through `site` — and the document rendered through its layout,
      // its own output the body.
      expect(page(core.render({ path: 'posts/a.md' }))).toBe(
        '<h1>A</h1>A\nposts/a.md\nhi\n'
      )
    })

    it('should print a float from data and front matter as JavaScript writes it', async () => {
      // Coordinates from limulus.net's photos. A fast, inexact reader lands three of the
      // five on the double one unit in the last place away, and the page prints that.
      const longitude = -110.76918888888889
      const photos = {
        split: { latitude: 32.44250555555555, longitude: -110.78919166666667 },
        'all-in-all-by-beth-lipman': {
          latitude: 42.150394444444444,
          longitude: -77.05377222222222,
        },
      }
      const core = await loadCore()
      const files: SourceFileInput[] = [
        site[0],
        {
          path: 'site.lem',
          text: [
            '<lem-type Photo>',
            '  latitude: float',
            '  longitude: float',
            '</lem-type>',
            '<lem-type Post extends Page>',
            '  longitude: float',
            '  layout: (Post, html): html = post',
            '</lem-type>',
            '<lem-fn post(p: Post, body: html)><lem.p.longitude>|<lem.body></lem-fn>',
            '<lem-let posts: [Post] from "posts/*.md" />',
            '<lem-let photos: {string: Photo} from "_data/photos.json" />',
          ].join('\n'),
        },
        {
          path: 'posts/a.md',
          text:
            `---\nlongitude: ${longitude}\nurl: /a/\n---\n` +
            '<lem-for alias, photo in site.photos><lem.alias> ' +
            '<lem.photo.latitude>,<lem.photo.longitude>;</lem-for>',
          record: 'Post',
        },
      ]
      expect(core.compile(files, typedSite).diagnostics).toEqual([])
      const envelope: DataInput = {
        sources: {
          posts: [{ path: 'posts/a.md', value: { longitude, url: '/a/' } }],
          photos: { path: '_data/photos.json', value: photos },
        },
      }
      expect(core.loadData(envelope)).toEqual([])
      const printed = Object.entries(photos)
        .map(([alias, at]) => `${alias} ${at.latitude},${at.longitude};`)
        .join('')
      expect(page(core.render({ path: 'posts/a.md' }))).toBe(`${longitude}|${printed}\n`)
    })

    it('should load a page no glob holds from `pages` and render it through its layout', async () => {
      const core = await loadCore()
      expect(core.compile([...site, home], typedSite).diagnostics).toEqual([])
      expect(core.loadData(homeLoaded('/'))).toEqual([])
      expect(page(core.render({ path: 'home.lem' }))).toBe('<h1>Home</h1><p>Home</p>')
    })

    it('should refuse two pages at one url at the second, labelled at the first', async () => {
      const core = await loadCore()
      expect(core.compile([...site, home], typedSite).diagnostics).toEqual([])
      const [clash] = core.loadData(homeLoaded('/a/'))
      expect(clash.path).toBe('home.lem')
      expect(clash.message).toBe('`home.lem` and `posts/a.md` are both at `/a/`')
      expect(clash.labels.map((label) => label.path)).toEqual(['posts/a.md'])
    })

    it('should refuse a document type that declares no url field the host names', async () => {
      const core = await loadCore()
      const [missing] = core.compile(site, { ...typedSite, url: 'permalink' }).diagnostics
      expect(missing.path).toBe('ambient.lem')
      expect(missing.message).toBe(
        "`Page` is the host's document type, so it declares where a page goes: " +
          '`permalink: url`'
      )
    })

    it("should bind the envelope's document value only where no loaded document stands", async () => {
      const core = await loadCore()
      expect(core.compile(site, typedSite).diagnostics).toEqual([])
      expect(
        core.compile([{ path: 'loose.md', text: '<lem.page.title>' }]).diagnostics
      ).toEqual([])
      expect(core.loadData({ ...loaded, ambient: { page: { title: 'Loose' } } })).toEqual(
        []
      )
      expect(page(core.render({ path: 'loose.md' }))).toBe('Loose\n')
      expect(page(core.render({ path: 'posts/a.md' }))).toBe(
        '<h1>A</h1>A\nposts/a.md\nhi\n'
      )
    })

    it('should place a load diagnostic by the positions the host sent', async () => {
      const core = await loadCore()
      expect(core.compile(site, typedSite).diagnostics).toEqual([])
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
      expect(core.compile(site, typedSite).diagnostics).toEqual([])
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
      expect(core.compile(site, typedSite).diagnostics).toEqual([])
      expect(core.loadData(loaded)).toEqual([])
      const diagnostics = core.loadData({
        ...loaded,
        sources: {
          ...loaded.sources,
          posts: [{ path: 'posts/a.md', value: { title: 3, url: '/a/' } }],
        },
      })
      expect(diagnostics).toHaveLength(1)
      expect(page(core.render({ path: 'posts/a.md' }))).toBe(
        '<h1>A</h1>A\nposts/a.md\nhi\n'
      )
    })

    it('should throw for an envelope the program cannot be loaded from', async () => {
      const core = await loadCore()
      expect(core.compile(site, typedSite).diagnostics).toEqual([])
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
      expect(core.compile(site, typedSite).diagnostics).toEqual([])
      const changed: HostDecl = {
        ...typedSite,
        ambient: { ...typedSite.ambient, build: 'Build' },
      }
      const [typo] = core.compile(
        [{ path: 'build.lem', text: '<lem-type Build>\n  generator: strng\n</lem-type>' }],
        changed
      ).diagnostics
      expect(typo.severity).toBe('error')
      expect(() => core.loadData(loaded)).toThrow(/declaration changed/)
    })
  })

  describe('plan()', () => {
    it('should list every loaded page in load order with its url', async () => {
      const core = await loadCore()
      expect(core.compile([...site, home], typedSite).diagnostics).toEqual([])
      expect(core.loadData(homeLoaded('/'))).toEqual([])
      expect(core.plan()).toEqual([
        { path: 'posts/a.md', url: '/a/' },
        { path: 'home.lem', url: '/' },
      ])
      // What the plan lists is what `render` takes.
      const [post] = core.plan()
      expect(page(core.render(post))).toBe('<h1>A</h1>A\nposts/a.md\nhi\n')
    })

    it('should list no url for a host that names no url field', async () => {
      const core = await loadCore()
      const { url: _, ...unplaced } = typedSite
      expect(core.compile(site, unplaced).diagnostics).toEqual([])
      expect(core.loadData(loaded)).toEqual([])
      expect(core.plan()).toEqual([{ path: 'posts/a.md' }])
    })

    it('should omit a page the envelope did not carry, whose render then throws', async () => {
      const core = await loadCore()
      expect(core.compile([...site, home], typedSite).diagnostics).toEqual([])
      expect(core.loadData(loaded)).toEqual([])
      expect(core.plan()).toEqual([{ path: 'posts/a.md', url: '/a/' }])
      expect(() => core.render({ path: 'home.lem' })).toThrow(
        /`home.lem` is a page the last load did not carry/
      )
    })

    it('should throw before any load has succeeded', async () => {
      const core = await loadCore()
      expect(() => core.plan()).toThrow(/`loadData`/)
      expect(core.compile(site, typedSite).diagnostics).toEqual([])
      expect(() => core.plan()).toThrow(/`loadData`/)
    })
  })

  describe('render()', () => {
    it('should return the runtime diagnostic instead of HTML when the page fails', async () => {
      const core = await loadCore()
      core.compile([{ path: 'page.lem', text: '<lem.page.absent>' }], untyped)
      core.loadData({ ambient: { page: {} } })
      const result = core.render({ path: 'page.lem' })
      expect(result.html).toBeNull()
      expect(result.marks).toBeNull()
      expect(result.fragments).toBeNull()
      expect(result.diagnostics).toHaveLength(1)
      expect(result.diagnostics[0].path).toBe('page.lem')
      expect(result.diagnostics[0].span.start).toBeGreaterThan(0)
    })

    it('should return the page as bytes with its marks and fragments', async () => {
      const core = await loadCore()
      const text =
        '<lem-fn card()>\n  <p>caf\u00e9</p>\n  <lem-style>\n    .card {}\n  ' +
        '</lem-style>\n</lem-fn>\n<lem.card()><lem.mark("styles")>'
      expect(
        core.compile([{ path: 'page.lem', text }], {
          ...untyped,
          marks: [{ name: 'styles' }],
        }).diagnostics
      ).toEqual([])
      const result = core.render({ path: 'page.lem' })
      expect(page(result)).toBe('<p>caf\u00e9</p>')
      // Both offsets are bytes, not characters: the page is twelve bytes where it is
      // eleven characters, and the call stands one byte past where `indexOf` puts it.
      const encoder = new TextEncoder()
      const at = (text: string) => encoder.encode(text).length
      expect(result.marks).toEqual([
        {
          name: 'styles',
          start: at('<p>caf\u00e9</p>'),
          end: at('<p>caf\u00e9</p>'),
          path: 'page.lem',
          span: expect.objectContaining({
            start: at(text.slice(0, text.indexOf('mark('))),
          }) as Span,
        },
      ])
      expect(result.fragments?.styles?.[0]?.text).toBe('.card {}')
      // What a host does with them: the bundler's link replaces the range, and the
      // page becomes a string once, afterwards.
      const resolved = resolveMarks(result, [
        {
          name: 'styles',
          produce: ({ fragments }) =>
            fragments.map(() => '<style>' + fragments[0].text + '</style>'),
        },
      ])
      expect(decoder.decode(resolved.html)).toBe('<p>caf\u00e9</p><style>.card {}</style>')
      expect([resolved.unclaimed, resolved.unplaced]).toEqual([[], []])
    })

    it('should report each code block once and name it from the render', async () => {
      // The highlighter claims two languages (D42). Two documents write one `sh` block
      // between them, which is one payload; the render names it by id, the host keeps
      // the payloads by id, and a `code` producer reads the code from there — never
      // from the render, which carries none.
      const core = await loadCore()
      const host: HostDecl = {
        ...untyped,
        marks: [{ name: 'code', keys: ['sh', 'ts'], payload: 'Code', keyedBy: 'language' }],
      }
      const { diagnostics, payloads } = core.compile(
        [
          { path: 'a.md', text: '```sh\nls\n```\n' },
          { path: 'b.md', text: '```sh\nls\n```\n\n```lem\n<lem.x>\n```\n' },
        ],
        host
      )
      expect(diagnostics.map((d) => d.message)).toEqual([
        'no producer claims `lem` for `code`, so this mark keeps its default content',
      ])
      expect(payloads.map(({ language, code }) => [language, code])).toEqual([
        ['sh', 'ls\n'],
        ['lem', '<lem.x>\n'],
      ])
      core.loadData({ ambient: {} })
      const result = core.render({ path: 'b.md' })
      expect(result.marks?.map(({ key, payloadId }) => [key, payloadId])).toEqual(
        payloads.map(({ id, language }) => [language, id])
      )
      const byId = new Map(payloads.map((payload) => [payload.id, payload]))
      const resolved = resolveMarks(result, [
        {
          name: 'code',
          keys: ['sh', 'ts'],
          produce: ({ marks }) =>
            marks.map(({ payloadId }) => `<kbd>${byId.get(payloadId!)!.code.trim()}</kbd>`),
        },
      ])
      expect(decoder.decode(resolved.html)).toBe(
        '<kbd>ls</kbd>\n<pre><code class="language-lem">&lt;lem.x&gt;\n</code></pre>\n'
      )
      expect(resolved.unclaimed).toEqual([{ name: 'code', key: 'lem' }])
    })

    it('should throw for two claims on one name that name different payloads', async () => {
      const core = await loadCore()
      expect(() =>
        core.compile([], {
          ...untyped,
          marks: [
            { name: 'code', keys: ['sh'], payload: 'Code' },
            { name: 'code', keys: ['ts'], payload: 'Snippet' },
          ],
        })
      ).toThrow(/two payloads, `Code` and `Snippet`/)
    })

    it('should warn at a mark name the declaration claims no handler for', async () => {
      const core = await loadCore()
      const diagnostics = core.compile(
        [{ path: 'page.lem', text: '<lem.mark("stylez")>' }],
        { ...untyped, marks: [{ name: 'styles' }] }
      ).diagnostics
      expect(diagnostics).toHaveLength(1)
      expect(diagnostics[0].severity).toBe('warning')
      expect(diagnostics[0].message).toContain('did you mean `styles`?')
      // And a host that declares no handlers at all hears nothing about the same name.
      // Its own core: dropping the claims under the file above would leave that one
      // stale, which is the other half of the same rule.
      const quiet = await loadCore()
      expect(
        quiet.compile([{ path: 'page.lem', text: '<lem.mark("stylez")>' }], untyped)
          .diagnostics
      ).toEqual([])
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

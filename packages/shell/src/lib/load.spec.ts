import { loadCore } from '@limulus/lemmon-core'
import { mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'

import { envelope, withBodies, type Envelope } from './load.ts'
import { compileSite } from './program.ts'
import { discover } from './site.ts'
import { configOf, ENV, GENERATING, project, SITE, SITE_LEM } from '../mocks/sites.ts'
import { at } from '../mocks/spans.ts'

const BUILD = { time: new Date('2026-10-15T12:00:00Z'), generator: 'Lemmon 0.0.0' }

/** How {@link enveloped} fulfils a program. */
interface Options {
  env?: object
  /** Changes the project between the compile and the load. */
  prepare?: (dir: string) => void
  /** The texts the build holds already, which the load records into. */
  texts?: Map<string, string>
}

/**
 * The envelope `files` load from: compiled in a fresh core, then fulfilled from the
 * project, in `env`.
 */
async function enveloped(
  files: Record<string, string>,
  { env = ENV, prepare = () => {}, texts = new Map() }: Options = {}
): Promise<Envelope> {
  const dir = project(files)
  const config = configOf(dir)
  const { program } = await compileSite(await loadCore(), await discover(config), texts)
  prepare(dir)
  return await envelope(config.input, program!, BUILD, env as Record<string, string>, texts)
}

describe('envelope()', () => {
  it('should carry `build`, every source by its kind, and the pages no glob holds', async () => {
    const { data, diagnostics } = await enveloped(SITE)
    const photos = SITE['_data/photos.json']
    const post = SITE['posts/a.md']

    expect(diagnostics).toEqual([])
    // No bodies yet: this is what the body pass renders under (D46).
    expect(data?.ambient).toEqual({
      build: { time: '2026-10-15T12:00:00.000Z', generator: 'Lemmon 0.0.0', bodies: {} },
    })
    expect(Object.keys(data?.sources ?? {})).toEqual(['posts', 'photos', 'motto'])
    // A glob's pages in path order, as discovery read their front matter.
    expect(data?.sources?.posts).toEqual([
      {
        path: 'posts/a.md',
        value: {
          title: 'A',
          hero: 'sunset',
          url: '/posts/a/',
          inputPath: 'www/posts/a.md',
        },
        spans: {
          '': at(post, 'title: A\nhero: sunset'),
          title: at(post, 'title: A'),
          hero: at(post, 'hero: sunset'),
        },
      },
      expect.objectContaining({ path: 'posts/b.md' }),
    ])
    // A data file by its path from the input, with positions.
    expect(data?.sources?.photos).toEqual({
      path: '_data/photos.json',
      value: { sunset: { caption: 'A sunset' } },
      spans: {
        '': at(photos, '{\n  "sunset": { "caption": "A sunset" }\n}'),
        sunset: at(photos, '"sunset": { "caption": "A sunset" }'),
        'sunset.caption': at(photos, '"caption": "A sunset"'),
      },
    })
    // An environment value has no file behind it.
    expect(data?.sources?.motto).toEqual({ value: 'hi' })
    // The home page its own `type:` named; the untyped note is no page to load.
    expect(data?.pages?.map((page) => page.path)).toEqual(['index.lem'])
  })

  it('should carry a generator’s `for:` key beside its front matter, a glob’s member’s too', async () => {
    // The core expands the one in `pages`, and refuses the one a glob holds at its key.
    const site = `${GENERATING['site.lem']}<lem-let sections: [Home] from "sections/*.lem" />\n`
    const held = '---\nfor:\n  title: site.topics\n---\n<p><lem.page.title></p>\n'
    const { data, diagnostics } = await enveloped({
      ...GENERATING,
      'site.lem': site,
      'sections/t.lem': held,
    })
    const topics = GENERATING['topics.lem']

    expect(diagnostics).toEqual([])
    expect(data?.pages).toContainEqual({
      path: 'topics.lem',
      value: { inputPath: 'www/topics.lem' },
      spans: { '': at(topics, 'type: Topic\nfor:\n  topic: site.topics') },
      for: {
        value: { topic: 'site.topics' },
        spans: {
          '': at(topics, 'topic: site.topics'),
          topic: at(topics, 'topic: site.topics'),
        },
        valueSpans: { topic: at(topics, 'topic: «site.topics»') },
      },
    })
    expect(data?.sources?.sections).toEqual([
      expect.objectContaining({
        path: 'sections/t.lem',
        for: expect.objectContaining({ value: { title: 'site.topics' } }) as unknown,
      }),
    ])
    // A page with no key carries none.
    expect(data?.pages?.find((page) => page.path === 'index.lem')).not.toHaveProperty('for')
  })

  it('should report an environment variable that is not set, at the binding', async () => {
    const { data, diagnostics } = await enveloped(SITE, { env: { OTHER: 'x' } })

    expect(data).toBeUndefined()
    expect(diagnostics).toEqual([
      {
        path: 'site.lem',
        span: at(SITE_LEM, '<lem-let «motto»'),
        severity: 'error',
        message: '`motto` loads the environment variable `MOTTO`, which is not set',
        labels: [],
      },
    ])
  })

  it('should report a plugin, which nothing runs yet', async () => {
    const site = `${SITE_LEM}<lem-let more: string from plugin("more") />\n`
    const { data, diagnostics } = await enveloped({ ...SITE, 'site.lem': site })

    expect(data).toBeUndefined()
    expect(diagnostics).toEqual([
      expect.objectContaining({
        span: at(site, '<lem-let «more»'),
        message: '`more` loads `plugin("more")`, and plugins are not supported yet (∆wOn)',
      }),
    ])
  })

  it('should report a data file that is not there, or cannot be read', async () => {
    const { '_data/photos.json': _, ...files } = SITE
    const missing = await enveloped(files)
    const directory = await enveloped(files, {
      prepare: (dir) => mkdirSync(join(dir, 'www/_data/photos.json'), { recursive: true }),
    })

    expect(missing.data).toBeUndefined()
    expect(missing.diagnostics).toEqual([
      expect.objectContaining({
        path: 'site.lem',
        span: at(SITE_LEM, '<lem-let «photos»'),
        message:
          '`photos` loads `_data/photos.json`, which is not a file in the input directory',
      }),
    ])
    expect(directory.data).toBeUndefined()
    expect(directory.diagnostics).toEqual([
      expect.objectContaining({
        message: expect.stringMatching(
          /^`photos` loads `_data\/photos.json`, which cannot be read: EISDIR/
        ) as string,
      }),
    ])
  })

  it('should refuse a data file outside the input directory, at the binding', async () => {
    const site = SITE_LEM.replace('"_data/photos.json"', '"../secrets.json"')
    const { data, diagnostics } = await enveloped(
      { ...SITE, 'site.lem': site },
      {
        prepare: (dir) => {
          writeFileSync(join(dir, 'secrets.json'), '{}')
        },
      }
    )

    expect(data).toBeUndefined()
    expect(diagnostics).toEqual([
      expect.objectContaining({
        path: 'site.lem',
        span: at(site, '<lem-let «photos»'),
        message: '`photos` loads `../secrets.json`, which is outside the input directory',
      }),
    ])
  })

  it('should carry a page its `type:` gives a subtype in the glob that holds it', async () => {
    const site = `${SITE_LEM}<lem-type Note extends Post>\n  mood: string?\n</lem-type>\n`
    const { data } = await enveloped({
      ...SITE,
      'site.lem': site,
      'posts/note.md': '---\ntype: Note\ntitle: N\n---\n',
    })

    expect((data?.sources?.posts as { path: string }[]).map((page) => page.path)).toEqual([
      'posts/a.md',
      'posts/b.md',
      'posts/note.md',
    ])
    expect(data?.pages?.map((page) => page.path)).toEqual(['index.lem'])
  })

  it('should report a data file that does not parse, in the file', async () => {
    const photos = '{"sunset": {"caption": "A"}, "sunset": {}}'
    const { data, diagnostics } = await enveloped({ ...SITE, '_data/photos.json': photos })

    expect(data).toBeUndefined()
    expect(diagnostics).toEqual([
      expect.objectContaining({
        path: '_data/photos.json',
        span: at(photos, '}, «"sunset"»'),
        message: '`sunset` is already given',
      }),
    ])
  })

  it('should carry a data file’s warning beside its value', async () => {
    const photos = 'sunset: !photo\n  caption: A\n'
    const { data, diagnostics } = await enveloped({ ...SITE, '_data/photos.json': photos })

    expect(diagnostics).toEqual([
      expect.objectContaining({ path: '_data/photos.json', severity: 'warning' }),
    ])
    expect(data?.sources?.photos).toMatchObject({ value: { sunset: { caption: 'A' } } })
  })

  describe('the texts it reads', () => {
    it('should record each data file by its binding’s literal', async () => {
      const texts = new Map<string, string>()

      await enveloped(SITE, { texts })

      expect(texts.get('_data/photos.json')).toBe(SITE['_data/photos.json'])
    })

    it('should take a file the build read already from its text, and not read it again', async () => {
      // A binding loading a document the walk read, which changed on disk since.
      const card = 'caption: As the walk read it\n'
      const site = `${SITE_LEM}<lem-let card: Photo from "notes/card.md" />\n`
      const texts = new Map([['notes/card.md', card]])

      const { data } = await enveloped(
        { ...SITE, 'site.lem': site, 'notes/card.md': card },
        {
          texts,
          prepare: (dir) => {
            writeFileSync(join(dir, 'www/notes/card.md'), 'caption: Since\n')
          },
        }
      )

      expect(data?.sources?.card).toMatchObject({
        value: { caption: 'As the walk read it' },
      })
      expect(texts.get('notes/card.md')).toBe(card)
    })

    it('should load two bindings of one file from the one text it records', async () => {
      const site = `${SITE_LEM}<lem-let again: {string: Photo} from "_data/photos.json" />\n`
      const texts = new Map<string, string>()

      const { data, diagnostics } = await enveloped(
        { ...SITE, 'site.lem': site },
        { texts }
      )

      expect(diagnostics).toEqual([])
      expect(data?.sources?.again).toEqual(data?.sources?.photos)
      expect(texts.get('_data/photos.json')).toBe(SITE['_data/photos.json'])
    })
  })
})

describe('withBodies()', () => {
  it('should carry the bodies as `build.bodies`, leaving the rest of the envelope as it was', async () => {
    const { data } = await enveloped(SITE)
    const bodies = { '/posts/a/': '<p>Hello <em>a</em>.</p>\n' }

    const second = withBodies(data!, bodies)

    expect(second.ambient).toEqual({
      build: { time: '2026-10-15T12:00:00.000Z', generator: 'Lemmon 0.0.0', bodies },
    })
    expect({ ...second, ambient: data?.ambient }).toEqual(data)
    // The first envelope is untouched: an untyped page's reload spreads the second.
    expect(data?.ambient?.build).toMatchObject({ bodies: {} })
  })
})

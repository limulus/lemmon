import { describe, expect, it } from 'vitest'

import type { Plugin } from './plugins.ts'
import { FILE_START } from './program.ts'
import { discover } from './site.ts'
import { appending } from '../mocks/plugins.ts'
import { configOf } from '../mocks/sites.ts'
import { at } from '../mocks/spans.ts'
import { tempDir } from '../mocks/temp-dir.ts'

describe('discover()', () => {
  const post = '---\ntype: Article\ntitle: The Rebound\n---\nBody\n'
  const broken = '---\ntitle: a\ntitle: b\n---\n'
  const files = {
    'feed/the-rebound/index.md': post,
    'feed/broken/index.md': broken,
    'feed/index.lem': '---\ntitle: Feed\n---\n<p></p>\n',
    'layouts.lem': '<lem-fn page(p: Page, body: html): html>…</lem-fn>\n',
    'robots.txt': '',
  }

  async function discovered(plugins: Plugin[] = []) {
    const root = tempDir(
      Object.fromEntries(Object.entries(files).map(([path, text]) => [`www/${path}`, text]))
    )
    const config = configOf(root, ['*.txt'], plugins)
    return { config, site: await discover(config) }
  }

  it('should carry the config and every entry the walk found', async () => {
    const { config, site } = await discovered()

    expect(site.config).toBe(config)
    expect(site.entries.map((entry) => entry.path)).toEqual([
      'feed/broken/index.md',
      'feed/index.lem',
      'feed/the-rebound/index.md',
      'layouts.lem',
      'robots.txt',
    ])
  })

  it('should read every document and page template, by path in path order', async () => {
    const { site } = await discovered()

    expect([...site.pages.keys()]).toEqual(['feed/index.lem', 'feed/the-rebound/index.md'])
    expect(site.pages.get('feed/the-rebound/index.md')).toEqual({
      loaded: {
        value: {
          title: 'The Rebound',
          url: '/feed/the-rebound/',
          inputPath: 'www/feed/the-rebound/index.md',
        },
        path: 'feed/the-rebound/index.md',
        spans: {
          '': at(post, 'type: Article\ntitle: The Rebound'),
          title: at(post, 'title: The Rebound'),
        },
      },
      record: { name: 'Article', span: at(post, 'Article') },
      body: at(post, 'Body').start,
    })
    expect(site.pages.get('feed/index.lem')?.loaded.value.url).toBe('/feed/')
  })

  it('should leave out a page whose front matter has an error, and report it', async () => {
    const { site } = await discovered()

    expect(site.pages.has('feed/broken/index.md')).toBe(false)
    expect(site.diagnostics).toEqual([
      expect.objectContaining({
        path: 'feed/broken/index.md',
        span: at(broken, 'title: a\n«title»'),
        severity: 'error',
      }),
    ])
  })

  describe('with plugins that transform documents', () => {
    it('should hold each document whose front matter read as transformed, and no template', async () => {
      const { site } = await discovered([appending('Appended.\n')])

      const texts = Object.fromEntries(
        site.entries.map((entry) => [entry.path, 'text' in entry ? entry.text : null])
      )
      expect(texts).toEqual({
        ...files,
        'feed/the-rebound/index.md': `${post}Appended.\n`,
        // Its front matter has an error, so there is nothing to give a transform.
        'feed/broken/index.md': broken,
        'robots.txt': null,
      })
      // The page read before the transform, and its body starts where it did.
      expect(site.pages.get('feed/the-rebound/index.md')?.body).toBe(at(post, 'Body').start)
    })

    it('should report a transform that fails beside what the front matter reported', async () => {
      const failing: Plugin = {
        name: 'failing',
        transformDocument: () => {
          throw new Error('no')
        },
      }

      const { site } = await discovered([failing])

      expect(site.diagnostics.map((d) => [d.path, d.span, d.severity])).toEqual([
        ['feed/broken/index.md', at(broken, 'title: a\n«title»'), 'error'],
        [
          'feed/the-rebound/index.md',
          at(post, 'type: Article\ntitle: The Rebound'),
          'error',
        ],
      ])
      expect(site.entries.find((e) => e.path === 'feed/the-rebound/index.md')).toEqual({
        kind: 'document',
        path: 'feed/the-rebound/index.md',
        text: post,
      })
    })
  })

  describe('a file named `.md` or `.lem` in another case', () => {
    /** What discovering `files`, under `www/`, with `passthrough` globs reported. */
    async function reported(files: Record<string, string>, passthrough: string[] = []) {
      const root = tempDir(
        Object.fromEntries(
          Object.entries(files).map(([path, text]) => [`www/${path}`, text])
        )
      )
      return (await discover(configOf(root, passthrough))).diagnostics
    }

    it('should be an error at its start, naming the file it would be', async () => {
      const diagnostics = await reported({ 'feed/x/index.MD': '# X\n', 'nav.Lem': '' })

      expect(diagnostics).toEqual([
        {
          path: 'feed/x/index.MD',
          span: FILE_START,
          severity: 'error',
          message:
            'only a lowercase `.md` is read, so `index.MD` would be skipped: rename it to ' +
            '`index.md`',
          labels: [],
        },
        expect.objectContaining({
          path: 'nav.Lem',
          message:
            'only a lowercase `.lem` is read, so `nav.Lem` would be skipped: rename it to ' +
            '`nav.lem`',
        }),
      ])
    })

    it('should be no error where a passthrough glob copies it, nor beside it', async () => {
      const diagnostics = await reported(
        { 'notes/README.MD': '# Notes\n', 'feed/index.md': '---\ntitle: Feed\n---\n' },
        ['notes/**']
      )

      expect(diagnostics).toEqual([])
    })
  })
})

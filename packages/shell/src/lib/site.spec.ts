import { describe, expect, it } from 'vitest'

import { discover } from './site.ts'
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

  async function discovered() {
    const root = tempDir(
      Object.fromEntries(Object.entries(files).map(([path, text]) => [`www/${path}`, text]))
    )
    const config = configOf(root, ['*.txt'])
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
})

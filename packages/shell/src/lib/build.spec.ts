import { loadCore, type Diagnostic } from '@limulus/lemmon-core'
import { existsSync, mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'

import { build, type BuildResult } from './build.ts'
import { FILE_START as START, SiteError } from './program.ts'
import { WriteError } from './write.ts'
import {
  configOf,
  ENV,
  PASSTHROUGH,
  project,
  SITE,
  SITE_LEM,
  tree,
} from '../mocks/sites.ts'
import { at } from '../mocks/spans.ts'

const NOW = new Date('2026-10-15T12:00:00Z')

/** Builds `files` as a project, in `env`, returning the result and the project. */
async function built(
  files: Record<string, string>,
  { env = ENV, passthrough = PASSTHROUGH }: { env?: object; passthrough?: string[] } = {}
): Promise<{ result: BuildResult; dir: string }> {
  const dir = project(files, passthrough)
  const result = await build(configOf(dir, passthrough), {
    now: NOW,
    env: env as Record<string, string>,
  })
  return { result, dir }
}

/** Builds `files`, expecting it to fail before writing anything: the errors it reported. */
async function refused(
  files: Record<string, string>,
  options: { env?: object; passthrough?: string[] } = {}
): Promise<Diagnostic[]> {
  const { result, dir } = await built(files, options)
  expect(result.pages).toEqual([])
  expect(result.copied).toEqual([])
  expect(existsSync(join(dir, 'dist'))).toBe(false)
  return result.diagnostics.filter((diagnostic) => diagnostic.severity === 'error')
}

describe('build()', () => {
  describe('a site that builds', () => {
    it('should write every page and copy every passthrough file', async () => {
      const { result, dir } = await built(SITE)
      const generator = `Lemmon ${(await loadCore()).version}`

      // The loaded pages in the plan's order — the glob's, then the page its own
      // `type:` named — then the document nothing typed.
      expect(result).toEqual({
        pages: [
          'posts/a/index.html',
          'posts/b/index.html',
          'index.html',
          'notes/loose/index.html',
        ],
        copied: ['assets/site.css'],
        diagnostics: [],
      })
      expect(tree(join(dir, 'dist'))).toEqual({
        'posts/a/index.html':
          '<title>A · hi</title>\n' +
          `<meta name="generator" content="${generator}">\n` +
          '<main><h1>A</h1>\n\n  <figure>A sunset</figure>\n\n' +
          '<p>Hello <em>a</em>.</p>\n</main>',
        'posts/b/index.html':
          '<title>B · hi</title>\n' +
          `<meta name="generator" content="${generator}">\n` +
          '<main><h1>B</h1>\n\n<p>Hello b.</p>\n</main>',
        'index.html':
          '<title>Home · hi</title>\n' +
          `<meta name="generator" content="${generator}">\n` +
          '<main>\n  <a href="/posts/a/">A</a>\n\n  <a href="/posts/b/">B</a>\n\n</main>',
        // No layout, and `page` its own front matter; `build.time` is the clock's.
        'notes/loose/index.html': 'Loose\n<p>Built at:</p>\n2026-10-15T12:00:00Z\n',
        'assets/site.css': 'main { margin: 0 }\n',
      })
    })

    it('should bind each untyped page to its own front matter', async () => {
      const { result, dir } = await built({
        ...SITE,
        'notes/loose.md': '---\ntitle: One\n---\n<lem.page.title>\n',
        'notes/other.md': '---\ntitle: Two\n---\n<lem.page.title>\n',
      })

      expect(result.diagnostics).toEqual([])
      expect(tree(join(dir, 'dist/notes'))).toEqual({
        'loose/index.html': 'One\n',
        'other/index.html': 'Two\n',
      })
    })

    it('should write a page at the file its url names, and the bytes as rendered', async () => {
      const { result, dir } = await built({
        ...SITE,
        'feed.lem':
          '---\ntype: Home\ntitle: Café ’\nurl: /feed.xml\n---\n<lem.page.title>\n',
        'posts/a b.md': '---\ntitle: Café\n---\n',
        'notes/a b.md': 'Café\n',
      })

      const written = tree(join(dir, 'dist'))
      expect(written['feed.xml']).toContain('Café ’')
      // A loaded page's url comes back from the core as a `url`, encoded —
      // `/posts/a%20b/` — and its file is named by what the encoding stands for.
      expect(result.pages).toContain('posts/a b/index.html')
      expect(written['posts/a b/index.html']).toMatch(/^<title>Café · hi/)
      // A page no record typed keeps the url the shell derived, which never passed
      // through the core, and is written at the same kind of name.
      expect(written['notes/a b/index.html']).toBe('<p>Café</p>\n')
    })

    it('should write a page its own `type:` gives a subtype, in its glob', async () => {
      const site = `${SITE_LEM}<lem-type Note extends Post>\n  mood: string?\n</lem-type>\n`
      const { result, dir } = await built({
        ...SITE,
        'site.lem': site,
        'posts/note.md': '---\ntype: Note\ntitle: N\nmood: calm\n---\n',
        'index.lem': SITE['index.lem'],
      })

      expect(result.diagnostics).toEqual([])
      expect(result.pages).toContain('posts/note/index.html')
      // The home page lists the glob's members, the subtype's page among them.
      expect(tree(join(dir, 'dist'))['index.html']).toContain(
        '<a href="/posts/note/">N</a>'
      )
    })

    it('should let a layout call what a page template defines', async () => {
      const about =
        '---\ntype: Home\ntitle: About\n---\n<lem-fn helper(): string = "helped" />\n' +
        '<p>About</p>\n'
      const layouts = SITE['layouts.lem'].replace(
        '<lem.base(h.title, body)>',
        '<lem.base(h.title + " " + helper(), body)>'
      )
      const { result, dir } = await built({
        ...SITE,
        'about.lem': about,
        'layouts.lem': layouts,
      })

      expect(result.diagnostics).toEqual([])
      expect(tree(join(dir, 'dist'))['about/index.html']).toMatch(
        /^<title>About helped · hi<\/title>/
      )
    })

    it('should report a page template’s warning once, as its record compiles it', async () => {
      const about = '---\ntype: Home\ntitle: About\n---\n<lem-let x = 1 />\n<p>About</p>\n'
      const { result } = await built({ ...SITE, 'about.lem': about })

      expect(result.diagnostics).toEqual([
        expect.objectContaining({
          path: 'about.lem',
          span: at(about, 'x'),
          severity: 'warning',
        }),
      ])
      expect(result.pages).toContain('about/index.html')
    })

    it('should leave what it does not write in the output, and replace what it does', async () => {
      const dir = project(SITE, PASSTHROUGH)
      mkdirSync(join(dir, 'dist/posts/a'), { recursive: true })
      writeFileSync(join(dir, 'dist/keep.txt'), 'kept')
      writeFileSync(join(dir, 'dist/posts/a/index.html'), 'stale')

      await build(configOf(dir, PASSTHROUGH), { now: NOW, env: ENV })

      const written = tree(join(dir, 'dist'))
      expect(written['keep.txt']).toBe('kept')
      expect(written['posts/a/index.html']).toMatch(/^<title>A/)
    })

    it('should report a warning and build all the same', async () => {
      const { result, dir } = await built({
        ...SITE,
        'helpers.lem': '<lem-fn unused()>\n  <lem-let x = 1 />\n</lem-fn>\n',
      })

      expect(result.diagnostics).toEqual([
        expect.objectContaining({ path: 'helpers.lem', severity: 'warning' }),
      ])
      expect(result.pages).toHaveLength(4)
      expect(existsSync(join(dir, 'dist/index.html'))).toBe(true)
    })
  })

  describe('a site that does not build', () => {
    it('should stop at front matter that does not read, before compiling', async () => {
      const post = '---\ntitle: a: b\n---\n'
      const errors = await refused({ ...SITE, 'posts/c.md': post, 'layouts.lem': '<lem.x' })

      expect(errors).toEqual([
        expect.objectContaining({ path: 'posts/c.md', span: at(post, 'title: «a»: b') }),
      ])
    })

    it('should refuse a site with no `site.lem`', async () => {
      const { 'site.lem': _, ...files } = SITE

      await expect(built(files)).rejects.toThrow(SiteError)
    })

    it('should report a compile error in a template, and compile no page', async () => {
      const broken = '<lem-fn broken()><lem.nope></lem-fn>\n'
      const errors = await refused({
        ...SITE,
        'broken.lem': broken,
        'posts/b.md': '---\ntitle: B\n---\n<lem.page.byline>\n',
      })

      expect(errors).toEqual([
        expect.objectContaining({
          path: 'broken.lem',
          span: at(broken, 'nope'),
          message: 'undefined name `nope`',
        }),
      ])
    })

    it('should report a compile error in a page', async () => {
      const post = '---\ntitle: B\n---\n<lem.page.byline>\n'
      const errors = await refused({ ...SITE, 'posts/b.md': post })

      expect(errors).toEqual([
        expect.objectContaining({
          path: 'posts/b.md',
          span: at(post, 'page.byline'),
          message: '`Post` has no field `byline`',
        }),
      ])
    })

    it('should report an environment variable that is not set', async () => {
      const errors = await refused(SITE, { env: {} })

      expect(errors).toEqual([
        expect.objectContaining({
          path: 'site.lem',
          span: at(SITE_LEM, '<lem-let «motto»'),
          message: '`motto` loads the environment variable `MOTTO`, which is not set',
        }),
      ])
    })

    it('should report a data file that is not there', async () => {
      const { '_data/photos.json': _, ...files } = SITE
      const errors = await refused(files)

      expect(errors).toEqual([
        expect.objectContaining({
          path: 'site.lem',
          span: at(SITE_LEM, '<lem-let «photos»'),
          message:
            '`photos` loads `_data/photos.json`, which is not a file in the input directory',
        }),
      ])
    })

    it('should report a plugin source, which nothing runs yet', async () => {
      const site = `${SITE_LEM}<lem-let more: string from plugin("more") />\n`
      const errors = await refused({ ...SITE, 'site.lem': site })

      expect(errors).toEqual([
        expect.objectContaining({
          path: 'site.lem',
          span: at(site, '<lem-let «more»'),
          message:
            '`more` loads `plugin("more")`, and plugins are not supported yet (∆c4j)',
        }),
      ])
    })

    it('should report what the core will not load', async () => {
      const post = '---\ntitle: A\nhero: sunst\n---\n'
      const errors = await refused({ ...SITE, 'posts/a.md': post })

      expect(errors).toEqual([
        expect.objectContaining({ path: 'posts/a.md', span: at(post, 'hero: sunst') }),
      ])
    })

    it('should report a page that fails to render, and write no other', async () => {
      const post = '---\ntitle: B\n---\n<lem.site.photos["nope"].caption>\n'
      const errors = await refused({ ...SITE, 'posts/b.md': post })

      expect(errors).toEqual([expect.objectContaining({ path: 'posts/b.md' })])
      expect(errors[0].message).toMatch(/nope/)
    })

    it('should report a url that names no file in the output', async () => {
      const query = '---\nurl: /notes?draft\n---\n'
      const number = '---\nurl: 3\n---\n'
      const errors = await refused({
        ...SITE,
        'notes/loose.md': query,
        'notes/number.md': number,
      })

      const names = 'names no file in the output directory'
      expect(errors).toEqual([
        expect.objectContaining({
          path: 'notes/loose.md',
          span: at(query, 'url: /notes?draft'),
          message: expect.stringContaining(`the url "/notes?draft" ${names}`) as string,
        }),
        expect.objectContaining({
          path: 'notes/number.md',
          span: at(number, 'url: 3'),
          message: expect.stringContaining(`the url 3 ${names}`) as string,
        }),
      ])
    })

    it('should refuse two pages, or a page and a passthrough file, at one file', async () => {
      const css = '---\nurl: /assets/site.css\n---\n'
      const errors = await refused({
        ...SITE,
        'notes/loose.md': css,
        'posts/a/index.md': 'No front matter.\n',
      })

      expect(errors).toEqual([
        {
          path: 'notes/loose.md',
          span: at(css, 'url: /assets/site.css'),
          severity: 'error',
          message:
            '`assets/site.css` is written by the passthrough file `assets/site.css` already',
          labels: [
            {
              path: 'assets/site.css',
              span: START,
              message: 'the first to write there, as `assets/site.css`',
            },
          ],
        },
        {
          path: 'posts/a/index.md',
          // With no front matter to point at, the page is placed at its start.
          span: START,
          severity: 'error',
          message: '`posts/a/index.html` is written by `posts/a.md` already',
          labels: [
            {
              path: 'posts/a.md',
              // The first page's url was derived, so it is placed at its front matter.
              span: at(SITE['posts/a.md'], 'title: A\nhero: sunset'),
              message: 'the first to write there, as `posts/a/index.html`',
            },
          ],
        },
      ])
    })

    it('should refuse files a file system may hold as one: case, or Unicode form', async () => {
      const upper = '---\nurl: /About/\n---\n'
      const decomposed = '---\nurl: /cafe%CC%81/\n---\n'
      const asset = '---\nurl: /assets/site.css\n---\n'
      // The stylesheet is spelled with a capital here alone: a case-insensitive file
      // system would hold the fixture's own `assets/` and this as one directory.
      const { 'assets/site.css': _, ...site } = SITE
      const errors = await refused(
        {
          ...site,
          'notes/a-about.md': '---\nurl: /about/\n---\n',
          'notes/b-about.md': upper,
          'notes/c-cafe.md': '---\nurl: /caf%C3%A9/\n---\n',
          'notes/d-cafe.md': decomposed,
          'notes/e-asset.md': asset,
          'Assets/site.css': 'main {}\n',
        },
        { passthrough: ['Assets/**'] }
      )

      expect(
        errors.map(({ path, span, message, labels }) => ({ path, span, message, labels }))
      ).toEqual([
        {
          path: 'notes/b-about.md',
          span: at(upper, 'url: /About/'),
          message:
            '`About/index.html` differs from `about/index.html`, which ' +
            '`notes/a-about.md` writes, only in case or Unicode form, and a file system ' +
            'may hold the two as one file',
          labels: [expect.objectContaining({ path: 'notes/a-about.md' })],
        },
        {
          path: 'notes/d-cafe.md',
          span: at(decomposed, 'url: /cafe%CC%81/'),
          message: expect.stringMatching(
            /^`cafe\u0301\/index.html` differs from `caf\u00e9\/index.html`, which/
          ) as string,
          labels: [expect.objectContaining({ path: 'notes/c-cafe.md' })],
        },
        {
          path: 'notes/e-asset.md',
          span: at(asset, 'url: /assets/site.css'),
          message: expect.stringContaining(
            'from `Assets/site.css`, which the passthrough file `Assets/site.css` writes'
          ) as string,
          labels: [
            {
              path: 'Assets/site.css',
              span: START,
              message: 'the first to write there, as `Assets/site.css`',
            },
          ],
        },
      ])
    })

    it('should refuse a file where another needs a directory, either way round', async () => {
      const feed = '---\nurl: /feed\n---\n'
      const robots = '---\nurl: /robots.txt/\n---\n'
      const post = '---\nurl: /posts/a\n---\n'
      const errors = await refused(
        {
          ...SITE,
          'feed.md': feed,
          'feed/x.md': 'X\n',
          'notes/post.md': post,
          'notes/robots.md': robots,
          'robots.txt': 'User-agent: *\n',
        },
        { passthrough: [...PASSTHROUGH, 'robots.txt'] }
      )

      expect(
        errors.map(({ path, message, labels }) => [path, message, labels[0].path])
      ).toEqual([
        [
          'feed/x.md',
          '`feed/x/index.html` would be inside `feed`, which `feed.md` writes as a file',
          'feed.md',
        ],
        [
          'notes/post.md',
          '`posts/a` would be a file, and `posts/a.md` writes `posts/a/index.html` inside it',
          'posts/a.md',
        ],
        [
          'notes/robots.md',
          '`robots.txt/index.html` would be inside `robots.txt`, which the passthrough ' +
            'file `robots.txt` writes as a file',
          'robots.txt',
        ],
      ])
      expect(errors[1].span).toEqual(at(post, 'url: /posts/a'))
    })

    it('should report a file the file system refuses, having written what came before', async () => {
      const dir = project(SITE, PASSTHROUGH)
      // An earlier build's file stands where this one needs a directory, and nothing
      // removes it.
      mkdirSync(join(dir, 'dist'))
      writeFileSync(join(dir, 'dist/posts'), 'stale')

      const result = await build(configOf(dir, PASSTHROUGH), { now: NOW, env: ENV })

      expect(result.pages).toEqual([])
      expect(result.failure).toBeInstanceOf(WriteError)
      expect(result.failure?.file).toBe(join(dir, 'dist/posts/a/index.html'))
      expect(result.failure?.message).toMatch(/^could not write .*: ENOTDIR: /)
    })
  })
})

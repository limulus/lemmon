import { loadCore, type Diagnostic } from '@limulus/lemmon-core'
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'

import { build, check, type BuildResult } from './build.ts'
import { bundler } from './bundler.ts'
import type { Plugin } from './plugins.ts'
import { AMBIENT_FILE, FILE_START as START, SiteError } from './program.ts'
import { WriteError } from './write.ts'
import {
  CALLOUT_LEM,
  callouts,
  copy,
  highlighter,
  inline,
  wrapping,
} from '../mocks/plugins.ts'
import {
  configOf,
  ENV,
  LAYOUTS_LEM,
  PASSTHROUGH,
  project,
  SITE,
  SITE_LEM,
  tree,
} from '../mocks/sites.ts'
import { at } from '../mocks/spans.ts'

const NOW = new Date('2026-10-15T12:00:00Z')

/** How {@link built} builds: in what environment, copying what, with which plugins. */
interface Options {
  env?: object
  passthrough?: string[]
  plugins?: Plugin[]
}

/** Builds `files` as a project, in `env`, returning the result and the project. */
async function built(
  files: Record<string, string>,
  { env = ENV, passthrough = PASSTHROUGH, plugins = [] }: Options = {}
): Promise<{ result: BuildResult; dir: string }> {
  const dir = project(files, passthrough)
  const result = await build(configOf(dir, passthrough, plugins), {
    now: NOW,
    env: env as Record<string, string>,
  })
  return { result, dir }
}

/** Builds `files`, expecting it to fail before writing anything: the errors it reported. */
async function refused(
  files: Record<string, string>,
  options: Options = {}
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
        produced: [],
        copied: ['assets/site.css'],
        diagnostics: [],
        texts: expect.any(Map) as Map<string, string>,
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

  describe('marks', () => {
    // The base layout places the page's styles, and a post's body holds a code block.
    const layouts = LAYOUTS_LEM.replace('<main>', '<lem.mark("styles")>\n  <main>')
    const post = '---\ntitle: A\n---\n\n```ts {1}\nlet x = 1\n```\n'
    const files = { ...SITE, 'layouts.lem': layouts, 'posts/a.md': post }
    const KEPT = '<pre><code class="language-ts">let x = 1\n</code></pre>'

    /** The page `posts/a.md` was written as, after building `files` with `plugins`. */
    async function postA(plugins: Plugin[], more: Record<string, string> = {}) {
      const { result, dir } = await built({ ...files, ...more }, { plugins })
      return { result, html: tree(join(dir, 'dist'))['posts/a/index.html'] }
    }

    /** A plugin whose `code` producer for `ts` is `produce`. */
    const ts = (produce: () => never[]): Plugin => ({
      name: 'ts',
      marks: [
        { name: 'code', keys: ['ts'], payload: 'Code', keyedBy: 'language', produce },
      ],
    })

    it('should replace each mark with its producer’s output, then decorators in plugin order', async () => {
      const frame: Plugin = { name: 'frame', marks: [wrapping('frame')] }
      const { result, html } = await postA([copy, inline, highlighter, frame])

      expect(result.diagnostics).toEqual([])
      // The producer runs first wherever its plugin is listed; the decorators follow in
      // the order the plugins are.
      expect(html).toContain('<frame><copy><pre class="ts">let x = 1</pre></copy></frame>')
      expect(html).toContain('<style></style>\n<main>')
    })

    it('should give a code block’s handlers the payload the compile reported', async () => {
      const seen: unknown[] = []
      const watching: Plugin = {
        name: 'watching',
        marks: [
          {
            name: 'code',
            keys: ['ts'],
            payload: 'Code',
            keyedBy: 'language',
            produce: ({ marks }) => {
              seen.push(...marks.map(({ payload, payloadId }) => ({ payload, payloadId })))
              return ['<p>made</p>']
            },
            decorate: ({ payload }, content) => {
              seen.push({ payload })
              return content
            },
          },
        ],
      }
      const { html } = await postA([inline, watching])

      const payload = { language: 'ts', info: 'ts {1}', code: 'let x = 1\n' }
      expect(seen).toEqual([
        { payload, payloadId: expect.stringMatching(/^[0-9a-f]{16}$/) as string },
        { payload },
      ])
      expect(html).toContain('<p>made</p>')
    })

    it('should dispatch a template’s mark by the payload field its claim keys by', async () => {
      const home =
        '---\ntype: Home\ntitle: Home\nurl: /\n---\n' +
        '<lem-let d><p>plain</p></lem-let>\n' +
        '<lem.mark("callout", {kind: "note", text: "Read me"}, default: d)>\n' +
        '<lem.mark("callout", {kind: "tip", text: "Or not"}, default: d)>\n'
      const { result, dir } = await built(
        { ...files, 'site.lem': `${SITE_LEM}${CALLOUT_LEM}`, 'index.lem': home },
        { plugins: [inline, highlighter, callouts] }
      )

      expect(result.diagnostics).toEqual([
        expect.objectContaining({
          path: 'index.lem',
          span: at(home, '"tip"'),
          severity: 'warning',
          message:
            'no producer claims `tip` for `callout`, so this mark keeps its default content',
        }),
      ])
      expect(tree(join(dir, 'dist'))['index.html']).toContain(
        '<main><aside class="note">Read me</aside>\n<p>plain</p>\n</main>'
      )
    })

    it('should leave a mark nothing claims its default content', async () => {
      const toc =
        '---\ntitle: A\n---\n<lem-let d><nav>none</nav></lem-let>\n' +
        '<lem.mark("toc", default: d)>\n'
      const { result, html } = await postA([inline], { 'posts/a.md': toc })

      expect(result.diagnostics.map((d) => d.message)).toEqual([
        'no handler claims `toc`, so this mark keeps its default content',
      ])
      expect(html).toContain('<nav>none</nav>')
    })

    it('should warn and keep the default content where a producer throws, resolving the rest', async () => {
      const throwing = ts(() => {
        throw new Error('no grammar')
      })
      const { result, html } = await postA([inline, throwing])

      expect(result.diagnostics).toEqual([
        {
          path: 'posts/a.md',
          span: at(post, '«ts» {1}'),
          severity: 'warning',
          message:
            "the `ts` plugin's producer for `code` failed on `posts/a.md`, so the marks it " +
            'answers keep their default content: it threw Error: no grammar',
          labels: [],
        },
      ])
      expect(html).toContain(KEPT)
      // The page's other mark is resolved all the same.
      expect(html).toContain('<style></style>')
      expect(result.pages).toHaveLength(4)
    })

    it('should warn and keep the default content where a producer miscounts', async () => {
      const { result, html } = await postA([inline, ts(() => [])])

      expect(result.diagnostics).toEqual([
        expect.objectContaining({
          span: at(post, '«ts» {1}'),
          message: expect.stringMatching(
            /: it returned 0 replacements for 1 mark$/
          ) as string,
        }),
      ])
      expect(html).toContain(KEPT)
    })

    it('should warn and keep the content so far where a decorator throws', async () => {
      const throwing: Plugin = {
        name: 'throwing',
        marks: [
          {
            name: 'code',
            decorate: () => {
              throw new Error('no button')
            },
          },
        ],
      }
      const { result, html } = await postA([inline, highlighter, throwing, copy])

      expect(result.diagnostics).toEqual([
        expect.objectContaining({
          path: 'posts/a.md',
          span: at(post, '«ts» {1}'),
          severity: 'warning',
          message:
            "the `throwing` plugin's decorator for `code` failed on `posts/a.md`, so this " +
            'mark keeps the content it had: it threw Error: no button',
        }),
      ])
      expect(html).toContain('<copy><pre class="ts">let x = 1</pre></copy>')
    })

    describe('what a page collects', () => {
      // Two fragments under one name: the warning points at the first.
      const card =
        '<lem-fn card()>\n  <p>card</p>\n  <lem-style>\n    .card {}\n  </lem-style>\n' +
        '  <lem-style>\n    .card p {}\n  </lem-style>\n</lem-fn>\n'
      const carded = '---\ntitle: C\n---\n<lem.card()>\n'

      it('should warn at the fragments a page collects where nothing on it places them', async () => {
        const { result } = await built(
          { ...SITE, 'card.lem': card, 'posts/c.md': carded },
          { plugins: [inline] }
        )

        // One warning per page that collected them, each naming its page.
        expect(result.diagnostics).toEqual([
          {
            path: 'card.lem',
            span: at(card, '<lem-style>\n    .card {}\n  </lem-style>'),
            severity: 'warning',
            message:
              '`posts/c.md` collects `styles` here, and nothing on that page places ' +
              '`mark("styles")`, so what it collected is not written',
            labels: [],
          },
        ])
      })

      it('should not warn where a page places what it collected, or collected nothing', async () => {
        const { result, dir } = await built(
          { ...files, 'card.lem': card, 'posts/c.md': carded },
          { plugins: [inline, highlighter] }
        )

        expect(result.diagnostics).toEqual([])
        expect(tree(join(dir, 'dist'))['posts/c/index.html']).toContain(
          '<style>.card {}\n.card p {}</style>'
        )
      })
    })

    it('should resolve an untyped page as a loaded one', async () => {
      const { result, dir } = await built(
        { ...files, 'notes/loose.md': '```ts\nlet y\n```\n' },
        { plugins: [inline, highlighter] }
      )

      expect(result.diagnostics).toEqual([])
      expect(tree(join(dir, 'dist'))['notes/loose/index.html']).toBe(
        '<pre class="ts">let y</pre>\n'
      )
    })
  })

  describe('files a producer writes', () => {
    // Every page the base layout renders places its styles; `posts/a.md` collects some.
    const layouts = LAYOUTS_LEM.replace('<main>', '<lem.mark("styles")>\n  <main>')
    const card = '<lem-fn card()>\n  <lem-style>\n    .card {}\n  </lem-style>\n</lem-fn>\n'
    const files = {
      ...SITE,
      'layouts.lem': layouts,
      'card.lem': card,
      'posts/a.md': `${SITE['posts/a.md']}<lem.card()>\n`,
    }
    const MARK = at(layouts, 'mark("styles")')

    /**
     * A plugin whose `styles` producer writes what the page collected to the file `url`
     * names for it, and links it.
     */
    const sheets = (
      url: (css: string) => string,
      content = (css: string) => css
    ): Plugin => ({
      name: 'sheets',
      marks: [
        {
          name: 'styles',
          produce: ({ marks, fragments, write }) => {
            const css = fragments.map((f) => f.text).join('\n')
            write(url(css), content(css))
            return marks.map(() => `<link href="${url(css)}">`)
          },
        },
      ],
    })

    /** A plugin whose `styles` producer writes each of `files`, `[url, content]`. */
    const writing = (...files: [string, string][]): Plugin => ({
      name: 'writing',
      marks: [
        {
          name: 'styles',
          produce: ({ marks, write }) => {
            for (const [url, content] of files) write(url, content)
            return marks.map(() => '')
          },
        },
      ],
    })
    const byContent = sheets((css) =>
      css === '' ? '/assets/none.css' : '/assets/card.css'
    )

    it('should write each produced file beside the pages, once however many write it', async () => {
      const { result, dir } = await built(files, { plugins: [byContent] })

      expect(result.diagnostics).toEqual([])
      // In the order first written: `posts/a.md` collected the card's styles, and the two
      // pages after it collected nothing, alike.
      expect(result.produced).toEqual(['assets/card.css', 'assets/none.css'])
      const written = tree(join(dir, 'dist'))
      expect(written['assets/card.css']).toBe('.card {}')
      expect(written['assets/none.css']).toBe('')
      expect(written['posts/b/index.html']).toContain('<link href="/assets/none.css">')
    })

    it('should refuse other content where another page produced a file', async () => {
      // Contents of one length, so that only the bytes tell them apart.
      const same = sheets(
        () => '/assets/s.css',
        (css) => (css === '' ? '.none {}' : css)
      )
      const errors = await refused(files, { plugins: [same] })

      // `posts/a.md` wrote it first; the two pages after it collected nothing.
      expect(errors).toEqual(
        ['posts/b.md', 'index.lem'].map((page) => ({
          path: 'layouts.lem',
          span: MARK,
          severity: 'error',
          message:
            `the \`sheets\` plugin writes a file for this mark on \`${page}\`, and the ` +
            '`sheets` plugin, for `posts/a.md`, writes other content to `assets/s.css`',
          labels: [
            {
              path: 'layouts.lem',
              span: MARK,
              message: 'the first to write there, as `assets/s.css`',
            },
          ],
        }))
      )
    })

    it('should refuse a produced url that names no file', async () => {
      const errors = await refused(files, { plugins: [sheets(() => '/assets/../s.css')] })

      expect(errors).toHaveLength(3)
      expect(errors[0]).toEqual({
        path: 'layouts.lem',
        span: MARK,
        severity: 'error',
        message:
          'the `sheets` plugin writes a file for this mark on `posts/a.md` at the url ' +
          '"/assets/../s.css", which names no file in the output directory: a file\'s url ' +
          'is a path from `/`, without an empty, `.` or `..` segment, a percent-encoded `/` ' +
          'or `\\`, a query, or a fragment',
        labels: [],
      })
    })

    it('should refuse a produced file where a passthrough file is, or would be but for case', async () => {
      const lead = 'the `sheets` plugin writes a file for this mark on `posts/a.md`, and '
      for (const [url, message] of [
        [
          '/assets/site.css',
          '`assets/site.css` is written by the passthrough file `assets/site.css` already',
        ],
        [
          '/Assets/site.css',
          '`Assets/site.css` differs from `assets/site.css`, which the passthrough file ' +
            '`assets/site.css` writes, only in case or Unicode form, and a file system ' +
            'may hold the two as one file',
        ],
      ]) {
        const errors = await refused(files, { plugins: [sheets(() => url)] })

        expect(errors[0]).toEqual({
          path: 'layouts.lem',
          span: MARK,
          severity: 'error',
          message: `${lead}${message}`,
          labels: [
            {
              path: 'assets/site.css',
              span: START,
              message: 'the first to write there, as `assets/site.css`',
            },
          ],
        })
      }
    })

    it('should refuse a producer writing one file twice with different content', async () => {
      const errors = await refused(files, {
        plugins: [writing(['/s.css', '.a {}'], ['/s.css', '.b {}'])],
      })

      expect(errors[0]).toEqual({
        path: 'layouts.lem',
        span: MARK,
        severity: 'error',
        message:
          'the `writing` plugin writes `s.css` twice with different content for this mark ' +
          'on `posts/a.md`',
        labels: [],
      })
    })

    it('should refuse two produced files that differ only in case, though alike', async () => {
      const errors = await refused(files, {
        plugins: [writing(['/s.css', '.a {}'], ['/S.css', '.a {}'])],
      })

      expect(errors[0]).toEqual({
        path: 'layouts.lem',
        span: MARK,
        severity: 'error',
        message:
          'the `writing` plugin writes a file for this mark on `posts/a.md`, and `S.css` ' +
          'differs from `s.css`, which the `writing` plugin, for `posts/a.md`, writes, only ' +
          'in case or Unicode form, and a file system may hold the two as one file',
        labels: [
          {
            path: 'layouts.lem',
            span: MARK,
            message: 'the first to write there, as `s.css`',
          },
        ],
      })
    })

    it('should refuse a produced file where a page is written, either way round', async () => {
      // The page's own file is claimed before its producers write; a page rendered
      // later meets the file another page's producer wrote.
      const own = await refused(files, { plugins: [sheets(() => '/posts/a/index.html')] })
      expect(own[0]).toMatchObject({
        path: 'layouts.lem',
        span: MARK,
        message:
          'the `sheets` plugin writes a file for this mark on `posts/a.md`, and ' +
          '`posts/a/index.html` is written by `posts/a.md` already',
        labels: [expect.objectContaining({ path: 'posts/a.md' })],
      })

      const loose = SITE['notes/loose.md']
      const atLoose = sheets((css) => (css === '' ? '/assets/none.css' : '/notes/loose/'))
      const later = await refused(files, { plugins: [atLoose] })
      expect(later).toEqual([
        {
          path: 'notes/loose.md',
          span: at(loose, 'title: Loose'),
          severity: 'error',
          message:
            '`notes/loose/index.html` is written by the `sheets` plugin, for `posts/a.md`, ' +
            'already',
          labels: [
            {
              path: 'layouts.lem',
              span: MARK,
              message: 'the first to write there, as `notes/loose/index.html`',
            },
          ],
        },
      ])
    })

    it('should drop what a producer that fails wrote, and say nothing of it', async () => {
      const failing: Plugin = {
        name: 'failing',
        marks: [
          {
            name: 'styles',
            produce: ({ write }) => {
              write('/kept.css', '.a {}')
              write('/../nowhere', '')
              throw new Error('no bundle')
            },
          },
        ],
      }
      const { result, dir } = await built(files, { plugins: [failing] })

      expect(result.diagnostics.map((d) => [d.severity, d.message])).toEqual(
        ['posts/a.md', 'posts/b.md', 'index.lem'].map((page) => [
          'warning',
          `the \`failing\` plugin's producer for \`styles\` failed on \`${page}\`, so the ` +
            'marks it answers keep their default content: it threw Error: no bundle',
        ])
      )
      expect(result.produced).toEqual([])
      expect(existsSync(join(dir, 'dist/kept.css'))).toBe(false)
    })
  })

  describe('the default plugin', () => {
    // The base layout places all three names the bundler produces; the two posts call
    // one styled component, and the home page that one and another.
    const layouts = LAYOUTS_LEM.replace(
      '<main>',
      '<lem.mark("styles")>\n  <lem.mark("head")>\n  <main>'
    ).replace('</main>', '</main>\n  <lem.mark("scripts")>')
    const card =
      '<lem-fn card()>\n  <p class="card">card</p>\n  <lem-style>\n    .card {\n' +
      '      color: red;\n    }\n  </lem-style>\n  <lem-head>\n' +
      '    <meta name="card" content="1">\n  </lem-head>\n</lem-fn>\n'
    const badge =
      '<lem-fn badge()>\n  <lem-style>\n    .badge {}\n  </lem-style>\n</lem-fn>\n'
    const files = {
      ...SITE,
      'layouts.lem': layouts,
      'components.lem': `${card}\n${badge}`,
      'posts/a.md': `${SITE['posts/a.md']}\n<lem.card()>\n`,
      'posts/b.md': `${SITE['posts/b.md']}\n<lem.card()>\n`,
      'index.lem': `${SITE['index.lem']}<lem.card()>\n<lem.badge()>\n`,
    }
    // Each name computed apart from the code, with node:crypto by hand.
    const CARD = 'assets/immutable/bundle/yksIFyWM8s.css'
    const BOTH = 'assets/immutable/bundle/2QUgyN6hdD.css'

    it('should write a page’s collected CSS as a hashed bundle its `mark("styles")` links', async () => {
      const { result, dir } = await built(files, { plugins: [bundler()] })

      // The claims cover every mark the layout makes, so nothing is warned at.
      expect(result.diagnostics).toEqual([])
      const written = tree(join(dir, 'dist'))
      expect(written[CARD]).toBe('.card {\n  color: red;\n}')
      expect(written['posts/a/index.html']).toContain(
        `<link rel="stylesheet" href="/${CARD}">\n<meta name="card" content="1">\n<main>`
      )
    })

    it('should share one bundle between pages with the same styles, and write two for two', async () => {
      const { result, dir } = await built(files, { plugins: [bundler()] })

      // In the order first written: by `posts/a.md`, which `posts/b.md` shares, and then
      // by the home page.
      expect(result.produced).toEqual([CARD, BOTH])
      const written = tree(join(dir, 'dist'))
      expect(written[BOTH]).toBe('.card {\n  color: red;\n}\n.badge {}')
      expect(written['posts/b/index.html']).toContain(`href="/${CARD}"`)
      expect(written['index.html']).toContain(`href="/${BOTH}"`)
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

  describe('the texts it read', () => {
    it('should carry every page, template and data file by its path, and its own declarations', async () => {
      const { result } = await built(SITE)
      const read = (path: string): [string, string] => [path, SITE[path]]

      // Not the stylesheet a passthrough glob copies, nor the component the walk
      // ignores: no step reads either.
      expect(result.texts).toEqual(
        new Map([
          read('site.lem'),
          read('layouts.lem'),
          read('posts/a.md'),
          read('posts/b.md'),
          read('index.lem'),
          read('notes/loose.md'),
          read('_data/photos.json'),
          [AMBIENT_FILE, readFileSync(AMBIENT_FILE, 'utf8')],
        ])
      )
    })

    it('should carry what it read before an error stopped it', async () => {
      const post = '---\ntitle: a: b\n---\n'
      const photos = '{"sunset": {}, "sunset": {}}'

      const early = await built({ ...SITE, 'posts/c.md': post })
      const late = await built({ ...SITE, '_data/photos.json': photos })

      expect(early.result.texts.get('posts/c.md')).toBe(post)
      // Nothing compiled, so nothing else was read.
      expect(early.result.texts.has(AMBIENT_FILE)).toBe(false)
      expect(late.result.texts.get('_data/photos.json')).toBe(photos)
    })

    it('should be the texts the spans were computed against, though a file changes during the build', async () => {
      const post = '---\ntitle: B\n---\n<lem.mark("edit")>\n'
      const dir = project({ ...SITE, 'posts/b.md': post }, PASSTHROUGH)
      const file = join(dir, 'www/posts/b.md')
      // A producer rewrites the page it answers for while the build renders it.
      const editor: Plugin = {
        name: 'editor',
        marks: [
          {
            name: 'edit',
            produce: ({ marks }) => {
              writeFileSync(file, 'changed since')
              return marks.map(() => '')
            },
          },
        ],
      }

      const result = await build(configOf(dir, PASSTHROUGH, [editor]), {
        now: NOW,
        env: ENV,
      })

      expect(result.diagnostics).toEqual([])
      expect(readFileSync(file, 'utf8')).toBe('changed since')
      expect(result.texts.get('posts/b.md')).toBe(post)
    })
  })
})

describe('check()', () => {
  /** Checks `files` as a project: the result, and the project. */
  async function checked(files: Record<string, string>, passthrough = PASSTHROUGH) {
    const dir = project(files, passthrough)
    const config = configOf(dir, passthrough)
    return { result: await check(config, { now: NOW, env: ENV }), dir }
  }

  it('should find what a build would write, and write nothing', async () => {
    const { result, dir } = await checked(SITE)
    const { result: wrote } = await built(SITE)

    expect(result).toEqual({ ...wrote, texts: expect.any(Map) as Map<string, string> })
    expect(result.texts).toEqual(wrote.texts)
    expect(existsSync(join(dir, 'dist'))).toBe(false)
  })

  it('should render every page, finding what only rendering finds', async () => {
    const post = '---\ntitle: B\n---\n<lem.site.photos["nope"].caption>\n'
    const css = '---\nurl: /assets/site.css\n---\n'
    const { result, dir } = await checked({
      ...SITE,
      'posts/b.md': post,
      'notes/loose.md': css,
    })

    expect(result.pages).toEqual([])
    expect(result.diagnostics).toEqual([
      expect.objectContaining({ path: 'posts/b.md', message: 'no key "nope"' }),
      expect.objectContaining({
        path: 'notes/loose.md',
        span: at(css, 'url: /assets/site.css'),
      }),
    ])
    expect(existsSync(join(dir, 'dist'))).toBe(false)
  })

  it('should leave the output as it is, though a build could not write over it', async () => {
    const dir = project(SITE, PASSTHROUGH)
    mkdirSync(join(dir, 'dist'))
    writeFileSync(join(dir, 'dist/posts'), 'stale')

    const result = await check(configOf(dir, PASSTHROUGH), { now: NOW, env: ENV })

    expect(result.pages).toHaveLength(4)
    expect(result.diagnostics).toEqual([])
    expect(tree(join(dir, 'dist'))).toEqual({ posts: 'stale' })
  })
})

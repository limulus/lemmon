import { loadCore, type Diagnostic } from '@limulus/lemmon-core'
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'

import { build, check, type BuildResult } from './build.ts'
import { bundler } from './bundler.ts'
import type { Plugin, PluginHandler } from './plugins.ts'
import { AMBIENT_FILE, FILE_START as START, SiteError } from './program.ts'
import { WriteError } from './write.ts'
import {
  appender,
  appending,
  CALLOUT_LEM,
  callouts,
  copy,
  inline,
  shipping,
  tsHighlighter,
  wrapping,
} from '../mocks/plugins.ts'
import {
  configOf,
  ENV,
  GENERATING,
  LAYOUTS_LEM,
  PASSTHROUGH,
  project,
  SITE,
  SITE_LEM,
  TOPIC_LEM,
  tree,
} from '../mocks/sites.ts'
import { at } from '../mocks/spans.ts'

const NOW = new Date('2026-10-15T12:00:00Z')

/**
 * How {@link built} builds: in what environment, copying what, with which plugins, for
 * which origin, whose host answers which paths without a file.
 */
interface Options {
  env?: object
  passthrough?: string[]
  plugins?: Plugin[]
  origin?: string
  served?: string[]
}

/** Builds `files` as a project, in `env`, returning the result and the project. */
async function built(
  files: Record<string, string>,
  { env = ENV, passthrough = PASSTHROUGH, plugins = [], origin, served = [] }: Options = {}
): Promise<{ result: BuildResult; dir: string }> {
  const dir = project(files, passthrough)
  const config = { ...configOf(dir, passthrough, plugins), served }
  const result = await build(origin === undefined ? config : { ...config, origin }, {
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
        assets: [],
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
          '---\ntype: Home\ntitle: Café ’\nurl: /feed.txt\n---\n<lem.page.title>\n',
        'posts/a b.md': '---\ntitle: Café\n---\n',
        'notes/a b.md': 'Café\n',
      })

      const written = tree(join(dir, 'dist'))
      expect(written['feed.txt']).toContain('Café ’')
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

  describe('a generator', () => {
    const topics = GENERATING['topics.lem']
    const entry = at(topics, 'topic: site.topics')

    it('should write a page per item at its type’s url, each linking to its neighbours', async () => {
      const { result, dir } = await built(GENERATING)

      expect(result.diagnostics).toEqual([])
      // Where the generator stands in the plan — after the glob's pages and the home
      // page, in path order — in its list's order.
      expect(result.pages).toEqual([
        'posts/a/index.html',
        'posts/b/index.html',
        'index.html',
        'tils/git/index.html',
        'tils/nodejs/index.html',
        'tils/rust/index.html',
        'notes/loose/index.html',
      ])
      const written = tree(join(dir, 'dist/tils'))
      const links = (file: string) =>
        [...written[file].matchAll(/<a rel="(\w+)" href="([^"]+)">([^<]+)</g)].map(
          ([, rel, href, title]) => [rel, href, title]
        )
      expect(written['git/index.html']).toMatch(/^<title>TILs: git · hi<\/title>/)
      expect(links('git/index.html')).toEqual([['next', '/tils/nodejs/', 'TILs: nodejs']])
      expect(written['nodejs/index.html']).toMatch(/^<title>TILs: nodejs · hi<\/title>/)
      expect(links('nodejs/index.html')).toEqual([
        ['prev', '/tils/git/', 'TILs: git'],
        ['next', '/tils/rust/', 'TILs: rust'],
      ])
      expect(links('rust/index.html')).toEqual([['prev', '/tils/nodejs/', 'TILs: nodejs']])
    })

    it('should label a render fault with the item it was made from, for that page alone', async () => {
      // Only `nodejs` is six letters, and only its page faults.
      const faulting = topics.replace(
        '<lem-if page.prev>',
        '<lem.["git", "rust"][page.topic.length() - 3]>\n<lem-if page.prev>'
      )
      const errors = await refused({ ...GENERATING, 'topics.lem': faulting })

      expect(errors).toEqual([
        {
          path: 'topics.lem',
          span: at(faulting, '["git", "rust"][page.topic.length() - 3]'),
          severity: 'error',
          message: 'index 3 is out of range for a list of 2',
          labels: [
            {
              path: 'topics.lem',
              span: at(faulting, 'topic: site.topics'),
              message: 'rendering `topics.lem` for `site.topics[1]`',
            },
          ],
        },
      ])
    })

    it('should name each page by its item where its handlers warn', async () => {
      // The layout places no `mark("styles")`, so what each page collects is unwritten.
      const styled = `${topics}<lem-style>a { color: red }</lem-style>\n`
      const { result } = await built({ ...GENERATING, 'topics.lem': styled })

      expect(result.diagnostics.map((diagnostic) => diagnostic.message)).toEqual(
        [0, 1, 2].map(
          (index) =>
            `\`topics.lem\` for \`site.topics[${index}]\` collects \`styles\` here, and ` +
            'nothing on that page places `mark("styles")`, so what it collected is not written'
        )
      )
    })

    it('should refuse two of its pages at files a file system may hold as one', async () => {
      // `/tils/A/` and `/tils/a/` are two urls, so the load lets them stand; their files
      // fold to one, and the second is refused at the generator's key, naming each.
      const site = GENERATING['site.lem'].replace('["git", "nodejs", "rust"]', '["A", "a"]')
      const errors = await refused({ ...GENERATING, 'site.lem': site })

      expect(errors).toEqual([
        {
          path: 'topics.lem',
          span: entry,
          severity: 'error',
          message:
            '`topics.lem` for `site.topics[1]` is at the url "/tils/a/", and ' +
            '`tils/a/index.html` differs from `tils/A/index.html`, which `topics.lem` for ' +
            '`site.topics[0]` writes, only in case or Unicode form, and a file system may ' +
            'hold the two as one file',
          labels: [
            {
              path: 'topics.lem',
              span: entry,
              message: 'the first to write there, as `tils/A/index.html`',
            },
          ],
        },
      ])
    })

    it('should name the page whose url names no file by its item', async () => {
      const site = GENERATING['site.lem'].replace(
        '["git", "nodejs", "rust"]',
        '["git", ".."]'
      )
      const errors = await refused({ ...GENERATING, 'site.lem': site })

      expect(errors).toEqual([
        expect.objectContaining({
          path: 'topics.lem',
          span: entry,
          message: expect.stringMatching(
            /^the url "\/tils\/..\/" of `topics.lem` for `site.topics\[1\]` names no file/
          ) as string,
        }),
      ])
    })

    describe('and the plugins that resolve its marks', () => {
      // Every page places one `probe` mark, which these plugins answer.
      const probed = { ...GENERATING, 'topics.lem': `${topics}<lem.mark("probe")>\n` }
      const MARK = at(probed['topics.lem'], 'mark("probe")')
      const decoder = new TextDecoder()

      /** A plugin whose `probe` producer is `produce`. */
      const probing = (produce: NonNullable<PluginHandler['produce']>): Plugin => ({
        name: 'probing',
        marks: [{ name: 'probe', produce }],
      })

      it('should name the page a producer failed on by its item', async () => {
        const plugin = probing(({ marks, page }) => {
          if (decoder.decode(page).includes('<title>TILs: nodejs')) throw new Error('no')
          return marks.map(() => '')
        })
        const { result } = await built(probed, { plugins: [plugin] })

        expect(result.diagnostics).toEqual([
          expect.objectContaining({
            path: 'topics.lem',
            span: MARK,
            severity: 'warning',
            message: expect.stringMatching(
              /^the `probing` plugin's producer for `probe` failed on `topics.lem` for `site.topics\[1\]`, /
            ) as string,
          }),
        ])
      })

      it('should name each page whose producer wrote another file at one url', async () => {
        const plugin = probing(({ marks, page, write }) => {
          write('/probe.txt', decoder.decode(page).slice(0, 20))
          return marks.map(() => '')
        })
        const errors = await refused(probed, { plugins: [plugin] })

        expect(errors.map((error) => error.message)).toEqual(
          [1, 2].map(
            (index) =>
              'the `probing` plugin writes a file for this mark on `topics.lem` for ' +
              `\`site.topics[${index}]\`, and the \`probing\` plugin, for \`topics.lem\` for ` +
              '`site.topics[0]`, writes other content to `probe.txt`'
          )
        )
      })

      it('should name the page whose producer misused `write` by its item', async () => {
        const plugin = probing(({ marks, write }) => {
          write(3 as unknown as string, 'x')
          return marks.map(() => '')
        })
        const errors = await refused(probed, { plugins: [plugin] })

        expect(errors.map((error) => error.message)).toEqual(
          [0, 1, 2].map(
            (index) =>
              "the `probing` plugin's producer for `probe` called `write` on `topics.lem` " +
              `for \`site.topics[${index}]\` with 3 and a string, where it takes a url and a ` +
              'string or bytes'
          )
        )
      })
    })

    it('should refuse one a glob holds, at its key, as the core loads it', async () => {
      const site = `${GENERATING['site.lem']}<lem-let sections: [Home] from "sections/*.lem" />\n`
      const held = '---\nfor:\n  title: site.topics\n---\n<p><lem.page.title></p>\n'
      const errors = await refused({
        ...GENERATING,
        'site.lem': site,
        'sections/t.lem': held,
      })

      expect(errors).toEqual([
        {
          path: 'sections/t.lem',
          span: at(held, 'title: site.topics'),
          severity: 'error',
          message:
            '`for:` makes this page a generator, whose pages are made once every binding ' +
            'has loaded, so no `from` glob can hold it',
          labels: [
            expect.objectContaining({
              path: 'site.lem',
              message: 'the glob of `sections` matches it',
            }),
          ],
        },
      ])
    })

    it('should refuse one no `type:` names, which has no type to make its pages of', async () => {
      const untyped = topics.replace('type: Topic\n', '')
      const errors = await refused({ ...GENERATING, 'topics.lem': untyped })

      expect(errors).toEqual([
        expect.objectContaining({
          path: 'topics.lem',
          span: at(untyped, 'topic: site.topics'),
          message:
            '`for:` makes this page a generator, and a generator names the type of its ' +
            'pages with `type:`',
        }),
      ])
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
      const { result, html } = await postA([copy, inline, tsHighlighter, frame])

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

      // Once as the body pass resolves the post's body, and again on its page (D46).
      const payload = { language: 'ts', info: 'ts {1}', code: 'let x = 1\n' }
      const produced = {
        payload,
        payloadId: expect.stringMatching(/^[0-9a-f]{16}$/) as string,
      }
      expect(seen).toEqual([produced, { payload }, produced, { payload }])
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
        { plugins: [inline, tsHighlighter, callouts] }
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
      const { result, html } = await postA([inline, tsHighlighter, throwing, copy])

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
          { plugins: [inline, tsHighlighter] }
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
        { plugins: [inline, tsHighlighter] }
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

  describe('the body pass (D46)', () => {
    /**
     * A page template that prints, for each post, the post's body as `build.bodies` holds
     * it: one item per line, at `/feed/`.
     */
    const FEED =
      '---\ntype: Home\ntitle: Feed\nurl: /feed/\n---\n<lem-for p in site.posts>\n' +
      '<item><lem.build.bodies[p.url.text()]></item>\n</lem-for>\n'

    /** What a build of `files` wrote at `file`, with what it reported. */
    async function wrote(
      files: Record<string, string>,
      file: string,
      options: Options = {}
    ) {
      const { result, dir } = await built(files, options)
      return { result, text: tree(join(dir, 'dist'))[file] }
    }

    it('should give a template each document’s finished body by its url, escaped once', async () => {
      const post = '---\ntitle: A\n---\nHello *a*.\n\n```ts\nlet x = 1\n```\n'
      const { result, text } = await wrote(
        { ...SITE, 'posts/a.md': post, 'feed.lem': FEED },
        'feed/index.html',
        { plugins: [tsHighlighter] }
      )

      expect(result.diagnostics).toEqual([])
      // Its code as the site's highlighter wrote it, and a string printed as text.
      expect(text).toContain(
        '<item>&lt;p&gt;Hello &lt;em&gt;a&lt;/em&gt;.&lt;/p&gt;\n' +
          '&lt;pre class=&quot;ts&quot;&gt;let x = 1&lt;/pre&gt;\n</item>'
      )
      expect(text).toContain('<item>&lt;p&gt;Hello b.&lt;/p&gt;\n</item>')
    })

    it('should key a body by its url as `url.text()` writes it, encoded', async () => {
      const keys =
        '---\ntype: Home\ntitle: Keys\nurl: /keys/\n---\n' +
        '<lem.build.bodies.keys().join(" ")>\n'
      const { result, text } = await wrote(
        {
          ...SITE,
          'posts/café.md': '---\ntitle: Café\n---\nBonjour.\n',
          'feed.lem': FEED,
          'keys.lem': keys,
        },
        'feed/index.html'
      )

      expect(result.diagnostics).toEqual([])
      expect(text).toContain('<item>&lt;p&gt;Bonjour.&lt;/p&gt;\n</item>')
      const { text: listed } = await wrote(
        { ...SITE, 'posts/café.md': '---\ntitle: Café\n---\n', 'keys.lem': keys },
        'keys/index.html'
      )
      expect(listed).toContain('/posts/a/ /posts/b/ /posts/caf%C3%A9/')
    })

    it('should give a layout every body, its own page’s among them', async () => {
      const layouts = LAYOUTS_LEM.replace(
        '<lem.body>\n  </lem-let>',
        '<lem.body>\n    <lem.build.bodies[p.url.text()]>\n  </lem-let>'
      )
      expect(layouts).not.toBe(LAYOUTS_LEM)
      const { result, text } = await wrote(
        { ...SITE, 'layouts.lem': layouts },
        'posts/b/index.html'
      )

      expect(result.diagnostics).toEqual([])
      expect(text).toContain('<p>Hello b.</p>\n\n&lt;p&gt;Hello b.&lt;/p&gt;\n')
    })

    it('should stop at a document that reads a body, where none exists yet, before any page renders', async () => {
      const post = '---\ntitle: B\n---\n<lem.build.bodies["/posts/a/"]>\n'
      // Each post's body beside the next one's, which holds for every body the pass
      // renders, and would fault on the one it could not.
      const site =
        `${SITE_LEM}<lem-let pairs: [string] = build.bodies.keys()` +
        '.map(|k| build.bodies[k.replace("/a/", "/b/")]) />\n'
      const errors = await refused({ ...SITE, 'site.lem': site, 'posts/b.md': post })

      // Once, from the body pass: no load or page runs on to report what it left out.
      expect(errors).toEqual([
        {
          path: 'posts/b.md',
          span: at(post, 'build.bodies["/posts/a/"]'),
          severity: 'error',
          message: 'no key "/posts/a/"',
          labels: [
            {
              path: 'posts/b.md',
              span: at(post, 'title: B'),
              message: 'rendering the body of `posts/b.md`, before any body exists',
            },
          ],
        },
      ])
    })

    it('should give an untyped page every body, its reload carrying them', async () => {
      const loose = '---\ntitle: Loose\n---\n<lem.build.bodies["/posts/b/"]>\n'
      const { result, text } = await wrote(
        { ...SITE, 'notes/loose.md': loose },
        'notes/loose/index.html'
      )

      expect(result.diagnostics).toEqual([])
      expect(text).toBe('&lt;p&gt;Hello b.&lt;/p&gt;\n\n')
    })

    describe('a body’s links', () => {
      const post =
        '---\ntitle: A\n---\nSee [the other](../b/#top), [a note](#fn), and ' +
        '[home](/).\n\n<p id="fn">A note.</p>\n'
      const files = { ...SITE, 'posts/a.md': post, 'feed.lem': FEED }
      const RELATIVE =
        '<a href="../b/#top">the other</a>, <a href="#fn">a note</a>, and ' +
        '<a href="/">home</a>'

      it('should be absolute against the site’s origin in the body, and as written on its page', async () => {
        const options = { origin: 'https://example.com' }
        const { result, text } = await wrote(files, 'feed/index.html', options)
        const { text: page } = await wrote(files, 'posts/a/index.html', options)

        expect(result.diagnostics).toEqual([])
        expect(text).toContain(
          '&lt;a href=&quot;https://example.com/posts/b/#top&quot;&gt;the other&lt;/a&gt;, ' +
            '&lt;a href=&quot;https://example.com/posts/a/#fn&quot;&gt;a note&lt;/a&gt;, ' +
            'and &lt;a href=&quot;https://example.com/&quot;&gt;'
        )
        expect(page).toContain(RELATIVE)
      })

      it('should be paths from `/` in the body where the site names no origin', async () => {
        const { result, text } = await wrote(files, 'feed/index.html')

        expect(result.diagnostics).toEqual([])
        expect(text).toContain(
          '&lt;a href=&quot;/posts/b/#top&quot;&gt;the other&lt;/a&gt;, ' +
            '&lt;a href=&quot;/posts/a/#fn&quot;&gt;a note&lt;/a&gt;, ' +
            'and &lt;a href=&quot;/&quot;&gt;'
        )
      })

      it('should keep a link the URL parser refuses as written, warning at its document', async () => {
        const broken =
          '---\ntitle: A\n---\n<a href="http://exa mple.com/">x</a> <a href="../b/">b</a>\n'
        const files = { ...SITE, 'posts/a.md': broken, 'feed.lem': FEED }
        const { result, text } = await wrote(files, 'feed/index.html', {
          origin: 'https://ex.com',
        })

        expect(result.diagnostics).toEqual([
          {
            path: 'posts/a.md',
            span: at(broken, 'title: A'),
            severity: 'warning',
            message:
              'the body of `posts/a.md` links to "http://exa mple.com/" in `<a href>`, ' +
              "which the URL parser refuses even against its page's url, so the body keeps " +
              'it as written',
            labels: [],
          },
        ])
        // The rest of the body rebased all the same, and every page written.
        expect(text).toContain(
          '&lt;a href=&quot;http://exa mple.com/&quot;&gt;x&lt;/a&gt; ' +
            '&lt;a href=&quot;https://ex.com/posts/b/&quot;&gt;'
        )
        expect(result.pages).toHaveLength(5)
      })

      it('should warn at a `srcset` candidate the rebase drops, and at none it keeps', async () => {
        const post = (srcset: string) => `---\ntitle: A\n---\n<img srcset="${srcset}">\n`
        const dropping = post('../../assets/site.css 1x, b.jpg 2q')
        const { result } = await built({ ...SITE, 'posts/a.md': dropping })
        const kept = post('https://x/a.jpg 1x, https://x/b.jpg 2q')
        const { result: left } = await built({ ...SITE, 'posts/a.md': kept })

        expect(result.diagnostics).toEqual([
          {
            path: 'posts/a.md',
            span: at(dropping, 'title: A'),
            severity: 'warning',
            message:
              'the body of `posts/a.md` holds `<img srcset>` as "../../assets/site.css 1x, ' +
              'b.jpg 2q", whose descriptor `2q` does not parse, so its candidate is dropped ' +
              'from the body as the others are rebased, as Eleventy drops it',
            labels: [],
          },
        ])
        expect(left.diagnostics).toEqual([])
      })

      it('should leave a document whose url names no file to its page, which reports it', async () => {
        const post = '---\ntitle: A\nurl: "a:b/"\n---\n[y](./y)\n'
        const { result } = await built({ ...SITE, 'posts/a.md': post })

        // No word from the body pass, whose links have no page to resolve against.
        expect(result.pages).toEqual([])
        expect(result.diagnostics).toEqual([
          expect.objectContaining({
            path: 'posts/a.md',
            span: at(post, 'url: "a:b/"'),
            message: expect.stringMatching(/^the url "a:b\/" names no file/) as string,
          }),
        ])
      })
    })

    describe('a computed binding over `build.bodies`', () => {
      it('should run at both loads, seeing none in the body pass and every one after', async () => {
        const site = `${SITE_LEM}<lem-let bodyCount: int = build.bodies.keys().length() />\n`
        const post = '---\ntitle: B\n---\n<lem.site.bodyCount>\n'
        const { result, text } = await wrote(
          { ...SITE, 'site.lem': site, 'posts/b.md': post, 'feed.lem': FEED },
          'feed/index.html'
        )
        const { text: page } = await wrote(
          { ...SITE, 'site.lem': site, 'posts/b.md': post },
          'posts/b/index.html'
        )

        expect(result.diagnostics).toEqual([])
        // The body of `posts/b.md` as the body pass rendered it, and then its page.
        expect(text).toContain('<item>0\n</item>')
        expect(page).toContain('<main><h1>B</h1>\n\n2\n</main>')
      })

      it('should fail the first load where it indexes one, before any body exists', async () => {
        const site = `${SITE_LEM}<lem-let first: string = build.bodies["/posts/a/"] />\n`
        const errors = await refused({ ...SITE, 'site.lem': site })

        expect(errors).toEqual([
          {
            path: 'site.lem',
            span: at(site, 'build.bodies["/posts/a/"]'),
            severity: 'error',
            message: 'no key "/posts/a/"',
            labels: [
              {
                path: 'site.lem',
                span: at(site, '«first»: string'),
                message: 'computing `first`',
              },
            ],
          },
        ])
      })
      it('should stop where it fails once the bodies are bound, before any page renders', async () => {
        const site =
          `${SITE_LEM}<lem-let misread: [string] = build.bodies.keys()` +
          '.map(|k| build.bodies[k + "/"]) />\n'
        const errors = await refused({ ...SITE, 'site.lem': site })

        expect(errors).toEqual([
          expect.objectContaining({
            path: 'site.lem',
            span: at(site, 'build.bodies[k + "/"]'),
            message: 'no key "/posts/a//"',
          }),
        ])
      })
    })

    describe('resolving a body', () => {
      // `posts/a.md` places a mark of its own, which its body and its page both resolve.
      const post = '---\ntitle: A\n---\n<lem.mark("styles")>\n'
      const files = { ...SITE, 'posts/a.md': post }
      const MARK = at(post, 'mark("styles")')

      /** A plugin whose `styles` producer writes `content(page)` to `/stamp.txt`. */
      const stamping = (content: (page: Uint8Array) => unknown): Plugin => ({
        name: 'stamping',
        marks: [
          {
            name: 'styles',
            produce: ({ marks, page, write }) => {
              write('/stamp.txt', content(page) as string)
              return marks.map(() => '')
            },
          },
        ],
      })

      it('should tell each handler whether it answers a body or a page', async () => {
        const seen: [string, boolean][] = []
        const telling: Plugin = {
          name: 'telling',
          marks: [
            {
              name: 'styles',
              produce: ({ marks, body }) => {
                seen.push(['produce', body])
                return marks.map(() => '\uFEFF<link>')
              },
              decorate: (_, content, { body }) => {
                seen.push(['decorate', body])
                return content
              },
            },
          ],
        }
        const { result, text } = await wrote(
          { ...files, 'feed.lem': FEED },
          'feed/index.html',
          {
            plugins: [telling],
          }
        )

        expect(result.diagnostics).toEqual([])
        // The body pass's first, then the page's.
        expect(seen).toEqual([
          ['produce', true],
          ['decorate', true],
          ['produce', false],
          ['decorate', false],
        ])
        // A body starting with U+FEFF keeps it: it is the body's, not a byte order mark.
        expect(text).toContain('<item>\uFEFF&lt;link&gt;\n</item>')
      })

      it('should write a file its body and its page write alike once', async () => {
        const { result, dir } = await built(files, { plugins: [stamping(() => 'same')] })

        expect(result.diagnostics).toEqual([])
        expect(result.produced).toEqual(['stamp.txt'])
        expect(tree(join(dir, 'dist'))['stamp.txt']).toBe('same')
      })

      it('should refuse other content where its body wrote, naming the body', async () => {
        const errors = await refused(files, {
          plugins: [stamping((page) => String(page.length))],
        })

        expect(errors).toEqual([
          {
            path: 'posts/a.md',
            span: MARK,
            severity: 'error',
            message:
              'the `stamping` plugin writes a file for this mark on `posts/a.md`, and the ' +
              '`stamping` plugin, for the body of `posts/a.md`, writes other content to ' +
              '`stamp.txt`',
            labels: [
              {
                path: 'posts/a.md',
                span: MARK,
                message: 'the first to write there, as `stamp.txt`',
              },
            ],
          },
        ])
      })

      it('should report a producer’s misuse of `write` on a body, and stop before any page', async () => {
        const errors = await refused(files, { plugins: [stamping(() => 3)] })

        expect(errors).toEqual([
          expect.objectContaining({
            path: 'posts/a.md',
            span: MARK,
            message:
              "the `stamping` plugin's producer for `styles` called `write` on the body of " +
              '`posts/a.md` with a string and 3, where it takes a url and a string or bytes',
          }),
        ])
      })

      it('should warn at a handler that fails on a body once, on its page', async () => {
        const failing: Plugin = {
          name: 'failing',
          marks: [
            {
              name: 'styles',
              produce: () => {
                throw new Error('no bundle')
              },
            },
          ],
        }
        const { result } = await built(files, { plugins: [failing] })

        expect(result.diagnostics.map((d) => [d.path, d.message])).toEqual([
          [
            'posts/a.md',
            "the `failing` plugin's producer for `styles` failed on `posts/a.md`, so the " +
              'marks it answers keep their default content: it threw Error: no bundle',
          ],
        ])
      })

      it('should warn at what a body collects and nothing places once, on its page', async () => {
        const styled = '---\ntitle: A\n---\n<lem-style>\n  p {}\n</lem-style>\nHi.\n'
        const { result } = await built(
          { ...SITE, 'posts/a.md': styled },
          { plugins: [inline] }
        )
        const placing = LAYOUTS_LEM.replace('<main>', '<lem.mark("styles")>\n  <main>')
        const { result: placed } = await built(
          { ...SITE, 'posts/a.md': styled, 'layouts.lem': placing },
          { plugins: [inline] }
        )

        expect(result.diagnostics).toEqual([
          expect.objectContaining({
            path: 'posts/a.md',
            severity: 'warning',
            message:
              '`posts/a.md` collects `styles` here, and nothing on that page places ' +
              '`mark("styles")`, so what it collected is not written',
          }),
        ])
        // The body places nothing either, and is not warned at: its page places them.
        expect(placed.diagnostics).toEqual([])
      })
    })

    it('should build a site with no documents, which has no bodies', async () => {
      const { 'posts/a.md': _a, 'posts/b.md': _b, 'notes/loose.md': _n, ...rest } = SITE
      const { result, text } = await wrote({ ...rest, 'feed.lem': FEED }, 'index.html')

      expect(result.diagnostics).toEqual([])
      expect(result.pages).toEqual(['feed/index.html', 'index.html'])
      expect(text).toContain('<main>\n</main>')
    })
  })

  describe('document transforms', () => {
    it('should compile each document as the plugins’ transforms leave it', async () => {
      // A body ending without a newline, and a document that is all front matter, its
      // closing `---` the file's last line.
      const loose =
        '---\ntitle: Loose\nlinkUrl: https://example.com/\n---\nSee [the post][link].'
      const bare = '---\ntitle: Bare\nlinkUrl: https://example.com/\n---'
      const { result, dir } = await built(
        { ...SITE, 'notes/loose.md': loose, 'notes/bare.md': bare },
        { plugins: [appender] }
      )

      expect(result.diagnostics).toEqual([])
      const written = tree(join(dir, 'dist'))
      expect(written['notes/loose/index.html']).toBe(
        '<p>See <a href="https://example.com/">the post</a>.</p>\n'
      )
      expect(written['notes/bare/index.html']).toBe('')
      // What compiled is what a diagnostic would be framed against; a document the
      // transform leaves alone is as it was read.
      expect(result.texts.get('notes/loose.md')).toBe(
        `${loose}\n\n[link]: https://example.com/\n`
      )
      expect(result.texts.get('notes/bare.md')).toBe(
        `${bare}\n\n[link]: https://example.com/\n`
      )
      expect(result.texts.get('posts/a.md')).toBe(SITE['posts/a.md'])
    })

    it('should place a diagnostic in what a transform appended, in the text it compiled', async () => {
      const { result } = await built(SITE, { plugins: [appending('<lem.page.byline>\n')] })

      const text = result.texts.get('posts/b.md')!
      expect(text).toBe(`${SITE['posts/b.md']}<lem.page.byline>\n`)
      expect(result.diagnostics).toContainEqual(
        expect.objectContaining({
          path: 'posts/b.md',
          span: at(text, 'page.byline'),
          message: '`Post` has no field `byline`',
        })
      )
    })

    it('should stop at a transform that fails, before compiling', async () => {
      const failing: Plugin = {
        name: 'failing',
        transformDocument: ({ path }) => {
          if (path === 'posts/b.md') throw new Error('no link')
          return ''
        },
      }
      const { result, dir } = await built(SITE, { plugins: [failing] })

      expect(result.diagnostics).toEqual([
        {
          path: 'posts/b.md',
          span: at(SITE['posts/b.md'], 'title: B'),
          severity: 'error',
          message:
            "the `failing` plugin's `transformDocument` failed on this document: it " +
            'threw Error: no link',
          labels: [],
        },
      ])
      expect(result.texts.has(AMBIENT_FILE)).toBe(false)
      expect(existsSync(join(dir, 'dist'))).toBe(false)
    })
  })

  describe('asset hooks', () => {
    const layouts = LAYOUTS_LEM.replace(
      '<main>',
      '<script src="<lem.asset("lib/app.js")>"></script>\n  <main>'
    )
    const files = { ...SITE, 'layouts.lem': layouts }
    const BUILT = { 'lib/app.js': '/lib/app.1a2b.js' }
    const app = shipping({ manifest: BUILT, files: { '/lib/app.1a2b.js': 'run()\n' } })

    it('should resolve `asset()` against the manifest, and write the files with the pages', async () => {
      const { result, dir } = await built(files, { plugins: [app] })

      expect(result.diagnostics).toEqual([])
      expect(result.assets).toEqual(['lib/app.1a2b.js'])
      const written = tree(join(dir, 'dist'))
      expect(written['lib/app.1a2b.js']).toBe('run()\n')
      expect(written['posts/a/index.html']).toContain(
        '<script src="/lib/app.1a2b.js"></script>\n<main>'
      )
    })

    it('should resolve an `[asset]` field in front matter against the same manifest', async () => {
      const site = SITE_LEM.replace(
        '  title: string\n  layout: (Home',
        '  title: string\n  script: asset\n  layout: (Home'
      )
      const index =
        '---\ntype: Home\ntitle: Home\nurl: /\nscript: lib/app.js\n---\n<lem.page.script>\n'
      const { result, dir } = await built(
        { ...files, 'site.lem': site, 'index.lem': index },
        { plugins: [app] }
      )

      expect(result.diagnostics).toEqual([])
      expect(tree(join(dir, 'dist'))['index.html']).toContain(
        '<main>/lib/app.1a2b.js\n</main>'
      )
    })

    it('should declare the manifest to the core, which refuses a path it does not map', async () => {
      const typo = layouts.replace('lib/app.js', 'lib/ap.js')
      const errors = await refused({ ...files, 'layouts.lem': typo }, { plugins: [app] })

      expect(errors).toEqual([
        expect.objectContaining({
          path: 'layouts.lem',
          span: at(typo, '"lib/ap.js"'),
          message: 'the manifest has no asset `lib/ap.js`; did you mean `lib/app.js`?',
        }),
      ])
    })

    it('should tell each hook whether the build writes and its environment, and check without writing', async () => {
      const told: [boolean, string | undefined][] = []
      const telling: Plugin = {
        name: 'telling',
        assets: ({ writes, env }) => {
          told.push([writes, env.MOTTO])
          return { manifest: BUILT, files: { '/lib/app.1a2b.js': '' } }
        },
      }
      const dir = project(files, PASSTHROUGH)
      const config = configOf(dir, PASSTHROUGH, [telling])

      const checked = await check(config, { now: NOW, env: ENV })
      expect(checked.assets).toEqual(['lib/app.1a2b.js'])
      expect(existsSync(join(dir, 'dist'))).toBe(false)
      await build(config, { now: NOW, env: ENV })

      // The environment `env("MOTTO")` reads in `site.lem`, not the process's.
      expect(told).toEqual([
        [false, 'hi'],
        [true, 'hi'],
      ])
      expect(existsSync(join(dir, 'dist/lib/app.1a2b.js'))).toBe(true)
    })

    it('should run every hook again on each build of one config, as `lem dev` does', async () => {
      const dir = project({ ...files, 'version.txt': '1' }, PASSTHROUGH)
      const versioned: Plugin = {
        name: 'versioned',
        transformDocument: ({ body }) => `${body}Transformed.\n`,
        assets: ({ input }) => {
          const version = readFileSync(join(input, 'version.txt'), 'utf8')
          const url = `/lib/app.${version}.js`
          return { manifest: { 'lib/app.js': url }, files: { [url]: version } }
        },
      }
      const config = configOf(dir, PASSTHROUGH, [versioned])
      await build(config, { now: NOW, env: ENV })

      writeFileSync(join(dir, 'www/version.txt'), '2')
      writeFileSync(join(dir, 'www/posts/b.md'), '---\ntitle: B\n---\nEdited.\n')
      const second = await build(config, { now: NOW, env: ENV })

      expect(second.diagnostics).toEqual([])
      expect(second.assets).toEqual(['lib/app.2.js'])
      const written = tree(join(dir, 'dist'))
      expect(written['posts/b/index.html']).toContain('src="/lib/app.2.js"')
      expect(written['posts/b/index.html']).toContain('<p>Edited.\nTransformed.</p>')
    })

    it('should stop at a hook that fails, at the config’s start, before compiling', async () => {
      const failing: Plugin = {
        name: 'failing',
        assets: () => {
          throw new Error('no esbuild')
        },
      }
      const { result, dir } = await built(files, { plugins: [failing] })

      // Without the manifest, the layout's `asset()` would be an error too.
      expect(result.diagnostics).toEqual([
        {
          path: join(dir, 'lemmon.config.ts'),
          span: START,
          severity: 'error',
          message:
            "the `failing` plugin's `assets` hook failed: it threw Error: no esbuild",
          labels: [],
        },
      ])
      expect(result.texts.has(AMBIENT_FILE)).toBe(false)
    })

    describe('the files a hook builds', () => {
      /** The errors building {@link files} with `plugins` reports, with its config file. */
      async function refusedFiles(...plugins: Plugin[]) {
        const { result, dir } = await built(files, { plugins: [app, ...plugins] })
        expect(existsSync(join(dir, 'dist'))).toBe(false)
        return { errors: result.diagnostics, config: join(dir, 'lemmon.config.ts') }
      }

      it('should refuse a url that names no file, at the config’s start', async () => {
        const { errors, config } = await refusedFiles(
          shipping({ files: { 'lib/x.js': '' } })
        )

        expect(errors).toEqual([
          {
            path: config,
            span: START,
            severity: 'error',
            message:
              "the `shipping` plugin's `assets` hook returns a file at the url " +
              '"lib/x.js", which names no file in the output directory: a file\'s url is ' +
              'a path from `/`, without an empty, `.` or `..` segment, a percent-encoded ' +
              '`/` or `\\`, a query, or a fragment',
            labels: [],
          },
        ])
      })

      it('should refuse a file where a passthrough file is, labelled at it', async () => {
        const { errors, config } = await refusedFiles(
          shipping({ files: { '/assets/site.css': '' } })
        )

        expect(errors).toEqual([
          {
            path: config,
            span: START,
            severity: 'error',
            message:
              "the `shipping` plugin's `assets` hook returns a file at " +
              '`/assets/site.css`, and `assets/site.css` is written by the passthrough ' +
              'file `assets/site.css` already',
            labels: [
              {
                path: 'assets/site.css',
                span: START,
                message: 'the first to write there, as `assets/site.css`',
              },
            ],
          },
        ])
      })

      it('should refuse another plugin’s file at one file but for case', async () => {
        const { errors, config } = await refusedFiles(
          shipping({ files: { '/lib/App.1a2b.js': '' } }, 'other')
        )

        expect(errors).toEqual([
          {
            path: config,
            span: START,
            severity: 'error',
            message:
              "the `other` plugin's `assets` hook returns a file at `/lib/App.1a2b.js`, " +
              'and `lib/App.1a2b.js` differs from `lib/app.1a2b.js`, which the `shipping` ' +
              "plugin's `assets` hook writes, only in case or Unicode form, and a file " +
              'system may hold the two as one file',
            labels: [
              {
                path: config,
                span: START,
                message: 'the first to write there, as `lib/app.1a2b.js`',
              },
            ],
          },
        ])
      })

      it('should refuse a page where a file is, at the page, labelled at the config', async () => {
        const { result, dir } = await built(files, {
          plugins: [app, shipping({ files: { '/notes/loose/': '' } }, 'other')],
        })

        expect(result.diagnostics).toEqual([
          {
            path: 'notes/loose.md',
            span: at(SITE['notes/loose.md'], 'title: Loose'),
            severity: 'error',
            message:
              "`notes/loose/index.html` is written by the `other` plugin's `assets` hook " +
              'already',
            labels: [
              {
                path: join(dir, 'lemmon.config.ts'),
                span: START,
                message: 'the first to write there, as `notes/loose/index.html`',
              },
            ],
          },
        ])
      })
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

  describe('the link check (D47)', () => {
    const post = '---\ntitle: A\nhero: sunset\n---\nSee [the photos](/photos/).\n'

    it('should write nothing on a link to a file it does not write, and say so at the page', async () => {
      const errors = await refused({ ...SITE, 'posts/a.md': post })

      expect(errors).toEqual([
        {
          path: 'posts/a.md',
          span: at(post, 'title: A\nhero: sunset'),
          severity: 'error',
          message:
            '`posts/a.md` links to "/photos/" in `<a href>`, which names ' +
            '`photos/index.html`, where the build writes no file',
          labels: [],
        },
      ])
    })

    it('should take the path where the config says its host answers, and only that one', async () => {
      const { result } = await built(
        { ...SITE, 'posts/a.md': post },
        { served: ['/photos/'] }
      )
      const errors = await refused({ ...SITE, 'posts/a.md': post }, { served: ['/photos'] })

      expect(result.diagnostics).toEqual([])
      expect(result.pages).toContain('posts/a/index.html')
      expect(errors).toHaveLength(1)
    })

    it('should check a link a layout writes on every page it lays out, each at its own', async () => {
      const layouts = LAYOUTS_LEM.replace('<main>', '<a href="/about/">About</a>\n  <main>')
      const errors = await refused({ ...SITE, 'layouts.lem': layouts })

      expect(
        errors.map(({ path, message }) => [path, message.split(' links to ')[0]])
      ).toEqual([
        ['posts/a.md', '`posts/a.md`'],
        ['posts/b.md', '`posts/b.md`'],
        ['index.lem', '`index.lem`'],
      ])
    })

    it('should place a generated page’s link at its `for:` entry, naming its item', async () => {
      const topics = `${TOPIC_LEM}<a href="#notes">Notes</a>\n`
      const errors = await refused({ ...GENERATING, 'topics.lem': topics })

      expect(errors).toEqual(
        ['git', 'nodejs', 'rust'].map((topic, index) => ({
          path: 'topics.lem',
          span: at(topics, 'topic: site.topics'),
          severity: 'error',
          message:
            `\`topics.lem\` for \`site.topics[${index}]\` links to "#notes" in \`<a href>\`, ` +
            `and \`tils/${topic}/index.html\` has no element whose \`id\`, or an \`a\` ` +
            'whose `name`, is `notes`',
          labels: [],
        }))
      )
    })

    it('should place a produced file’s link at its producer’s mark, naming the body it was first written for', async () => {
      const styled = '---\ntitle: A\n---\n<lem.mark("styles")>\n'
      const styling: Plugin = {
        name: 'styling',
        marks: [
          {
            name: 'styles',
            produce: ({ marks, write }) => {
              write('/a.css', 'body { background: url(/assets/missing.png) }')
              return marks.map(() => '<link rel="stylesheet" href="/a.css">')
            },
          },
        ],
      }
      const errors = await refused(
        { ...SITE, 'posts/a.md': styled },
        { plugins: [styling] }
      )

      expect(errors).toEqual([
        {
          path: 'posts/a.md',
          span: at(styled, 'mark("styles")'),
          severity: 'error',
          message:
            '`a.css`, which the `styling` plugin writes for the body of `posts/a.md`, links ' +
            'to "/assets/missing.png" in `url()`, which names `assets/missing.png`, where ' +
            'the build writes no file',
          labels: [],
        },
      ])
    })

    it('should place a url an asset hook mapped to no file at the config’s start', async () => {
      const mapping = shipping(
        { manifest: { 'lib/app.js': '/lib/app.1a2b.js' } },
        'scripts'
      )
      const { result, dir } = await built(SITE, { plugins: [mapping] })

      expect(result.pages).toEqual([])
      expect(existsSync(join(dir, 'dist'))).toBe(false)
      expect(result.diagnostics).toEqual([
        {
          path: join(dir, 'lemmon.config.ts'),
          span: START,
          severity: 'error',
          message:
            'the `scripts` plugin\'s `assets` hook maps `lib/app.js` to "/lib/app.1a2b.js", ' +
            'which names `lib/app.1a2b.js`, where the build writes no file',
          labels: [],
        },
      ])
    })

    it('should refuse an XML page the XML parser refuses, whose links it cannot read', async () => {
      // An HTML layout around a feed: three elements at the top, where XML holds one.
      const feed = '---\ntype: Home\ntitle: Feed\nurl: /feed.xml\n---\n'
      const errors = await refused({ ...SITE, 'feed.lem': feed })

      expect(errors).toEqual([
        expect.objectContaining({
          path: 'feed.lem',
          span: at(feed, 'url: /feed.xml'),
          message: expect.stringMatching(
            /^`feed.xml`, which `feed.lem` writes, is refused by the XML parser \(.+\), so no link in it can be read$/
          ) as string,
        }),
      ])
    })

    it('should not run where an error came before it', async () => {
      // A page that fails to render writes nothing, so a link to it would be a fault
      // downstream of the author's, and its own links go unread.
      const failing = '---\ntitle: A\n---\n<lem.site.photos["dusk"].caption>\n'
      const linkingA = '---\ntitle: B\n---\nSee [A](../a/) and [the photos](/photos/).\n'
      const errors = await refused({
        ...SITE,
        'posts/a.md': failing,
        'posts/b.md': linkingA,
      })

      expect(errors).toEqual([expect.objectContaining({ message: 'no key "dusk"' })])
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
            '`more` loads `plugin("more")`, and plugins are not supported yet (∆wOn)',
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
    // In a page template, which the body pass does not render, so that every page is.
    const home = SITE['index.lem'] + '<lem.site.photos["nope"].caption>\n'
    const css = '---\nurl: /assets/site.css\n---\n'
    const { result, dir } = await checked({
      ...SITE,
      'index.lem': home,
      'notes/loose.md': css,
    })

    expect(result.pages).toEqual([])
    expect(result.diagnostics).toEqual([
      expect.objectContaining({ path: 'index.lem', message: 'no key "nope"' }),
      expect.objectContaining({
        path: 'notes/loose.md',
        span: at(css, 'url: /assets/site.css'),
      }),
    ])
    expect(existsSync(join(dir, 'dist'))).toBe(false)
  })

  it('should report the body of every document that fails, in one run', async () => {
    const failing = (key: string) =>
      `---\ntitle: X\n---\n<lem.site.photos["${key}"].caption>\n`
    const { result } = await checked({
      ...SITE,
      'posts/a.md': failing('one'),
      'posts/b.md': failing('two'),
    })

    expect(result.diagnostics.map(({ path, message }) => [path, message])).toEqual([
      ['posts/a.md', 'no key "one"'],
      ['posts/b.md', 'no key "two"'],
    ])
  })

  it('should report every link that names no file, as a build would', async () => {
    const post = '---\ntitle: B\n---\nSee [A](../a/#intro) and [the photos](/photos/).\n'
    const { result, dir } = await checked({ ...SITE, 'posts/b.md': post })
    const { result: wrote } = await built({ ...SITE, 'posts/b.md': post })

    expect(result.pages).toEqual([])
    expect(result.diagnostics.map(({ message }) => message)).toEqual([
      '`posts/b.md` links to "../a/#intro" in `<a href>`, and `posts/a/index.html` has no ' +
        'element whose `id`, or an `a` whose `name`, is `intro`',
      '`posts/b.md` links to "/photos/" in `<a href>`, which names `photos/index.html`, ' +
        'where the build writes no file',
    ])
    expect(wrote.diagnostics).toEqual(result.diagnostics)
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

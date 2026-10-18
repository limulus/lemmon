import { loadCore } from '@limulus/lemmon-core'
import { readFileSync } from 'node:fs'
import { isAbsolute, join } from 'node:path'
import { describe, expect, it } from 'vitest'

import type { Plugin } from './plugins.ts'
import { AMBIENT_FILE, compileSite, HOST, SiteError, type Compiled } from './program.ts'
import { discover } from './site.ts'
import { copy, inline, shell, tsHighlighter } from '../mocks/plugins.ts'
import { configOf, LAYOUTS_LEM, project, SITE, SITE_LEM } from '../mocks/sites.ts'
import { at } from '../mocks/spans.ts'

/** Compiles `files` as a project's input, listing `plugins`, in a fresh core. */
async function compiled(
  files: Record<string, string>,
  plugins: Plugin[] = []
): Promise<Compiled> {
  const site = await discover(configOf(project(files), [], plugins))
  return await compileSite(await loadCore(), site, new Map())
}

/** The pages of a compile that is expected to succeed, as `[path, record, binding]`. */
async function typed(files: Record<string, string>) {
  const { program, diagnostics } = await compiled(files)
  expect(diagnostics).toEqual([])
  return [...program!.pages].map(([path, page]) => [path, page.record, page.binding])
}

describe('the shell’s ambient declarations', () => {
  it('should be a file of the package, known to the core by its absolute path', () => {
    expect(isAbsolute(AMBIENT_FILE)).toBe(true)
    expect(readFileSync(AMBIENT_FILE, 'utf8')).toMatch(/^<lem-type Page>\n {2}url: url\n/)
  })

  it('should declare the names the host binds, and a page type a site extends', async () => {
    const core = await loadCore()
    const text = readFileSync(AMBIENT_FILE, 'utf8')
    const card = '<lem-fn card()><lem.page.inputPath><lem.build.generator></lem-fn>'

    expect(
      core.compile(
        [
          { path: AMBIENT_FILE, text },
          { path: 'site.lem', text: '' },
          { path: 'card.lem', text: card },
        ],
        HOST
      ).diagnostics
    ).toEqual([])
  })
})

describe('compileSite()', () => {
  it('should report the sources the bindings file declares', async () => {
    const { program } = await compiled(SITE)

    expect(
      program?.sources.map(({ name, kind, collects }) => [name, kind, collects])
    ).toEqual([
      ['posts', 'file', 'Post'],
      ['photos', 'file', undefined],
      ['motto', 'env', undefined],
    ])
  })

  it('should compile each page as its own `type:`, else as the glob that holds it', async () => {
    const note = '---\ntype: Note\ntitle: N\n---\n'
    const site = `${SITE_LEM}<lem-type Note extends Post>\n  mood: string?\n</lem-type>\n`

    expect(
      await typed({
        ...SITE,
        'site.lem': site,
        'posts/note.md': note,
        'posts/.dot.md': '---\ntitle: D\n---\n',
      })
    ).toEqual([
      ['index.lem', 'Home', undefined],
      ['notes/loose.md', undefined, undefined],
      // The glob matches as passthrough's do, dotfiles included.
      ['posts/.dot.md', 'Post', 'posts'],
      ['posts/a.md', 'Post', 'posts'],
      ['posts/b.md', 'Post', 'posts'],
      ['posts/note.md', 'Note', 'posts'],
    ])
  })

  it('should match a collection glob without case, as passthrough globs match', async () => {
    // The fixture's own posts are left out: a case-insensitive file system would hold
    // `posts/` and `Posts/` as one directory.
    const { 'posts/a.md': _a, 'posts/b.md': _b, ...files } = SITE

    expect(await typed({ ...files, 'Posts/c.md': '---\ntitle: C\n---\n' })).toEqual([
      ['Posts/c.md', 'Post', 'posts'],
      ['index.lem', 'Home', undefined],
      ['notes/loose.md', undefined, undefined],
    ])
  })

  it('should compile a page template with the templates, so a layout sees its definitions', async () => {
    const about = '---\ntype: Home\ntitle: A\n---\n<lem-fn helper(): string = "h" />\n'
    const layout = '<lem-fn shell()><lem.helper()></lem-fn>\n'

    expect(
      await typed({ ...SITE, 'about.lem': about, 'shell.lem': layout })
    ).toContainEqual(['about.lem', 'Home', undefined])
  })

  it('should point a diagnostic about the record at the `type:` key', async () => {
    const note = '---\ntype: Missive\ntitle: N\n---\n'
    const { program, diagnostics } = await compiled({ ...SITE, 'posts/note.md': note })

    expect(program).toBeUndefined()
    expect(diagnostics).toEqual([
      expect.objectContaining({ path: 'posts/note.md', span: at(note, 'Missive') }),
    ])
  })

  it('should refuse a site with no `site.lem`, naming where it looked', async () => {
    const { 'site.lem': _, ...files } = SITE
    const dir = project(files)

    const compiling = compileSite(
      await loadCore(),
      await discover(configOf(dir)),
      new Map()
    )

    await expect(compiling).rejects.toThrow(SiteError)
    await expect(compiling).rejects.toMatchObject({
      file: join(dir, 'www', 'site.lem'),
      message: expect.stringContaining('site.lem: no bindings file') as string,
    })
  })

  it('should take a `site.lem` with front matter for a page, not the bindings file', async () => {
    const dir = project({ ...SITE, 'site.lem': `---\ntitle: Site\n---\n${SITE_LEM}` })

    await expect(
      compileSite(await loadCore(), await discover(configOf(dir)), new Map())
    ).rejects.toThrow(SiteError)
  })

  it('should stop after the templates when one has an error', async () => {
    const broken = '<lem-fn broken()><lem.nope></lem-fn>\n'
    const { program, diagnostics } = await compiled({
      ...SITE,
      'broken.lem': broken,
      'posts/b.md': '<lem.page.byline>\n',
    })

    expect(program).toBeUndefined()
    expect(diagnostics.map((d) => d.path)).toEqual(['broken.lem'])
  })

  it('should carry a warning and compile on', async () => {
    const { program, diagnostics } = await compiled({
      ...SITE,
      'helpers.lem': '<lem-fn unused()>\n  <lem-let x = 1 />\n</lem-fn>\n',
    })

    expect(diagnostics.map((d) => d.severity)).toEqual(['warning'])
    expect(program?.pages.size).toBe(4)
  })

  it('should refuse a page two collecting globs match, labelled at each', async () => {
    const site = `${SITE_LEM}<lem-let all: [Post] from "**/*.md" />\n`
    const post = SITE['posts/a.md']
    const { program, diagnostics } = await compiled({ ...SITE, 'site.lem': site })

    expect(program).toBeUndefined()
    expect(diagnostics).toContainEqual({
      path: 'posts/a.md',
      span: at(post, 'title: A\nhero: sunset'),
      severity: 'error',
      message:
        'the globs of `posts` and `all` each match this page, and a page belongs to ' +
        'one collection',
      labels: [
        {
          path: 'site.lem',
          span: at(site, '<lem-let «posts»'),
          message: '`posts/*.md` matches it',
        },
        {
          path: 'site.lem',
          span: at(site, '<lem-let «all»'),
          message: '`**/*.md` matches it',
        },
      ],
    })
  })

  it('should point a page two globs match at its `type:` where it has one', async () => {
    const site = `${SITE_LEM}<lem-let all: [Post] from "**/*.md" />\n`
    const post = '---\ntype: Post\ntitle: A\n---\n'
    const { diagnostics } = await compiled({
      ...SITE,
      'site.lem': site,
      'posts/a.md': post,
    })

    expect(diagnostics).toContainEqual(
      expect.objectContaining({ path: 'posts/a.md', span: at(post, 'type: «Post»') })
    )
  })

  it('should refuse a generator, which nothing expands yet', async () => {
    const topic = '---\ntype: Home\nfor:\n  title: site.titles\n---\n'
    const { program, diagnostics } = await compiled({ ...SITE, 'topic.lem': topic })

    expect(program).toBeUndefined()
    expect(diagnostics).toEqual([
      {
        path: 'topic.lem',
        span: at(topic, 'title: site.titles'),
        severity: 'error',
        message:
          '`for:` makes this page a generator, and `lem build` expands none yet (∆99b)',
        labels: [],
      },
    ])
  })

  describe('the marks', () => {
    // The base layout places the page's styles, and a post's body holds a code block.
    const layouts = LAYOUTS_LEM.replace('<main>', '<lem.mark("styles")>\n  <main>')
    const fenced = (language: string, code = 'let x = 1') =>
      `---\ntitle: A\n---\n\n\`\`\`${language}\n${code}\n\`\`\`\n`
    const post = fenced('ts')
    const files = { ...SITE, 'layouts.lem': layouts, 'posts/a.md': post }

    /** What compiling `files` with `plugins` warned, as `[path, message, span]`. */
    async function warned(files: Record<string, string>, plugins: Plugin[]) {
      const { program, diagnostics } = await compiled(files, plugins)
      expect(program).toBeDefined()
      return diagnostics.map(({ path, message, span }) => [path, message, span])
    }

    it('should declare what the listed plugins claim, so a misspelled mark is warned at', async () => {
      const misspelled = layouts.replace('mark("styles")', 'mark("stylez")')

      expect(
        await warned({ ...files, 'layouts.lem': misspelled }, [inline, tsHighlighter])
      ).toEqual([
        [
          'layouts.lem',
          'no handler claims `stylez`; did you mean `styles`?',
          at(misspelled, '"stylez"'),
        ],
      ])
    })

    it('should warn at every mark of a site listing no plugins, and at none a plugin claims', async () => {
      expect(await warned(files, [])).toEqual([
        [
          'layouts.lem',
          'no handler claims `styles`, so this mark keeps its default content',
          at(layouts, '"styles"'),
        ],
        [
          'posts/a.md',
          'no handler claims `code`, so this mark keeps its default content',
          at(post, '«ts»\n'),
        ],
      ])
      expect(await warned(files, [inline, tsHighlighter])).toEqual([])
    })

    it('should leave a key only a decorator lists warned at', async () => {
      // The decorator for `sh` chooses what it decorates; it covers no key.
      const sh = fenced('sh', 'ls')

      expect(
        await warned({ ...files, 'posts/b.md': sh }, [inline, tsHighlighter, shell])
      ).toEqual([
        [
          'posts/b.md',
          expect.stringMatching(/^no producer claims `sh` for `code`/),
          at(sh, '«sh»\n'),
        ],
      ])
    })

    it('should leave every code block warned at where only a decorator handles `code`', async () => {
      expect(await warned(files, [inline, copy])).toEqual([
        [
          'posts/a.md',
          'no producer claims `ts` for `code`, so this mark keeps its default content',
          at(post, '«ts»\n'),
        ],
      ])
    })

    it('should leave every code block naming a language warned at for a keyless producer', async () => {
      // It answers only the blocks with no language, so the one with none is silent.
      const plain: Plugin = { name: 'plain', marks: [{ name: 'code', produce: () => [] }] }
      const bare = fenced('')

      expect(await warned({ ...files, 'posts/b.md': bare }, [inline, plain])).toEqual([
        [
          'posts/a.md',
          'no producer claims `ts` for `code`, so this mark keeps its default content',
          at(post, '«ts»\n'),
        ],
      ])
    })

    it('should keep every code block’s payload by id, each once', async () => {
      const both = `${post}\n\`\`\`sh {1}\nls\n\`\`\`\n`
      const { program } = await compiled(
        { ...files, 'posts/a.md': both, 'posts/b.md': post },
        [inline, tsHighlighter]
      )

      const payloads = [...program!.payloads]
      expect(
        payloads.map(([, { language, info, code }]) => [language, info, code])
      ).toEqual([
        ['ts', 'ts', 'let x = 1\n'],
        ['sh', 'sh {1}', 'ls\n'],
      ])
      expect(payloads.every(([id, payload]) => id === payload.id)).toBe(true)
    })
  })
})

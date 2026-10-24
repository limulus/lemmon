import { mkdirSync, symlinkSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'

import { skipped, walk, type Entry } from './walk.ts'
import { tempDir } from '../mocks/temp-dir.ts'

/** limulus.net's two Eleventy passthrough globs, written relative to its input. */
const ELEVENTY_GLOBS = ['assets/**/*', '**/*.{png,svg,jpg,jpeg,xsd,xsl,txt,pdf}']

/** The walk of a directory holding `files`, with `passthrough` globs. */
async function walked(
  files: Record<string, string>,
  passthrough: string[] = []
): Promise<Entry[]> {
  const input = tempDir(files)
  return await walk({ input, output: join(input, '..', 'dist'), passthrough })
}

/** Each entry as `kind path`, which is all most cases read. */
const kinds = (entries: Entry[]) => entries.map((entry) => `${entry.kind} ${entry.path}`)

describe('walk()', () => {
  it('should classify every file under the input, sorted by path', async () => {
    const entries = await walked(
      {
        'z.md': '# Z\n',
        'feed/the-rebound/index.md': '---\ntitle: x\n---\n',
        'feed/index.lem': '---\ntype: Page\n---\n',
        'components.lem': '<lem-fn f() />\n',
        'feed/index.webc': '<p></p>\n',
        '_data/site.json': '{}\n',
        'assets/x.png': '',
      },
      ELEVENTY_GLOBS
    )

    expect(kinds(entries)).toEqual([
      'ignored _data/site.json',
      'passthrough assets/x.png',
      'template components.lem',
      'template feed/index.lem',
      'ignored feed/index.webc',
      'document feed/the-rebound/index.md',
      'document z.md',
    ])
  })

  it('should read a document and a template with their text', async () => {
    const entries = await walked({ 'a.md': '# A\n', 'b.lem': 'Hi\n' })

    expect(entries).toEqual([
      { kind: 'document', path: 'a.md', text: '# A\n' },
      { kind: 'template', path: 'b.lem', text: 'Hi\n', page: false },
    ])
  })

  it('should mark a template that opens with front matter as a page', async () => {
    const entries = await walked({
      'page.lem': '---\ntitle: x\n---\n<p></p>\n',
      'later.lem': '<p></p>\n---\ntitle: x\n---\n',
      'unclosed.lem': '---\ntitle: x\n',
    })

    expect(entries.map((entry) => entry.kind === 'template' && entry.page)).toEqual([
      false,
      true,
      false,
    ])
  })

  it('should not read a passthrough or an ignored file', async () => {
    const entries = await walked({ 'robots.txt': 'x', 'x.webc': 'y' }, ELEVENTY_GLOBS)

    expect(entries).toEqual([
      { kind: 'passthrough', path: 'robots.txt' },
      { kind: 'ignored', path: 'x.webc' },
    ])
  })

  it('should take a `.md` or `.lem` in another case as misnamed, read with its text', async () => {
    const entries = await walked({
      'feed/x/index.MD': '# X\n',
      'nav.Lem': '<lem-fn f() />\n',
      'notes.MDX': '',
      'site.lemon': '',
    })

    expect(entries).toEqual([
      { kind: 'misnamed', path: 'feed/x/index.MD', text: '# X\n', extension: '.md' },
      { kind: 'misnamed', path: 'nav.Lem', text: '<lem-fn f() />\n', extension: '.lem' },
      { kind: 'ignored', path: 'notes.MDX' },
      { kind: 'ignored', path: 'site.lemon' },
    ])
  })

  describe('passthrough globs', () => {
    it('should match relative to the input, as Eleventy’s do relative to it', async () => {
      const entries = await walked(
        {
          'assets/fonts/mona.woff2': '',
          'assets/svg/common/bell.svg': '',
          'feed/the-social-contract/order.pdf': '',
          'robots.txt': '',
          'xsd/tcx/v1.xsd': '',
          'feed.xsl': '',
          'lib/index.ts': '',
          'www/assets/x.css': '',
        },
        ELEVENTY_GLOBS
      )

      expect(kinds(entries)).toEqual([
        'passthrough assets/fonts/mona.woff2',
        'passthrough assets/svg/common/bell.svg',
        'passthrough feed.xsl',
        'passthrough feed/the-social-contract/order.pdf',
        'ignored lib/index.ts',
        'passthrough robots.txt',
        'ignored www/assets/x.css',
        'passthrough xsd/tcx/v1.xsd',
      ])
    })

    it('should match whatever the case, as Eleventy’s do', async () => {
      const entries = await walked({ 'IMG.PNG': '', 'Feed/Photo.Png': '' }, ELEVENTY_GLOBS)

      expect(kinds(entries)).toEqual(['passthrough Feed/Photo.Png', 'passthrough IMG.PNG'])
    })

    it('should match a dotfile, as Eleventy’s do', async () => {
      const entries = await walked({ 'assets/.nojekyll': '', '.env': '' }, ELEVENTY_GLOBS)

      expect(kinds(entries)).toEqual(['ignored .env', 'passthrough assets/.nojekyll'])
    })

    it('should take a document or template it matches as passthrough', async () => {
      const entries = await walked(
        { 'assets/notes.md': '# x\n', 'assets/t.lem': '', 'assets/README.MD': '' },
        ELEVENTY_GLOBS
      )

      expect(kinds(entries)).toEqual([
        'passthrough assets/README.MD',
        'passthrough assets/notes.md',
        'passthrough assets/t.lem',
      ])
    })
  })

  it('should never enter a node_modules or .git directory, at any depth', async () => {
    const entries = await walked(
      {
        'node_modules/pkg/README.md': '',
        '.git/HEAD': '',
        'lib/node_modules/x/index.md': '',
        'lib/.git/config': '',
        'node_modules.md': '',
        '.github/notes.md': '',
        'a.md': '',
      },
      ['**']
    )

    expect(kinds(entries)).toEqual([
      'passthrough .github/notes.md',
      'passthrough a.md',
      'passthrough node_modules.md',
    ])
  })

  it('should pass over a .git that is a file, as a worktree’s is', async () => {
    const entries = await walked({ '.git': 'gitdir: ../.git/worktrees/x\n', 'a.md': '' }, [
      '**',
    ])

    expect(kinds(entries)).toEqual(['passthrough a.md'])
  })

  it('should leave out an output directory inside the input', async () => {
    const input = tempDir({ 'a.md': '', '_site/a/index.html': '', '_site.md': '' })

    const entries = await walk({ input, output: join(input, '_site'), passthrough: ['**'] })

    expect(kinds(entries)).toEqual(['passthrough _site.md', 'passthrough a.md'])
  })

  it('should count a symbolic link as the file it names, by its own name', async () => {
    const input = tempDir({
      'assets/svg/ldn.dcaedd8b.svg': '<svg/>',
      'shared/post.md': '# x',
    })
    mkdirSync(join(input, 'assets/svg/common'))
    symlinkSync('../ldn.dcaedd8b.svg', join(input, 'assets/svg/common/ldn.svg'))
    symlinkSync('shared/post.md', join(input, 'feed-post.md'))
    mkdirSync(join(input, 'linked'))
    symlinkSync('../shared', join(input, 'linked/dir'))

    const entries = await walk({
      input,
      output: join(input, '..', 'dist'),
      passthrough: ['**/*.svg'],
    })

    expect(kinds(entries)).toEqual([
      'passthrough assets/svg/common/ldn.svg',
      'passthrough assets/svg/ldn.dcaedd8b.svg',
      'document feed-post.md',
      'document shared/post.md',
    ])
    expect(entries[2]).toMatchObject({ text: '# x' })
  })

  it('should fail on a symbolic link that names nothing, not leave it out', async () => {
    const input = tempDir()
    symlinkSync('missing.md', join(input, 'post.md'))

    await expect(
      walk({ input, output: join(input, '..', 'dist'), passthrough: [] })
    ).rejects.toThrow(/ENOENT/)
  })

  it('should find nothing in an empty input', async () => {
    expect(await walked({})).toEqual([])
  })
})

describe('skipped()', () => {
  const input = join('/site', 'www')
  const config = { input, output: join(input, 'dist') }
  const under = (path: string) => join(input, ...path.split('/'))

  it('should pass over the output directory inside the input, and what is in it', () => {
    expect(skipped(config, under('dist'))).toBe(true)
    expect(skipped(config, under('dist/x.html'))).toBe(true)
    expect(skipped(config, under('dist/feed/index.html'))).toBe(true)
  })

  it('should not pass over a neighbour whose name begins as the output’s does', () => {
    expect(skipped(config, under('dist.md'))).toBe(false)
    expect(skipped(config, under('distx/a.md'))).toBe(false)
    expect(skipped(config, under('feed/dist/a.md'))).toBe(false)
  })

  it('should pass over node_modules and .git at any depth, and what is in them', () => {
    for (const path of [
      'node_modules',
      'node_modules/pkg/README.md',
      '.git',
      '.git/HEAD',
      'lib/node_modules/x/index.md',
      'lib/.git',
    ]) {
      expect(skipped(config, under(path)), path).toBe(true)
    }
  })

  it('should not pass over a name that only begins as theirs does', () => {
    for (const path of ['node_modules.md', '.github/notes.md', '.gitignore', 'a.md']) {
      expect(skipped(config, under(path)), path).toBe(false)
    }
  })

  it('should not pass over the input itself, nor a file in it beside an output outside', () => {
    expect(skipped(config, input)).toBe(false)
    expect(skipped({ input, output: join('/site', 'dist') }, under('a.md'))).toBe(false)
  })
})

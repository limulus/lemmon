import { dirname, join } from 'node:path'
import { describe, expect, it } from 'vitest'

import { readPage } from './front-matter.ts'
import type { AssetsContext, DocumentSource, Plugin } from './plugins.ts'
import { FILE_START as START } from './program.ts'
import { provided, transformed } from './providers.ts'
import { appender, appending, copy, shipping } from '../mocks/plugins.ts'
import { configOf } from '../mocks/sites.ts'
import { at } from '../mocks/spans.ts'
import { tempDir } from '../mocks/temp-dir.ts'

describe('transformed()', () => {
  // Front matter that is not ASCII, so that a prefix kept by string index rather than
  // byte for byte would move every span after it.
  const LINK =
    '---\ntype: Link\ntitle: Café ’\nlinkUrl: https://example.com/\n---\n' +
    'See [the post][link].\n'
  const PATH = 'links/x/index.md'

  /** `text` at `path` through `plugins`, its front matter read as discovery reads it. */
  function through(plugins: Plugin[], text = LINK, path = PATH) {
    const { page } = readPage(path, text)
    return transformed(path, text, page!, plugins)
  }

  /** A plugin whose transform records each document it is given, returning its body. */
  function recording(name = 'recording') {
    const given: DocumentSource[] = []
    const plugin: Plugin = {
      name,
      transformDocument: (document) => {
        given.push(document)
        return document.body
      },
    }
    return { plugin, given }
  }

  it('should leave the text as it is where no plugin transforms documents', () => {
    expect(through([copy])).toEqual({ text: LINK, diagnostics: [] })
  })

  it('should give a transform the path, the front matter as loaded, and the body', () => {
    const { plugin, given } = recording()

    through([plugin])

    // `type:` is read off and `url` and `inputPath` derived, as the core loads it.
    expect(given).toEqual([
      {
        path: PATH,
        frontMatter: {
          title: 'Café ’',
          linkUrl: 'https://example.com/',
          url: '/links/x/',
          inputPath: PATH,
        },
        body: 'See [the post][link].\n',
      },
    ])
  })

  it('should compile what the transform returned, after the front matter byte for byte', () => {
    expect(through([appender])).toEqual({
      text: `${LINK}\n\n[link]: https://example.com/\n`,
      diagnostics: [],
    })
  })

  it('should run the transforms in the order listed, each over the body before it', () => {
    const { plugin, given } = recording()

    const { text } = through([appending('a', 'a'), plugin, appending('b', 'b')])

    expect(given[0].body).toBe('See [the post][link].\na')
    expect(text).toBe(`${LINK}ab`)
  })

  it('should give a document with no front matter its whole text as the body', () => {
    const { plugin, given } = recording()

    through([plugin], 'Hello.\n', 'notes/a.md')

    expect(given).toEqual([
      {
        path: 'notes/a.md',
        frontMatter: { url: '/notes/a/', inputPath: 'notes/a.md' },
        body: 'Hello.\n',
      },
    ])
  })

  it('should call a transform with its own plugin as `this`', () => {
    const plugin: Plugin = {
      name: 'self',
      transformDocument(this: Plugin, { body }) {
        return `${body}${this.name}`
      },
    }

    expect(through([plugin]).text).toBe(`${LINK}self`)
  })

  describe('a transform that fails', () => {
    const SPAN = at(LINK, 'type: Link\ntitle: Café ’\nlinkUrl: https://example.com/')

    const failures: [string, Plugin['transformDocument'], string][] = [
      [
        'throws',
        () => {
          throw new Error('no link')
        },
        'it threw Error: no link',
      ],
      [
        'throws what is not an error',
        () => {
          const thrown: unknown = { why: 1 }
          throw thrown
        },
        'it threw an object',
      ],
      [
        'returns nothing',
        () => undefined as unknown as string,
        "it returned undefined rather than the document's body, a string",
      ],
      [
        'returns bytes',
        () => new Uint8Array() as unknown as string,
        "it returned an object rather than the document's body, a string",
      ],
      [
        // Rejecting later, which must not surface as an unhandled rejection.
        'returns a promise',
        () => Promise.reject(new Error('late')) as unknown as string,
        "it returned a promise rather than the document's body, a string",
      ],
      [
        'writes to the front matter',
        (document) => {
          ;(document.frontMatter as Record<string, unknown>).title = 'Other'
          return document.body
        },
        "it threw TypeError: Cannot assign to read only property 'title' of object '#<Object>'",
      ],
      [
        'returns a lone surrogate',
        ({ body }) => `${body}\uD800`,
        'it returned a body holding a lone surrogate, which has no UTF-8 form',
      ],
    ]
    for (const [label, transformDocument, problem] of failures) {
      it(`should be an error at the document's front matter when it ${label}`, () => {
        expect(through([{ name: 'failing', transformDocument }])).toEqual({
          text: LINK,
          diagnostics: [
            {
              path: PATH,
              span: SPAN,
              severity: 'error',
              message:
                "the `failing` plugin's `transformDocument` failed on this document: " +
                problem,
              labels: [],
            },
          ],
        })
      })
    }

    it('should keep the text from before it, and run no transform after it', () => {
      const { plugin, given } = recording('after')
      const failing: Plugin = {
        name: 'failing',
        transformDocument: () => {
          throw new Error('no link')
        },
      }

      const { text, diagnostics } = through([appending('a'), failing, plugin])

      expect(text).toBe(`${LINK}a`)
      expect(diagnostics).toHaveLength(1)
      expect(given).toEqual([])
    })

    it('should leave the front matter the page loads as it read', () => {
      const text = '---\ntags:\n  - a\n---\n'
      const { page } = readPage('notes/a.md', text)
      const pushing: Plugin = {
        name: 'pushing',
        transformDocument: ({ frontMatter, body }) => {
          ;(frontMatter.tags as string[]).push('b')
          return body
        },
      }

      const { diagnostics } = transformed('notes/a.md', text, page!, [pushing])

      expect(diagnostics[0].message).toMatch(/: it threw TypeError: /)
      expect(page!.loaded.value.tags).toEqual(['a'])
    })

    it('should be an error at the start of a document with no front matter', () => {
      const failing: Plugin = { name: 'failing', transformDocument: () => 3 as never }

      const { diagnostics } = through([failing], 'Hello.\n', 'notes/a.md')

      expect(diagnostics).toEqual([
        expect.objectContaining({ path: 'notes/a.md', span: START, severity: 'error' }),
      ])
    })
  })

  it('should take a body holding a surrogate pair, which has a UTF-8 form', () => {
    expect(through([appending('😀')])).toEqual({
      text: `${LINK}😀`,
      diagnostics: [],
    })
  })

  describe('a body that would make other front matter of the text', () => {
    const FAILED = "the `opening` plugin's `transformDocument` failed on this document: "
    const opening = (body: string): Plugin => ({
      name: 'opening',
      transformDocument: (document) => body + document.body,
    })
    const messages = (plugin: Plugin, text: string) =>
      through([plugin], text).diagnostics.map((d) => d.message)

    it('should be refused where it opens a document that had no front matter', () => {
      const { text, diagnostics } = through([opening('---\ntitle: X\n---\n')], 'Hi.\n')

      expect(text).toBe('Hi.\n')
      expect(diagnostics.map((d) => d.message)).toEqual([
        `${FAILED}it returned a body opening with a \`---\` line, which would read as ` +
          'front matter the document does not have',
      ])
    })

    it('should be refused where it closes a block nothing closed, or drops its opening', () => {
      // The core reports an unclosed block at its first line; a transform may neither
      // close it nor make it go away.
      const unclosed = '---\ntitle: X\n'
      const closing: Plugin = {
        name: 'opening',
        transformDocument: ({ body }) => `${body}---\n`,
      }
      const dropping: Plugin = {
        name: 'opening',
        transformDocument: ({ body }) => body.slice('---\n'.length),
      }
      const unclosedBy = (how: string) =>
        `${FAILED}it returned a body that ${how} the front matter the document leaves ` +
        'unclosed, which the core reports as it stands'

      expect(messages(closing, unclosed)).toEqual([unclosedBy('closes')])
      expect(messages(dropping, unclosed)).toEqual([
        unclosedBy('drops the `---` line opening'),
      ])
      expect(through([appending('more\n')], unclosed).diagnostics).toEqual([])
    })

    it('should be taken where the document has front matter, which a `---` line follows', () => {
      const { text, diagnostics } = through([opening('---\n')])

      expect(diagnostics).toEqual([])
      expect(text).toBe(LINK.replace('---\nSee', '---\n---\nSee'))
    })

    describe('where the closing `---` ends the file', () => {
      // A link post can be all front matter, its last line the closing rule.
      const BARE = '---\ntitle: X\nlinkUrl: https://example.com/\n---'

      it('should be taken where the body starts with the newline that line lacks', () => {
        for (const ending of ['\n', '\r\n']) {
          const { text, diagnostics } = through([appending(`${ending}More.\n`)], BARE)

          expect(diagnostics).toEqual([])
          expect(text).toBe(`${BARE}${ending}More.\n`)
        }
        expect(through([appender], BARE).text).toBe(
          `${BARE}\n\n[link]: https://example.com/\n`
        )
      })

      it('should be refused where the body does not start with a newline', () => {
        expect(messages(opening('More.\n'), BARE)).toEqual([
          `${FAILED}it returned a body that does not start with a newline, and the front ` +
            "matter's closing `---` ends the file, so that line would no longer close " +
            'it: start the body with a newline',
        ])
      })
    })
  })
})

describe('provided()', () => {
  /** A config listing `plugins`. */
  const config = (plugins: Plugin[]) => configOf(tempDir(), [], plugins)
  /** A build that writes, in an environment of its own. */
  const BUILD = { writes: true, env: { TOKEN: 'abc' } }

  it('should merge every manifest and every file, in the order the plugins are listed', async () => {
    const bytes = new Uint8Array([0, 1, 2])
    const listing = config([
      shipping(
        { manifest: { 'lib/a.js': '/lib/a.1.js' }, files: { '/lib/a.1.js': 'a' } },
        'a'
      ),
      copy,
      {
        name: 'b',
        // A hook that returns as it runs is taken as one that awaits.
        assets: () => ({
          manifest: { 'lib/b.js': 'https://cdn.example/b.js' },
          files: { '/b.wasm': bytes },
        }),
      },
    ])

    expect(await provided(listing, BUILD)).toEqual({
      manifest: { 'lib/a.js': '/lib/a.1.js', 'lib/b.js': 'https://cdn.example/b.js' },
      mappedBy: new Map([
        ['lib/a.js', 'a'],
        ['lib/b.js', 'b'],
      ]),
      files: [
        { url: '/lib/a.1.js', bytes: new TextEncoder().encode('a'), plugin: 'a' },
        { url: '/b.wasm', bytes, plugin: 'b' },
      ],
      diagnostics: [],
    })
  })

  it('should take a hook that builds nothing', async () => {
    expect(await provided(config([shipping({})]), BUILD)).toEqual({
      manifest: {},
      mappedBy: new Map(),
      files: [],
      diagnostics: [],
    })
  })

  it('should give each hook the root, the directories, whether the build writes, and its environment', async () => {
    const given: [AssetsContext, string][] = []
    const plugin: Plugin = {
      name: 'given',
      assets(this: Plugin, context) {
        given.push([context, this.name])
        return {}
      },
    }
    const listing = config([plugin])

    await provided(listing, BUILD)
    await provided(listing, { writes: false, env: {} })

    // The root is the config file's directory, which `input` and `output` are under
    // here: `www` and `dist`.
    const { file, input, output } = listing
    const root = dirname(file)
    expect([input, output]).toEqual([join(root, 'www'), join(root, 'dist')])
    expect(given).toEqual([
      [{ root, input, output, writes: true, env: { TOKEN: 'abc' } }, 'given'],
      [{ root, input, output, writes: false, env: {} }, 'given'],
    ])
  })

  it('should give the hooks one frozen copy of the environment, which the build keeps', async () => {
    const env: Record<string, string | undefined> = { TOKEN: 'abc' }
    const writing: Plugin = {
      name: 'writing',
      assets: ({ env: given }) => {
        ;(given as Record<string, string>).TOKEN = 'changed'
        return {}
      },
    }

    const { diagnostics } = await provided(config([writing]), { writes: true, env })

    expect(diagnostics.map((d) => d.message)).toEqual([
      "the `writing` plugin's `assets` hook failed: it threw TypeError: Cannot assign " +
        "to read only property 'TOKEN' of object '#<Object>'",
    ])
    expect(env.TOKEN).toBe('abc')
  })

  it('should refuse a logical path two plugins both map, naming both, and keep the first', async () => {
    const listing = config([
      shipping({ manifest: { 'lib/a.js': '/1.js' } }, 'first'),
      shipping({ manifest: { 'lib/a.js': '/2.js', 'lib/b.js': '/3.js' } }, 'second'),
    ])

    const { manifest, mappedBy, diagnostics } = await provided(listing, BUILD)

    // A path one plugin alone maps is taken from a plugin that also maps a shared one.
    expect(manifest).toEqual({ 'lib/a.js': '/1.js', 'lib/b.js': '/3.js' })
    expect(mappedBy).toEqual(
      new Map([
        ['lib/a.js', 'first'],
        ['lib/b.js', 'second'],
      ])
    )
    expect(diagnostics).toEqual([
      {
        path: listing.file,
        span: START,
        severity: 'error',
        message:
          'the plugins `first` and `second` both map the asset `lib/a.js`; an asset ' +
          'ships at one URL',
        labels: [],
      },
    ])
  })

  describe('a hook that fails', () => {
    const failures: [string, Plugin['assets'], string][] = [
      [
        'throws',
        () => {
          throw new Error('no esbuild')
        },
        'it threw Error: no esbuild',
      ],
      [
        'rejects',
        () => Promise.reject(new Error('no esbuild')),
        'it threw Error: no esbuild',
      ],
      [
        'returns nothing',
        () => undefined as never,
        'it returned undefined rather than a plain object, `{manifest?, files?}`',
      ],
      [
        'returns a map',
        () => new Map() as never,
        'it returned an object rather than a plain object, `{manifest?, files?}`',
      ],
      [
        'returns a field it does not have',
        () => ({ file: {} }) as never,
        'it returned `file`, which this shell does not read; it returns `manifest`, `files`',
      ],
      [
        'returns a manifest that is a list',
        () => ({ manifest: [] }) as never,
        'its `manifest` is an object rather than a plain object, `{…}`, of logical paths ' +
          'and URLs',
      ],
      [
        'returns files that are a string',
        () => ({ files: 'a.js' }) as never,
        'its `files` is a string rather than a plain object, `{…}`, of urls and contents',
      ],
      [
        'maps an empty path',
        () => ({ manifest: { '': '/a.js' } }),
        'its `manifest` names the logical path "", where a path is a non-empty string ' +
          'with a UTF-8 form',
      ],
      [
        'maps a path holding a lone surrogate',
        () => ({ manifest: { 'a\uD800': '/a.js' } }),
        'its `manifest` names the logical path "a\\ud800", where a path is a non-empty ' +
          'string with a UTF-8 form',
      ],
      [
        'maps a path to what is not a string',
        () => ({ manifest: { 'a.js': 3 } }) as never,
        'its `manifest` maps `a.js` to 3, where it maps a path to the URL the asset ' +
          'ships at, a non-empty string with a UTF-8 form',
      ],
      [
        'maps a path to an empty URL',
        () => ({ manifest: { 'a.js': '' } }),
        'its `manifest` maps `a.js` to "", where it maps a path to the URL the asset ' +
          'ships at, a non-empty string with a UTF-8 form',
      ],
      [
        'builds a file that is neither a string nor bytes',
        () => ({ files: { '/a.js': 3 } }) as never,
        'its `files` holds 3 at `/a.js`, where a file is a string or bytes',
      ],
    ]
    for (const [label, assets, problem] of failures) {
      it(`should be an error at the config's start, adding nothing, when it ${label}`, async () => {
        const listing = config([
          { name: 'failing', assets },
          shipping({ manifest: { 'lib/a.js': '/a.js' }, files: { '/a.js': '' } }),
        ])

        const result = await provided(listing, BUILD)

        expect(result.diagnostics).toEqual([
          {
            path: listing.file,
            span: START,
            severity: 'error',
            message: `the \`failing\` plugin's \`assets\` hook failed: ${problem}`,
            labels: [],
          },
        ])
        // The plugin after it still runs.
        expect(result.manifest).toEqual({ 'lib/a.js': '/a.js' })
        expect(result.files.map((file) => file.plugin)).toEqual(['shipping'])
      })
    }
  })
})

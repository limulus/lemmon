import { join, resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

import { ConfigError, contains, loadConfig } from './config.ts'
import { tempDir } from '../mocks/temp-dir.ts'

/**
 * The path of a `lemmon.config.ts` with `source` as its text, in a directory that also
 * holds a `www/` for it to name.
 */
function configFile(source: string): string {
  return join(
    tempDir({ 'lemmon.config.ts': source, 'www/index.md': '' }),
    'lemmon.config.ts'
  )
}

/** The config error loading `source` throws. */
async function refusal(source: string): Promise<ConfigError> {
  const file = configFile(source)
  const error: unknown = await loadConfig(file).then(
    () => undefined,
    (thrown: unknown) => thrown
  )
  if (!(error instanceof ConfigError)) throw new Error(`no ConfigError: ${String(error)}`)
  expect(error.file).toBe(file)
  expect(error.message.startsWith(`${file}: `)).toBe(true)
  return error
}

describe('loadConfig()', () => {
  it('should resolve the directories against the config file’s own', async () => {
    const file = configFile(
      "export default { input: 'www', output: '../dist/www', passthrough: ['assets/**'] }"
    )

    expect(await loadConfig(file)).toEqual({
      file,
      input: resolve(file, '..', 'www'),
      output: resolve(file, '..', '..', 'dist', 'www'),
      served: [],
      passthrough: ['assets/**'],
      plugins: [],
      dev: { headers: {} },
    })
  })

  it('should take the type a site checks its config against', async () => {
    const file = configFile(
      [
        "import type { Config } from '@limulus/lemmon-shell'",
        "export default { input: 'www', output: 'dist' } satisfies Config",
      ].join('\n')
    )

    expect((await loadConfig(file)).input).toBe(resolve(file, '..', 'www'))
  })

  it('should default to no passthrough globs', async () => {
    const file = configFile("export default { input: 'www', output: 'dist' }")

    expect((await loadConfig(file)).passthrough).toEqual([])
  })

  it('should take an absolute directory as it is', async () => {
    const input = tempDir()
    const file = configFile(`export default { input: '${input}', output: '/srv/dist' }`)

    expect(await loadConfig(file)).toMatchObject({ input, output: '/srv/dist' })
  })

  it('should look for lemmon.config.ts in the working directory by default', async () => {
    await expect(loadConfig()).rejects.toThrow(
      `${resolve('lemmon.config.ts')}: no config file here`
    )
  })

  describe('refusing', () => {
    it('should refuse a file that is not there', async () => {
      const file = join(tempDir(), 'lemmon.config.ts')

      await expect(loadConfig(file)).rejects.toMatchObject({
        name: 'ConfigError',
        file,
        message: `${file}: no config file here`,
      })
    })

    it('should refuse a module that throws as it loads, keeping the cause', async () => {
      const error = await refusal("throw new Error('boom')")

      expect(error.message).toMatch(/: could not be loaded: Error: boom$/)
      expect(error.cause).toBeInstanceOf(Error)
      expect(error.field).toBeUndefined()
    })

    it('should describe a thrown error by its stack up to the frames', async () => {
      const framed = await refusal("throw new Error('boom')")
      const bare = await refusal(
        "const e = new Error('bare'); e.stack = 'Error: bare\\n  at line 1'; throw e"
      )

      expect(framed.message).not.toMatch(/\n {4}at /)
      expect(bare.message).toMatch(/: could not be loaded: Error: bare\n {2}at line 1$/)
    })

    it('should describe a thrown non-error, or a stackless one, as a string', async () => {
      const value = await refusal("throw 'boom'")
      const stackless = await refusal(
        "const e = new Error('quiet'); delete e.stack; throw e"
      )

      expect(value.message).toMatch(/: could not be loaded: boom$/)
      expect(stackless.message).toMatch(/: could not be loaded: Error: quiet$/)
    })

    it('should refuse a module with no default export', async () => {
      expect((await refusal("export const input = 'www'")).message).toMatch(
        /: has no default export$/
      )
    })

    it('should refuse a default export that is not an object', async () => {
      for (const exported of ["'www'", 'null', "['www']"]) {
        expect((await refusal(`export default ${exported}`)).message).toMatch(
          /: the default export must be an object$/
        )
      }
    })

    it('should refuse an unknown field, naming it and the fields there are', async () => {
      const error = await refusal(
        "export default { input: 'www', output: 'dist', passthru: [] }"
      )

      expect(error.field).toBe('passthru')
      expect(error.message).toMatch(
        /: unknown field `passthru`; a config has `input`, `output`, `origin`, `served`, `passthrough`, `plugins`, `dev`$/
      )
    })

    for (const field of ['input', 'output']) {
      const other = field === 'input' ? "output: 'dist'" : "input: 'www'"

      it(`should refuse a missing \`${field}\``, async () => {
        const error = await refusal(`export default { ${other} }`)

        expect(error.field).toBe(field)
        expect(error.message).toMatch(new RegExp(`: \`${field}\` is required$`))
      })

      it(`should refuse an \`${field}\` that is not a non-empty string`, async () => {
        for (const value of ["''", '3']) {
          const error = await refusal(`export default { ${other}, ${field}: ${value} }`)

          expect(error.field).toBe(field)
          expect(error.message).toMatch(
            new RegExp(`: \`${field}\` must be a non-empty string$`)
          )
        }
      })
    }

    it('should refuse an input that names no directory', async () => {
      for (const input of ["'src'", "'www/index.md'"]) {
        const error = await refusal(`export default { input: ${input}, output: 'dist' }`)

        expect(error.field).toBe('input')
        expect(error.message).toMatch(
          /: `input` names no directory: \/.*(src|www\/index\.md)$/
        )
      }
    })

    it('should refuse an output that is the input, or holds it', async () => {
      for (const output of ["'www'", "'.'", "'www/'"]) {
        const error = await refusal(`export default { input: 'www', output: ${output} }`)

        expect(error.field).toBe('output')
        expect(error.message).toMatch(
          /: `output` may not be the input directory or hold it$/
        )
      }
    })

    it('should take an output inside the input, which the walk leaves out', async () => {
      const file = configFile("export default { input: '.', output: '_site' }")

      expect((await loadConfig(file)).output).toBe(resolve(file, '..', '_site'))
    })

    it('should refuse passthrough globs that are not a list', async () => {
      const error = await refusal(
        "export default { input: 'www', output: 'dist', passthrough: 'assets/**' }"
      )

      expect(error.field).toBe('passthrough')
      expect(error.message).toMatch(/: `passthrough` must be a list of glob strings$/)
    })

    it('should refuse a passthrough glob that is not a non-empty string', async () => {
      for (const glob of ["''", '3']) {
        const error = await refusal(
          `export default { input: 'www', output: 'dist', passthrough: ['a/**', ${glob}] }`
        )

        expect(error.field).toBe('passthrough')
        expect(error.message).toMatch(
          /: `passthrough\[1\]` must be a non-empty glob string$/
        )
      }
    })
  })

  describe('plugins', () => {
    /** A config listing `plugins`, the source of a JavaScript expression. */
    const listing = (plugins: string) =>
      `export default { input: 'www', output: 'dist', plugins: ${plugins} }`

    /** Why loading a config that lists `plugins` is refused, past the file's name. */
    async function refused(plugins: string): Promise<string> {
      const error = await refusal(listing(plugins))
      expect(error.field).toBe('plugins')
      return error.message.slice(`${error.file}: `.length)
    }

    /** The plugins a config listing `plugins` loads with. */
    async function loaded(plugins: string) {
      return (await loadConfig(configFile(listing(plugins)))).plugins
    }

    // A producer listing keys names the field they are read from, as one must.
    const produces = (name: string, keys = '') =>
      `{ name: '${name}', ${keys === '' ? '' : `keys: [${keys}], payload: 'Code', keyedBy: 'language', `}produce: () => [] }`
    const decorates = (name: string) => `{ name: '${name}', decorate: (_, c) => c }`
    const plugin = (name: string, ...handlers: string[]) =>
      `{ name: '${name}', marks: [${handlers.join(', ')}] }`

    it('should default to no plugins', async () => {
      const file = configFile("export default { input: 'www', output: 'dist' }")

      expect((await loadConfig(file)).plugins).toEqual([])
    })

    it('should take plugins whose handlers produce and decorate, as listed', async () => {
      const plugins = await loaded(
        `[${plugin('a', produces('styles'), decorates('code'))}, { name: 'b' }]`
      )

      expect(plugins.map((p) => [p.name, p.marks?.map((h) => h.name)])).toEqual([
        ['a', ['styles', 'code']],
        ['b', undefined],
      ])
      // Taken as they are, so a handler is the function the config wrote.
      expect(plugins[0].marks?.[0].produce?.({} as never)).toEqual([])
    })

    it('should refuse plugins that are not a list of objects with a name', async () => {
      expect(await refused("{ name: 'a' }")).toBe('`plugins` must be a list of plugins')
      expect(await refused("[{ name: 'a' }, 'b']")).toBe('`plugins[1]` must be an object')
      expect(await refused('[[]]')).toBe('`plugins[0]` must be an object')
      for (const name of ['', '3', 'undefined']) {
        const value = name === '' ? "''" : name
        expect(await refused(`[{ name: ${value} }]`)).toBe(
          '`plugins[0].name` must be a non-empty string'
        )
      }
    })

    it('should refuse a hole in a list rather than skip it', async () => {
      expect(await refused("[, { name: 'a' }]")).toBe('`plugins[0]` must be an object')
      expect(await refused(`[${plugin('a', '', decorates('head'))}]`)).toBe(
        '`marks[0]` of the plugin `a` must be an object'
      )
      expect(await refused(`[${plugin('a', produces('code', ", 'ts'"))}]`)).toBe(
        "`keys` of the plugin `a`'s handler for `code` must be a non-empty list of " +
          'non-empty strings'
      )
    })

    it('should refuse a function where a plugin belongs, saying to call it', async () => {
      expect(await refused('[function bundler() { return {} }]')).toBe(
        '`plugins[0]` is the function `bundler` rather than a plugin: call it, ' +
          '`bundler()`, for the plugin it makes'
      )
      expect(await refused('[() => ({})]')).toBe(
        '`plugins[0]` is a function rather than a plugin: call the plugin factory for the ' +
          'plugin it makes'
      )
    })

    it('should refuse a name or key the core cannot be sent, and take one it can', async () => {
      // A lone surrogate has no UTF-8 form; a pair is one character, and has.
      expect(await refused(`[${plugin('a', produces('code', "'\\uD800'"))}]`)).toBe(
        "`keys` of the plugin `a`'s handler for `code` must be a non-empty list of " +
          'non-empty strings'
      )
      expect(await refused(`[${plugin('a', decorates('\\uDC00'))}]`)).toBe(
        '`marks[0].name` of the plugin `a` must be a non-empty string'
      )
      const payload = "{ name: 'note', payload: 'N\\uD800', decorate: (_, c) => c }"
      expect(await refused(`[${plugin('a', payload)}]`)).toBe(
        "`payload` of the plugin `a`'s handler for `note` must be a non-empty string"
      )
      expect(
        await loaded(`[${plugin('a', produces('code', "'\\uD83D\\uDE00'"))}]`)
      ).toHaveLength(1)
    })

    it('should refuse an async handler, and take one that returns a promise', async () => {
      // The per-page check is what a promise returned in time meets.
      for (const field of ['produce', 'decorate']) {
        const handler = `{ name: 'styles', ${field}: async () => [] }`

        expect(await refused(`[${plugin('a', handler)}]`)).toBe(
          `\`${field}\` of the plugin \`a\`'s handler for \`styles\` is async, and a ` +
            'handler returns its content as it runs: the build waits for no promise'
        )
      }
      const promising = "{ name: 'styles', produce: () => Promise.resolve([]) }"
      expect(await loaded(`[${plugin('a', promising)}]`)).toHaveLength(1)
    })

    describe('keys for a name', () => {
      const callout = (extra: string, role = 'produce: () => []') =>
        `{ name: 'callout', ${extra}, ${role} }`
      const keyless =
        "the plugin `a`'s handler for `callout` lists keys, and no handler for `callout` " +
        "names `keyedBy`, the payload field a mark's key is read from, so a `mark()` " +
        "call's mark would carry no key: add `keyedBy`, with the record's `payload` — " +
        "`payload: 'Code', keyedBy: 'language'` for `code`"

      it('should refuse keys where no handler of the name names the field they are read from', async () => {
        expect(
          await refused(`[${plugin('a', callout("keys: ['note'], payload: 'Callout'"))}]`)
        ).toBe(keyless)
        // A decorator's keys are read from the same field.
        const decorator = callout("keys: ['note']", 'decorate: (_, c) => c')
        expect(await refused(`[${plugin('a', decorator)}]`)).toBe(keyless)
      })

      it('should take keys where a handler of the name names the field, this one or another', async () => {
        const named = callout("keys: ['note'], payload: 'Callout', keyedBy: 'kind'")
        expect(await loaded(`[${plugin('a', named)}]`)).toHaveLength(1)
        const field = callout(
          "payload: 'Callout', keyedBy: 'kind'",
          'decorate: (_, c) => c'
        )
        const plugins = `[${plugin('a', callout("keys: ['note']"))}, ${plugin('b', field)}]`
        expect(await loaded(plugins)).toHaveLength(2)
      })
    })

    it('should refuse two plugins of one name', async () => {
      expect(await refused(`[${plugin('a')}, ${plugin('b')}, ${plugin('a')}]`)).toBe(
        'two plugins are named `a`; each needs its own, which is what a message calls it'
      )
    })

    it('should refuse a plugin field this shell does not run, naming the ones it does', async () => {
      const seo = (extra: string) =>
        `[{ name: 'seo', marks: [${decorates('head')}]${extra} }]`

      expect(await refused(seo(', transformOutput: (file) => file'))).toBe(
        'the plugin `seo` has `transformOutput`, which this shell does not run; a plugin ' +
          'has `name`, `transformDocument`, `assets`, `marks`'
      )
      expect(await loaded(seo(''))).toHaveLength(1)
    })

    describe('the hooks before compile', () => {
      it('should take a document transform and an asset hook, an async one among them', async () => {
        const plugins = await loaded(
          "[{ name: 'a', transformDocument: ({ body }) => body + '!', " +
            'assets: async () => ({}) }]'
        )

        // Taken as they are, so a hook is the function the config wrote.
        const document = { path: 'a.md', frontMatter: {}, body: 'Hi' }
        expect(plugins[0].transformDocument?.(document)).toBe('Hi!')
        const context = { root: '', input: '', output: '', writes: false, env: {} }
        expect(await plugins[0].assets?.(context)).toEqual({})
      })

      it('should refuse a hook that is not a function', async () => {
        expect(await refused("[{ name: 'a', transformDocument: 'append' }]")).toBe(
          '`transformDocument` of the plugin `a` must be a function'
        )
        expect(await refused("[{ name: 'a', assets: { manifest: {} } }]")).toBe(
          '`assets` of the plugin `a` must be a function'
        )
      })

      it('should refuse an async document transform, and take one that returns a promise', async () => {
        // The per-document check is what a promise returned in time meets.
        expect(
          await refused("[{ name: 'a', transformDocument: async ({ body }) => body }]")
        ).toBe(
          '`transformDocument` of the plugin `a` is async, and a transform returns the ' +
            "document's body as it runs: the build waits for no promise from it"
        )
        const promising = "[{ name: 'a', transformDocument: () => Promise.resolve('') }]"
        expect(await loaded(promising)).toHaveLength(1)
      })
    })

    it('should refuse marks that are not a list of objects with a name', async () => {
      expect(await refused("[{ name: 'a', marks: {} }]")).toBe(
        '`marks` of the plugin `a` must be a list of handlers'
      )
      expect(await refused(`[${plugin('a', produces('styles'), 'null')}]`)).toBe(
        '`marks[1]` of the plugin `a` must be an object'
      )
      expect(await refused(`[${plugin('a', "{ name: '', produce: () => [] }")}]`)).toBe(
        '`marks[0].name` of the plugin `a` must be a non-empty string'
      )
    })

    it('should refuse a handler that neither produces nor decorates', async () => {
      expect(await refused(`[${plugin('a', "{ name: 'styles', keys: ['x'] }")}]`)).toBe(
        "the plugin `a`'s handler for `styles` neither produces nor decorates: it needs " +
          'a `produce` or a `decorate` function'
      )
    })

    it('should refuse a handler field that is not a function where one is', async () => {
      for (const field of ['produce', 'decorate']) {
        expect(await refused(`[${plugin('a', `{ name: 'styles', ${field}: [] }`)}]`)).toBe(
          `\`${field}\` of the plugin \`a\`'s handler for \`styles\` must be a function`
        )
      }
    })

    it('should refuse an unknown handler field, naming the ones there are', async () => {
      const handler = "{ name: 'code', produce: () => [], language: 'ts' }"

      expect(await refused(`[${plugin('a', handler)}]`)).toBe(
        "the plugin `a`'s handler for `code` has `language`, which a mark handler does " +
          'not; a handler has `name`, `keys`, `payload`, `keyedBy`, `produce`, `decorate`'
      )
    })

    it('should refuse keys that are not a non-empty list of non-empty strings', async () => {
      // A producer listing none answers no mark, and neither does such a decorator.
      for (const keys of ["'ts'", '[]', "['ts', '']", '[3]']) {
        const handler = `{ name: 'code', keys: ${keys}, produce: () => [] }`

        expect(await refused(`[${plugin('a', handler)}]`)).toBe(
          "`keys` of the plugin `a`'s handler for `code` must be a non-empty list of " +
            'non-empty strings'
        )
      }
    })

    it('should refuse a payload or key field that is not a non-empty string', async () => {
      for (const [field, extra] of [
        ['payload', "payload: ''"],
        ['keyedBy', "payload: 'Note', keyedBy: 3"],
      ]) {
        const handler = `{ name: 'note', ${extra}, produce: () => [] }`

        expect(await refused(`[${plugin('a', handler)}]`)).toBe(
          `\`${field}\` of the plugin \`a\`'s handler for \`note\` must be a non-empty string`
        )
      }
    })

    it('should refuse a key field without a payload to find it in', async () => {
      const handler = "{ name: 'note', keyedBy: 'kind', produce: () => [] }"

      expect(await refused(`[${plugin('a', handler)}]`)).toBe(
        "the plugin `a`'s handler for `note` keys its marks by `kind` but names no " +
          '`payload` to find that field in'
      )
    })

    it('should refuse two producers of one name, naming both plugins', async () => {
      expect(
        await refused(
          `[${plugin('bundler', produces('styles'))}, ${plugin('site', produces('styles'))}]`
        )
      ).toBe(
        'the plugins `bundler` and `site` both produce `styles`; a mark has one producer ' +
          '(D42)'
      )
      expect(
        await refused(`[${plugin('a', produces('styles'), produces('styles'))}]`)
      ).toBe('the plugin `a` produces `styles` twice; a mark has one producer (D42)')
    })

    it('should refuse a site’s own `styles` producer beside the default plugin, naming both', async () => {
      // The config imports the shell as a site's does, by the file the package exports,
      // since the temporary directory has no `node_modules` to find it in.
      const shell = new URL('../index.ts', import.meta.url).href
      const listed = (plugins: string) =>
        `import { bundler } from '${shell}'\n${listing(plugins)}`
      const error = await refusal(
        listed(`[bundler(), ${plugin('site', produces('styles'))}]`)
      )

      expect(error.field).toBe('plugins')
      expect(error.message).toBe(
        `${error.file}: the plugins \`bundler\` and \`site\` both produce \`styles\`; a ` +
          'mark has one producer (D42)'
      )
      // Producing a name the bundler does not is no overlap.
      const loaded = await loadConfig(
        configFile(listed(`[bundler(), ${plugin('site', produces('toc'))}]`))
      )
      expect(loaded.plugins.map((p) => p.name)).toEqual(['bundler', 'site'])
    })

    it('should refuse two producers of one key, and take producers whose keys are apart', async () => {
      const hljs = plugin('hljs', produces('code', "'ts', 'js'"))

      expect(await refused(`[${hljs}, ${plugin('ts', produces('code', "'ts'"))}]`)).toBe(
        'the plugins `hljs` and `ts` both produce `code` for `ts`; a mark has one ' +
          'producer (D42)'
      )
      // A keyless producer answers the blocks with no language, which no other does.
      const apart = `[${hljs}, ${plugin('rust', produces('code', "'rust'"), produces('code'))}]`
      expect(await loaded(apart)).toHaveLength(2)
    })

    it('should take a producer and a decorator of one name', async () => {
      const plugins = `[${plugin('a', produces('code', "'ts'"))}, ${plugin('b', decorates('code'))}]`

      expect(await loaded(plugins)).toHaveLength(2)
    })

    describe('a payload or key field for one name', () => {
      const note = (extra: string) => `{ name: 'note', ${extra}, decorate: (_, c) => c }`

      it('should refuse two handlers naming different payloads, naming both plugins', async () => {
        const plugins = [
          plugin('a', note("payload: 'Note'")),
          plugin('b', note("keys: ['x']")),
          plugin('c', note("payload: 'Callout'")),
        ]

        expect(await refused(`[${plugins.join(', ')}]`)).toBe(
          'the plugins `a` and `c` name different payloads for `note`, `Note` and ' +
            '`Callout`; every mark of a name carries one payload'
        )
      })

      it('should refuse two handlers naming different key fields', async () => {
        const handlers = [
          note("payload: 'Note', keyedBy: 'kind'"),
          note("payload: 'Note', keyedBy: 'type'"),
        ]

        expect(await refused(`[${plugin('a', ...handlers)}]`)).toBe(
          'the plugin `a` names different key fields for `note`, `kind` and `type`; ' +
            'every mark of a name is keyed by one field'
        )
      })

      it('should take handlers where one names a payload and another none', async () => {
        const plugins = [
          plugin('a', note("payload: 'Note', keyedBy: 'kind'")),
          plugin('b', note("payload: 'Note'"), note("keys: ['x']")),
          plugin('c', "{ name: 'other', payload: 'Callout', produce: () => [] }"),
        ]

        expect(await loaded(`[${plugins.join(', ')}]`)).toHaveLength(3)
      })
    })
  })

  describe('origin', () => {
    /** A config whose `origin` is `origin`, the source of a JavaScript expression. */
    const listing = (origin: string) =>
      `export default { input: 'www', output: 'dist', origin: ${origin} }`

    /** The origin a config whose `origin` is `origin` loads with. */
    async function loaded(origin: string) {
      return (await loadConfig(configFile(listing(origin)))).origin
    }

    /** Why loading a config whose `origin` is `origin` is refused, past the file's name. */
    async function refused(origin: string): Promise<string> {
      const error = await refusal(listing(origin))
      expect(error.field).toBe('origin')
      return error.message.slice(`${error.file}: `.length)
    }

    const RULE =
      ': it is where the site is served, an http or https URL with nothing after its ' +
      'host and port, as `https://example.com`'

    it('should default to none', async () => {
      const omitted = configFile("export default { input: 'www', output: 'dist' }")

      expect(await loadConfig(omitted)).not.toHaveProperty('origin')
    })

    it('should take an origin as the URL parser writes one', async () => {
      expect(await loaded("'https://limulus.net'")).toBe('https://limulus.net')
      expect(await loaded("'HTTPS://Limulus.NET/'")).toBe('https://limulus.net')
      expect(await loaded("'http://localhost:80'")).toBe('http://localhost')
      expect(await loaded("'http://localhost:8080'")).toBe('http://localhost:8080')
      expect(await loaded("'https://[::1]:3000'")).toBe('https://[::1]:3000')
    })

    it('should refuse what is not a string', async () => {
      expect(await refused('3')).toBe(`\`origin\` must be a string${RULE}`)
    })

    it('should refuse, as written, what the URL parser would read past', async () => {
      const reasons: [string, string][] = [
        ['  https://limulus.net  ', 'holds whitespace'],
        ['https://limu\tlus.net', 'holds whitespace'],
        ['limulus.net', 'does not start `http://` or `https://`'],
        ['http:limulus.net', 'does not start `http://` or `https://`'],
        ['https:\\\\limulus.net', 'does not start `http://` or `https://`'],
        ['ftp://limulus.net', 'does not start `http://` or `https://`'],
        ['https://limu\\lus.net', 'holds a backslash'],
      ]
      for (const [origin, reason] of reasons) {
        // A JSON string is a JavaScript one, so the config holds `origin` as it is here.
        const written = JSON.stringify(origin)
        expect(await refused(written)).toBe(
          `\`origin\` is ${written}, which ${reason}${RULE}`
        )
      }
    })

    it('should refuse anything after the host and port, each for what it is', async () => {
      const reasons: [string, string][] = [
        [
          'https://limulus.net/blog/',
          "has a path, and Lemmon has no path prefix: a page's url is a path from `/` on " +
            'the origin',
        ],
        ['https://limulus.net/?', 'has a query, which no origin has'],
        ['https://limulus.net?a=1', 'has a query, which no origin has'],
        ['https://limulus.net/#', 'has a fragment, which no origin has'],
        ['https://eric@limulus.net', 'holds credentials, which no origin has'],
      ]
      for (const [origin, reason] of reasons) {
        expect(await refused(`'${origin}'`)).toBe(
          `\`origin\` is "${origin}", which ${reason}${RULE}`
        )
      }
    })

    it('should refuse what the URL parser refuses', async () => {
      expect(await refused("'https://[::1'")).toBe(
        `\`origin\` is "https://[::1", which the URL parser refuses${RULE}`
      )
    })
  })

  describe('served', () => {
    /** A config whose `served` is `served`, the source of a JavaScript expression. */
    const listing = (served: string) =>
      `export default { input: 'www', output: 'dist', served: ${served} }`

    /** The paths a config whose `served` is `served` loads with. */
    async function loaded(served: string) {
      return (await loadConfig(configFile(listing(served)))).served
    }

    /** Why loading a config whose `served` is `served` is refused, past the file's name. */
    async function refused(served: string): Promise<string> {
      const error = await refusal(listing(served))
      expect(error.field).toBe('served')
      return error.message.slice(`${error.file}: `.length)
    }

    const RULE =
      ': `served` lists paths on the origin its host answers without a file the build ' +
      'writes, each a path from `/` as a link writes it, as `/@eric`'

    it('should default to none', async () => {
      const omitted = configFile("export default { input: 'www', output: 'dist' }")

      expect((await loadConfig(omitted)).served).toEqual([])
    })

    it('should take each path as the URL parser writes one', async () => {
      expect(await loaded("['/@eric', '/tils', '/feed/']")).toEqual([
        '/@eric',
        '/tils',
        '/feed/',
      ])
      expect(await loaded("['/café', '/a//b', '/%7e']")).toEqual([
        '/caf%C3%A9',
        '/a//b',
        '/%7e',
      ])
    })

    it('should refuse what is not a list of strings', async () => {
      expect(await refused("'/@eric'")).toBe(`\`served\` must be a list of paths${RULE}`)
      expect(await refused("['/a', 3]")).toBe(`\`served[1]\` must be a string${RULE}`)
      expect(await refused("['']")).toBe(`\`served[0]\` is empty${RULE}`)
    })

    it('should refuse, as written, what is not a path from `/`, each for what it is', async () => {
      const reasons: [string, string][] = [
        ['@eric', 'does not start `/`'],
        ['https://limulus.net/@eric', 'does not start `/`'],
        ['//limulus.net/@eric', 'starts `//`, which names a host'],
        [' /a', 'holds whitespace'],
        ['/a b', 'holds whitespace'],
        ['/a\tb', 'holds whitespace'],
        ['/a\\b', 'holds a backslash'],
        ['\\a', 'holds a backslash'],
        ['/a?b=1', 'has a query'],
        ['/a?', 'has a query'],
        ['/a#b', 'has a fragment'],
        ['/a/./b', 'has a `.` or `..` segment'],
        ['/a/..', 'has a `.` or `..` segment'],
        ['/a/%2E%2e/b', 'has a `.` or `..` segment'],
      ]
      for (const [path, reason] of reasons) {
        // A JSON string is a JavaScript one, so the config holds `path` as it is here.
        const written = JSON.stringify(path)
        expect(await refused(`[${written}]`)).toBe(
          `\`served[0]\` is ${written}, which ${reason}${RULE}`
        )
      }
    })

    it('should refuse two paths the URL parser writes as one, and keep two that differ', async () => {
      expect(await refused("['/café', '/x', '/caf%C3%A9']")).toBe(
        '`served[2]` is "/caf%C3%A9", which names `/caf%C3%A9`, as `served[0]` does' + RULE
      )
      expect(await refused("['/x', '/x']")).toBe(
        '`served[1]` is "/x", which names `/x`, as `served[0]` does' + RULE
      )
      // Case, a trailing `/`, and a dot that is no segment of its own all name another path.
      expect(await loaded("['/x', '/X', '/x/', '/x.']")).toEqual(['/x', '/X', '/x/', '/x.'])
    })
  })

  describe('dev', () => {
    /** A config whose `dev` is `dev`, the source of a JavaScript expression. */
    const listing = (dev: string) =>
      `export default { input: 'www', output: 'dist', dev: ${dev} }`

    /** Why loading a config whose `dev` is `dev` is refused, past the file's name. */
    async function refused(dev: string): Promise<string> {
      const error = await refusal(listing(dev))
      expect(error.field).toBe('dev')
      return error.message.slice(`${error.file}: `.length)
    }

    /** The headers a config whose `dev.headers` is `headers` loads with. */
    async function loaded(headers: string) {
      return (await loadConfig(configFile(listing(`{ headers: ${headers} }`)))).dev.headers
    }

    it('should default to no headers, with `dev` or its `headers` omitted', async () => {
      const omitted = configFile("export default { input: 'www', output: 'dist' }")

      expect((await loadConfig(omitted)).dev).toEqual({ headers: {} })
      expect((await loadConfig(configFile(listing('{}')))).dev).toEqual({ headers: {} })
    })

    it('should take the headers as written', async () => {
      const headers = await loaded(
        "{ 'Cross-Origin-Opener-Policy': 'same-origin', 'Cross-Origin-Embedder-Policy': " +
          "'require-corp' }"
      )

      expect(headers).toEqual({
        'Cross-Origin-Opener-Policy': 'same-origin',
        'Cross-Origin-Embedder-Policy': 'require-corp',
      })
    })

    it('should take what Node sends: an empty value, a tab, and Latin-1 past ASCII', async () => {
      // U+0085 is a control character, but one Node sends as the byte it is.
      const headers =
        "{ 'X-Empty': '', 'X-Tab': 'a\\tb', 'X-Latin': 'café', 'X-C1': 'a\\u0085b' }"

      expect(await loaded(headers)).toEqual({
        'X-Empty': '',
        'X-Tab': 'a\tb',
        'X-Latin': 'café',
        'X-C1': 'a\u0085b',
      })
    })

    it('should take headers in an object with no prototype', async () => {
      const headers = "Object.assign(Object.create(null), { 'X-A': 'b' })"

      expect({ ...(await loaded(headers)) }).toEqual({ 'X-A': 'b' })
    })

    it('should refuse a `dev` that is not an object written as one', async () => {
      for (const dev of ["'x'", 'null', '[]', 'new Map()']) {
        expect(await refused(dev), dev).toBe('`dev` must be a plain object, `{…}`')
      }
    })

    it('should refuse an unknown field, naming the fields there are', async () => {
      expect(await refused("{ header: { 'X-A': 'b' } }")).toBe(
        'unknown field `dev.header`; `dev` has `headers`'
      )
    })

    it('should refuse headers that are not an object of names and values', async () => {
      for (const headers of [
        "'X-A: b'",
        'null',
        "[['X-A', 'b']]",
        "new Headers({ 'X-A': 'b' })",
      ]) {
        expect(await refused(`{ headers: ${headers} }`), headers).toBe(
          '`dev.headers` must be a plain object, `{…}`, of header names and their values'
        )
      }
    })

    it('should refuse a name that is not a header name', async () => {
      for (const name of ['X A', 'é', '', 'X:A']) {
        expect(await refused(`{ headers: { '${name}': 'b' } }`), name).toBe(
          `\`dev.headers\` names \`${name}\`, which is no header name: a name is ` +
            "letters, digits, and any of ``!#$%&'*+-.^_`|~``"
        )
      }
      expect(await loaded("{ \"X-!#$%&'*+-.^_`|~\": 'b' }")).toHaveProperty(
        "X-!#$%&'*+-.^_`|~"
      )
    })

    it('should refuse a value that is not a string', async () => {
      for (const value of ['3', 'true', "['a', 'b']", 'undefined']) {
        expect(await refused(`{ headers: { 'X-A': ${value} } }`), value).toBe(
          "`dev.headers['X-A']` must be a string"
        )
      }
    })

    it('should refuse a value Node would not send', async () => {
      for (const value of ['a\\nb', 'a\\rb', 'a\\u0000b', 'a\\u007fb', '€']) {
        expect(await refused(`{ headers: { 'X-A': '${value}' } }`), value).toBe(
          "`dev.headers['X-A']` is no header value: a value holds only tabs and the " +
            'characters from U+0020 to U+00FF but U+007F'
        )
      }
    })

    it('should refuse two names equal but for case, naming both', async () => {
      expect(await refused("{ headers: { 'X-A': 'b', 'x-a': 'c' } }")).toBe(
        '`dev.headers` names `X-A` and `x-a`, which are one header'
      )
    })

    /** Why a config naming `name` in `dev.headers` is refused, past the file's name. */
    const owned = (name: string) =>
      `\`dev.headers\` names \`${name}\`, which is the dev server's to send or leave out`

    it('should refuse each name the dev server sends itself, in any case', async () => {
      for (const name of [
        'allow',
        'cache-control',
        'connection',
        'content-length',
        'content-type',
        'date',
        'keep-alive',
        'location',
        'transfer-encoding',
        'upgrade',
      ]) {
        for (const written of [name, name.toUpperCase()]) {
          expect(await refused(`{ headers: { '${written}': 'b' } }`), written).toBe(
            owned(written)
          )
        }
      }
      // A name holding one, or starting as one does, is the site's.
      const neighbours = {
        'X-Content-Type-Options': 'nosniff',
        'Content-Security-Policy': "default-src 'self'",
      }
      expect(await loaded(JSON.stringify(neighbours))).toEqual(neighbours)
    })

    it('should refuse a header that would describe the body other than as it is sent', async () => {
      // A body goes out as the build wrote it, with its length, so it is neither
      // encoded nor followed by trailers; Node refuses to write a `Trailer` beside a
      // length, so the server could not answer at all.
      expect(await refused("{ headers: { 'Content-Encoding': 'gzip' } }")).toBe(
        owned('Content-Encoding')
      )
      expect(await refused("{ headers: { Trailer: 'Server-Timing' } }")).toBe(
        owned('Trailer')
      )
    })
  })
})

describe('contains()', () => {
  it('should hold a directory itself and anything under it', () => {
    expect(contains('/a/www', '/a/www')).toBe(true)
    expect(contains('/a/www', '/a/www/feed/x.md')).toBe(true)
  })

  it('should not hold its parent, a sibling, or a sibling that shares its prefix', () => {
    expect(contains('/a/www', '/a')).toBe(false)
    expect(contains('/a/www', '/a/dist')).toBe(false)
    expect(contains('/a/www', '/a/www-out/x')).toBe(false)
  })

  it('should hold a name that only starts with two dots', () => {
    expect(contains('/a', '/a/..hidden')).toBe(true)
  })
})

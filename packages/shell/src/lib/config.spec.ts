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
      passthrough: ['assets/**'],
      plugins: [],
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
        /: unknown field `passthru`; a config has `input`, `output`, `passthrough`, `plugins`$/
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
          'has `name`, `marks`'
      )
      expect(await loaded(seo(''))).toHaveLength(1)
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

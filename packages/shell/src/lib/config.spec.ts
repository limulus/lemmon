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
        /: unknown field `passthru`; a config has `input`, `output`, `passthrough`$/
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

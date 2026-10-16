import { spawnSync } from 'node:child_process'
import { existsSync, mkdirSync, readFileSync, symlinkSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

import { run, USAGE } from './cli.ts'
import { AMBIENT_FILE } from './program.ts'
import { ENV, PASSTHROUGH, project, SITE, SITE_LEM } from '../mocks/sites.ts'
import { tempDir } from '../mocks/temp-dir.ts'

/** What one run wrote, and the code it exited with. */
interface Ran {
  code: number
  stdout: string
  stderr: string
}

/**
 * Runs `lem` with `args` in `cwd`, capturing what it writes: in `env`, at a fixed
 * time unless `now` is `null`, when the system clock tells it.
 */
async function lem(
  args: string[],
  cwd = tempDir(),
  {
    env = ENV,
    now = new Date('2026-10-15T12:00:00Z'),
  }: { env?: object; now?: Date | null } = {}
): Promise<Ran> {
  const out: string[] = []
  const err: string[] = []
  const code = await run(args, {
    stdout: { write: (text: string) => out.push(text) },
    stderr: { write: (text: string) => err.push(text) },
    cwd: () => cwd,
    env: env as Record<string, string>,
    ...(now === null ? {} : { now: () => now }),
  })
  return { code, stdout: out.join(''), stderr: err.join('') }
}

describe('run()', () => {
  describe('the command line', () => {
    it('should print the usage for --help and -h, and exit 0', async () => {
      for (const flag of ['--help', '-h']) {
        expect(await lem([flag])).toEqual({ code: 0, stdout: USAGE, stderr: '' })
      }
      expect(await lem(['build', '--help'])).toEqual({ code: 0, stdout: USAGE, stderr: '' })
    })

    it('should print the usage and exit 2 with no command', async () => {
      expect(await lem([])).toEqual({
        code: 2,
        stdout: '',
        stderr: `lem: no command given\n\n${USAGE}`,
      })
    })

    it('should print the usage and exit 2 for an unknown command', async () => {
      expect(await lem(['serve'])).toMatchObject({
        code: 2,
        stderr: `lem: unknown command 'serve'\n\n${USAGE}`,
      })
    })

    it('should print the usage and exit 2 for an unknown flag', async () => {
      const ran = await lem(['build', '--verbose'])

      expect(ran.code).toBe(2)
      expect(ran.stderr).toMatch(/^lem: Unknown option '--verbose'/)
      expect(ran.stderr.endsWith(USAGE)).toBe(true)
    })

    it('should print the usage and exit 2 for a flag missing its value', async () => {
      const ran = await lem(['build', '--config'])

      expect(ran.code).toBe(2)
      expect(ran.stderr).toMatch(/^lem: Option '--config <value>' argument missing/)
    })

    it('should print the usage and exit 2 for an argument it does not take', async () => {
      expect(await lem(['build', 'www'])).toMatchObject({
        code: 2,
        stderr: `lem: unexpected argument 'www'\n\n${USAGE}`,
      })
    })
  })

  describe('build', () => {
    it('should build the site and say what it wrote', async () => {
      const cwd = project(SITE, PASSTHROUGH)

      expect(await lem(['build'], cwd)).toEqual({
        code: 0,
        stdout: 'wrote 4 pages, copied 1 file\n',
        stderr: '',
      })
      expect(readFileSync(join(cwd, 'dist/notes/loose/index.html'), 'utf8')).toContain(
        '2026-10-15T12:00:00Z'
      )
    })

    it('should count one in the singular, and read the system clock by default', async () => {
      const cwd = project({ 'site.lem': '', 'a.md': '<lem.build.time.year()>\n' })

      expect(await lem(['build'], cwd, { now: null })).toEqual({
        code: 0,
        stdout: 'wrote 1 page, copied 0 files\n',
        stderr: '',
      })
      expect(readFileSync(join(cwd, 'dist/a/index.html'), 'utf8')).toBe(
        `${new Date().getUTCFullYear()}\n`
      )
    })

    it('should read the config --config names, from the working directory', async () => {
      const cwd = tempDir({
        'site/lemmon.config.ts': "export default { input: 'src', output: 'out' }",
        'site/src/site.lem': '',
        'site/src/a.md': '',
      })

      const ran = await lem(['build', '--config', 'site/lemmon.config.ts'], cwd)

      expect(ran).toMatchObject({ code: 0, stdout: 'wrote 1 page, copied 0 files\n' })
      expect(existsSync(join(cwd, 'site/out/a/index.html'))).toBe(true)
    })

    it('should print each diagnostic at its place, and exit 1 on an error', async () => {
      const cwd = project({
        ...SITE,
        'a.md': '---\ntitle: a: b\n---\n',
        'feed/b.md': '---\nnote: x\nwhen: !!timestamp 2024\n---\n',
        'c.md': '---\nsubhead: I’m here\ntitle: a\ntitle: b\n---\n',
      })

      const ran = await lem(['build'], cwd)

      expect(ran).toEqual({
        code: 1,
        stdout: '',
        stderr: [
          'www/a.md:2:8: error: front matter is not valid YAML: ' +
            'Nested mappings are not allowed in compact mappings',
          'www/c.md:4:1: error: `title` is already given',
          '  www/c.md:3:1: first given here',
          'www/feed/b.md:3:7: warning: Unresolved tag: tag:yaml.org,2002:timestamp',
          'lem: 2 errors, so nothing was written',
          '',
        ].join('\n'),
      })
      expect(existsSync(join(cwd, 'dist'))).toBe(false)
    })

    it('should print every failure the build reports, and exit 1', async () => {
      const failures: [string, Record<string, string>, object, string][] = [
        [
          'a compile error',
          { 'broken.lem': '<lem-fn broken()><lem.nope></lem-fn>\n' },
          ENV,
          'www/broken.lem:1:23: error: undefined name `nope`',
        ],
        [
          'an environment variable that is not set',
          {},
          {},
          'www/site.lem:18:10: error: `motto` loads the environment variable `MOTTO`, ' +
            'which is not set',
        ],
        [
          'a data file that is not there',
          { '_data/photos.json': '' },
          ENV,
          'www/site.lem:17:10: error: `photos` loads `_data/photos.json`, which is not a ' +
            'file in the input directory',
        ],
        [
          'a plugin source',
          { 'site.lem': `${SITE_LEM}<lem-let more: string from plugin("more") />\n` },
          ENV,
          'www/site.lem:19:10: error: `more` loads `plugin("more")`, and plugins are not ' +
            'supported yet (∆c4j)',
        ],
        [
          'a render error',
          { 'posts/b.md': '---\ntitle: B\n---\n<lem.site.photos["nope"].caption>\n' },
          ENV,
          'www/posts/b.md:4:6: error: no key "nope"',
        ],
      ]
      for (const [what, files, env, line] of failures) {
        const site = Object.fromEntries(
          Object.entries({ ...SITE, ...files }).filter(([, text]) => text !== '')
        )
        const cwd = project(site, PASSTHROUGH)

        const ran = await lem(['build'], cwd, { env })

        expect(ran.code, what).toBe(1)
        expect(ran.stdout, what).toBe('')
        expect(ran.stderr, what).toMatch(/\nlem: 1 error, so nothing was written\n$/)
        expect(ran.stderr.startsWith(line), `${what}: ${ran.stderr}`).toBe(true)
        expect(existsSync(join(cwd, 'dist')), what).toBe(false)
      }
    })

    it('should report a file the file system refuses, and exit 1', async () => {
      const cwd = project(SITE, PASSTHROUGH)
      mkdirSync(join(cwd, 'dist'))
      writeFileSync(join(cwd, 'dist/posts'), 'stale')

      const ran = await lem(['build'], cwd)

      expect(ran.code).toBe(1)
      expect(ran.stdout).toBe('')
      expect(ran.stderr).toMatch(
        /^lem: could not write dist\/posts\/a\/index\.html: ENOTDIR: [^\n]*\n/
      )
      expect(ran.stderr).toMatch(
        /\nlem: every page rendered, and the output may hold what was written before it\n$/
      )
    })

    it('should show a label in the shell’s own declarations where the file is', async () => {
      const cwd = project({
        ...SITE,
        'bad.lem': '<lem-type Odd extends Page>\n  url: string\n</lem-type>\n',
      })

      const ran = await lem(['build'], cwd)

      expect(ran.code).toBe(1)
      expect(ran.stderr).toContain(
        'www/bad.lem:2:3: error: `url` is a `url` in `Page` and cannot be redeclared as ' +
          `a \`string\`\n  ${AMBIENT_FILE}:2:3: declared here\n`
      )
    })

    it('should exit 0 when every diagnostic is a warning', async () => {
      const ran = await lem(
        ['build'],
        project({ ...SITE, 'feed/b.md': '---\nwhen: !!timestamp 2024\n---\n' })
      )

      expect(ran.code).toBe(0)
      expect(ran.stderr).toMatch(/: warning: /)
      expect(ran.stdout).toBe('wrote 5 pages, copied 0 files\n')
    })

    it('should report a site with no `site.lem`, and exit 1', async () => {
      const cwd = project({ 'a.md': '' })

      expect(await lem(['build'], cwd)).toEqual({
        code: 1,
        stdout: '',
        stderr:
          `lem: ${join(cwd, 'www', 'site.lem')}: no bindings file: a site declares its ` +
          'collections and data in a `site.lem` without front matter here, and an empty ' +
          'one declares none\n',
      })
    })

    it('should report a config it cannot load, and exit 1', async () => {
      const cwd = tempDir()

      expect(await lem(['build'], cwd)).toEqual({
        code: 1,
        stdout: '',
        stderr: `lem: ${join(cwd, 'lemmon.config.ts')}: no config file here\n`,
      })
    })

    it('should name the line of a syntax error in the config, run as Node runs it', () => {
      // Vitest compiles the config itself and words the error its own way; the bin is
      // where Node's own loader reports one, placing the line in the stack alone.
      const bin = fileURLToPath(new URL('../../bin/lem.ts', import.meta.url))
      const cwd = tempDir({
        'lemmon.config.ts': "export default {\n  input: 'www',\n  output: ,\n}\n",
        'package.json': '{ "type": "module" }\n',
      })

      const ran = spawnSync(process.execPath, ['--conditions=source', bin, 'build'], {
        cwd,
        encoding: 'utf8',
      })

      expect(ran.status).toBe(1)
      expect(ran.stderr).toMatch(
        /: could not be loaded: file:\/\/\S*\/lemmon\.config\.ts:3\n/
      )
      expect(ran.stderr).toMatch(/SyntaxError \[ERR_INVALID_TYPESCRIPT_SYNTAX\]/)
    })

    it('should let a failure that is not the config’s propagate', async () => {
      const cwd = project({})
      symlinkSync('missing.md', join(cwd, 'www', 'post.md'))

      await expect(lem(['build'], cwd)).rejects.toThrow(/ENOENT/)
    })
  })
})

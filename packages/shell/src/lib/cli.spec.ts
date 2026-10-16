import { spawnSync } from 'node:child_process'
import { mkdirSync, symlinkSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

import { run, USAGE } from './cli.ts'
import { tempDir } from '../mocks/temp-dir.ts'

/** What one run wrote, and the code it exited with. */
interface Ran {
  code: number
  stdout: string
  stderr: string
}

/** Runs `lem` with `args` in `cwd`, capturing what it writes. */
async function lem(args: string[], cwd = tempDir()): Promise<Ran> {
  const out: string[] = []
  const err: string[] = []
  const code = await run(args, {
    stdout: { write: (text: string) => out.push(text) },
    stderr: { write: (text: string) => err.push(text) },
    cwd: () => cwd,
  })
  return { code, stdout: out.join(''), stderr: err.join('') }
}

/** A project directory: a config naming `www` and `dist`, and `files` under `www/`. */
function project(files: Record<string, string>, passthrough: string[] = []): string {
  const fields = JSON.stringify({ input: 'www', output: 'dist', passthrough })
  const config = `export default ${fields}`
  const entries = Object.entries(files).map(([path, text]) => [`www/${path}`, text])
  const dir = tempDir({ 'lemmon.config.ts': config, ...Object.fromEntries(entries) })
  mkdirSync(join(dir, 'www'), { recursive: true })
  return dir
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
    it('should summarize the site and list each page with its url', async () => {
      const cwd = project(
        {
          'feed/the-rebound/index.md': '---\ntitle: x\n---\n',
          'feed/index.lem': '---\nurl: /feed.xml\n---\n',
          'tils/topic.lem': '---\nfor:\n  topic: site.tilTopics\n---\n',
          'odd.md': '---\nurl: 3\n---\n',
          'layouts.lem': '<p></p>\n',
          'assets/x.png': '',
          'x.webc': '',
        },
        ['assets/**']
      )

      expect(await lem(['build'], cwd)).toEqual({
        code: 0,
        stdout: [
          'www: 2 documents, 3 templates (2 with front matter), 1 passthrough, 1 ignored',
          '',
          'pages:',
          '  /feed.xml           feed/index.lem',
          '  /feed/the-rebound/  feed/the-rebound/index.md',
          '  3                   odd.md',
          '  (generator)         tils/topic.lem',
          '',
        ].join('\n'),
        stderr: '',
      })
    })

    it('should count one in the singular, and list no pages if none', async () => {
      const cwd = project({ 'a.lem': '', 'b.md': '' })

      expect((await lem(['build'], cwd)).stdout).toBe(
        'www: 1 document, 1 template (0 with front matter), 0 passthrough, 0 ignored\n' +
          '\npages:\n  /b/  b.md\n'
      )
      expect((await lem(['build'], project({}))).stdout).toBe(
        'www: 0 documents, 0 templates (0 with front matter), 0 passthrough, 0 ignored\n'
      )
    })

    it('should read the config --config names, from the working directory', async () => {
      const cwd = tempDir({
        'site/lemmon.config.ts': "export default { input: 'src', output: 'out' }",
        'site/src/a.md': '',
      })

      const ran = await lem(['build', '--config', 'site/lemmon.config.ts'], cwd)

      expect(ran.code).toBe(0)
      expect(ran.stdout).toMatch(/^site\/src: 1 document,/)
    })

    it('should show the input as `.` from inside, and absolute from outside', async () => {
      const cwd = project({})

      expect(
        (await lem(['build', '--config', '../lemmon.config.ts'], join(cwd, 'www'))).stdout
      ).toMatch(/^\.: 0 documents/)
      expect(
        (await lem(['build', '--config', join(cwd, 'lemmon.config.ts')], tempDir())).stdout
      ).toMatch(new RegExp(`^${join(cwd, 'www')}: 0 documents`))
    })

    it('should print each diagnostic at its place, and exit 1 on an error', async () => {
      const cwd = project({
        'a.md': '---\ntitle: a: b\n---\n',
        'feed/b.md': '---\nnote: x\nwhen: !!timestamp 2024\n---\n',
        'c.md': '---\nsubhead: I’m here\ntitle: a\ntitle: b\n---\n',
      })

      const ran = await lem(['build'], cwd)

      expect(ran.code).toBe(1)
      expect(ran.stderr).toBe(
        [
          'www/a.md:2:8: error: front matter is not valid YAML: ' +
            'Nested mappings are not allowed in compact mappings',
          'www/c.md:4:1: error: `title` is already given',
          '  www/c.md:3:1: first given here',
          'www/feed/b.md:3:7: warning: Unresolved tag: tag:yaml.org,2002:timestamp',
          '',
        ].join('\n')
      )
      expect(ran.stdout).toMatch(/^www: 3 documents/)
    })

    it('should exit 0 when every diagnostic is a warning', async () => {
      const ran = await lem(
        ['build'],
        project({ 'b.md': '---\nwhen: !!timestamp 2024\n---\n' })
      )

      expect(ran.code).toBe(0)
      expect(ran.stderr).toMatch(/: warning: /)
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

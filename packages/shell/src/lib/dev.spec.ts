import { spawn } from 'node:child_process'
import { once } from 'node:events'
import {
  chmodSync,
  existsSync,
  mkdirSync,
  readFileSync,
  rmSync,
  symlinkSync,
  unlinkSync,
  writeFileSync,
} from 'node:fs'
import { createServer } from 'node:net'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

import {
  fetched,
  free,
  reloadsBetween,
  started,
  type Event,
  type Page,
  type Running,
} from '../mocks/dev.ts'
import { BARE_LEM, ENV, PASSTHROUGH, project, SITE } from '../mocks/sites.ts'
import { until } from '../mocks/until.ts'

const TAG = '<script type="module" src="/.lemmon/reload.js"></script>'

/** `SITE`'s second post, with `body` beneath its front matter. */
const post = (body: string) => `---\ntitle: B\n---\n${body}\n`

/** The error a post reading a field its type lacks is reported with, framed. */
const NO_BYLINE = [
  'www/posts/b.md:4:6: error: `Post` has no field `byline`',
  '> 4 | <lem.page.byline>',
  '    |      ^^^^^^^^^^^',
  'lem: 1 error, so nothing was written',
  '',
].join('\n')

/** Whether `event` is a write to stderr holding `text`. */
const printed = (text: string) => (event: Event) =>
  event.kind === 'stderr' && event.text.includes(text)

/** Whether `event` is the spec's mark `text`. */
const marked = (text: string) => (event: Event) =>
  event.kind === 'mark' && event.text === text

/** Waits until `page` has reloaded to a body holding `shown`. */
async function reloadedTo(page: Page, shown: string, dev: Running): Promise<void> {
  await until(
    () => page.reloads.some((body) => body.includes(shown)),
    `a reload to a page holding ${JSON.stringify(shown)}`,
    dev.output
  )
}

// What a spec here asserts is something `lem dev` did, and what it did not do only
// between two things it did — never that nothing happened in some time: macOS replays to
// a new watch the writes made just before it began, so a build may come that nothing
// asked for. "No rebuild happened" is the scheduler's spec's, under fake timers. Each
// edit is written once and in place, as an editor without atomic saves writes it, after
// `serving()` has seen a change start a build.
describe('lem dev', { timeout: 30_000 }, () => {
  it('should build, serve the output with the reload client, and stop when interrupted', async () => {
    const cwd = project(SITE, PASSTHROUGH)
    const dev = started(cwd, ['--port', '0'], ENV)
    let port = 0
    try {
      port = await dev.serving()

      expect(dev.stdout()).toMatch(
        new RegExp(
          '^wrote 4 pages, copied 1 file\\nbuilt in \\d+ ms\\n' +
            `serving dist at http://127\\.0\\.0\\.1:${port}/\\n` +
            'restart lem dev after changing lemmon\\.config\\.ts\\n'
        )
      )
      const { status, body } = await fetched(port, '/posts/b/')
      expect(status).toBe(200)
      expect(body).toContain('Hello b.')
      expect(body).toContain(TAG)
      expect(readFileSync(join(cwd, 'dist/posts/b/index.html'), 'utf8')).not.toContain(TAG)
    } finally {
      expect(await dev.stop()).toBe(0)
    }
    expect(await free(port)).toBe(true)
  })

  it('should rebuild an edit, and reload the pages open on it to show it', async () => {
    const cwd = project(SITE, PASSTHROUGH)
    const dev = started(cwd, ['--port', '0'], ENV)
    let page: Page | undefined
    try {
      await dev.serving()
      page = await dev.open('/posts/b/')

      writeFileSync(join(cwd, 'www/posts/b.md'), post('Hello again, b.'))

      await reloadedTo(page, 'Hello again, b.', dev)
      expect(dev.stdout()).toMatch(/\nwrote 4 pages, copied 1 file\nrebuilt in \d+ ms\n/)
    } finally {
      page?.close()
      await dev.stop()
    }
  })

  it('should print an error with its source, and reload nothing until it is fixed', async () => {
    const cwd = project(SITE, PASSTHROUGH)
    const dev = started(cwd, ['--port', '0'], ENV)
    let page: Page | undefined
    try {
      const port = await dev.serving()
      page = await dev.open('/posts/b/')
      const file = join(cwd, 'www/posts/b.md')

      writeFileSync(file, post('<lem.page.byline>'))
      await until(() => dev.stderr().includes(NO_BYLINE), 'the error', dev.output)
      // An error stops a build before it writes, so the last good page is still there.
      // The round trip also lets a reload the error's build sent arrive before the mark.
      expect((await fetched(port, '/posts/b/')).body).toContain('Hello b.')
      dev.mark('fix')
      writeFileSync(file, post('Fixed, b.'))

      await reloadedTo(page, 'Fixed, b.', dev)
      // Builds never overlap, so every reload after the error's report and before the
      // fix is the error's build's own.
      expect(reloadsBetween(dev.events, printed('has no field'), marked('fix'))).toBe(0)
    } finally {
      page?.close()
      await dev.stop()
    }
  })

  it('should serve after a first build that failed, its 404 reloading once it is fixed', async () => {
    const cwd = project({ ...SITE, 'posts/b.md': post('<lem.page.byline>') }, PASSTHROUGH)
    const dev = started(cwd, ['--port', '0'], ENV)
    let page: Page | undefined
    try {
      const port = await dev.serving()
      expect(dev.stderr().startsWith(NO_BYLINE)).toBe(true)
      expect(dev.stdout()).toMatch(/^built in \d+ ms\nserving /)
      // Nothing written, and a tab open on the page has the reload client all the same.
      const missing = await fetched(port, '/posts/b/')
      expect(missing.status).toBe(404)
      expect(missing.body).toContain(TAG)
      page = await dev.open('/posts/b/')

      writeFileSync(join(cwd, 'www/posts/b.md'), post('Fixed, b.'))

      await reloadedTo(page, 'Fixed, b.', dev)
    } finally {
      page?.close()
      await dev.stop()
    }
  })

  it('should print a build that throws or finds no site.lem, and go on', async () => {
    const cwd = project({ 'a.md': 'A\n' })
    const dev = started(cwd)
    const lock = join(cwd, 'www/.#a.md')
    let page: Page | undefined
    try {
      await dev.serving()
      expect(dev.stderr()).toContain(
        `lem: ${join(cwd, 'www', 'site.lem')}: no bindings file`
      )
      page = await dev.open('/a/')

      // A link that names nothing, as an editor's lock file is, fails the walk.
      symlinkSync('eric@limulus.test.1234', lock)
      writeFileSync(join(cwd, 'www/a.md'), 'A, locked\n')
      await until(
        () => dev.stderr().includes('lem: Error: ENOENT: no such file or directory'),
        'the walk’s failure',
        dev.output
      )

      unlinkSync(lock)
      writeFileSync(join(cwd, 'www/site.lem'), BARE_LEM)
      await reloadedTo(page, '<p>A, locked</p>', dev)
    } finally {
      page?.close()
      await dev.stop()
    }
  })

  it('should print a write the file system refuses, and reload nothing until it can write', async () => {
    const cwd = project(SITE, PASSTHROUGH)
    const dev = started(cwd, ['--port', '0'], ENV)
    let page: Page | undefined
    try {
      const port = await dev.serving()
      page = await dev.open('/posts/b/')
      const file = join(cwd, 'www/posts/b.md')

      // A file where a directory of pages was. The output is not watched, so the edit
      // starts the build.
      rmSync(join(cwd, 'dist/posts'), { recursive: true })
      writeFileSync(join(cwd, 'dist/posts'), 'stale')
      writeFileSync(file, post('Refused, b.'))
      await until(
        () => dev.stderr().includes('lem: every page rendered, and the output may hold'),
        'the refused write',
        dev.output
      )
      expect(dev.stderr()).toMatch(
        /^lem: could not write dist\/posts\/a\/index\.html: ENOTDIR: [^\n]*\n/m
      )
      // The round trip lets a reload the refused build sent arrive before the mark.
      await fetched(port, '/posts/b/')
      dev.mark('fix')
      rmSync(join(cwd, 'dist/posts'))
      writeFileSync(file, post('Written, b.'))

      await reloadedTo(page, 'Written, b.', dev)
      expect(reloadsBetween(dev.events, printed('could not write'), marked('fix'))).toBe(0)
    } finally {
      page?.close()
      await dev.stop()
    }
  })

  it('should print a directory it cannot watch, and go on', async () => {
    const cwd = project(SITE, PASSTHROUGH)
    const dev = started(cwd, ['--port', '0'], ENV)
    const locked = join(cwd, 'www/locked')
    let page: Page | undefined
    try {
      await dev.serving()
      page = await dev.open('/posts/b/')

      mkdirSync(locked, { mode: 0 })
      await until(
        () =>
          dev.stderr().includes(`lem: Error: EACCES: permission denied, watch '${locked}'`),
        'the watch’s error',
        dev.output
      )

      chmodSync(locked, 0o755)
      writeFileSync(join(cwd, 'www/posts/b.md'), post('Watched, b.'))
      await reloadedTo(page, 'Watched, b.', dev)
    } finally {
      if (existsSync(locked)) chmodSync(locked, 0o755)
      page?.close()
      await dev.stop()
    }
  })

  it('should rebuild an edit to the file a link in the input names', async () => {
    const cwd = project(
      { ...SITE, 'notes/c.md': '---\ntitle: C\n---\nHello c.\n' },
      PASSTHROUGH
    )
    symlinkSync('../notes/c.md', join(cwd, 'www/posts/c.md'))
    const dev = started(cwd, ['--port', '0'], ENV)
    let page: Page | undefined
    try {
      await dev.serving()
      page = await dev.open('/posts/c/')

      writeFileSync(join(cwd, 'www/notes/c.md'), '---\ntitle: C\n---\nLinked, c.\n')

      await reloadedTo(page, 'Linked, c.', dev)
    } finally {
      page?.close()
      await dev.stop()
    }
  })

  it('should reload nothing for a build an interrupt came during', async () => {
    const cwd = project(SITE, PASSTHROUGH)
    const dev = started(cwd, ['--port', '0'], ENV)
    let page: Page | undefined
    try {
      await dev.serving()
      page = await dev.open('/posts/b/')
      // Interrupted as the edit's build reports what it wrote, before it would reload.
      dev.interruptWhen(
        (event) => event.kind === 'stdout' && event.text.startsWith('wrote ')
      )

      writeFileSync(join(cwd, 'www/posts/b.md'), post('Interrupted, b.'))

      expect(await dev.ended).toBe(0)
      await page.closed
      expect(reloadsBetween(dev.events, marked('interrupted'))).toBe(0)
    } finally {
      page?.close()
      await dev.stop()
    }
  })

  it('should serve at port 8080 by default, and exit 1 where it is in use', async () => {
    // Held here, or by whatever holds it already: in use either way.
    const held = createServer()
    held.on('error', () => {})
    held.listen(8080, '127.0.0.1')
    await once(held, 'listening').catch(() => {})
    const dev = started(project(SITE, PASSTHROUGH), [], ENV)
    try {
      expect(await dev.ended).toBe(1)
      expect(dev.stderr()).toBe(
        'lem: cannot listen on port 8080: address already in use; pass another with ' +
          '--port\n'
      )
      expect(dev.stdout()).toMatch(/^wrote 4 pages, copied 1 file\nbuilt in \d+ ms\n$/)
    } finally {
      await dev.stop()
      if (held.listening) held.close()
    }
  })

  it('should stop without serving where it is interrupted as it starts', async () => {
    const dev = started(project(SITE, PASSTHROUGH), ['--port', '0'], ENV)

    expect(await dev.stop()).toBe(0)
    expect(dev.stdout()).toMatch(/^wrote 4 pages, copied 1 file\nbuilt in \d+ ms\n$/)
  })

  it('should stop on Ctrl-C, run as the bin', async () => {
    const bin = fileURLToPath(new URL('../../bin/lem.js', import.meta.url))
    const cwd = project(SITE, PASSTHROUGH)
    const child = spawn(
      process.execPath,
      ['--conditions=source', bin, 'dev', '--port', '0'],
      {
        cwd,
        env: { ...process.env, ...ENV },
      }
    )
    const out: string[] = []
    child.stdout.setEncoding('utf8').on('data', (text: string) => out.push(text))
    child.stderr.setEncoding('utf8').on('data', (text: string) => out.push(text))
    const exited = once(child, 'exit')
    try {
      await until(
        () => out.join('').includes('\nserving dist at '),
        'the serving line',
        () => out.join('')
      )

      child.kill('SIGINT')

      expect(await exited).toEqual([0, null])
    } finally {
      child.kill('SIGKILL')
    }
  })
})

import { chmodSync, mkdirSync, symlinkSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'

import { watch, type Watcher } from './watch.ts'
import { tempDir } from '../mocks/temp-dir.ts'
import { retried, until } from '../mocks/until.ts'

/** What a watch told of, by the path changed or the error met. */
interface Told {
  changed: string[]
  failed: unknown[]
}

/**
 * Watches an input holding `files`, its output `dist` beside it, collecting what the
 * watch tells of until `test` ends. `test` runs once a write has been seen reported: a
 * watch on macOS reports no change made in its first milliseconds, so a spec can then
 * write each change once.
 */
async function watching(
  files: Record<string, string>,
  test: (input: string, told: Told, watcher: Watcher) => Promise<void>,
  prepare: (input: string) => void = () => {}
): Promise<void> {
  const dir = tempDir(
    Object.fromEntries(Object.entries(files).map(([p, t]) => [`www/${p}`, t]))
  )
  const input = join(dir, 'www')
  mkdirSync(input, { recursive: true })
  prepare(input)
  const told: Told = { changed: [], failed: [] }
  const watcher = await watch(
    { input, output: join(dir, 'dist') },
    (path) => told.changed.push(path),
    (error) => told.failed.push(error)
  )
  try {
    const warmUp = join(input, '.warm-up')
    await retried(
      () => {
        writeFileSync(warmUp, String(Date.now()))
      },
      () => told.changed.includes(warmUp),
      'the warm-up write'
    )
    await test(input, told, watcher)
  } finally {
    await watcher.close()
  }
}

/** Waits until `told` holds a change to `path`. */
async function reported(told: Told, path: string): Promise<void> {
  await until(
    () => told.changed.includes(path),
    `a change to ${path}`,
    () => told.changed.join('\n')
  )
}

// Each assertion is of something the watch did, never of what it did not: macOS replays
// to a new watch the writes made just before it began, so a spec cannot tell a change
// left unreported from one reported late. What is not watched is read from what is.
describe('watch()', { timeout: 20_000 }, () => {
  it('should tell of a file written under the input, by its path', async () => {
    await watching({ 'posts/a.md': 'a' }, async (input, told) => {
      const written = join(input, 'posts', 'b.md')

      writeFileSync(written, 'b')

      await reported(told, written)
    })
  })

  it('should watch nothing the walk passes over', async () => {
    const files = {
      'posts/a.md': '',
      'dist/posts/a/index.html': '',
      'dist.md': '',
      'node_modules/pkg/README.md': '',
      '.git/HEAD': '',
    }
    const dir = tempDir(files)
    const watcher = await watch(
      { input: dir, output: join(dir, 'dist') },
      () => {},
      () => {}
    )
    try {
      const watched = watcher.getWatched()

      expect([...watched[dir]].sort()).toEqual(['dist.md', 'posts'])
      expect(watched[join(dir, 'posts')]).toEqual(['a.md'])
      expect(Object.keys(watched).filter((path) => path.startsWith(dir))).toEqual([
        dir,
        join(dir, 'posts'),
      ])
    } finally {
      await watcher.close()
    }
  })

  it('should watch a linked directory as the link, not what it leads to', async () => {
    // A link to the output, whose every write would otherwise be a change.
    await watching(
      { 'posts/a.md': '' },
      async (input, _told, watcher) => {
        const watched = watcher.getWatched()

        expect(watched[input]).toContain('built')
        expect(Object.keys(watched).filter((path) => path.startsWith(input))).toEqual([
          input,
          join(input, 'posts'),
        ])
      },
      (input) => {
        mkdirSync(join(input, '..', 'dist', 'posts', 'a'), { recursive: true })
        writeFileSync(join(input, '..', 'dist', 'posts', 'a', 'index.html'), '')
        symlinkSync('../dist', join(input, 'built'))
      }
    )
  })

  it('should tell of an edit to the file a link names, where it is under the input', async () => {
    await watching(
      { 'shared/c.md': 'c' },
      async (input, told) => {
        const target = join(input, 'shared', 'c.md')

        writeFileSync(target, 'c, edited')

        await reported(told, target)
      },
      (input) => {
        symlinkSync('../shared/c.md', join(input, 'posts-c.md'))
      }
    )
  })

  it('should tell of a directory it cannot read, and go on watching the rest', async () => {
    await watching({ 'a.md': '' }, async (input, told) => {
      const locked = join(input, 'locked')
      try {
        mkdirSync(locked, { mode: 0 })
        await until(
          () => told.failed.length > 0,
          'an error',
          () => told.changed.join('\n')
        )
        expect(told.failed).toContainEqual(
          expect.objectContaining({
            code: 'EACCES',
            message: expect.stringContaining(locked),
          })
        )

        const written = join(input, 'b.md')
        writeFileSync(written, '')
        await reported(told, written)
      } finally {
        chmodSync(locked, 0o755)
      }
    })
  })
})

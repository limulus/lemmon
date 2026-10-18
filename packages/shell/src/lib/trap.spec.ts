import { writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { setTimeout as sleep } from 'node:timers/promises'
import { setFlagsFromString } from 'node:v8'
import { runInNewContext } from 'node:vm'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { build, check, CoreTrapped } from './build.ts'
import { run } from './cli.ts'
import { free, started } from '../mocks/dev.ts'
import { configOf, ENV, PASSTHROUGH, project, SITE } from '../mocks/sites.ts'
import { until } from '../mocks/until.ts'

/** Two ways a call into the core never returns (∆ugN), and what each throws. */
const SHAPES = [
  {
    // Overflows the `.wasm`'s own stack, in its memory: a trap.
    shape: 'a long operator chain',
    text: `<lem.${'1 + '.repeat(2999)}1>\n`,
    thrown: WebAssembly.RuntimeError,
    message: /^RuntimeError: /,
  },
  {
    // Overflows V8's own stack as the instance runs: no trap, and as poisonous.
    shape: 'a long method chain',
    text: `<lem.[1]${'.first()'.repeat(5000)}>\n`,
    thrown: RangeError,
    message: /^RangeError: Maximum call stack size exceeded$/,
  },
] as const

/** What `lem` says for a trap, after `the core failed with `. */
const SAID = /^lem: the core failed with (.+), and cannot run again in this process\n/

/**
 * Runs the collector, and lets the finalizers it schedules run: one the glue registered
 * for a core left unfreed would free it in the trapped instance, and what that throws
 * fails the run.
 */
async function collected(): Promise<void> {
  setFlagsFromString('--expose-gc')
  const gc = runInNewContext('gc') as () => void
  for (let i = 0; i < 3; i++) {
    gc()
    await sleep(20)
  }
}

/** Runs `lem` with `args` in `cwd`, capturing what it writes. */
async function lem(args: string[], cwd: string) {
  const out: string[] = []
  const err: string[] = []
  const code = await run(args, {
    stdout: { write: (text: string) => out.push(text) },
    stderr: { write: (text: string) => err.push(text) },
    cwd: () => cwd,
    env: ENV,
    interrupted: () => new AbortController().signal,
  })
  return { code, stdout: out.join(''), stderr: err.join('') }
}

// A trap poisons the `.wasm` instance the glue module holds, which every later
// `loadCore` returns; each spec re-evaluates the module for an instance of its own.
describe('a call into the core that never returns', { timeout: 30_000 }, () => {
  beforeEach(() => {
    vi.resetModules()
  })

  afterEach(async () => {
    await collected()
  })

  describe('build() and check()', () => {
    for (const { shape, text, thrown } of SHAPES) {
      it(`should throw CoreTrapped for ${shape}, caused by what the core threw`, async () => {
        const config = configOf(project({ ...SITE, 'deep.lem': text }, PASSTHROUGH))
        const options = { now: new Date('2026-10-17T12:00:00Z'), env: ENV }

        const error: unknown = await build(config, options).catch((e: unknown) => e)

        expect(error).toBeInstanceOf(CoreTrapped)
        const { cause, message } = error as CoreTrapped
        expect(cause).toBeInstanceOf(thrown)
        expect(message).toBe(
          `the core failed with ${String(cause)}, and cannot run again in this process`
        )
        vi.resetModules()
        await expect(check(config, options)).rejects.toBeInstanceOf(CoreTrapped)
      })
    }
  })

  describe('lem build and lem check', () => {
    for (const { shape, text, message } of SHAPES) {
      it(`should say so in a line of their own for ${shape}, and exit 1`, async () => {
        const cwd = project({ ...SITE, 'deep.lem': text }, PASSTHROUGH)

        for (const command of ['build', 'check']) {
          vi.resetModules()
          const ran = await lem([command], cwd)

          expect(ran.code, command).toBe(1)
          expect(ran.stdout, command).toBe('')
          expect(ran.stderr, command).toMatch(new RegExp(`${SAID.source}$`))
          expect(SAID.exec(ran.stderr)?.[1], command).toMatch(message)
        }
      })
    }
  })

  describe('lem dev', () => {
    it('should say so, say to restart it, close, and exit 1, where a rebuild traps', async () => {
      const cwd = project(SITE, PASSTHROUGH)
      const dev = started(cwd, ['--port', '0'], ENV)
      let port = 0
      try {
        port = await dev.serving()

        writeFileSync(join(cwd, 'www/deep.lem'), SHAPES[0].text)

        await until(() => dev.stderr().includes('restart'), 'the trap', dev.output)
        expect(await dev.ended).toBe(1)
        expect(dev.stderr()).toMatch(
          new RegExp(`${SAID.source}lem: restart lem dev to build again\\n$`)
        )
        expect(SAID.exec(dev.stderr())?.[1]).toMatch(SHAPES[0].message)
      } finally {
        await dev.stop()
      }
      expect(await free(port)).toBe(true)
    })

    it('should say so and exit 1 without serving, where its first build traps', async () => {
      const cwd = project({ ...SITE, 'deep.lem': SHAPES[1].text }, PASSTHROUGH)
      const dev = started(cwd, ['--port', '0'], ENV)
      try {
        expect(await dev.ended).toBe(1)
        expect(dev.stdout()).toBe('')
        expect(dev.stderr()).toMatch(
          new RegExp(`${SAID.source}lem: restart lem dev to build again\\n$`)
        )
        expect(SAID.exec(dev.stderr())?.[1]).toMatch(SHAPES[1].message)
      } finally {
        await dev.stop()
      }
    })
  })
})

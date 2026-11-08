import { spawnSync } from 'node:child_process'
import { writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { setTimeout as sleep } from 'node:timers/promises'
import { fileURLToPath } from 'node:url'
import { setFlagsFromString } from 'node:v8'
import { runInNewContext } from 'node:vm'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { build, check, CoreTrapped } from './build.ts'
import { run } from './cli.ts'
import { free, started } from '../mocks/dev.ts'
import { configOf, ENV, PASSTHROUGH, project, SITE } from '../mocks/sites.ts'
import { until } from '../mocks/until.ts'

// Two ways a call into the core never returns (∆ugN), and what each throws. A chain
// recurses on two stacks at once: the `.wasm`'s own, a megabyte of its memory, and V8's,
// 984 KiB by default, which holds a frame for every call the instance makes. Which runs
// out first depends on the machine, since V8 compiles the same `.wasm` to frames of a
// different size on each architecture: `.first()` chained 5,000 times runs out V8's
// stack first on arm64, and the `.wasm`'s first on x64. So each shape runs on a V8 stack
// sized for the stack it names to run out first: a trap on V8's default, in this process,
// and V8's own overflow on a small one, in the bin.

/**
 * Overflows the `.wasm`'s own stack, in its memory: a trap. That stack holds 2,185 terms
 * of this chain on any machine, and a term takes 340 bytes of V8's on arm64, so V8's
 * default stack holds more of them on either architecture.
 */
const TRAP = {
  shape: 'a long operator chain',
  text: `<lem.${'1 + '.repeat(2999)}1>\n`,
  thrown: WebAssembly.RuntimeError,
  message: /^RuntimeError: /,
}

/**
 * Overflows V8's own stack as the instance runs: no trap, and as poisonous. The
 * `.wasm`'s stack holds 1,639 terms of this chain on any machine, and a V8 stack of
 * {@link SMALL_STACK} KiB 351 on arm64; x64's frames would have to be under a quarter
 * the size of arm64's to turn that round.
 */
const OVERFLOW = {
  shape: 'a long method chain',
  text: `<lem.[1]${'.first()'.repeat(5000)}>\n`,
  message: /^RangeError: Maximum call stack size exceeded$/,
}

/**
 * The V8 stack, in KiB, the bin runs {@link OVERFLOW} on: under a third of V8's default,
 * and over three times the 92 `lem` needs to start on arm64.
 */
const SMALL_STACK = 300

const BIN = fileURLToPath(new URL('../../bin/lem.js', import.meta.url))

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

/**
 * Runs the `lem` bin with `args` in `cwd` as Node runs it, on a V8 stack of
 * {@link SMALL_STACK} KiB. A `lem dev` that serves is stopped after 20 s, exit 0.
 */
function lemOnSmallStack(args: string[], cwd: string) {
  const ran = spawnSync(
    process.execPath,
    ['--conditions=source', `--stack-size=${SMALL_STACK}`, BIN, ...args],
    { cwd, encoding: 'utf8', env: { ...process.env, ...ENV }, timeout: 20_000 }
  )
  return { code: ran.status, stdout: ran.stdout, stderr: ran.stderr }
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
    it(`should throw CoreTrapped for ${TRAP.shape}, caused by what the core threw`, async () => {
      const config = configOf(project({ ...SITE, 'deep.lem': TRAP.text }, PASSTHROUGH))
      const options = { now: new Date('2026-10-17T12:00:00Z'), env: ENV }

      const error: unknown = await build(config, options).catch((e: unknown) => e)

      expect(error).toBeInstanceOf(CoreTrapped)
      const { cause, message } = error as CoreTrapped
      expect(cause).toBeInstanceOf(TRAP.thrown)
      expect(message).toBe(
        `the core failed with ${String(cause)}, and cannot run again in this process`
      )
      vi.resetModules()
      await expect(check(config, options)).rejects.toBeInstanceOf(CoreTrapped)
    })
  })

  describe('lem build and lem check', () => {
    it(`should say so in a line of their own for ${TRAP.shape}, and exit 1`, async () => {
      const cwd = project({ ...SITE, 'deep.lem': TRAP.text }, PASSTHROUGH)

      for (const command of ['build', 'check']) {
        vi.resetModules()
        const ran = await lem([command], cwd)

        expect(ran.code, command).toBe(1)
        expect(ran.stdout, command).toBe('')
        expect(ran.stderr, command).toMatch(new RegExp(`${SAID.source}$`))
        expect(SAID.exec(ran.stderr)?.[1], command).toMatch(TRAP.message)
      }
    })

    it(`should say so in a line of their own for ${OVERFLOW.shape}, and exit 1`, () => {
      const cwd = project({ ...SITE, 'deep.lem': OVERFLOW.text }, PASSTHROUGH)

      for (const command of ['build', 'check']) {
        const ran = lemOnSmallStack([command], cwd)

        expect(ran.code, command).toBe(1)
        expect(ran.stdout, command).toBe('')
        expect(ran.stderr, command).toMatch(new RegExp(`${SAID.source}$`))
        expect(SAID.exec(ran.stderr)?.[1], command).toMatch(OVERFLOW.message)
      }
    })
  })

  describe('lem dev', () => {
    it('should say so, say to restart it, close, and exit 1, where a rebuild traps', async () => {
      const cwd = project(SITE, PASSTHROUGH)
      const dev = started(cwd, ['--port', '0'], ENV)
      let port = 0
      try {
        port = await dev.serving()

        writeFileSync(join(cwd, 'www/deep.lem'), TRAP.text)

        await until(() => dev.stderr().includes('restart'), 'the trap', dev.output)
        expect(await dev.ended).toBe(1)
        expect(dev.stderr()).toMatch(
          new RegExp(`${SAID.source}lem: restart lem dev to build again\\n$`)
        )
        expect(SAID.exec(dev.stderr())?.[1]).toMatch(TRAP.message)
      } finally {
        await dev.stop()
      }
      expect(await free(port)).toBe(true)
    })

    it('should say so and exit 1 without serving, where its first build overflows', () => {
      const cwd = project({ ...SITE, 'deep.lem': OVERFLOW.text }, PASSTHROUGH)

      const ran = lemOnSmallStack(['dev', '--port', '0'], cwd)

      expect(ran.code).toBe(1)
      expect(ran.stdout).toBe('')
      expect(ran.stderr).toMatch(
        new RegExp(`${SAID.source}lem: restart lem dev to build again\\n$`)
      )
      expect(SAID.exec(ran.stderr)?.[1]).toMatch(OVERFLOW.message)
    })
  })
})

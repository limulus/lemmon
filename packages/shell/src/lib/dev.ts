import { strict as assert } from 'node:assert'
import { getSystemErrorMessage } from 'node:util'

import { CoreTrapped } from './build.ts'
import type { Io } from './cli.ts'
import { display, type LoadedConfig } from './config.ts'
import { Scheduler } from './scheduler.ts'
import { serve, type DevServer } from './serve.ts'
import { watch } from './watch.ts'

/** What `lem dev` runs with. */
export interface DevOptions {
  config: LoadedConfig
  /** The port served at, on `127.0.0.1`; 0 for one the system picks. */
  port: number
  io: Io
  /**
   * Builds the site once and prints its report, as `lem build` does: whether it built
   * with no error and every file written, so that the pages open on the server may
   * reload.
   */
  build(): Promise<boolean>
}

/**
 * `lem dev`: builds the site, serves the output at `127.0.0.1` with the config's
 * `dev.headers` (`serve`), and builds it again, whole, each time changes to its input
 * settle (`Scheduler`), until the `Io` is interrupted; each build's report ends with
 * how long it took. The input is watched before the first build, so that an edit made
 * while it runs is built after it.
 *
 * A build with no error and every file written reloads every page open on the server.
 * One with an error reloads nothing, since the error stopped it before it wrote, so a
 * browser keeps the last good pages; nor does one the file system refused partway, nor
 * one an interrupt came during, which would reload pages into a server closing. A
 * first build that fails is served all the same, so that the fix reloads it. A build
 * that throws — on a file the walk cannot read, such as the dangling link an editor's
 * lock file is — is printed, and so is an error the watch or the server meets, and the
 * loop goes on; but for a call into the core that never returned (`CoreTrapped`),
 * since the instance it poisoned is the one every later build would load, so `lem dev`
 * stops there and says to restart it. The config is read once, so a change to it needs
 * a restart, which the startup says.
 *
 * Resolves the exit code: 0 once interrupted, after the build in flight; 1 where the
 * port cannot be listened at, or the core trapped.
 */
export async function dev({ config, port, io, build }: DevOptions): Promise<number> {
  const scheduler = new Scheduler()
  const interrupted = io.interrupted()
  interrupted.addEventListener('abort', () => {
    scheduler.close()
  })
  const complain = (error: unknown) => {
    io.stderr.write(`lem: ${String(error)}\n`)
  }
  /** Builds once, reporting how long it took: whether pages may reload. */
  const timed = async (verb: string): Promise<boolean> => {
    const started = performance.now()
    let clean = false
    try {
      clean = await build()
    } catch (error) {
      if (error instanceof CoreTrapped) throw error
      complain(error)
    }
    io.stdout.write(`${verb} in ${Math.round(performance.now() - started)} ms\n`)
    return clean
  }
  const watcher = await watch(
    config,
    () => {
      scheduler.changed()
    },
    complain
  )
  let server: DevServer | undefined
  try {
    await timed('built')
    if (interrupted.aborted) return 0
    const { headers } = config.dev
    try {
      server = await serve({ root: config.output, port, headers, onError: complain })
    } catch (error) {
      // The listen's own error, a port in use or one the system reserves.
      const { errno } = error as { errno: number }
      io.stderr.write(
        `lem: cannot listen on port ${port}: ${getSystemErrorMessage(errno)}; ` +
          'pass another with --port\n'
      )
      return 1
    }
    const cwd = io.cwd()
    io.stdout.write(
      `serving ${display(cwd, config.output)} at http://127.0.0.1:${server.port}/\n` +
        `restart lem dev after changing ${display(cwd, config.file)}\n`
    )
    while (await scheduler.due()) {
      if ((await timed('rebuilt')) && !interrupted.aborted) server.reload()
    }
    return 0
  } catch (error) {
    // Only a trap is let through `timed`; anything else is a bug, rethrown as it is.
    assert(error instanceof CoreTrapped, error as Error)
    io.stderr.write(`lem: ${error.message}\nlem: restart lem dev to build again\n`)
    return 1
  } finally {
    scheduler.close()
    await watcher.close()
    await server?.close()
  }
}

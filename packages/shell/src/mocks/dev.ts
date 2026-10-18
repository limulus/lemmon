import { once } from 'node:events'
import { writeFileSync } from 'node:fs'
import { createServer } from 'node:net'
import { join } from 'node:path'

import { retried, until } from './until.ts'
import { run } from '../lib/cli.ts'

/**
 * One thing that happened while `lem dev` ran, in the order this process saw it: a
 * write to its stdout or stderr, a reload message a page's socket received, or a mark
 * the spec set.
 */
export interface Event {
  kind: 'stdout' | 'stderr' | 'reload' | 'mark'
  text: string
}

/**
 * A page open on a `lem dev`, as a browser has it: the page as it was fetched again
 * after each reload message its socket received — never before one, so a body here was
 * read because a rebuild said to — and when that socket closed.
 */
export interface Page {
  reloads: string[]
  closed: Promise<unknown>
  close(): void
}

/** A `lem dev` running in a directory, and what it has done so far. */
export interface Running {
  stdout(): string
  stderr(): string
  /** Both, as a failed wait shows them. */
  output(): string
  /** Everything that happened, in order. */
  events: Event[]
  /** The exit code, once it ends. */
  ended: Promise<number>
  /** Interrupts it, as Ctrl-C does, and resolves the exit code it ends with. */
  stop(): Promise<number>
  /**
   * Interrupts it as soon as it writes something `when` holds true of, marking that
   * `interrupted`.
   */
  interruptWhen(when: (event: Event) => boolean): void
  /**
   * The port it serves at, once it says it is serving and a change has been seen to
   * start a build: a watch on macOS reports no change made in its first milliseconds,
   * so a spec that has this port can write each change once.
   */
  serving(): Promise<number>
  /** Opens `path` on its server, as {@link Page} describes. */
  open(path: string): Promise<Page>
  /** Records that the spec did `text`, so that what follows can be told from before. */
  mark(text: string): void
}

/** Runs `lem dev` in `cwd` with `args` after it, in `env`, until it is stopped. */
export function started(
  cwd: string,
  args: string[] = ['--port', '0'],
  env: Record<string, string> = {}
): Running {
  const events: Event[] = []
  const controller = new AbortController()
  let interruptWhen: ((event: Event) => boolean) | undefined
  const record = (kind: Event['kind'], text: string) => {
    const event = { kind, text }
    events.push(event)
    if (interruptWhen?.(event) !== true) return
    interruptWhen = undefined
    events.push({ kind: 'mark', text: 'interrupted' })
    controller.abort()
  }
  const written = (kind: Event['kind']) =>
    events
      .filter((event) => event.kind === kind)
      .map((event) => event.text)
      .join('')
  const ended = run(['dev', ...args], {
    stdout: {
      write: (text: string) => {
        record('stdout', text)
      },
    },
    stderr: {
      write: (text: string) => {
        record('stderr', text)
      },
    },
    cwd: () => cwd,
    env,
    interrupted: () => controller.signal,
  })
  let port: number | undefined
  const running: Running = {
    stdout: () => written('stdout'),
    stderr: () => written('stderr'),
    output: () => `stdout:\n${running.stdout()}\nstderr:\n${running.stderr()}`,
    events,
    ended,
    async stop() {
      controller.abort()
      return await ended
    },
    interruptWhen(when) {
      interruptWhen = when
    },
    async serving() {
      const served = await until(
        () => /^serving .* at http:\/\/127\.0\.0\.1:(\d+)\/$/m.exec(running.stdout())?.[1],
        'the serving line',
        running.output
      )
      // A file the walk ignores, which only the watch sees.
      const builds = rebuilds(running)
      await retried(
        () => {
          writeFileSync(join(cwd, 'www', '.warm-up'), String(Date.now()))
        },
        () => rebuilds(running) > builds,
        'a rebuild for the warm-up write',
        running.output
      )
      port = Number(served)
      return port
    },
    async open(path) {
      if (port === undefined) throw new Error('open a page once it is serving')
      return await opened(port, path, (text) => {
        record('reload', text)
      })
    },
    mark(text) {
      record('mark', text)
    },
  }
  return running
}

/** How many rebuilds `dev` has reported. */
const rebuilds = (dev: Running) => dev.stdout().match(/^rebuilt in \d+ ms$/gm)?.length ?? 0

/** Opens `path` on the server at `port`, telling `received` of each reload message. */
async function opened(
  port: number,
  path: string,
  received: (text: string) => void
): Promise<Page> {
  const socket = new WebSocket(`ws://127.0.0.1:${port}/.lemmon/reload`)
  const reloads: string[] = []
  socket.addEventListener('message', (message: MessageEvent) => {
    received(String(message.data))
    // One still on its way as the server closes fails, and is no reload.
    fetch(`http://127.0.0.1:${port}${path}`)
      .then(async (response) => await response.text())
      .then((body) => reloads.push(body))
      .catch(() => {})
  })
  const closed = once(socket, 'close')
  await once(socket, 'open')
  return {
    reloads,
    closed,
    close() {
      socket.close()
    },
  }
}

/**
 * The reload messages received after the first event `from` holds true of and before
 * the first after it `to` does, or before the end.
 */
export function reloadsBetween(
  events: readonly Event[],
  from: (event: Event) => boolean,
  to: (event: Event) => boolean = () => false
): number {
  const start = events.findIndex(from)
  if (start === -1) throw new Error('no event begins the window')
  const after = events.slice(start + 1)
  const end = after.findIndex(to)
  return (end === -1 ? after : after.slice(0, end)).filter((e) => e.kind === 'reload')
    .length
}

/** The body the server at `port` answers `path` with, and its status. */
export async function fetched(
  port: number,
  path: string
): Promise<{ status: number; body: string }> {
  const response = await fetch(`http://127.0.0.1:${port}${path}`)
  return { status: response.status, body: await response.text() }
}

/** Whether `port` at `127.0.0.1` can be listened at: whether nothing holds it. */
export async function free(port: number): Promise<boolean> {
  const server = createServer()
  server.listen(port, '127.0.0.1')
  try {
    await once(server, 'listening')
    return true
  } catch {
    return false
  } finally {
    server.close()
  }
}

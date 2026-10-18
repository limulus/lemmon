import { setTimeout as sleep } from 'node:timers/promises'

/**
 * What `read` returns once it returns something, read every 10 ms: a spec's wait for
 * what a watch or a server does in its own time. Fails after `timeout` ms, saying what
 * it waited for and, from `context`, what there was instead.
 */
export async function until<T>(
  read: () => T | undefined | false | Promise<T | undefined | false>,
  what: string,
  context: () => string = () => '',
  timeout = 10_000
): Promise<T> {
  const deadline = Date.now() + timeout
  for (;;) {
    const value = await read()
    if (value !== undefined && value !== false) return value
    if (Date.now() > deadline) {
      throw new Error(`waited ${timeout} ms for ${what}\n${context()}`)
    }
    await sleep(10)
  }
}

/**
 * Does `act`, and again every half second, until `done` reads true: for a change a
 * watch must report. On macOS, Node's `fs.watch` starts the FSEvents stream beneath it
 * on a thread of its own, and a change made before that stream runs is never reported:
 * under twenty processes at once, a third of the writes made 5 ms after a watch began,
 * and none of those made 100 ms after.
 */
export async function retried(
  act: () => void,
  done: () => boolean,
  what: string,
  context?: () => string
): Promise<void> {
  let last = -Infinity
  await until(
    () => {
      if (Date.now() - last >= 500) {
        act()
        last = Date.now()
      }
      return done()
    },
    what,
    context
  )
}

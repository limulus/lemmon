import { strict as assert } from 'node:assert'

/**
 * How long, in milliseconds, no change may arrive before the changes so far are taken
 * as settled: chokidar's own 50 ms, the window in which it drops a second `change` to
 * one path (an editor writing a file twice, say). A rebuild that starts later than that
 * after the last change reported reads what the dropped one wrote; one that started
 * sooner could read the first write and never hear of the second.
 */
export const QUIET = 50

/**
 * When `lem dev` rebuilds: once changes have settled, at most once for every burst of
 * them, and never while a rebuild runs, since the loop asks {@link due} only between
 * them. Changes that arrive while a rebuild runs make it due once more, after it.
 * Separate from the file system, so that its timing is tested with fake timers.
 */
export class Scheduler {
  /** The quiet period's timer, while changes are arriving. */
  #timer: NodeJS.Timeout | undefined
  /** Whether a burst of changes has settled since {@link due} last answered. */
  #settled = false
  #closed = false
  /** Wakes the loop waiting in {@link due}, where it is waiting. */
  #wake: (() => void) | undefined

  /** Reports a change, which settles once {@link QUIET} passes with no other. */
  changed(): void {
    if (this.#closed) return
    clearTimeout(this.#timer)
    this.#timer = setTimeout(() => {
      this.#settled = true
      this.#wake?.()
    }, QUIET)
  }

  /**
   * Resolves `true` once a rebuild is due — at once where changes settled since it last
   * answered, while the last rebuild ran — or `false` once the scheduler is closed. One
   * loop asks, and only between rebuilds.
   */
  async due(): Promise<boolean> {
    if (!this.#settled && !this.#closed) {
      assert(this.#wake === undefined, 'one loop waits on the scheduler')
      await new Promise<void>((resolve) => {
        this.#wake = resolve
      })
      this.#wake = undefined
    }
    this.#settled = false
    return !this.#closed
  }

  /**
   * Stops scheduling: {@link due} answers `false` from now on, and a change is ignored.
   * A rebuild already running finishes, since only the loop running it can stop.
   */
  close(): void {
    this.#closed = true
    clearTimeout(this.#timer)
    this.#wake?.()
  }
}

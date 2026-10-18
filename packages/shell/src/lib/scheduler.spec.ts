import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { QUIET, Scheduler } from './scheduler.ts'

/** What `due()` has answered so far: `undefined` while it waits. */
function asked(scheduler: Scheduler): { answer: boolean | undefined } {
  const state: { answer: boolean | undefined } = { answer: undefined }
  void scheduler.due().then((due) => {
    state.answer = due
  })
  return state
}

/** Lets the fake clock run `ms`, and every promise it settles resolve. */
const elapse = async (ms: number) => {
  await vi.advanceTimersByTimeAsync(ms)
}

describe('Scheduler', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('should make a rebuild due once a change has had the quiet period to itself', async () => {
    const scheduler = new Scheduler()
    scheduler.changed()
    const due = asked(scheduler)

    await elapse(QUIET - 1)
    expect(due.answer).toBeUndefined()
    await elapse(1)
    expect(due.answer).toBe(true)
  })

  it('should wait out a burst of changes and make one rebuild due for it', async () => {
    const scheduler = new Scheduler()
    const due = asked(scheduler)
    for (let i = 0; i < 5; i++) {
      scheduler.changed()
      await elapse(QUIET - 1)
    }
    expect(due.answer).toBeUndefined()
    await elapse(1)
    expect(due.answer).toBe(true)

    const next = asked(scheduler)
    await elapse(10 * QUIET)
    expect(next.answer).toBeUndefined()
  })

  it('should make nothing due with no change', async () => {
    const due = asked(new Scheduler())

    await elapse(10 * QUIET)
    expect(due.answer).toBeUndefined()
  })

  describe('changes while a rebuild runs', () => {
    it('should make one more due, at once, where they settled before it ended', async () => {
      const scheduler = new Scheduler()
      scheduler.changed()
      await elapse(QUIET)
      expect(await scheduler.due()).toBe(true)

      // The rebuild runs, and the loop asks again only once it has ended.
      for (let i = 0; i < 3; i++) {
        scheduler.changed()
        await elapse(QUIET / 2)
      }
      await elapse(QUIET)
      const once = asked(scheduler)
      await elapse(0)
      expect(once.answer).toBe(true)

      const twice = asked(scheduler)
      await elapse(10 * QUIET)
      expect(twice.answer).toBeUndefined()
    })

    it('should make one more due once they settle, where they had not when it ended', async () => {
      const scheduler = new Scheduler()
      scheduler.changed()
      await elapse(QUIET)
      expect(await scheduler.due()).toBe(true)

      scheduler.changed()
      await elapse(QUIET - 1)
      const due = asked(scheduler)
      await elapse(0)
      expect(due.answer).toBeUndefined()
      await elapse(1)
      expect(due.answer).toBe(true)
    })
  })

  describe('closing', () => {
    it('should answer a loop waiting for a rebuild that none is due', async () => {
      const scheduler = new Scheduler()
      const due = asked(scheduler)
      scheduler.changed()

      scheduler.close()
      await elapse(0)
      expect(due.answer).toBe(false)
    })

    it('should make nothing due that settled while the last rebuild ran', async () => {
      const scheduler = new Scheduler()
      scheduler.changed()
      await elapse(QUIET)
      expect(await scheduler.due()).toBe(true)
      scheduler.changed()
      await elapse(QUIET)

      scheduler.close()
      expect(await scheduler.due()).toBe(false)
    })

    it('should leave no quiet period running, from a change before it or after', () => {
      const scheduler = new Scheduler()
      scheduler.changed()

      scheduler.close()
      expect(vi.getTimerCount()).toBe(0)
      scheduler.changed()
      expect(vi.getTimerCount()).toBe(0)
    })
  })
})

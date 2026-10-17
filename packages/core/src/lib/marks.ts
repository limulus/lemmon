import type { Fragment, Mark, RenderResult } from './types.ts'

/**
 * What a producer is given: every mark on this page it answers, the fragments the page
 * collected under their name, and the page as the core returned it — so a handler can
 * read the default content a mark stands over.
 */
export interface MarkContext {
  name: string
  marks: Mark[]
  fragments: Fragment[]
  page: Uint8Array
}

/**
 * One handler for one mark name (D42). A **producer** makes the content, and runs at
 * most once per page, for every mark it answers; a **decorator** transforms whatever
 * content a mark has, and runs per mark. A handler may be both.
 *
 * Under one name a mark is dispatched by its key. A producer lists every key it covers
 * and answers only marks carrying one of them — there is no wildcard producer — and a
 * producer that lists none answers only marks with no key, as the shell's `styles`
 * bundler does. A decorator that lists keys runs only on marks carrying one of them; one
 * that lists none runs on every mark of its name.
 *
 * Content crosses as a string or as bytes: a string is encoded once, so a handler that
 * already holds bytes need not decode them.
 */
export interface MarkHandler {
  /** The name this handler answers for. */
  name: string
  /** The keys it answers for, where the name's marks are keyed (above). */
  keys?: readonly string[]
  /** One replacement per mark it answers, in the order the marks were given. */
  produce?: (context: MarkContext) => readonly (string | Uint8Array)[]
  /** This mark's content so far, replaced by what is returned. */
  decorate?: (mark: Mark, content: Uint8Array) => string | Uint8Array
}

/** A mark no handler answered: its name, and its key where it has one. */
export interface Unclaimed {
  name: string
  key?: string
}

/** A page with its marks filled in, and what nothing answered for. */
export interface Resolved {
  /**
   * The page, with each mark's range replaced by what its handlers made. A page that
   * held no marks comes back as the render's own bytes rather than a copy of them.
   */
  html: Uint8Array
  /**
   * Every name and key no producer or decorator answered, once each, in the order the
   * page first marks it — a code block in a language no highlighter covers. Their marks
   * keep the default content the core wrote, so the page is correct: this is what a
   * build reports, not what it fails on.
   */
  unclaimed: Unclaimed[]
  /**
   * Every name the page collected fragments under but never marked, so nothing was
   * placed to receive them — a layout missing its `<lem.styles()>`. Look the fragments
   * up in {@link RenderResult.fragments} to say where they came from.
   */
  unplaced: string[]
}

const encoder = new TextEncoder()

/**
 * Whether `handler` produces at all: what {@link resolveMarks} dispatches producers by,
 * and {@link producerOverlap} counts them by.
 */
const isProducer = (handler: { produce?: unknown }): boolean =>
  typeof handler.produce === 'function'

/** Whether `handler` is the producer for `mark`: no wildcard, and no key for no key. */
const produces = (handler: MarkHandler, mark: Mark): boolean =>
  handler.keys === undefined
    ? mark.key === undefined
    : mark.key !== undefined && handler.keys.includes(mark.key)

/** Whether `handler` decorates `mark`: every mark of its name unless it lists keys. */
const decorates = (handler: MarkHandler, mark: Mark): boolean =>
  handler.keys === undefined || (mark.key !== undefined && handler.keys.includes(mark.key))

/**
 * Fills each mark of a rendered page with what the host's handlers make of it, before
 * any output transform (D42).
 *
 * Each producer runs once, over the marks of its name it answers, given the page as the
 * core returned it — never another producer's output. Then every decorator runs over the
 * marks it decorates, in the order the handlers are given, each over the previous
 * one's output. A mark nothing answers keeps the default content the core wrote and is
 * reported in {@link Resolved.unclaimed}.
 *
 * A code block's mark carries a `payloadId`, not its code: a producer for `code` looks
 * the payload up among those `Core.compile` returned, which the host keeps.
 *
 * @throws If the render failed, if two producers answer one name and key — or two
 * produce one name without keys — if a producer returns a number of replacements other
 * than one per mark it answers, or if the marks do not stand in order: each a caller
 * bug rather than anything a template can provoke. A host that loads handlers refuses
 * the overlap first, with {@link producerOverlap}, naming whoever listed the two: the
 * shell's config load names both plugins.
 */
export function resolveMarks(
  result: RenderResult,
  handlers: readonly MarkHandler[] = []
): Resolved {
  const { html, marks, fragments } = result
  if (html === null || marks === null || fragments === null) {
    throw new TypeError('a render that failed has no page to resolve')
  }
  const overlap = producerOverlap(handlers)
  if (overlap !== undefined) {
    const { name, key } = overlap
    throw new TypeError(
      key === undefined
        ? `two handlers produce \`${name}\`; only one may`
        : `two handlers produce \`${name}\` for \`${key}\`; only one may`
    )
  }
  const producers = handlers.filter(isProducer)
  const unplaced = Object.keys(fragments).filter(
    (name) => !marks.some((mark) => mark.name === name)
  )
  const contents = marks.map((mark) => html.subarray(mark.start, mark.end))
  const answered = marks.map(() => false)
  for (const producer of producers) {
    const mine = marks
      .map((mark, index) => ({ mark, index }))
      .filter(({ mark }) => mark.name === producer.name && produces(producer, mark))
    if (mine.length === 0) continue
    const made = producer.produce!({
      name: producer.name,
      marks: mine.map(({ mark }) => mark),
      fragments: Object.hasOwn(fragments, producer.name) ? fragments[producer.name] : [],
      page: html,
    })
    if (made.length !== mine.length) {
      throw new TypeError(
        `the producer for \`${producer.name}\` returned ${made.length} replacements ` +
          `for ${mine.length} marks`
      )
    }
    for (const [at, { index }] of mine.entries()) {
      contents[index] = bytes(made[at])
      answered[index] = true
    }
  }
  for (const decorator of handlers.filter((handler) => handler.decorate)) {
    for (const [index, mark] of marks.entries()) {
      if (mark.name === decorator.name && decorates(decorator, mark)) {
        contents[index] = bytes(decorator.decorate!(mark, contents[index]))
        answered[index] = true
      }
    }
  }
  const unclaimed: Unclaimed[] = []
  for (const [index, { name, key }] of marks.entries()) {
    if (answered[index]) continue
    if (unclaimed.some((seen) => seen.name === name && seen.key === key)) continue
    unclaimed.push(key === undefined ? { name } : { name, key })
  }
  return { html: splice(html, marks, contents), unclaimed, unplaced }
}

/** Two producers that would both answer one mark, as {@link producerOverlap} finds them. */
export interface ProducerOverlap<H> {
  /** The one given first. */
  first: H
  second: H
  /** The name both produce. */
  name: string
  /** The key both list under it, where they list keys; absent for two keyless ones. */
  key?: string
}

/**
 * The first two producers among `handlers` that would both answer one mark — two keyless
 * producers of one name, or two that list one key under it — or `undefined` where every
 * mark has one producer at most. A decorator overlaps nothing, and one producer listing
 * a key twice is still one producer. Checked over the handlers rather than a page, so a
 * plugin list that cannot work fails on the first page it meets, or before any.
 *
 * This is the rule {@link resolveMarks} throws on, exported so that a host loading
 * handlers can refuse the same lists and say who listed the two. It reads whether a
 * handler produces, never what with, so it takes a host's own handler shape — and hands
 * back the two as given.
 */
export function producerOverlap<
  H extends Pick<MarkHandler, 'name' | 'keys'> & { produce?: unknown },
>(handlers: readonly H[]): ProducerOverlap<H> | undefined {
  const seen = new Map<string, H>()
  for (const handler of handlers) {
    if (!isProducer(handler)) continue
    const { name, keys } = handler
    for (const key of keys === undefined ? [undefined] : new Set(keys)) {
      const slot = JSON.stringify([name, key ?? null])
      const first = seen.get(slot)
      if (first !== undefined) {
        const overlap = { first, second: handler, name }
        return key === undefined ? overlap : { ...overlap, key }
      }
      seen.set(slot, handler)
    }
  }
  return undefined
}

/** A handler's answer as bytes: a string is encoded once, bytes pass through. */
function bytes(content: string | Uint8Array): Uint8Array {
  return typeof content === 'string' ? encoder.encode(content) : content
}

/**
 * The page with each mark's range replaced by its content. The marks stand in
 * non-decreasing order of `start` and never overlap, which is what lets one pass build
 * the page; a result that says otherwise was not one the core made.
 */
function splice(html: Uint8Array, marks: Mark[], contents: Uint8Array[]): Uint8Array {
  if (marks.length === 0) return html
  const parts: Uint8Array[] = []
  let at = 0
  for (const [index, mark] of marks.entries()) {
    if (mark.start < at || mark.end < mark.start) {
      throw new TypeError(`the mark \`${mark.name}\` does not stand in order`)
    }
    parts.push(html.subarray(at, mark.start), contents[index])
    at = mark.end
  }
  parts.push(html.subarray(at))
  const out = new Uint8Array(parts.reduce((size, part) => size + part.length, 0))
  let written = 0
  for (const part of parts) {
    out.set(part, written)
    written += part.length
  }
  return out
}

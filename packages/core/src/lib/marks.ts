import type { Fragment, Mark, RenderResult } from './types.ts'

/**
 * What a producer is given: every mark of its name on this page, the fragments the page
 * collected under that name, and the page as the core returned it — so a handler can
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
 * most once per page per name, whatever the page marks; a **decorator** transforms
 * whatever content a mark has, and runs per mark. A handler may be both.
 *
 * Content crosses as a string or as bytes: a string is encoded once, so a handler that
 * already holds bytes need not decode them.
 */
export interface MarkHandler {
  /** The name this handler answers for. */
  name: string
  /** One replacement per mark of `name`, in the order the marks were given. */
  produce?: (context: MarkContext) => readonly (string | Uint8Array)[]
  /** This mark's content so far, replaced by what is returned. */
  decorate?: (mark: Mark, content: Uint8Array) => string | Uint8Array
}

/** A page with its marks filled in, and what nothing answered for. */
export interface Resolved {
  /**
   * The page, with each mark's range replaced by what its handlers made. A page that
   * held no marks comes back as the render's own bytes rather than a copy of them.
   */
  html: Uint8Array
  /**
   * Every marked name no handler claimed, in the order the page marks them. Their marks
   * keep the default content the core wrote, so the page is correct — this is what a
   * build reports, not what it fails on.
   */
  unclaimed: string[]
  /**
   * Every name the page collected fragments under but never marked, so nothing was
   * placed to receive them — a layout missing its `<lem.styles()>`. Look the fragments
   * up in {@link RenderResult.fragments} to say where they came from.
   */
  unplaced: string[]
}

const encoder = new TextEncoder()

/**
 * Fills each mark of a rendered page with what the host's handlers make of it, before
 * any output transform (D42).
 *
 * Handlers run in the order given: the one producer for a name makes each of its marks'
 * content, then every decorator for that name runs over what stands there, each over the
 * previous one's output. A name no handler claims keeps the default content the core
 * wrote and is reported in {@link Resolved.unclaimed}.
 *
 * @throws If the render failed, if two handlers produce for one name, if a producer
 * returns a number of replacements other than one per mark, or if the marks do not stand
 * in order — each a caller bug rather than anything a template can provoke. (The
 * plugin-load error that names both plugins claiming a name is the shell's, ∆PgF.)
 */
export function resolveMarks(
  result: RenderResult,
  handlers: readonly MarkHandler[] = []
): Resolved {
  const { html, marks, fragments } = result
  if (html === null || marks === null || fragments === null) {
    throw new TypeError('a render that failed has no page to resolve')
  }
  const unplaced = Object.keys(fragments).filter(
    (name) => !marks.some((mark) => mark.name === name)
  )
  const unclaimed: string[] = []
  const contents = marks.map((mark) => html.subarray(mark.start, mark.end))
  for (const name of new Set(marks.map((mark) => mark.name))) {
    const mine = marks
      .map((mark, index) => ({ mark, index }))
      .filter(({ mark }) => mark.name === name)
    const producers = handlers.filter((handler) => handler.name === name && handler.produce)
    if (producers.length > 1) {
      throw new TypeError(`two handlers produce \`${name}\`; only one may`)
    }
    const [producer] = producers
    if (producer?.produce) {
      const made = producer.produce({
        name,
        marks: mine.map(({ mark }) => mark),
        fragments: Object.hasOwn(fragments, name) ? fragments[name] : [],
        page: html,
      })
      if (made.length !== mine.length) {
        throw new TypeError(
          `the producer for \`${name}\` returned ${made.length} replacements for ` +
            `${mine.length} marks`
        )
      }
      for (const [at, { index }] of mine.entries()) {
        contents[index] = bytes(made[at])
      }
    }
    const decorators = handlers.filter(
      (handler) => handler.name === name && handler.decorate
    )
    for (const decorator of decorators) {
      for (const { mark, index } of mine) {
        contents[index] = bytes(decorator.decorate!(mark, contents[index]))
      }
    }
    if (!producer && decorators.length === 0) unclaimed.push(name)
  }
  return { html: splice(html, marks, contents), unclaimed, unplaced }
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

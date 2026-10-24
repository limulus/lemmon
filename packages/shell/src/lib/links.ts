/**
 * Where a page's markup holds urls, as `@11ty/posthtml-urls` reads them for Eleventy: the
 * attributes, by the element they stand on, and how a `srcset` and a `ping` list theirs.
 * The body pass rebases what this names (D46), and the link check reads the table and
 * the `srcset` candidates (D47), splitting a `ping` as HTML does instead.
 */

import { createRequire } from 'node:module'

/** One candidate of a `srcset`: its url, and the descriptors it was written with. */
export interface Candidate {
  url: string
  /** Its width descriptor, `640w`. */
  w?: number
  /** Its height descriptor, `480h`. */
  h?: number
  /** Its pixel density descriptor, `2x`. */
  d?: number
}

/**
 * `parse-srcset` 1.0.2, which posthtml-urls parses a `srcset` with for Eleventy: the
 * candidates by the WHATWG algorithm, one whose descriptors do not parse left out and said
 * so with `console.log`. Required, since it ships no types and none are published for it.
 */
const parseSrcset = createRequire(import.meta.url)('parse-srcset') as (
  input: string
) => Candidate[]

/**
 * The attributes holding a url, by the element they stand on: the table
 * `@11ty/posthtml-urls` walks for Eleventy, but its `meta` `content`, which only a
 * refreshing `http-equiv` makes a url. Keyed by the tag name alone, as there, so an SVG
 * `<a href>` is in it and a custom element's `href` is not; a namespaced attribute, SVG's
 * `xlink:href`, is in no row.
 */
export const LINKS: ReadonlyMap<string, readonly string[]> = new Map([
  ['a', ['href', 'ping']],
  ['area', ['href', 'ping']],
  ['audio', ['src']],
  ['base', ['href']],
  ['blockquote', ['cite']],
  ['button', ['formaction']],
  ['del', ['cite']],
  ['embed', ['src']],
  ['form', ['action']],
  ['iframe', ['src']],
  ['img', ['src', 'srcset']],
  ['input', ['formaction', 'src']],
  ['ins', ['cite']],
  ['link', ['href']],
  ['object', ['data']],
  ['q', ['cite']],
  ['script', ['src']],
  ['source', ['src', 'srcset']],
  ['track', ['src']],
  ['video', ['poster', 'src']],
])

/**
 * The candidates of `value`, a `srcset`, as `parse-srcset` reads them, and the descriptor
 * of each it refused, by {@link refusal} — which it would otherwise say on standard
 * output, past the streams `lem` writes its diagnostics to, naming no file.
 */
export function srcset(value: string): { candidates: Candidate[]; refused: string[] } {
  const refused: string[] = []
  const log = console.log
  console.log = (message: string) => {
    refused.push(refusal(value, message))
  }
  try {
    return { candidates: parseSrcset(value), refused }
  } finally {
    console.log = log
  }
}

/**
 * The descriptor named by `message`, which `parse-srcset` 1.0.2 logs on refusing a
 * candidate of `value` as `Invalid srcset descriptor found in '<value>' at '<descriptor>'.`
 * — read past that prefix, since `value` may hold ` at '` itself. A message of any other
 * shape is kept whole, so that another release's wording reads oddly in the warning rather
 * than throwing out of the parse.
 */
export function refusal(value: string, message: string): string {
  const prefix = `Invalid srcset descriptor found in '${value}' at '`
  const rest = message.startsWith(prefix) ? message.slice(prefix.length) : ''
  return rest.endsWith("'.") ? rest.slice(0, -2) : message
}

/**
 * The urls of `value`, a `ping`, as posthtml-urls lists them for the body pass: split at
 * commas, trimmed.
 */
export function pings(value: string): string[] {
  return value
    .split(',')
    .map((url) => url.trim())
    .filter((url) => url !== '')
}

/**
 * Rebases a document's body for reading on another page (D46): every link in it made
 * absolute against the site's origin, by the rule Eleventy's HTML base plugin applies to
 * a feed's content, or root-relative where the site names no origin. The body is parsed
 * with parse5 and only the attributes that change are rewritten, so every other byte of
 * it stays as the core rendered it.
 */

import { createRequire } from 'node:module'
import { parseFragment, type DefaultTreeAdapterTypes as Tree } from 'parse5'

/** One candidate of a `srcset`: its url, and the descriptors it was written with. */
interface Candidate {
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
 * refreshing `http-equiv` makes a url, and which a body does not hold. Keyed by the tag
 * name alone, as there, so an SVG `<a href>` is rebased and a custom element's `href` is
 * not; a namespaced attribute, SVG's `xlink:href`, is in no row.
 */
const LINKS: ReadonlyMap<string, readonly string[]> = new Map([
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
 * What a rebase could not do, which the attribute it stands in shows: `at` is the element
 * and attribute, `<a href>`. A url the URL parser refuses, even against the page's url,
 * is kept as written; and a `srcset` candidate whose descriptors do not parse is dropped
 * from the attribute, which its other candidates' rebasing wrote anew — `srcset` the value
 * as written, `descriptor` the one `parse-srcset` refused.
 */
export type Unrebased =
  | { kind: 'refused'; at: string; url: string }
  | { kind: 'dropped'; at: string; srcset: string; descriptor: string }

/** A body rebased, with what could not be. */
export interface Rebased {
  body: string
  unrebased: Unrebased[]
}

/**
 * `body`, the body of the page at `url` — a path from `/`, as `plan()` reports it — with
 * every link its elements hold rebased for reading on another page:
 *
 * - a url the URL parser reads on its own — `https:`, `mailto:`, `data:` — is kept,
 *   trimmed;
 * - with `origin`, any other is resolved against the page's url on that origin, whole,
 *   so `../x/#a`, `./x.pdf`, `#fn1`, `/y` and `//cdn/z` all become absolute;
 * - with none, a url that does not start with `/` and names no host of its own is
 *   resolved against the page's url and becomes its path, query and fragment, so it still
 *   names what it named, read anywhere on the site; any other is kept, trimmed.
 *
 * A `srcset`'s candidates and a `ping`'s urls are rebased one by one, and the attribute is
 * rewritten as Eleventy writes it — candidates `url[ Nw][ Nh][ Nx]`, both joined by
 * `, ` — where one of them changed, and left as written where none did. What could not be
 * rebased is reported beside the body rather than thrown, so that one link a reader's
 * browser would refuse too costs no build.
 */
export function rebaseLinks(body: string, url: string, origin?: string): Rebased {
  const unrebased: Unrebased[] = []
  const rebase = (value: string, at: string): string => {
    const trimmed = value.trim()
    if (URL.canParse(trimmed)) return trimmed
    if (origin === undefined && trimmed.startsWith('/')) return trimmed
    const base = `${origin ?? PLACEHOLDER}${url}`
    if (!URL.canParse(trimmed, base)) {
      unrebased.push({ kind: 'refused', at, url: trimmed })
      return value
    }
    const resolved = new URL(trimmed, base)
    if (origin !== undefined) return resolved.href
    // `\\cdn/x` names a host of its own, as `//cdn/x` does, which a path would lose: a url
    // the page's base does not place keeps its host, and is kept.
    if (new URL(trimmed, `${OTHER}${url}`).host === resolved.host) return trimmed
    const { pathname, search, hash } = resolved
    // As the URL serializer writes a path starting `//` with no host to read it as one.
    return (pathname.startsWith('//') ? `/.${pathname}` : pathname) + search + hash
  }
  // By start offset: an element the parser reconstructs carries the location of the one
  // it copied, so two elements can name one attribute in the text.
  const splices = new Map<number, { end: number; text: string }>()
  const visit = (node: Tree.ParentNode): void => {
    for (const child of node.childNodes) {
      if (!('tagName' in child)) continue
      const names = LINKS.get(child.tagName) ?? []
      const location = child.sourceCodeLocation
      for (const { name, value, prefix } of child.attrs) {
        // An element with no location is one the parser made, its attributes copied
        // from one it has, which rebases them.
        if (prefix !== undefined || !names.includes(name) || location == null) continue
        const at = `<${child.tagName} ${name}>`
        const rebased = rewritten(name, value, at, rebase, unrebased)
        if (rebased === value) continue
        const { startOffset, endOffset } = location.attrs![name]
        splices.set(startOffset, { end: endOffset, text: `${name}="${escaped(rebased)}"` })
      }
      visit('content' in child ? child.content : child)
    }
  }
  visit(parseFragment(body, { sourceCodeLocationInfo: true, scriptingEnabled: false }))
  let rebased = body
  for (const [start, { end, text }] of [...splices].sort(([a], [b]) => b - a)) {
    rebased = rebased.slice(0, start) + text + rebased.slice(end)
  }
  return { body: rebased, unrebased }
}

/** The origin a page's url is resolved on where the site names none, then dropped. */
const PLACEHOLDER = 'http://page.invalid'

/** A second such origin, against which a url naming its own host resolves the same. */
const OTHER = 'http://other.invalid'

/**
 * The value of the attribute `name`, standing at `at`, with each url in it rebased by
 * `rebase`: a `srcset` candidate by candidate, a `ping` url by url, any other whole. A list
 * is joined anew only where a url in it changed; a `srcset` candidate `parse-srcset`
 * refuses is then dropped, and said so in `unrebased`.
 */
function rewritten(
  name: string,
  value: string,
  at: string,
  rebase: (url: string, at: string) => string,
  unrebased: Unrebased[]
): string {
  if (name === 'srcset') {
    const { candidates, refused } = srcset(value)
    const urls = candidates.map(({ url }) => rebase(url, at))
    if (urls.every((url, i) => url === candidates[i].url)) return value
    for (const descriptor of refused) {
      unrebased.push({ kind: 'dropped', at, srcset: value, descriptor })
    }
    return candidates
      .map(({ w, h, d }, i) => {
        const descriptor = (n: number | undefined, unit: string) =>
          n === undefined ? '' : ` ${n}${unit}`
        return urls[i] + descriptor(w, 'w') + descriptor(h, 'h') + descriptor(d, 'x')
      })
      .join(', ')
  }
  if (name === 'ping') {
    const urls = value
      .split(',')
      .map((url) => url.trim())
      .filter((url) => url !== '')
    const rebased = urls.map((url) => rebase(url, at))
    return rebased.every((url, i) => url === urls[i]) ? value : rebased.join(', ')
  }
  return rebase(value, at)
}

/**
 * The candidates of `value` as `parse-srcset` reads them, and the descriptor of each it
 * refused — which it would otherwise say on standard output, past the streams `lem` writes
 * its diagnostics to, naming no file.
 */
function srcset(value: string): { candidates: Candidate[]; refused: string[] } {
  const refused: string[] = []
  const log = console.log
  console.log = (message: string) => {
    refused.push(/ at '(.*)'\.$/s.exec(message)![1])
  }
  try {
    return { candidates: parseSrcset(value), refused }
  } finally {
    console.log = log
  }
}

/** `value` as a double-quoted attribute value holds it. */
function escaped(value: string): string {
  return value.replaceAll('&', '&amp;').replaceAll('"', '&quot;')
}

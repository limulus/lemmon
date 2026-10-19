/**
 * Rebases a document's body for reading on another page (D46): every link in it made
 * absolute against the site's origin, by the rule Eleventy's HTML base plugin applies to
 * a feed's content, or root-relative where the site names no origin. The body is parsed
 * with parse5 and only the attributes that change are rewritten, so every other byte of
 * it stays as the core rendered it.
 */

import { parseFragment, type DefaultTreeAdapterTypes as Tree } from 'parse5'

import { LINKS, pings, srcset } from './links.ts'

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
  // Walked in document order with a stack of its own, since a body may nest deeper than
  // the call stack goes (invariant 8).
  const pending: (Tree.Element | Tree.Template)[] = []
  const push = (parent: Tree.ParentNode): void => {
    for (let i = parent.childNodes.length - 1; i >= 0; i--) {
      const child = parent.childNodes[i]
      if ('tagName' in child) pending.push(child)
    }
  }
  push(parseFragment(body, { sourceCodeLocationInfo: true, scriptingEnabled: false }))
  for (let child = pending.pop(); child !== undefined; child = pending.pop()) {
    const names = LINKS.get(child.tagName) ?? []
    const location = child.sourceCodeLocation
    for (const { name, value, prefix } of child.attrs) {
      // An element with no location is one the parser made, its attributes copied from
      // one it has, which rebases them.
      if (prefix !== undefined || !names.includes(name) || location == null) continue
      const at = `<${child.tagName} ${name}>`
      const rebased = rewritten(name, value, at, rebase, unrebased)
      if (rebased === value) continue
      const { startOffset, endOffset } = location.attrs![name]
      splices.set(startOffset, { end: endOffset, text: `${name}="${escaped(rebased)}"` })
    }
    push('content' in child ? child.content : child)
  }
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
    const urls = pings(value)
    const rebased = urls.map((url) => rebase(url, at))
    return rebased.every((url, i) => url === urls[i]) ? value : rebased.join(', ')
  }
  return rebase(value, at)
}

/** `value` as a double-quoted attribute value holds it. */
function escaped(value: string): string {
  return value.replaceAll('&', '&amp;').replaceAll('"', '&quot;')
}

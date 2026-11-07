/**
 * Rebases a document's body for reading on another page (D46): every url the link check
 * reads in its elements' attributes (D47) made absolute against the site's origin, or
 * root-relative where the site names no origin. The body is parsed with parse5, and only
 * an attribute holding a url that changes is written anew — double-quoted, its character
 * references decoded — so every byte outside those stays as the core rendered it.
 */

import { parseFragment } from 'parse5'

import { attributeLinks, type AttributeLink, type AttributeUrl } from './link-scan.ts'
import { elements, stripped } from './links.ts'

/**
 * A url the URL parser refuses, even against the page's url, which a rebase keeps as
 * written: `at` is the element and attribute that hold it, `<a href>`.
 */
export interface Unrebased {
  at: string
  url: string
}

/** A body rebased, with what could not be. */
export interface Rebased {
  body: string
  unrebased: Unrebased[]
}

/**
 * `body`, the body of the page at `url` — a path from `/`, as `plan()` reports it — with
 * every url {@link attributeLinks} reads in its elements' attributes rebased for reading
 * on another page, each stripped of the ASCII whitespace HTML strips from a url first:
 *
 * - a url that is only a fragment is kept where its element embeds what it names — an
 *   SVG `<use href="#icon">`, a `style` attribute's `url(#clip)` — since it names part of
 *   the body itself, which a reader would not load from another document;
 * - a url the URL parser reads on its own — `https:`, `mailto:`, `data:` — is kept;
 * - with `origin`, any other is resolved against the page's url on that origin, whole,
 *   so `../x/#a`, `./x.pdf`, `#fn1`, `/y` and `//cdn/z` all become absolute;
 * - with none, a url that does not start with `/` and names no host of its own is
 *   resolved against the page's url and becomes its path, query and fragment, so it still
 *   names what it named, read anywhere on the site; any other is kept.
 *
 * Each url that changes is written over itself in its attribute's value — a `srcset`'s
 * descriptors and commas, a `ping`'s spacing and the rest of a `style` kept, a url in CSS
 * written as a string, a refresh's url with its quotes percent-encoded — and an attribute
 * where one changed is written anew, double-quoted, its character references decoded;
 * one where none did is left as written, and so is a `<style>`'s text, which the check
 * reads, and a `srcset` `@prettier/parse-srcset` refuses, which the check reports. A url
 * the URL parser refuses is reported beside the body rather than thrown, so that one link
 * a reader's browser would refuse too costs no build.
 */
export function rebaseLinks(body: string, url: string, origin?: string): Rebased {
  const unrebased: Unrebased[] = []
  const rebase = ({ url: value, at, embedded }: AttributeUrl): string => {
    const trimmed = stripped(value)
    // `use` loads no other origin's document, and CSS reads a `url(#…)` in the one that
    // holds it: rebased, either would name what no reader of the body loads.
    if (embedded && trimmed.startsWith('#')) return trimmed
    if (URL.canParse(trimmed)) return trimmed
    if (origin === undefined && trimmed.startsWith('/')) return trimmed
    const base = `${origin ?? PLACEHOLDER}${url}`
    if (!URL.canParse(trimmed, base)) {
      unrebased.push({ at, url: trimmed })
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
  /** `value`, an attribute's, with each of `links`, the urls it holds, rebased. */
  const rewritten = (value: string, links: AttributeLink[]): string => {
    let text = ''
    let from = 0
    for (const link of links) {
      // A `srcset` the parser refuses is kept as written, and said nothing of: the link
      // check fails the build at it, on the page that prints the body.
      if (link.kind === 'refused') continue
      // Only a refreshing `meta`'s url stands inside another, the `content` an Open Graph
      // property it names too reads whole: where that was rebased, this one is gone.
      if (link.start < from) continue
      const rebased = rebase(link)
      if (rebased === link.url) continue
      text += value.slice(from, link.start) + written(rebased, link)
      from = link.end
    }
    return text + value.slice(from)
  }
  // By start offset: an element the parser reconstructs carries the location of the one
  // it copied, so two elements can name one attribute in the text, which is rebased, and
  // warned at, once.
  const splices = new Map<number, { end: number; text: string }>()
  const seen = new Set<number>()
  const options = { sourceCodeLocationInfo: true, scriptingEnabled: false }
  for (const [child] of elements(parseFragment(body, options))) {
    // An element with no location is one the parser made, its attributes copied from
    // one it has, which rebases them.
    const location = child.sourceCodeLocation
    if (location == null) continue
    const attributes = new Map<string, AttributeLink[]>()
    for (const link of attributeLinks(child)) {
      attributes.set(link.key, [...(attributes.get(link.key) ?? []), link])
    }
    for (const [key, links] of attributes) {
      const { startOffset, endOffset } = location.attrs![key]
      if (seen.has(startOffset)) continue
      seen.add(startOffset)
      const { value } = links[0]
      const rebased = rewritten(value, links)
      if (rebased === value) continue
      splices.set(startOffset, { end: endOffset, text: `${key}="${escaped(rebased)}"` })
    }
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
 * `url` as it is written over the url `link` held: as it is, but where CSS held it, as a
 * string, inside `url()` where the url was bare — single-quoted, so that the attribute's
 * double quotes need no escaping, with `'` and `\` escaped, and a newline by its code
 * point, since one would end the string — and where it is a refresh's, with each quote
 * percent-encoded, since HTML's refresh steps end a quoted url at the quote it opened
 * with, and a page's url holds `'` as it is.
 */
function written(url: string, { token, refresh }: AttributeUrl): string {
  if (refresh) return url.replaceAll("'", '%27').replaceAll('"', '%22')
  if (token === undefined) return url
  const string = url
    .replace(/['\\]/g, '\\$&')
    .replace(/[\n\f\r]/g, (newline) => `\\${newline.charCodeAt(0).toString(16)} `)
  return token === 'url' ? `url('${string}')` : `'${string}'`
}

/** `value` as a double-quoted attribute value holds it. */
function escaped(value: string): string {
  return value.replaceAll('&', '&amp;').replaceAll('"', '&quot;')
}

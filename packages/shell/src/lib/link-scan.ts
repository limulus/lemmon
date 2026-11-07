/**
 * The links an output file holds, read as its kind is read (D47): an HTML page parsed as a
 * browser parses it, a stylesheet tokenized, an XML document parsed — never a pattern over
 * the text, so a url inside a comment, a string or another url is not one — each with the
 * base it resolves against, and the ids an HTML page's fragments can name. The body pass
 * rebases the urls an element's attributes hold as the check reads them here (D46).
 */

import { tokenize, TokenType, type CSSToken } from '@csstools/css-tokenizer'
import { DOMParser, type Document, type Element, type Node } from '@xmldom/xmldom'
import { isUtf8 } from 'node:buffer'
import { parse, type DefaultTreeAdapterTypes as Tree } from 'parse5'

import { elements, LINKS, srcset, tokens, type Located } from './links.ts'

/** A url an output file holds, as written, with where it stands and its base. */
export interface Link {
  url: string
  /** The element and attribute, `<a href>`, or what else holds it: `url()`, `<loc>`. */
  at: string
  /** The absolute url it resolves against: the file's own, or one its markup sets. */
  base: string
}

/**
 * What an HTML page links to, what it holds whose links cannot be read, and what its
 * fragments can name.
 */
export interface HtmlLinks {
  links: Link[]
  /**
   * Each `srcset` that `@prettier/parse-srcset` refuses, as it stands and as written, in
   * document order: no link in it can be read, so none can be checked.
   */
  unreadable: { at: string; srcset: string }[]
  /**
   * Every `id` in the document, and every `name` of an HTML `a` in it — what a fragment
   * can indicate, by HTML's rule for the indicated part of a document — but those inside
   * a `<template>`'s content, which is no part of the document.
   */
  targets: Set<string>
}

/**
 * A url an element's attribute holds, as the link check reads it, with what the body pass
 * needs to rewrite it in place: `start` and `end` are its offsets into `value`.
 */
export interface AttributeUrl extends Located {
  kind: 'url'
  /** The attribute, as parse5's source locations name it: `href`, `xlink:href`. */
  key: string
  /** The attribute's value, its character references decoded. */
  value: string
  /** The element and attribute, `<a href>`, or which `meta` it is. */
  at: string
  /** The CSS token that holds it, in a `style` attribute. */
  token?: CssToken
  /** Whether it is a refreshing `meta`'s url, which a quote may end. */
  refresh?: true
  /**
   * Whether it names what its element embeds — any SVG element's `href` but an `a`'s, a
   * `style` attribute's urls — rather than a document to go to.
   */
  embedded: boolean
}

/**
 * A `srcset` that `@prettier/parse-srcset` refuses, since a candidate's descriptors do not
 * parse, so that no url is read from it.
 */
export interface RefusedSrcset {
  kind: 'refused'
  key: string
  value: string
  at: string
}

/** What an element's attribute holds that links: a url, or a `srcset` refused whole. */
export type AttributeLink = AttributeUrl | RefusedSrcset

/**
 * The Open Graph properties whose `content` is a url: the page's own, and its image's,
 * video's and audio's, in each spelling the protocol gives. A vocabulary, not "any
 * `meta` whose content parses as a url", since a title such as "TILs: git" does.
 */
const OPEN_GRAPH: ReadonlySet<string> = new Set(
  ['og:url'].concat(
    ...['image', 'video', 'audio'].map((kind) => [
      `og:${kind}`,
      `og:${kind}:url`,
      `og:${kind}:secure_url`,
    ])
  )
)

const HTML = 'http://www.w3.org/1999/xhtml'
const SVG = 'http://www.w3.org/2000/svg'
const XLINK = 'http://www.w3.org/1999/xlink'
const XML = 'http://www.w3.org/XML/1998/namespace'

/**
 * The links of `text`, an HTML page served at `url`, parsed as a document with scripting
 * off — as the body pass parses a body, so a `<noscript>`'s content is markup — and the
 * ids its fragments can name. A link is a url {@link attributeLinks} reads in an element's
 * attributes, but `<base href>`, which sets the base the others resolve against — the
 * first one's in the document, resolved against `url`, or `url` itself where the URL
 * parser refuses it — and each `url()` and `@import` of a `<style>`'s text; a `srcset`
 * whose candidates it refuses is returned beside them, unread. A `<template>`'s content
 * is walked for links. An inline `<script>`'s text and an SVG paint's `url(#…)` are not
 * read.
 */
export function htmlLinks(text: string, url: string): HtmlLinks {
  const found: Omit<Link, 'base'>[] = []
  const unreadable: HtmlLinks['unreadable'] = []
  const targets = new Set<string>()
  let base: string | undefined
  for (const [child, inert] of elements(parse(text, { scriptingEnabled: false }))) {
    const { tagName } = child
    if (!inert) {
      const id = attribute(child, 'id')
      if (id !== undefined && id !== '') targets.add(id)
      const name =
        tagName === 'a' && child.namespaceURI === HTML && attribute(child, 'name')
      if (name) targets.add(name)
      // The first `base` with an `href` sets the base, or leaves it `url` where the URL
      // parser refuses it.
      const href = tagName === 'base' ? attribute(child, 'href') : undefined
      if (href !== undefined && base === undefined) {
        base = URL.canParse(href, url) ? new URL(href, url).href : url
      }
    }
    for (const link of attributeLinks(child)) {
      if (link.kind === 'refused') unreadable.push({ at: link.at, srcset: link.value })
      // A `base`'s `href` sets the base rather than linking, but its `style` links.
      else if (tagName !== 'base' || link.token !== undefined) {
        found.push({ url: link.url, at: link.at })
      }
    }
    if (tagName === 'style') {
      // An SVG `style`'s comments are nodes of their own, which hold no CSS.
      const css = child.childNodes.map((text) => ('value' in text ? text.value : ''))
      for (const link of cssLinks(css.join(''))) {
        found.push({ url: link.url, at: '<style>' })
      }
    }
  }
  const resolvedAgainst = base ?? url
  const links = found.map((link) => ({ ...link, base: resolvedAgainst }))
  return { links, unreadable, targets }
}

/**
 * Each url `element`'s attributes hold, in the order they stand, as HTML reads them: one
 * in an attribute {@link LINKS} lists, `<base href>` among them, each `srcset` candidate
 * on its own — or the `srcset` refused, where a candidate's descriptors do not parse —
 * and a `ping`'s urls split at whitespace; any SVG element's `href` or `xlink:href`
 * (`use`, `image`, `feImage`, `a` …); and each `url()` and `@import` of a `style`
 * attribute, where it stands; then a refreshing `meta`'s url, and an Open Graph url's
 * `content`. Those of one attribute come in the order they start, and none stands inside
 * another but a refreshing `meta`'s url, where the `meta` names an Open Graph property too.
 */
export function* attributeLinks(element: Tree.Element): Generator<AttributeLink> {
  const { tagName } = element
  const rows = LINKS.get(tagName) ?? []
  for (const { name, value, prefix } of element.attrs) {
    const key = prefix === undefined ? name : `${prefix}:${name}`
    const at = `<${tagName} ${key}>`
    if (prefix === undefined && name === 'style') {
      for (const { url, start, end, token } of cssLinks(value)) {
        yield { kind: 'url', key, value, at, url, start, end, token, embedded: true }
      }
    }
    // In SVG, `href` is a link wherever it stands, bare or as `xlink:href`, the one
    // prefix the parser gives it.
    const svg = element.namespaceURI === SVG && name === 'href'
    if (!svg && (prefix !== undefined || !rows.includes(name))) continue
    const row = { kind: 'url', key, value, at, embedded: svg && tagName !== 'a' } as const
    if (name === 'srcset') {
      const candidates = srcset(value)
      if (candidates === undefined) yield { kind: 'refused', key, value, at }
      else for (const candidate of candidates) yield { ...row, ...candidate }
    } else if (name === 'ping') {
      for (const token of tokens(value)) yield { ...row, ...token }
    } else yield { ...row, url: value, start: 0, end: value.length }
  }
  const content = tagName === 'meta' ? attribute(element, 'content') : undefined
  if (content === undefined) return
  const row = { kind: 'url', key: 'content', value: content, embedded: false } as const
  const property = attribute(element, 'property')
  if (property !== undefined && OPEN_GRAPH.has(property)) {
    const at = `<meta property="${property}" content>`
    yield { ...row, at, url: content, start: 0, end: content.length }
  }
  const refresh = attribute(element, 'http-equiv')?.toLowerCase() === 'refresh'
  const target = refresh ? refreshUrl(content) : undefined
  if (target !== undefined) {
    yield { ...row, at: '<meta http-equiv="refresh" content>', ...target, refresh: true }
  }
}

/** The value of `element`'s attribute `name`, which no prefix names, if it has one. */
function attribute(element: Tree.Element, name: string): string | undefined {
  const named = element.attrs.find(
    (attr) => attr.prefix === undefined && attr.name === name
  )
  return named?.value
}

/**
 * The url a refreshing `meta`'s `content` names, if any, read by HTML's shared
 * declarative refresh steps: a time, then a `;` or `,` or whitespace, then the url,
 * optionally after `url=` and inside quotes, which end it.
 */
export function refreshUrl(content: string): Located | undefined {
  const time = /^[\t\n\f\r ]*[.\d]+/.exec(content)
  if (time === null) return undefined
  let rest = content.slice(time[0].length)
  if (rest === '') return undefined
  if (!/^[;,\t\n\f\r ]/.test(rest)) return undefined
  rest = rest.replace(/^[\t\n\f\r ]*[;,]?[\t\n\f\r ]*/, '')
  if (rest === '') return undefined
  rest = rest.replace(/^url[\t\n\f\r ]*=[\t\n\f\r ]*/i, '')
  // Each step above took from the front, so what is left ends where `content` does.
  const start = content.length - rest.length
  const quote = /^["']/.exec(rest)?.[0]
  if (quote === undefined) return { url: rest, start, end: content.length }
  const quoted = rest.slice(1)
  const end = quoted.indexOf(quote)
  const url = end === -1 ? quoted : quoted.slice(0, end)
  return { url, start: start + 1, end: start + 1 + url.length }
}

/** How CSS wrote a url: a bare `url(…)`, which is one token, or a string. */
export type CssToken = 'url' | 'string'

/** A url in CSS, where it stands, and the token that holds it, which its offsets span. */
export interface CssLink extends Located {
  /** `url()` or `@import`. */
  at: string
  token: CssToken
}

/**
 * The urls `css` names, each with where it stands: as CSS Syntax tokenizes it, a `url()`
 * written bare or with a string, `src()` with a string, and `@import`'s string or url —
 * so the `url(%23clip)` inside an SVG `data:` URI held in a string is no url of its own.
 * Each stands at `url()` or `@import`, and spans its token, quotes or `url(` and all.
 */
export function cssLinks(css: string): CssLink[] {
  const tokens = tokenize({ css }).filter(
    (token) => token[0] !== TokenType.Whitespace && token[0] !== TokenType.Comment
  )
  const links: CssLink[] = []
  // A token's end is the offset of its last character, where a `Located`'s is past it.
  const link = (token: CSSToken, at: string, kind: CssToken, url: string): CssLink => ({
    url,
    at,
    start: token[2],
    end: token[3] + 1,
    token: kind,
  })
  tokens.forEach((token, i) => {
    const at = imported(tokens[i - 1]) ? '@import' : 'url()'
    if (token[0] === TokenType.URL) links.push(link(token, at, 'url', token[4].value))
    const next = tokens[i + 1] as CSSToken | undefined
    if (next?.[0] !== TokenType.String) return
    if (token[0] === TokenType.Function && /^(?:url|src)$/i.test(token[4].value)) {
      links.push(link(next, at, 'string', next[4].value))
    }
    if (imported(token)) links.push(link(next, '@import', 'string', next[4].value))
  })
  return links
}

/** Whether `token` is `@import`, which a url or a string follows. */
function imported(token: CSSToken | undefined): boolean {
  return token?.[0] === TokenType.AtKeyword && token[4].value.toLowerCase() === 'import'
}

/**
 * What an XML document links to, or why none of its links can be read: a clause about the
 * document, `is not UTF-8`.
 */
export type XmlLinks = { links: Link[] } | { refused: string }

/**
 * The links of `bytes`, an XML document served at `url`: the `href` of an `xml-stylesheet`
 * instruction before the root, which one inside a comment is not; every element's `href`
 * and `xlink:href`; and, matched by local name, the text of a `link` with no `href` (RSS
 * 2.0), a `url` inside an `image` (RSS 2.0's channel image), a `loc` or any name ending
 * `_loc` (Sitemaps, and their image and video extensions), and a `guid` unless its
 * `isPermaLink` is `false` (RSS 2.0). Each resolves against the nearest `xml:base`, else
 * `url`. An Atom `id` is an identifier, not a link, and the escaped markup in a feed's
 * `description` or `content` is its page's, whose links that page's own check reads.
 *
 * Its links cannot be read, and why is returned instead, where it declares an encoding
 * other than UTF-8, which is all the check reads; where it is not UTF-8; and where the XML
 * parser reports any problem with it — an error, or a warning, every one of which marks
 * markup that is not well-formed, but the one at a U+FFFD, which UTF-8 may hold. The
 * parser expands no entity a document declares, so one that uses its own is refused.
 */
export function xmlLinks(bytes: Uint8Array, url: string): XmlLinks {
  const text = UTF8.decode(bytes)
  const declared = /^<\?xml\s[^>]*?\bencoding\s*=\s*(["'])(.*?)\1/.exec(text)?.[2]
  if (declared !== undefined && declared.toLowerCase() !== 'utf-8') {
    return { refused: `declares the encoding \`${declared}\`, where the check reads UTF-8` }
  }
  if (!isUtf8(bytes)) return { refused: 'is not UTF-8' }
  const problems: string[] = []
  const parser = new DOMParser({
    locator: false,
    onError: (level, message) => {
      if (level !== 'warning' || !message.startsWith(REPLACEMENT)) problems.push(message)
    },
  })
  let document: Document
  try {
    document = parser.parseFromString(text, 'text/xml')
  } catch (error) {
    // The parser reports a fatal error before it throws, so the report comes first; what
    // it threw stands in, should it ever throw unreported.
    return { refused: `is refused by the XML parser (${[...problems, String(error)][0]})` }
  }
  if (problems.length > 0)
    return { refused: `is refused by the XML parser (${problems[0]})` }
  const links: Link[] = []
  for (let node = document.firstChild; node !== null; node = node.nextSibling) {
    if (node.nodeType !== node.PROCESSING_INSTRUCTION_NODE) continue
    const { target, data } = node as Node & { target: string; data: string }
    const href = target === 'xml-stylesheet' ? pseudoAttribute(data, 'href') : undefined
    if (href !== undefined) {
      links.push({ url: href, at: '<?xml-stylesheet href?>', base: url })
    }
  }
  // In document order with a stack of its own, as an HTML page is walked: each element
  // with its parent's base and local name.
  const pending: [Element, string, string][] = [[document.documentElement!, url, '']]
  for (let next = pending.pop(); next !== undefined; next = pending.pop()) {
    const [element, parent, parentName] = next
    const xmlBase = element.getAttributeNS(XML, 'base')
    const base =
      xmlBase !== null && URL.canParse(xmlBase, parent)
        ? new URL(xmlBase, parent).href
        : parent
    const { tagName } = element
    const localName = tagName.slice(tagName.indexOf(':') + 1)
    const href = element.getAttributeNS(null, 'href')
    if (href !== null) links.push({ url: href, at: `<${tagName} href>`, base })
    const xlink = element.getAttributeNodeNS(XLINK, 'href')
    if (xlink !== null) {
      links.push({ url: xlink.value, at: `<${tagName} ${xlink.name}>`, base })
    }
    const permaLink = element.getAttribute('isPermaLink')?.trim().toLowerCase() !== 'false'
    const textual =
      (localName === 'link' && href === null) ||
      (localName === 'url' && parentName === 'image') ||
      localName === 'loc' ||
      localName.endsWith('_loc') ||
      (localName === 'guid' && permaLink)
    const content = textual ? element.textContent!.trim() : ''
    if (content !== '') links.push({ url: content, at: `<${tagName}>`, base })
    const children: [Element, string, string][] = []
    for (let child = element.firstChild; child !== null; child = child.nextSibling) {
      if (child.nodeType === child.ELEMENT_NODE) {
        children.push([child as Element, base, localName])
      }
    }
    pending.push(...children.reverse())
  }
  return { links }
}

/** XML's text, which a document need not be: decoded, its byte order mark dropped. */
const UTF8 = new TextDecoder()

/** How the XML parser's warning at a U+FFFD begins, which valid UTF-8 may hold. */
const REPLACEMENT = 'Unicode replacement character'

/**
 * The value of the pseudo-attribute `name` in `data`, a processing instruction's text, if
 * it is there: with the entities XML predefines and its character references decoded, as
 * the instruction's rules have them, and any other kept as written.
 */
function pseudoAttribute(data: string, name: string): string | undefined {
  const match = new RegExp(`(?:^|\\s)${name}\\s*=\\s*(["'])(.*?)\\1`, 's').exec(data)
  if (match === null) return undefined
  return match[2].replace(
    /&(?:#x([\da-fA-F]+)|#(\d+)|(\w+));/g,
    (entity, hex, dec, named) => {
      if (named !== undefined) return PREDEFINED[named as string] ?? entity
      const code =
        hex === undefined ? parseInt(dec as string, 10) : parseInt(hex as string, 16)
      return code <= 0x10ffff ? String.fromCodePoint(code) : entity
    }
  )
}

/** The entities XML predefines. */
const PREDEFINED: Record<string, string> = {
  amp: '&',
  apos: "'",
  gt: '>',
  lt: '<',
  quot: '"',
}

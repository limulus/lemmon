/**
 * The links an output file holds, read as its kind is read (D47): an HTML page parsed as a
 * browser parses it, a stylesheet tokenized, an XML document parsed — never a pattern over
 * the text, so a url inside a comment, a string or another url is not one — each with the
 * base it resolves against, and the ids an HTML page's fragments can name.
 */

import { tokenize, TokenType, type CSSToken } from '@csstools/css-tokenizer'
import { DOMParser, type Document, type Element, type Node } from '@xmldom/xmldom'
import { isUtf8 } from 'node:buffer'
import { parse, type DefaultTreeAdapterTypes as Tree } from 'parse5'

import { LINKS, srcset } from './links.ts'

/** A url an output file holds, as written, with where it stands and its base. */
export interface Link {
  url: string
  /** The element and attribute, `<a href>`, or what else holds it: `url()`, `<loc>`. */
  at: string
  /** The absolute url it resolves against: the file's own, or one its markup sets. */
  base: string
}

/** What an HTML page links to, and what its fragments can name. */
export interface HtmlLinks {
  links: Link[]
  /**
   * Every `id` in the document, and every `name` of an HTML `a` in it — what a fragment
   * can indicate, by HTML's rule for the indicated part of a document — but those inside
   * a `<template>`'s content, which is no part of the document.
   */
  targets: Set<string>
}

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
 * ids its fragments can name. A link is a url in an attribute {@link LINKS} lists, each
 * `srcset` candidate on its own and a `ping`'s urls split as HTML splits them, at
 * whitespace; but `<base href>`, which sets the base the others resolve against — the
 * first one's in the document, resolved against `url`, or `url` itself where the URL
 * parser refuses it. And a refreshing `meta`'s url, any SVG element's `href` or
 * `xlink:href` (`use`, `image`, `feImage`, `a` …), an Open Graph url's `content`, and
 * each `url()` and `@import` of a `<style>`'s text and of a `style` attribute. A
 * `<template>`'s content is walked for links. An inline `<script>`'s text and an SVG
 * paint's `url(#…)` are not read.
 */
export function htmlLinks(text: string, url: string): HtmlLinks {
  const found: Omit<Link, 'base'>[] = []
  const targets = new Set<string>()
  let base: string | undefined
  const add = (at: string) => (link: string) => found.push({ url: link, at })
  // Walked in document order with a stack of its own, since a page may nest deeper than
  // the call stack goes (invariant 8): each element with whether it is inside a
  // `<template>`'s content, whose ids are no part of the document.
  const pending: [Tree.Element | Tree.Template, boolean][] = []
  const push = (parent: Tree.ParentNode, inert: boolean): void => {
    for (let i = parent.childNodes.length - 1; i >= 0; i--) {
      const child = parent.childNodes[i]
      if ('tagName' in child) pending.push([child, inert])
    }
  }
  push(parse(text, { scriptingEnabled: false }), false)
  for (let next = pending.pop(); next !== undefined; next = pending.pop()) {
    const [child, inert] = next
    const { tagName } = child
    const attribute = (name: string) =>
      child.attrs.find((attr) => attr.prefix === undefined && attr.name === name)?.value
    if (!inert) {
      const id = attribute('id')
      if (id !== undefined && id !== '') targets.add(id)
      const name = tagName === 'a' && child.namespaceURI === HTML && attribute('name')
      if (name) targets.add(name)
      // The first `base` with an `href` sets the base, or leaves it `url` where the URL
      // parser refuses it.
      const href = tagName === 'base' ? attribute('href') : undefined
      if (href !== undefined && base === undefined) {
        base = URL.canParse(href, url) ? new URL(href, url).href : url
      }
    }
    const rows = LINKS.get(tagName) ?? []
    for (const { name, value, prefix } of child.attrs) {
      const at = `<${tagName} ${prefix === undefined ? '' : `${prefix}:`}${name}>`
      if (prefix === undefined && name === 'style') {
        for (const link of cssLinks(value)) add(at)(link.url)
      }
      // In SVG, `href` is a link wherever it stands, bare or as `xlink:href`, the one
      // prefix the parser gives it.
      const svg = child.namespaceURI === SVG && name === 'href'
      if (!svg && (prefix !== undefined || !rows.includes(name) || tagName === 'base')) {
        continue
      }
      if (name === 'srcset') srcset(value).candidates.forEach((c) => add(at)(c.url))
      else if (name === 'ping') tokens(value).forEach(add(at))
      else add(at)(value)
    }
    if (tagName === 'meta') {
      const content = attribute('content')
      const property = attribute('property')
      if (content !== undefined && property !== undefined && OPEN_GRAPH.has(property)) {
        add(`<meta property="${property}" content>`)(content)
      }
      const refresh = attribute('http-equiv')?.toLowerCase() === 'refresh'
      const target = refresh && content !== undefined ? refreshUrl(content) : undefined
      if (target !== undefined) add('<meta http-equiv="refresh" content>')(target)
    }
    if (tagName === 'style') {
      // An SVG `style`'s comments are nodes of their own, which hold no CSS.
      const css = child.childNodes.map((text) => ('value' in text ? text.value : ''))
      for (const link of cssLinks(css.join(''))) add('<style>')(link.url)
    }
    if ('content' in child) push(child.content, true)
    else push(child, inert)
  }
  const resolvedAgainst = base ?? url
  return { links: found.map((link) => ({ ...link, base: resolvedAgainst })), targets }
}

/**
 * The urls of `value`, a `ping`, as HTML reads them: a set of tokens separated by ASCII
 * whitespace. (The body pass splits one at commas instead, as posthtml-urls does.)
 */
function tokens(value: string): string[] {
  return value.split(/[\t\n\f\r ]+/).filter((token) => token !== '')
}

/**
 * The url a refreshing `meta`'s `content` names, if any, read by HTML's shared
 * declarative refresh steps: a time, then a `;` or `,` or whitespace, then the url,
 * optionally after `url=` and inside quotes, which end it.
 */
export function refreshUrl(content: string): string | undefined {
  const time = /^[\t\n\f\r ]*[.\d]+/.exec(content)
  if (time === null) return undefined
  let rest = content.slice(time[0].length)
  if (rest === '') return undefined
  if (!/^[;,\t\n\f\r ]/.test(rest)) return undefined
  rest = rest.replace(/^[\t\n\f\r ]*[;,]?[\t\n\f\r ]*/, '')
  if (rest === '') return undefined
  rest = rest.replace(/^url[\t\n\f\r ]*=[\t\n\f\r ]*/i, '')
  const quote = /^["']/.exec(rest)?.[0]
  if (quote === undefined) return rest
  const quoted = rest.slice(1)
  const end = quoted.indexOf(quote)
  return end === -1 ? quoted : quoted.slice(0, end)
}

/**
 * The urls `css` names, each with where it stands: as CSS Syntax tokenizes it, a `url()`
 * written bare or with a string, `src()` with a string, and `@import`'s string or url —
 * so the `url(%23clip)` inside an SVG `data:` URI held in a string is no url of its own.
 * Each stands at `url()` or `@import`.
 */
export function cssLinks(css: string): Omit<Link, 'base'>[] {
  const tokens = tokenize({ css }).filter(
    (token) => token[0] !== TokenType.Whitespace && token[0] !== TokenType.Comment
  )
  const links: Omit<Link, 'base'>[] = []
  tokens.forEach((token, i) => {
    const at = imported(tokens[i - 1]) ? '@import' : 'url()'
    if (token[0] === TokenType.URL) links.push({ url: token[4].value, at })
    const next = tokens[i + 1] as CSSToken | undefined
    if (next?.[0] !== TokenType.String) return
    if (token[0] === TokenType.Function && /^(?:url|src)$/i.test(token[4].value)) {
      links.push({ url: next[4].value, at })
    }
    if (imported(token)) links.push({ url: next[4].value, at: '@import' })
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

/**
 * Where a page's markup holds urls, as HTML reads them: the attributes, by the element
 * they stand on, and how a `srcset` and a `ping` list theirs, each url with where it
 * stands in the value; and the walk over its elements. The link check reads them (D47),
 * and the body pass rebases what it reads (D46).
 */

import parseSrcset from '@prettier/parse-srcset'
import type { DefaultTreeAdapterTypes as Tree } from 'parse5'

/** A url in a value, and where it stands there: its UTF-16 offsets, the end exclusive. */
export interface Located {
  url: string
  start: number
  end: number
}

/**
 * The attributes holding a url, by the element they stand on: the table
 * `@11ty/posthtml-urls` walks, but its `meta` `content`, which only a refreshing
 * `http-equiv` makes a url. Keyed by the tag name alone, as there, so an SVG `<a href>` is
 * in it and a custom element's `href` is not; a namespaced attribute, SVG's `xlink:href`,
 * is in no row.
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
 * The urls of `value`, a `srcset`, by HTML's rules for parsing one, which
 * `@prettier/parse-srcset` follows; or `undefined` where it refuses the value, which it
 * does whole where any candidate's descriptors do not parse — HTML drops that candidate
 * alone. A value of nothing but whitespace and commas has no candidates, and the library
 * throws at that too, so it is not asked.
 */
export function srcset(value: string): Located[] | undefined {
  if (/^[\t\n\f\r ,]*$/.test(value)) return []
  try {
    return parseSrcset(value).map(({ source }) => ({
      url: source.value,
      start: source.startOffset,
      end: source.startOffset + source.value.length,
    }))
  } catch {
    return undefined
  }
}

/**
 * The urls of `value`, a `ping`, as HTML reads them: a set of tokens separated by ASCII
 * whitespace.
 */
export function tokens(value: string): Located[] {
  return [...value.matchAll(/[^\t\n\f\r ]+/g)].map(({ 0: url, index: start }) => ({
    url,
    start,
    end: start + url.length,
  }))
}

/** `url` without the ASCII whitespace HTML strips from either end of one. */
export function stripped(url: string): string {
  return url.replace(/^[\t\n\f\r ]+|[\t\n\f\r ]+$/g, '')
}

/**
 * Each element under `root`, a tree parse5 parsed, in document order, with whether it
 * stands inside a `<template>`'s content, which is walked too though it is no part of the
 * document. Walked with a stack of its own, since markup may nest deeper than the call
 * stack goes (invariant 8).
 */
export function* elements(
  root: Tree.ParentNode
): Generator<[Tree.Element | Tree.Template, boolean]> {
  const pending: [Tree.Element | Tree.Template, boolean][] = []
  const push = (parent: Tree.ParentNode, inert: boolean): void => {
    for (let i = parent.childNodes.length - 1; i >= 0; i--) {
      const child = parent.childNodes[i]
      if ('tagName' in child) pending.push([child, inert])
    }
  }
  push(root, false)
  for (let next = pending.pop(); next !== undefined; next = pending.pop()) {
    yield next
    const [element, inert] = next
    if ('content' in element) push(element.content, true)
    else push(element, inert)
  }
}

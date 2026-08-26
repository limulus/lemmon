/**
 * Compares two HTML pages as what a browser would render rather than as bytes (D28):
 * parse5 trees, attributes by value, and text after the UA stylesheet's white-space
 * processing. A definition's margin, blank lines between blocks, or the column a
 * multi-line value lands on are invisible here by construction; a space between two
 * inline runs, or anything inside `pre`/`textarea`, still compares exactly.
 *
 * The model knows only the UA stylesheet. Where a site's CSS changes what whitespace
 * renders — a custom element made `display: block`, a flex container swallowing the
 * whitespace between its items — the caller declares it via `display`, and owns the
 * claim.
 */

import { parse, type DefaultTreeAdapterTypes as Tree } from 'parse5'

export interface RenderedElement {
  kind: 'element'
  tag: string
  attrs: Map<string, string>
  children: RenderedNode[]
}

export interface RenderedText {
  kind: 'text'
  text: string
}

export type RenderedNode = RenderedElement | RenderedText

export interface RenderedPage {
  doctype: string | null
  root: RenderedElement
}

/**
 * What a site's CSS does to an element the UA stylesheet leaves inline: `block` makes
 * its edges line breaks; `flex` additionally makes every child its own item, so
 * whitespace-only text between children is never rendered.
 */
export type Display = 'block' | 'flex'

export interface RenderOptions {
  display?: Record<string, Display>
}

// --- Rendering roles (the UA stylesheet) --------------------------------------------

/** Elements whose edges are line breaks; whitespace at either edge is not rendered. */
const BLOCK = new Set([
  'html',
  'head',
  'title',
  'body',
  'address',
  'article',
  'aside',
  'blockquote',
  'br',
  'caption',
  'dd',
  'details',
  'dialog',
  'div',
  'dl',
  'dt',
  'fieldset',
  'figcaption',
  'figure',
  'footer',
  'form',
  'h1',
  'h2',
  'h3',
  'h4',
  'h5',
  'h6',
  'header',
  'hgroup',
  'hr',
  'legend',
  'li',
  'main',
  'menu',
  'nav',
  'ol',
  'optgroup',
  'option',
  'p',
  'pre',
  'search',
  'section',
  'summary',
  'table',
  'tbody',
  'td',
  'tfoot',
  'th',
  'thead',
  'tr',
  'ul',
])

/** Replaced or otherwise atomic inlines: they occupy the line like a character does. */
const ATOMIC = new Set([
  'audio',
  'button',
  'canvas',
  'embed',
  'iframe',
  'img',
  'input',
  'math',
  'meter',
  'object',
  'progress',
  'select',
  'svg',
  'textarea',
  'video',
])

/** Never rendered; whitespace collapses across them as if they were not there. */
const NOT_RENDERED = new Set([
  'base',
  'link',
  'meta',
  'noscript',
  'script',
  'source',
  'style',
  'template',
  'track',
])

/** Text kept byte for byte: the browser renders its whitespace. */
const EXACT = new Set(['pre', 'textarea'])

/** Text nobody renders, compared with each line's indentation removed. */
const CODE = new Set(['script', 'style', 'noscript', 'template'])

type Mode = 'collapse' | 'exact' | 'code'

interface Role {
  outside: 'block' | 'atomic' | 'inline' | 'none'
  /** Every child is its own item (flex container, or the inside of an SVG). */
  wrapsChildren: boolean
}

function roleOf(tag: string, inSvg: boolean, display: Record<string, Display>): Role {
  if (NOT_RENDERED.has(tag)) return { outside: 'none', wrapsChildren: false }
  // Whitespace between SVG elements is never rendered, and `svg` itself sits on the
  // line as one atomic box.
  if (tag === 'svg') return { outside: 'atomic', wrapsChildren: true }
  if (inSvg) return { outside: 'inline', wrapsChildren: true }
  const declared = display[tag]
  if (declared !== undefined)
    return { outside: 'block', wrapsChildren: declared === 'flex' }
  if (BLOCK.has(tag)) return { outside: 'block', wrapsChildren: false }
  if (ATOMIC.has(tag)) return { outside: 'atomic', wrapsChildren: false }
  return { outside: 'inline', wrapsChildren: false }
}

// --- Building the model -----------------------------------------------------------

function modeFor(tag: string, inherited: Mode): Mode {
  if (inherited !== 'collapse') return inherited
  if (EXACT.has(tag)) return 'exact'
  if (CODE.has(tag)) return 'code'
  return 'collapse'
}

function normalizeText(raw: string, mode: Mode): string {
  switch (mode) {
    case 'exact':
      return raw
    case 'code':
      return raw
        .split('\n')
        .map((line) => line.trimStart())
        .join('\n')
        .trim()
    case 'collapse':
      return raw.replace(/[\t\n\f\r ]+/g, ' ')
  }
}

function isElement(node: Tree.ChildNode): node is Tree.Element {
  return 'tagName' in node
}

function isText(node: Tree.ChildNode): node is Tree.TextNode {
  return node.nodeName === '#text'
}

function isDoctype(node: Tree.ChildNode): node is Tree.DocumentType {
  return node.nodeName === '#documentType'
}

function build(element: Tree.Element, inherited: Mode): RenderedElement {
  const mode = modeFor(element.tagName, inherited)
  const children: RenderedNode[] = []
  for (const child of element.childNodes) {
    if (isElement(child)) {
      children.push(build(child, mode))
    } else if (isText(child)) {
      // Comments are dropped, so two text nodes a comment once separated are now one
      // run and must collapse as one.
      const previous = children.at(-1)
      if (previous?.kind === 'text') {
        previous.text = normalizeText(previous.text + child.value, mode)
      } else {
        children.push({ kind: 'text', text: normalizeText(child.value, mode) })
      }
    }
  }
  return {
    kind: 'element',
    tag: element.tagName,
    attrs: new Map(element.attrs.map(({ name, value }) => [name, value])),
    children,
  }
}

// --- White-space processing (CSS Text §4.1, phases I and II, approximately) ---------

type LineItem = RenderedText | 'break' | 'atomic'

/**
 * Lays the page's rendered text out as one sequence of runs and line breaks, so a
 * collapsible space can be judged by what actually precedes and follows it on the line
 * — which may be in another element entirely.
 */
function collectLine(
  element: RenderedElement,
  mode: Mode,
  inSvg: boolean,
  display: Record<string, Display>,
  items: LineItem[]
): void {
  const role = roleOf(element.tag, inSvg, display)
  for (const child of element.children) {
    if (role.wrapsChildren) items.push('break')
    if (child.kind === 'text') {
      if (mode === 'collapse') items.push(child)
    } else {
      const childRole = roleOf(child.tag, inSvg, display)
      const childMode = modeFor(child.tag, mode)
      switch (childRole.outside) {
        case 'none':
          break
        case 'atomic':
          // The box is one character to this line; whatever it contains is laid out
          // on lines of its own (an inline `svg`'s interior).
          items.push('atomic')
          if (childRole.wrapsChildren) {
            const inner: LineItem[] = ['break']
            collectLine(child, childMode, true, display, inner)
            trimLineEdges(inner)
          }
          break
        case 'block':
          items.push('break')
          collectLine(child, childMode, inSvg, display, items)
          items.push('break')
          break
        case 'inline':
          collectLine(child, childMode, inSvg, display, items)
          break
      }
    }
    if (role.wrapsChildren) items.push('break')
  }
}

function trimLineEdges(items: LineItem[]): void {
  // Phase I: a collapsible space at the start of a line, or following one, is removed.
  let afterSpace = true
  for (const item of items) {
    if (item === 'break') {
      afterSpace = true
    } else if (item === 'atomic') {
      afterSpace = false
    } else {
      if (afterSpace && item.text.startsWith(' ')) item.text = item.text.slice(1)
      if (item.text !== '') afterSpace = item.text.endsWith(' ')
    }
  }
  // Phase II: a collapsible space at the end of a line is removed.
  let beforeBreak = true
  for (let n = items.length - 1; n >= 0; n -= 1) {
    const item = items[n]
    if (item === 'break') {
      beforeBreak = true
    } else if (item === 'atomic') {
      beforeBreak = false
    } else if (item.text !== '') {
      if (beforeBreak && item.text.endsWith(' ')) item.text = item.text.slice(0, -1)
      if (item.text !== '') beforeBreak = false
    }
  }
}

function pruneEmptyText(element: RenderedElement): void {
  element.children = element.children.filter(
    (child) => child.kind === 'element' || child.text !== ''
  )
  for (const child of element.children) {
    if (child.kind === 'element') pruneEmptyText(child)
  }
}

/** Parses a page and reduces it to what renders. */
export function renderedPage(html: string, options: RenderOptions = {}): RenderedPage {
  const document = parse(html)
  const doctype = document.childNodes.find(isDoctype)
  // The HTML parser always produces an `html` element, even for the empty string.
  const rootElement = document.childNodes.find(isElement) as Tree.Element
  const root = build(rootElement, 'collapse')
  const display = options.display ?? {}
  const items: LineItem[] = ['break']
  collectLine(root, 'collapse', false, display, items)
  trimLineEdges(items)
  pruneEmptyText(root)
  return { doctype: doctype?.name ?? null, root }
}

// --- Comparison -------------------------------------------------------------------

/**
 * A known, accepted difference. Each class names precisely what it forgives so it
 * cannot absorb a difference it does not explain: a normalization that erased every
 * `p` would hide a dropped paragraph beside the one it meant to excuse.
 */
export interface Forgiveness {
  name: string
  /** A reference-side element ours lacks, in `parent`, that this class accounts for. */
  extra?: (theirs: RenderedElement, parent: RenderedElement) => boolean
  /** An element pair whose attribute difference this class accounts for. */
  attrs?: (ours: RenderedElement, theirs: RenderedElement) => boolean
}

export interface Difference {
  /** Where in the tree, as `html > body > main > p[3]` (index among element siblings). */
  path: string
  ours: string | null
  theirs: string | null
}

export interface Comparison {
  differences: Difference[]
  /** How many times each forgiveness class was applied, by name. */
  forgiven: Map<string, number>
  /** Elements and text runs that matched. */
  matched: number
}

/** Whether two attribute maps hold the same names with the same values. */
export function sameAttrs(ours: Map<string, string>, theirs: Map<string, string>): boolean {
  if (ours.size !== theirs.size) return false
  for (const [name, value] of ours) {
    if (theirs.get(name) !== value) return false
  }
  return true
}

const SHOW_LIMIT = 160

/** A node as the report shows it: its open tag, or its text. */
export function show(node: RenderedNode): string {
  const full =
    node.kind === 'text'
      ? JSON.stringify(node.text)
      : `<${node.tag}${[...node.attrs].map(([k, v]) => ` ${k}="${v}"`).join('')}>`
  return full.length > SHOW_LIMIT ? `${full.slice(0, SHOW_LIMIT - 1)}…` : full
}

class Comparator {
  readonly differences: Difference[] = []
  readonly forgiven = new Map<string, number>()
  matched = 0
  private readonly classes: Forgiveness[]

  constructor(classes: Forgiveness[]) {
    this.classes = classes
  }

  private forgive(name: string): void {
    this.forgiven.set(name, (this.forgiven.get(name) ?? 0) + 1)
  }

  private differ(
    path: string,
    ours: RenderedNode | null,
    theirs: RenderedNode | null
  ): void {
    this.differences.push({
      path,
      ours: ours && show(ours),
      theirs: theirs && show(theirs),
    })
  }

  private forgivesExtra(theirs: RenderedNode, parent: RenderedElement): boolean {
    if (theirs.kind !== 'element') return false
    const found = this.classes.find((cls) => cls.extra?.(theirs, parent))
    if (found) this.forgive(found.name)
    return found !== undefined
  }

  private forgivesAttrs(ours: RenderedElement, theirs: RenderedElement): boolean {
    const found = this.classes.find((cls) => cls.attrs?.(ours, theirs))
    if (found) this.forgive(found.name)
    return found !== undefined
  }

  element(ours: RenderedElement, theirs: RenderedElement, path: string): void {
    if (!sameAttrs(ours.attrs, theirs.attrs) && !this.forgivesAttrs(ours, theirs)) {
      this.differ(path, ours, theirs)
      return
    }
    this.matched += 1
    const counts = new Map<string, number>()
    let i = 0
    let j = 0
    while (i < ours.children.length || j < theirs.children.length) {
      const o = ours.children.at(i) ?? null
      const t = theirs.children.at(j) ?? null
      const pair = pairOf(o, t)
      if (pair === null) {
        if (t !== null && this.forgivesExtra(t, theirs)) {
          j += 1
          continue
        }
        this.differ(`${path} > child ${i}`, o, t)
        return
      }
      if (pair.kind === 'text') {
        if (pair.ours.text !== pair.theirs.text) {
          this.differ(`${path} > child ${i}`, o, t)
          return
        }
        this.matched += 1
      } else {
        const n = counts.get(pair.ours.tag) ?? 0
        counts.set(pair.ours.tag, n + 1)
        this.element(pair.ours, pair.theirs, `${path} > ${pair.ours.tag}[${n}]`)
      }
      i += 1
      j += 1
    }
  }
}

type Pair =
  | { kind: 'text'; ours: RenderedText; theirs: RenderedText }
  | { kind: 'element'; ours: RenderedElement; theirs: RenderedElement }

/** Two children that line up — same kind, and for elements the same tag — or null. */
function pairOf(o: RenderedNode | null, t: RenderedNode | null): Pair | null {
  if (o === null || t === null) return null
  if (o.kind === 'text' && t.kind === 'text') return { kind: 'text', ours: o, theirs: t }
  if (o.kind === 'element' && t.kind === 'element' && o.tag === t.tag) {
    return { kind: 'element', ours: o, theirs: t }
  }
  return null
}

/**
 * Compares two rendered pages; `ours` is the page under test and `theirs` the
 * reference. Within a parent, comparison stops at the first difference — one extra
 * child would otherwise misalign every sibling after it and report each as a change.
 */
export function compareRendered(
  ours: RenderedPage,
  theirs: RenderedPage,
  classes: Forgiveness[] = []
): Comparison {
  const comparator = new Comparator(classes)
  if (ours.doctype !== theirs.doctype) {
    comparator.differences.push({
      path: 'doctype',
      ours: ours.doctype,
      theirs: theirs.doctype,
    })
  }
  comparator.element(ours.root, theirs.root, ours.root.tag)
  return {
    differences: comparator.differences,
    forgiven: comparator.forgiven,
    matched: comparator.matched,
  }
}

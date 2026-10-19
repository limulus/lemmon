/**
 * The link check (D47): every link the build's HTML, CSS and XML output holds, and every
 * url an asset hook mapped, resolved as a browser resolves it and matched against the files
 * the build writes — exactly, case and Unicode form included, since a host that matches
 * names exactly serves nothing else — and each fragment into an HTML page against the ids
 * in it, before anything is written.
 */

import type { Diagnostic, Label } from '@limulus/lemmon-core'
import { Buffer } from 'node:buffer'

import { cssLinks, htmlLinks, xmlLinks, type Link } from './link-scan.ts'
import { pageName } from './plugins.ts'
import { FILE_START } from './program.ts'
import {
  outputFile,
  type Found,
  type Output,
  type OutputFiles,
  type Writer,
} from './write.ts'

/** What the check reads: what the build holds in memory to write, and the config's say. */
export interface LinkCheck {
  /** Every file the build writes from memory — pages, asset files, produced files. */
  outputs: readonly Output[]
  /** Every output file's claim, passthrough files' among them, which a link may reach. */
  files: OutputFiles
  /** The merged manifest of the asset hooks, and who mapped each of its paths. */
  manifest: Record<string, string>
  mappedBy: ReadonlyMap<string, string>
  /** The site's origin, if the config names one: what tells its own links from others. */
  origin?: string
  /** The paths on the origin its host answers without a file the build writes. */
  served: readonly string[]
  /** The config file, where a manifest entry's fault is placed. */
  config: string
}

/**
 * The origin a page's url is placed on where the site names none, so that a link
 * written as a path resolves to it and every absolute link is another origin's.
 */
const PLACEHOLDER = 'http://site.invalid'

/** How a link failed to reach what it names, as a diagnostic's message ends with it. */
interface Broken {
  /** What the message ends with: what the url names, and why that is not there. */
  why: string
  /** The writer whose file the message names, where one does. */
  label?: Label
}

/** One scanned file: its claim, and what links it holds or why none could be read. */
interface Scanned {
  output: Output
  writer: Writer
  links: Link[]
  refused?: string
}

/**
 * The errors the check finds in `check`: one per output file per distinct url it writes
 * that reaches no file the build writes, or no element of the page its fragment names —
 * placed at the file's writer, a page's url or its generator's `for:` entry, a producer's
 * mark, the config's start for an asset hook's file, and naming each way the url is
 * written and each place it stands; one per manifest entry that names no file, at the
 * config's start, whose url no page is then faulted for again; and one per XML file
 * whose links cannot be read. And a warning at the config's start for each `served` path
 * that names a file the build writes, which a link to it is then not checked against.
 *
 * A link is resolved against its base, a page's own url on the site's origin, and is
 * skipped where the URL parser refuses it, it names a scheme other than `http` or `https`
 * or a host other than the origin's, or its path is one the config lists as `served`.
 * Any other names the file {@link outputFile} maps its path to, which some writer must
 * claim exactly; where one claims a file that differs only in case or Unicode form, or
 * claims the `index.html` inside it, the message says so and is labelled at that writer.
 * A fragment into an HTML page the build writes must name an element of it by HTML's
 * rule; one into any other file is not checked.
 */
export function checkLinks(check: LinkCheck): Diagnostic[] {
  const origin = check.origin ?? PLACEHOLDER
  const { host } = new URL(origin)
  const served = new Set(check.served)
  const decoder = new TextDecoder()
  const targets = new Map<string, Set<string>>()
  const scanned: Scanned[] = []
  for (const output of check.outputs) {
    const { writer } = check.files.find(output.file)!
    const base = new URL(output.url, origin).href
    const kind = /\.(html|css|xml)$/i.exec(output.file)?.[1].toLowerCase()
    if (kind === undefined) continue
    if (kind === 'html') {
      const { links, targets: ids } = htmlLinks(decoder.decode(output.bytes), base)
      targets.set(output.file, ids)
      scanned.push({ output, writer, links })
    } else if (kind === 'css') {
      const links = cssLinks(decoder.decode(output.bytes)).map((link) => ({
        ...link,
        base,
      }))
      scanned.push({ output, writer, links })
    } else {
      scanned.push({ output, writer, links: [], ...xmlLinks(output.bytes, base) })
    }
  }
  /** `url` resolved against `base`, where it is the site's own and not `served`. */
  const resolve = (url: string, base: string): URL | undefined => {
    if (!URL.canParse(url, base)) return undefined
    const resolved = new URL(url, base)
    // The site's own on either scheme, which a `blob:` url on its origin is not.
    const own = /^https?:$/.test(resolved.protocol) && resolved.host === host
    return own && !served.has(resolved.pathname) ? resolved : undefined
  }
  /** Why `resolved`, a link in `from`, reaches nothing the build writes, if it does not. */
  const reach = (resolved: URL, from: string): Broken | undefined => {
    const file = outputFile(resolved.pathname)
    if (file === undefined) {
      return { why: `which names \`${resolved.pathname}\`, where the build writes no file` }
    }
    const found = check.files.find(file)
    if (found === undefined) {
      const index = check.files.find(`${file}/index.html`)
      const named = `which names \`${file}\`, where the build writes no file`
      if (index?.exact !== true) return { why: named }
      const at = `${resolved.pathname}/`
      return {
        why: `${named}; it writes \`${index.file}\`, at \`${at}\``,
        label: writtenBy(index),
      }
    }
    if (!found.exact) {
      return {
        why:
          `which names \`${file}\`: the build writes \`${found.file}\`, which differs ` +
          'from it only in case or Unicode form, and a host that matches names exactly ' +
          'serves nothing there',
        label: writtenBy(found),
      }
    }
    const ids = targets.get(found.file)
    if (ids === undefined || indicates(ids, resolved.hash)) return undefined
    return {
      why:
        `and \`${found.file}\` has no element whose \`id\`, or an \`a\` whose ` +
        `\`name\`, is \`${percentDecoded(fragment(resolved.hash))}\``,
      label: found.file === from ? undefined : writtenBy(found),
    }
  }
  const diagnostics: Diagnostic[] = []
  for (const path of check.served) {
    const file = outputFile(path)
    const found = file === undefined ? undefined : check.files.find(file)
    if (found?.exact !== true) continue
    diagnostics.push({
      path: check.config,
      span: FILE_START,
      severity: 'warning',
      message:
        `\`served\` lists \`${path}\`, where the build writes \`${found.file}\`, so ` +
        'a link there is not checked, nor a fragment into it: `served` is for the paths ' +
        'the host answers without a file',
      labels: [writtenBy(found)],
    })
  }
  // A manifest url that reaches nothing is said once, here, and not again at each page
  // that prints it.
  const faulted = new Set<string>()
  for (const [path, url] of Object.entries(check.manifest)) {
    const resolved = resolve(stripped(url), `${origin}/`)
    const broken = resolved === undefined ? undefined : reach(resolved, '')
    if (broken === undefined) continue
    faulted.add(resolved!.pathname)
    const plugin = check.mappedBy.get(path)!
    diagnostics.push({
      path: check.config,
      span: FILE_START,
      severity: 'error',
      message:
        `the \`${plugin}\` plugin's \`assets\` hook maps \`${path}\` to ` +
        `${JSON.stringify(url)}, ${broken.why}`,
      labels: broken.label === undefined ? [] : [broken.label],
    })
  }
  for (const { output, writer, links, refused } of scanned) {
    const place = { path: writer.path, span: writer.span ?? FILE_START }
    if (refused !== undefined) {
      diagnostics.push({
        ...place,
        severity: 'error',
        message: `${written(output.file, writer)} ${refused}, so no link in it can be read`,
        labels: [],
      })
      continue
    }
    // By the url each resolves to, however it is written and wherever it stands.
    const verdicts = new Map<string, Faulted | undefined>()
    for (const { url: as, at, base } of links) {
      const url = stripped(as)
      const resolved = resolve(url, base)
      if (resolved === undefined || faulted.has(resolved.pathname)) continue
      if (!verdicts.has(resolved.href)) {
        const broken = reach(resolved, output.file)
        verdicts.set(resolved.href, broken && { urls: new Set(), ats: [], broken })
      }
      const fault = verdicts.get(resolved.href)
      fault?.urls.add(url)
      fault?.ats.push(at)
    }
    for (const fault of verdicts.values()) {
      if (fault === undefined) continue
      const urls = LIST.format([...fault.urls].map((url) => JSON.stringify(url)))
      diagnostics.push({
        ...place,
        severity: 'error',
        message:
          `${subject(output.file, writer)} links to ${urls} in ${places(fault.ats)}, ` +
          fault.broken.why,
        labels: fault.broken.label === undefined ? [] : [fault.broken.label],
      })
    }
  }
  return diagnostics
}

/** A url a file links to that reaches nothing, each way it is written, and where. */
interface Faulted {
  urls: Set<string>
  ats: string[]
  broken: Broken
}

/**
 * How a message names the file `file` a link stands in, by its writer: a page by its path
 * — a generated one by its item too — and a plugin's file by itself and the plugin.
 */
function subject(file: string, writer: Writer): string {
  return writer.producer === undefined && writer.assets === undefined
    ? pageName(writer.path, writer.generated)
    : written(file, writer)
}

/**
 * How a message names `file` with its writer: `` `feed.xml`, which `feed.lem` writes, ``,
 * or the plugin that wrote it — a producer for a page, an asset hook.
 */
function written(file: string, { path, generated, producer, assets }: Writer): string {
  if (producer !== undefined) {
    const { plugin, page, body } = producer
    const named = pageName(page, producer.generated, body)
    return `\`${file}\`, which the \`${plugin}\` plugin writes for ${named},`
  }
  const by =
    assets === undefined
      ? pageName(path, generated)
      : `the \`${assets.plugin}\` plugin's \`assets\` hook`
  return `\`${file}\`, which ${by} writes,`
}

/** The label at the writer of the file `found` names. */
function writtenBy({ file, writer }: Found): Label {
  return {
    path: writer.path,
    span: writer.span ?? FILE_START,
    message: `the writer of \`${file}\``,
  }
}

/** Each place in `ats` once, in the order first met, and how often where more than once. */
function places(ats: string[]): string {
  const counts = new Map<string, number>()
  for (const at of ats) counts.set(at, (counts.get(at) ?? 0) + 1)
  return LIST.format([...counts].map(([at, n]) => `\`${at}\`${OFTEN[n] ?? ` ${n} times`}`))
}

const LIST = new Intl.ListFormat('en', { type: 'conjunction' })

/** How often a place holds a url, by the count, where a message says it in a word. */
const OFTEN: readonly string[] = ['', '', ' twice']

/**
 * `hash` — a url's fragment as the URL parser writes it, `#` and all — without the text
 * directive that a `:~:` starts, which names no element.
 */
function fragment(hash: string): string {
  const raw = hash.slice(1)
  const directive = raw.indexOf(':~:')
  return directive === -1 ? raw : raw.slice(0, directive)
}

/**
 * Whether a document whose ids and `a` names are `ids` has the element `hash` indicates,
 * or the top of the document, by HTML's rule for the indicated part of a document: no
 * fragment, or `top` in any case, is the top; else an element whose `id`, or an `a`
 * whose `name`, is the fragment, or is the fragment percent-decoded.
 */
function indicates(ids: Set<string>, hash: string): boolean {
  const raw = fragment(hash)
  if (raw === '' || ids.has(raw)) return true
  const decoded = percentDecoded(raw)
  return ids.has(decoded) || decoded.toLowerCase() === 'top'
}

/** `url` without the ASCII whitespace HTML strips from either end of one. */
function stripped(url: string): string {
  return url.replace(/^[\t\n\f\r ]+|[\t\n\f\r ]+$/g, '')
}

/**
 * `text` — ASCII, as the URL parser writes a fragment — percent-decoded and read as
 * UTF-8, as a browser decodes a fragment.
 */
function percentDecoded(text: string): string {
  const bytes = Buffer.from(
    text.replace(/%([\da-f]{2})/gi, (_, hex: string) =>
      String.fromCharCode(parseInt(hex, 16))
    ),
    'latin1'
  )
  return new TextDecoder('utf-8', { ignoreBOM: true }).decode(bytes)
}

import {
  producerOverlap,
  resolveMarks,
  type Claim,
  type CodePayload,
  type Diagnostic,
  type JsonValue,
  type Mark,
  type MarkContext,
  type MarkHandler,
  type RenderResult,
} from '@limulus/lemmon-core'
import { strict as assert } from 'node:assert'

import type { Environment } from './load.ts'

/**
 * A plugin, as a site's `lemmon.config.ts` lists it under `plugins`. Its hooks in this
 * shell are {@link transformDocument}, {@link assets} and {@link marks} (D44, D42); any
 * other field is refused when the config loads, so that a hook nothing runs is never
 * loaded in silence.
 */
export interface Plugin {
  /** What a refusal at config load, or a hook's failure, calls this plugin. */
  name: string
  /**
   * Its document transform (D44): given each document (`.md`) whose front matter read,
   * before anything compiles, and returning the body it compiles with — what follows
   * the front matter, which a transform cannot change. The transforms run in the order
   * the plugins are listed, each over the body the one before returned, and the text
   * they leave is what the core compiles and what a diagnostic in the document is
   * framed against: a transform that appends keeps every position the author's file
   * has, and one that changes anything earlier moves them. A page template (`.lem`) is
   * not given to it: it has no markdown, and every layout compiles against what it
   * defines, so a transform there would reach other files.
   *
   * It returns as it runs: an `async` transform is refused when the config loads. One
   * that throws, or returns anything but a string the core can be sent — or a body that
   * would give the text other front matter — a `---` first line in a document that had
   * none, or, where the closing `---` ended the file, a first line that is not a newline
   * — is an error at the document's front matter, or at its start where it has
   * none, and stops the build before it compiles.
   */
  transformDocument?: (document: DocumentSource) => string
  /**
   * Its asset hook (D44, D38): called once per build, after the documents are
   * transformed and before anything compiles, and returning the manifest `asset("…")`
   * and `[asset]` fields resolve against, with the files it built. It may be `async`,
   * and the build waits for it. Each plugin's manifest is declared to the core with
   * every other's, and a logical path two plugins both map is an error naming both. Its
   * files are written with the pages, checked first against every passthrough file and
   * every other plugin's, then each page and produced file against them.
   *
   * `lem check` calls it too, and writes none of its files. One that throws, or returns
   * anything but {@link Assets}, is an error at the start of the config file, which
   * lists the plugin, and stops the build before it compiles.
   */
  assets?: (context: AssetsContext) => Assets | Promise<Assets>
  /**
   * Its mark handlers (D42): what they claim is declared to the core at compile, and a
   * page's marks are resolved by every listed plugin's — each mark's producer, then its
   * decorators in the order the plugins, and their handlers, are listed.
   */
  marks?: PluginHandler[]
}

/** A document as a {@link Plugin.transformDocument} is given it. */
export interface DocumentSource {
  /** Its path, relative to the input directory, with `/` separators: `links/x/index.md`. */
  path: string
  /**
   * Its front matter as the core loads it: `type:` and `for:` read off, and `url` and
   * `inputPath` derived where it gives neither. Frozen, since what the front matter says
   * is read before any transform runs: a write to it throws in strict-mode code, as an
   * ES module is, and is ignored elsewhere.
   */
  frontMatter: { readonly [key: string]: JsonValue }
  /**
   * Its text after the front matter — the whole of it where there is none — as the
   * transform before this one returned it.
   */
  body: string
}

/** What an {@link Plugin.assets} hook is given: the build it runs for. */
export interface AssetsContext {
  /**
   * The directory the config file is in, absolute, which its `input` and `output` are
   * relative to: the project's root, where a tool run for the site runs as it would
   * from the command line, wherever `lem` was started — esbuild, whose messages name
   * each source from it, and whose maps the shell's `esbuild()` makes relative to it.
   */
  root: string
  /** The input directory, absolute. */
  input: string
  /**
   * The output directory, absolute: where the build writes the files the hook returns,
   * each at the file its url names.
   */
  output: string
  /**
   * Whether the build writes its output: `true` for `lem build` and `lem dev`, `false`
   * for `lem check`, which checks the hook's files as a build would and writes none.
   */
  writes: boolean
  /**
   * The environment variables the build reads, the values a site's `env("…")` bindings
   * load — `process.env`, as `lem` runs — so that a hook and the site see one
   * environment, and a hook can be given one without touching the process's. A frozen
   * copy: writing to it changes nothing the build reads, and throws in strict-mode code.
   */
  env: Readonly<Environment>
}

/** What an {@link Plugin.assets} hook returns. */
export interface Assets {
  /**
   * Each asset's logical path — what `asset("…")` names — mapped to the finished URL it
   * ships at, since the core does no path arithmetic (D38): both non-empty strings. None
   * where omitted.
   */
  manifest?: Record<string, string>
  /**
   * Each file the hook built, by its url — a path from `/`, mapped to a file as a page's
   * url is — as a string, written as UTF-8, or bytes. None where omitted.
   */
  files?: Record<string, string | Uint8Array>
}

/**
 * One mark handler of a plugin — a producer, a decorator, or both, as a core
 * {@link MarkHandler} is — with the record its marks carry, and a producer given a
 * {@link ProducerContext}, which can write files. A handler sees a code block's mark with
 * its payload filled in, `{language, info, code}` as `Code` declares it, beside the id
 * the render names it by.
 */
export interface PluginHandler extends Omit<MarkHandler, 'produce'> {
  /** One replacement per mark it answers, in the order the marks were given. */
  produce?: (context: ProducerContext) => readonly (string | Uint8Array)[]
  /**
   * The declared record a mark of {@link name} carries, which every `mark()` call's
   * payload is checked against — `Code` for `code`, which the shell declares; any other
   * is declared by the site's own `.lem`.
   */
  payload?: string
  /**
   * The field of {@link payload} holding a mark's key — `language` for `code` — so a
   * template's mark is dispatched by it as a code block is by its language. Needs
   * {@link payload}.
   */
  keyedBy?: string
}

/**
 * What a producer is given: the core's {@link MarkContext}, and a way to write a file of
 * its own beside the pages — a stylesheet its marks link to.
 */
export interface ProducerContext extends MarkContext {
  /**
   * Queues `content` as the output file `url` names — a path from `/`, mapped to a file
   * as a page's url is — written with the pages once every page has rendered. A file
   * another page's producer wrote with the same bytes is written once. It works only
   * while the producer runs: a call after the producer returns is dropped.
   *
   * It never throws. A url that is not a string, content neither a string nor bytes, a
   * url naming no file, and other content where this producer or another page's wrote
   * already are each an error at this producer's first mark. The file is also checked
   * against every page, passthrough file, asset hook's file and other produced file —
   * the same file, the same but for case or Unicode form, or a file where the other
   * needs a directory — an error at the later writer, a page's url or a producer's mark,
   * labelled at the first.
   * Any of these stops the build before it writes. A producer that fails writes nothing,
   * and nothing is said about what it queued.
   */
  write(url: string, content: string | Uint8Array): void
}

const PLUGIN_FIELDS: readonly string[] = [
  'name',
  'transformDocument',
  'assets',
  'marks',
] satisfies (keyof Plugin)[]

const HANDLER_FIELDS: readonly string[] = [
  'name',
  'keys',
  'payload',
  'keyedBy',
  'produce',
  'decorate',
] satisfies (keyof PluginHandler)[]

/** A handler as the rules across plugins read it, with the plugin that lists it. */
interface Listed {
  plugin: string
  name: string
  keys?: readonly string[]
  produce?: unknown
  payload?: string
  keyedBy?: string
}

/**
 * A config's `plugins` as the plugins it lists — none where it is omitted — or a
 * `TypeError` saying why it is not one: the shape of each plugin, its hooks and its
 * handlers — an `async` function where a hook returns as it runs among them — and then
 * the rules across them, each refused here rather than on the first page that meets it,
 * naming the plugins to blame. One name names one plugin; a mark has one producer, so no
 * two producers answer one name and key (D42); every mark of a name carries one payload,
 * keyed by one field — the rule the core's declaration refuses too, restated so that the
 * refusal can name the plugins; and a name a handler lists keys for is keyed by a field.
 */
export function validatePlugins(value: unknown): Plugin[] {
  if (value === undefined) return []
  if (!Array.isArray(value)) throw new TypeError('`plugins` must be a list of plugins')
  const names = new Set<string>()
  const listed: Listed[] = []
  // A hole in a list is read as `undefined`, so it is refused rather than skipped.
  Array.from(value as unknown[]).forEach((plugin, index) => {
    const name = pluginName(plugin, `plugins[${index}]`)
    if (names.has(name)) {
      throw new TypeError(
        `two plugins are named \`${name}\`; each needs its own, which is what a ` +
          'message calls it'
      )
    }
    names.add(name)
    hooksOf(plugin as Record<string, unknown>, name)
    listed.push(...handlersOf(plugin as Record<string, unknown>, name))
  })
  const overlap = producerOverlap(listed)
  if (overlap !== undefined) {
    const { first, second, name, key } = overlap
    const what = key === undefined ? `\`${name}\`` : `\`${name}\` for \`${key}\``
    throw new TypeError(
      (first.plugin === second.plugin
        ? `the plugin \`${first.plugin}\` produces ${what} twice`
        : `the plugins \`${first.plugin}\` and \`${second.plugin}\` both produce ${what}`) +
        '; a mark has one producer (D42)'
    )
  }
  agree(listed, 'payload', 'payloads', 'carries one payload')
  agree(listed, 'keyedBy', 'key fields', 'is keyed by one field')
  for (const handler of listed) {
    const { plugin, name } = handler
    if (handler.keys === undefined) continue
    if (listed.some((other) => other.name === name && other.keyedBy !== undefined)) continue
    throw new TypeError(
      `the plugin \`${plugin}\`'s handler for \`${name}\` lists keys, and no handler ` +
        `for \`${name}\` names \`keyedBy\`, the payload field a mark's key is read from, ` +
        "so a `mark()` call's mark would carry no key: add `keyedBy`, with the record's " +
        "`payload` — `payload: 'Code', keyedBy: 'language'` for `code`"
    )
  }
  return value as Plugin[]
}

/** The name of `plugin`, at `at` in the list, once it is known to be a plugin object. */
function pluginName(plugin: unknown, at: string): string {
  if (typeof plugin === 'function') {
    const { name } = plugin
    throw new TypeError(
      name === ''
        ? `\`${at}\` is a function rather than a plugin: call the plugin factory for the ` +
            'plugin it makes'
        : `\`${at}\` is the function \`${name}\` rather than a plugin: call it, ` +
            `\`${name}()\`, for the plugin it makes`
    )
  }
  if (!isObject(plugin)) throw new TypeError(`\`${at}\` must be an object`)
  const { name } = plugin
  if (!isName(name)) throw new TypeError(`\`${at}.name\` must be a non-empty string`)
  for (const field of Object.keys(plugin)) {
    if (!PLUGIN_FIELDS.includes(field)) {
      throw new TypeError(
        `the plugin \`${name}\` has \`${field}\`, which this shell does not run; a ` +
          `plugin has ${fields(PLUGIN_FIELDS)}`
      )
    }
  }
  return name
}

/**
 * Checks the hooks of `plugin`, the plugin named `name`, that run before anything
 * compiles: each a function, and the document transform not `async`, since a document
 * is transformed as discovery reads it. The asset hook may be.
 */
function hooksOf(plugin: Record<string, unknown>, name: string): void {
  for (const hook of ['transformDocument', 'assets'] as const) {
    const value = plugin[hook]
    if (value !== undefined && typeof value !== 'function') {
      throw new TypeError(`\`${hook}\` of the plugin \`${name}\` must be a function`)
    }
  }
  if (isAsync(plugin.transformDocument)) {
    throw new TypeError(
      `\`transformDocument\` of the plugin \`${name}\` is async, and a transform returns ` +
        "the document's body as it runs: the build waits for no promise from it"
    )
  }
}

/** The handlers of `plugin`, the plugin named `name`, each checked for its shape. */
function handlersOf(plugin: Record<string, unknown>, name: string): Listed[] {
  const { marks } = plugin
  if (marks === undefined) return []
  if (!Array.isArray(marks)) {
    throw new TypeError(`\`marks\` of the plugin \`${name}\` must be a list of handlers`)
  }
  return Array.from(marks as unknown[]).map((handler, index) => {
    const of = `of the plugin \`${name}\``
    if (!isObject(handler)) {
      throw new TypeError(`\`marks[${index}]\` ${of} must be an object`)
    }
    if (!isName(handler.name)) {
      throw new TypeError(`\`marks[${index}].name\` ${of} must be a non-empty string`)
    }
    return handlerOf(
      handler,
      name,
      `the plugin \`${name}\`'s handler for \`${handler.name}\``
    )
  })
}

/** `handler`, which `label` describes, checked field by field. */
function handlerOf(
  handler: Record<string, unknown>,
  plugin: string,
  label: string
): Listed {
  for (const field of Object.keys(handler)) {
    if (!HANDLER_FIELDS.includes(field)) {
      throw new TypeError(
        `${label} has \`${field}\`, which a mark handler does not; a handler has ` +
          fields(HANDLER_FIELDS)
      )
    }
  }
  const { name, keys, payload, keyedBy, produce, decorate } = handler
  if (
    keys !== undefined &&
    !(Array.isArray(keys) && keys.length > 0 && Array.from(keys as unknown[]).every(isName))
  ) {
    throw new TypeError(
      `\`keys\` of ${label} must be a non-empty list of non-empty strings`
    )
  }
  for (const [field, value] of [
    ['payload', payload],
    ['keyedBy', keyedBy],
  ] as const) {
    if (value !== undefined && !isName(value)) {
      throw new TypeError(`\`${field}\` of ${label} must be a non-empty string`)
    }
  }
  if (keyedBy !== undefined && payload === undefined) {
    throw new TypeError(
      `${label} keys its marks by \`${keyedBy as string}\` but names no \`payload\` to ` +
        'find that field in'
    )
  }
  for (const [field, value] of [
    ['produce', produce],
    ['decorate', decorate],
  ] as const) {
    if (value !== undefined && typeof value !== 'function') {
      throw new TypeError(`\`${field}\` of ${label} must be a function`)
    }
    // An async handler's content would arrive after the page is written: refused here
    // rather than warned at on every page.
    if (isAsync(value)) {
      throw new TypeError(
        `\`${field}\` of ${label} is async, and a handler returns its content as it ` +
          'runs: the build waits for no promise'
      )
    }
  }
  if (produce === undefined && decorate === undefined) {
    throw new TypeError(
      `${label} neither produces nor decorates: it needs a \`produce\` or a \`decorate\` ` +
        'function'
    )
  }
  return {
    plugin,
    name: name as string,
    keys,
    produce,
    payload: payload as string | undefined,
    keyedBy: keyedBy as string | undefined,
  }
}

/**
 * Refuses two handlers that name different values of `field` for one mark name, as the
 * core's declaration does: a handler naming none agrees with any (D42).
 */
function agree(
  listed: Listed[],
  field: 'payload' | 'keyedBy',
  what: string,
  rule: string
): void {
  const first = new Map<string, Listed>()
  for (const handler of listed) {
    const named = handler[field]
    if (named === undefined) continue
    const was = first.get(handler.name)
    if (was === undefined) {
      first.set(handler.name, handler)
    } else if (was[field] !== named) {
      const who =
        was.plugin === handler.plugin
          ? `the plugin \`${was.plugin}\` names`
          : `the plugins \`${was.plugin}\` and \`${handler.plugin}\` name`
      throw new TypeError(
        `${who} different ${what} for \`${handler.name}\`, \`${was[field]}\` and ` +
          `\`${named}\`; every mark of a name ${rule}`
      )
    }
  }
}

/** Whether `value` is an `async` function. */
const isAsync = (value: unknown): boolean =>
  (value as { [Symbol.toStringTag]?: unknown } | undefined)?.[Symbol.toStringTag] ===
  'AsyncFunction'

const isObject = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value)

/**
 * Whether `value` is a non-empty string the core can be sent: a lone surrogate has no
 * UTF-8 form, and would fail the call that declares it.
 */
export const isName = (value: unknown): value is string =>
  typeof value === 'string' && value !== '' && !/\p{Cs}/u.test(value)

export const fields = (names: readonly string[]): string =>
  names.map((name) => `\`${name}\``).join(', ')

/**
 * What `plugins` claim (D42), which `lem build` declares to the core: one claim per
 * handler, in the order listed — a producer's name with the keys it lists, a decorator's
 * name with none. Each claim carries a key list, empty where the handler lists no keys,
 * because a claim with no list at all covers every key: a decorator's would silence the
 * warning at a code block's language no producer covers, and so would a keyless
 * producer's, though the dispatch hands that producer no keyed mark. The core merges the
 * claims made for one name.
 */
export function claimsOf(plugins: readonly Plugin[]): Claim[] {
  return plugins.flatMap((plugin) =>
    (plugin.marks ?? []).map(({ name, keys, payload, keyedBy, produce }) => ({
      name,
      keys: produce === undefined ? [] : [...(keys ?? [])],
      payload,
      keyedBy,
    }))
  )
}

/**
 * Which of a generator's pages one is (D43): its index among them, and the list they
 * were made from, as its `for:` key names it — `site.gallery`.
 */
export interface Generated {
  index: number
  list: string
}

/**
 * How a message names the page at `path`: `` `about.md` ``, and a page a generator made
 * by the item it was made from, in the language's own index syntax —
 * `` `photos/photo.lem` for `site.gallery[3]` `` — as the core names it, since every page
 * of a generator shares its path (D43).
 */
export function pageName(path: string, generated?: Generated): string {
  if (generated === undefined) return `\`${path}\``
  return `\`${path}\` for \`${generated.list}[${generated.index}]\``
}

/** What resolving one page's marks takes beside its render. */
export interface PageMarks {
  /**
   * The page's path, relative to the input — what a handler's warning names, since its
   * marks stand in its layouts as often as in the page itself.
   */
  path: string
  /** Which of its generator's pages it is, where a generator made it. */
  generated?: Generated
  plugins: readonly Plugin[]
  /** Every code block's payload the compile reported, by id. */
  payloads: ReadonlyMap<string, CodePayload>
}

/** A file a producer wrote through its context, for the build to claim and write. */
export interface ProducedFile {
  /** The url the producer named, as it named it. */
  url: string
  bytes: Uint8Array
  /** The plugin whose producer wrote it. */
  plugin: string
  /** The producer's first mark on the page: where a refusal of the file points. */
  mark: Mark
}

/** A page with its marks resolved, what its producers wrote, and what was reported. */
export interface ResolvedPage {
  bytes: Uint8Array
  /** Every file a producer that succeeded wrote, in the order written. */
  files: ProducedFile[]
  diagnostics: Diagnostic[]
}

/**
 * The page `rendered` with its marks filled in by the handlers of `page.plugins` (step 7
 * of the build pipeline, D42): each mark's producer over the page as the core returned
 * it, then its decorators, in the order the plugins and their handlers are listed.
 *
 * No handler's failure breaks the page or stops the build (D42). One that throws, or
 * returns anything but its content — a promise, a count other than one per mark, a value
 * neither a string nor bytes — is a warning at the mark, the producer's first, and the
 * marks keep the content they had: the default for a producer's, the content so far for
 * a decorator's. A file a producer that succeeded wrote is another matter: one `write`
 * was given the wrong kind of value for is an error here, and the build checks the rest
 * ({@link ProducerContext.write}). What the page collected under a name it never marks is
 * a warning at the first such fragment, since nothing writes it. A mark nothing answers
 * keeps its default content unreported here: the compile warned at a literal name or key
 * no handler claims, and a key computed at render, or a code block with no language where
 * the producers of `code` list keys, is not reported at all.
 */
export function resolvePage(rendered: RenderResult, page: PageMarks): ResolvedPage {
  const resolved: Omit<ResolvedPage, 'bytes'> = { files: [], diagnostics: [] }
  const { diagnostics } = resolved
  const handlers = page.plugins.flatMap((plugin) =>
    (plugin.marks ?? []).map((handler) => wrapped(plugin.name, handler, page, resolved))
  )
  const { html, unplaced } = resolveMarks(rendered, handlers)
  for (const name of unplaced) {
    const [first] = rendered.fragments![name]
    diagnostics.push({
      path: first.path,
      span: first.span,
      severity: 'warning',
      message:
        `${pageName(page.path, page.generated)} collects \`${name}\` here, and nothing ` +
        `on that page places \`mark("${name}")\`, so what it collected is not written`,
      labels: [],
    })
  }
  return { bytes: html, ...resolved }
}

/**
 * `handler`, of the plugin `plugin`, as the core's dispatch runs it for one page: given
 * marks with their code payloads filled in, kept from failing the page (above), and a
 * producer's files taken into `resolved` only once it has succeeded.
 */
function wrapped(
  plugin: string,
  handler: PluginHandler,
  page: PageMarks,
  resolved: Omit<ResolvedPage, 'bytes'>
): MarkHandler {
  const { diagnostics, files } = resolved
  const { name, keys, produce, decorate } = handler
  const wrapper: MarkHandler = { name, keys }
  const failed = (role: string, mark: Mark, kept: string, problem: string) => {
    diagnostics.push({
      path: mark.path,
      span: mark.span,
      severity: 'warning',
      message:
        `the \`${plugin}\` plugin's ${role} for \`${name}\` failed on ` +
        `${pageName(page.path, page.generated)}, so ${kept}: ${problem}`,
      labels: [],
    })
  }
  if (produce !== undefined) {
    wrapper.produce = (context) => {
      const marks = context.marks.map((mark) => filled(mark, page.payloads))
      // Queued as given and judged once the call has succeeded, so that a producer that
      // fails leaves no file and no error behind, and `write` has nothing to throw.
      const queued: { url: unknown; content: unknown }[] = []
      const write = (url: string, content: string | Uint8Array) => {
        // Bytes are copied as they stand now: the producer may reuse its buffer.
        // `new Uint8Array` copies; a Node `Buffer`'s `slice` is a view.
        const copy = content instanceof Uint8Array ? new Uint8Array(content) : content
        queued.push({ url, content: copy })
      }
      let made: unknown
      const problem = failure(() => {
        made = produce.call(handler, { ...context, marks, write })
        return unproduced(made, marks.length)
      })
      if (problem === undefined) {
        for (const { url, content } of queued) {
          if (typeof url === 'string' && isContent(content)) {
            const bytes = typeof content === 'string' ? encoder.encode(content) : content
            files.push({ url, bytes, plugin, mark: marks[0] })
            continue
          }
          diagnostics.push({
            path: marks[0].path,
            span: marks[0].span,
            severity: 'error',
            message:
              `the \`${plugin}\` plugin's producer for \`${name}\` called \`write\` on ` +
              `${pageName(page.path, page.generated)} with ${shown(url)} and ` +
              `${shown(content)}, where it takes a url and a string or bytes`,
            labels: [],
          })
        }
        return made as (string | Uint8Array)[]
      }
      settle(made)
      failed(
        'producer',
        marks[0],
        'the marks it answers keep their default content',
        problem
      )
      return context.marks.map(({ start, end }) => context.page.subarray(start, end))
    }
  }
  if (decorate !== undefined) {
    wrapper.decorate = (mark, content) => {
      let made: unknown
      const problem = failure(() => {
        made = decorate.call(handler, filled(mark, page.payloads), content)
        return isContent(made)
          ? undefined
          : `it returned ${shown(made)} rather than a string or bytes`
      })
      if (problem === undefined) return made as string | Uint8Array
      settle(made)
      failed('decorator', mark, 'this mark keeps the content it had', problem)
      return content
    }
  }
  return wrapper
}

/**
 * `mark` as a handler is given it: a code block's with its payload beside the id the
 * render names it by.
 */
function filled(mark: Mark, payloads: ReadonlyMap<string, CodePayload>): Mark {
  if (mark.payloadId === undefined) return mark
  const payload = payloads.get(mark.payloadId)
  assert(payload !== undefined, 'the compile reported every code block a render marks')
  const { language, info, code } = payload
  return { ...mark, payload: { language, info, code } }
}

export const encoder = new TextEncoder()

/** How `call` failed — the problem it returned, or what it threw — if it did. */
export function failure(call: () => string | undefined): string | undefined {
  try {
    return call()
  } catch (error) {
    return threw(error)
  }
}

/** What a hook threw, said as its failure: `it threw Error: …`. */
export function threw(error: unknown): string {
  const what =
    error instanceof Error || typeof error === 'string' ? String(error) : shown(error)
  return `it threw ${what}`
}

/** What is wrong with `made` as a producer's answer for `marks` marks, if anything. */
function unproduced(made: unknown, marks: number): string | undefined {
  if (!Array.isArray(made)) {
    return `it returned ${shown(made)} rather than a list of replacements`
  }
  if (made.length !== marks) {
    return `it returned ${count(made.length, 'replacement')} for ${count(marks, 'mark')}`
  }
  const bad = made.findIndex((item) => !isContent(item))
  if (bad === -1) return undefined
  const what = shown(made[bad])
  return `it returned ${what} as a replacement, which is neither a string nor bytes`
}

const count = (n: number, noun: string) => `${n} ${noun}${n === 1 ? '' : 's'}`

export const isContent = (value: unknown): value is string | Uint8Array =>
  typeof value === 'string' || value instanceof Uint8Array

/**
 * What a handler gave back, said without printing it: a string may be a page long, and
 * an object may not print at all.
 */
export function shown(value: unknown): string {
  if (isThenable(value)) return 'a promise'
  if (typeof value === 'string') return 'a string'
  if (typeof value === 'function') return 'a function'
  if (typeof value === 'object' && value !== null) return 'an object'
  return String(value)
}

/** Whether `value` is a promise, or anything else a promise would take as one. */
const isThenable = (value: unknown): value is PromiseLike<unknown> =>
  (typeof value === 'object' || typeof value === 'function') &&
  value !== null &&
  typeof (value as { then?: unknown }).then === 'function'

/**
 * Handles the rejection of every promise a handler returned, alone or in its list, which
 * the build has already refused, so that a handler that made promises cannot surface
 * afterwards as an unhandled rejection that ends `lem` with a stack trace.
 */
export function settle(made: unknown): void {
  for (const value of Array.isArray(made) ? made : [made]) {
    if (isThenable(value)) Promise.resolve(value).catch(() => undefined)
  }
}

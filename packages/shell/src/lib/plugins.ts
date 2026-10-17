import {
  producerOverlap,
  resolveMarks,
  type Claim,
  type CodePayload,
  type Diagnostic,
  type Mark,
  type MarkContext,
  type MarkHandler,
  type RenderResult,
} from '@limulus/lemmon-core'
import { strict as assert } from 'node:assert'

/**
 * A plugin, as a site's `lemmon.config.ts` lists it under `plugins`. Its one hook in this
 * shell is {@link marks}; any other field is refused when the config loads, so that a
 * hook nothing runs is never loaded in silence.
 */
export interface Plugin {
  /** What a refusal at config load, or a handler's failure, calls this plugin. */
  name: string
  /**
   * Its mark handlers (D42): what they claim is declared to the core at compile, and a
   * page's marks are resolved by every listed plugin's — each mark's producer, then its
   * decorators in the order the plugins, and their handlers, are listed.
   */
  marks?: PluginHandler[]
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
   * against every page, passthrough file and other produced file — the same file, the
   * same but for case or Unicode form, or a file where the other needs a directory — an
   * error at the later writer, a page's url or a producer's mark, labelled at the first.
   * Any of these stops the build before it writes. A producer that fails writes nothing,
   * and nothing is said about what it queued.
   */
  write(url: string, content: string | Uint8Array): void
}

const PLUGIN_FIELDS: readonly string[] = ['name', 'marks'] satisfies (keyof Plugin)[]

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
 * `TypeError` saying why it is not one: the shape of each plugin and handler, and then
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
    if (
      (value as { [Symbol.toStringTag]?: unknown })?.[Symbol.toStringTag] ===
      'AsyncFunction'
    ) {
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

const isObject = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value)

/**
 * Whether `value` is a non-empty string the core can be sent: a lone surrogate has no
 * UTF-8 form, and would fail the call that declares the claims.
 */
const isName = (value: unknown): value is string =>
  typeof value === 'string' && value !== '' && !/\p{Cs}/u.test(value)

const fields = (names: readonly string[]): string =>
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

/** What resolving one page's marks takes beside its render. */
export interface PageMarks {
  /**
   * The page's path, relative to the input — what a handler's warning names, since its
   * marks stand in its layouts as often as in the page itself.
   */
  path: string
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
 * ({@link ProducerContext.write}). What the page collected under a name it never marks is a warning at the
 * first such fragment, since nothing writes it. A mark nothing answers keeps its default
 * content unreported here: the compile warned at a literal name or key no handler
 * claims, and a key computed at render, or a code block with no language where the
 * producers of `code` list keys, is not reported at all.
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
        `\`${page.path}\` collects \`${name}\` here, and nothing on that page places ` +
        `\`mark("${name}")\`, so what it collected is not written`,
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
        `the \`${plugin}\` plugin's ${role} for \`${name}\` failed on \`${page.path}\`, ` +
        `so ${kept}: ${problem}`,
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
              `\`${page.path}\` with ${shown(url)} and ${shown(content)}, where it takes a ` +
              'url and a string or bytes',
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

const encoder = new TextEncoder()

/** How `call` failed — the problem it returned, or what it threw — if it did. */
function failure(call: () => string | undefined): string | undefined {
  try {
    return call()
  } catch (error) {
    const what =
      error instanceof Error || typeof error === 'string' ? String(error) : shown(error)
    return `it threw ${what}`
  }
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

const isContent = (value: unknown): value is string | Uint8Array =>
  typeof value === 'string' || value instanceof Uint8Array

/**
 * What a handler gave back, said without printing it: a string may be a page long, and
 * an object may not print at all.
 */
function shown(value: unknown): string {
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
function settle(made: unknown): void {
  for (const value of Array.isArray(made) ? made : [made]) {
    if (isThenable(value)) Promise.resolve(value).catch(() => undefined)
  }
}

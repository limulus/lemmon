import { existsSync, statSync } from 'node:fs'
import { validateHeaderName, validateHeaderValue } from 'node:http'
import { dirname, isAbsolute, relative, resolve, sep } from 'node:path'
import { pathToFileURL } from 'node:url'

import { validatePlugins, type Plugin } from './plugins.ts'

/**
 * What a site's `lemmon.config.ts` exports by default:
 *
 * ```ts
 * import { bundler, type Config } from '@limulus/lemmon-shell'
 *
 * export default {
 *   input: 'www',
 *   output: 'dist/www',
 *   origin: 'https://example.com',
 *   served: ['/@eric'],
 *   passthrough: ['assets/**', 'robots.txt'],
 *   plugins: [bundler()],
 *   dev: { headers: { 'Cross-Origin-Opener-Policy': 'same-origin' } },
 * } satisfies Config
 * ```
 *
 * Directories are relative to the config file's own.
 */
export interface Config {
  /** The directory the site's documents, templates, and passthrough files are in. */
  input: string
  /** The directory a build writes to; it may not be the input or hold it. */
  output: string
  /**
   * Where the site is served: an `http` or `https` URL with nothing after its host and
   * port, `https://example.com`, since a page's url is a path from `/` and Lemmon has no
   * path prefix. A document's body is read on other pages — a feed's — so its links are
   * made absolute against it before the body is bound in `build.bodies` (D46). With none,
   * they are made root-relative, a path from `/`, which reads the same on every page of
   * the site and nowhere else.
   */
  origin?: string
  /**
   * The paths on {@link origin} that the site's host answers without a file the build
   * writes — a redirect, a rewrite — each a path from `/` as a link writes it, matched
   * exactly. A link to one is taken as answered, where the link check (D47) would
   * otherwise refuse it for naming no file; nothing else reads it. None when omitted.
   */
  served?: string[]
  /**
   * Globs naming the files copied to the output as they are, matched against each
   * file's path relative to {@link input} with Eleventy's glob semantics: picomatch,
   * dotfiles included, case-insensitive. Unlike Eleventy, the walk does not follow a
   * symbolically linked directory, so no glob reaches the files under one. A file a
   * glob matches is passthrough even when it is a document or template. None when
   * omitted.
   */
  passthrough?: string[]
  /**
   * The site's plugins, in the order their hooks run: their document transforms rewrite
   * each document's body before it compiles, their asset hooks hand the compile its
   * manifest and the build their files (D44), and their mark handlers fill each page's
   * marks (D42) — what those claim is declared to the core, and a page's decorators run
   * in this order. None when omitted, and a site with none is warned at every mark,
   * since nothing will fill one — not even `styles`, which the shell's default plugin,
   * `bundler()`, produces only where it is listed.
   */
  plugins?: Plugin[]
  /** How the dev server serves the output. */
  dev?: DevConfig
}

/** How the dev server serves a site's output. */
export interface DevConfig {
  /**
   * The headers it sends with every response it writes itself, as the site's host
   * sends them — a `Cross-Origin-Embedder-Policy` the site's pages need, say. Each name
   * and value is one Node would send; two names equal but for case, and a name that is
   * the server's ({@link SERVER_HEADERS}), are refused. None when omitted.
   */
  headers?: Record<string, string>
}

/**
 * The headers that are the dev server's, lowercased: those it sends — the redirect's
 * `location`, the refused method's `allow`, the 426's `upgrade` — and those Node adds
 * to frame a response, and two that would describe its body other than as it is sent:
 * `content-encoding`, since the body goes out as the build wrote it, and `trailer`,
 * since a body sent with its length has no trailers (Node refuses to write one). A
 * site's `dev.headers` may name none of them, so that the server never decides which
 * side wins. The WebSocket handshake is the `ws` package's and carries none of the
 * site's headers, so its own are not here.
 */
export const SERVER_HEADERS = [
  'allow',
  'cache-control',
  'connection',
  'content-encoding',
  'content-length',
  'content-type',
  'date',
  'keep-alive',
  'location',
  'trailer',
  'transfer-encoding',
  'upgrade',
] as const

/** A header that is the dev server's. */
export type ServerHeader = (typeof SERVER_HEADERS)[number]

/** A config as loaded: where it came from, and its directories made absolute. */
export interface LoadedConfig {
  /** The config file, absolute. */
  file: string
  /** {@link Config.input}, absolute. */
  input: string
  /** {@link Config.output}, absolute. */
  output: string
  /** {@link Config.origin} as the URL parser writes an origin, `https://example.com`. */
  origin?: string
  /** {@link Config.served}, each path as the URL parser writes one, `/caf%C3%A9`. */
  served: string[]
  passthrough: string[]
  plugins: Plugin[]
  /** {@link Config.dev}, its `headers` none where it names none. */
  dev: { headers: Record<string, string> }
}

/**
 * A config that could not be loaded, naming the file and, where one is to blame, the
 * field.
 */
export class ConfigError extends Error {
  readonly file: string
  readonly field?: string

  constructor(
    file: string,
    message: string,
    options: { field?: string; cause?: unknown } = {}
  ) {
    super(`${file}: ${message}`, { cause: options.cause })
    this.name = 'ConfigError'
    this.file = file
    this.field = options.field
  }
}

const FIELDS: readonly string[] = [
  'input',
  'output',
  'origin',
  'served',
  'passthrough',
  'plugins',
  'dev',
] satisfies (keyof Config)[]

const DEV_FIELDS: readonly string[] = ['headers'] satisfies (keyof DevConfig)[]

/**
 * Loads the config at `path`, relative to the working directory: imports the module —
 * Node runs it as TypeScript — and validates its default export. Every way it can be
 * wrong is a {@link ConfigError}.
 */
export async function loadConfig(path = 'lemmon.config.ts'): Promise<LoadedConfig> {
  const file = resolve(path)
  if (!existsSync(file)) throw new ConfigError(file, 'no config file here')
  let module: { default?: unknown }
  try {
    module = (await import(pathToFileURL(file).href)) as { default?: unknown }
  } catch (cause) {
    throw new ConfigError(file, `could not be loaded: ${described(cause)}`, { cause })
  }
  if (module.default === undefined) throw new ConfigError(file, 'has no default export')
  return validated(file, module.default)
}

/**
 * What went wrong loading the module. Node places a syntax error's `file:line` and code
 * frame in its stack, ahead of the frames, and not in its message.
 */
function described(cause: unknown): string {
  if (!(cause instanceof Error) || cause.stack === undefined) return String(cause)
  const frames = cause.stack.indexOf('\n    at ')
  return frames === -1 ? cause.stack : cause.stack.slice(0, frames)
}

function validated(file: string, exported: unknown): LoadedConfig {
  if (typeof exported !== 'object' || exported === null || Array.isArray(exported)) {
    throw new ConfigError(file, 'the default export must be an object')
  }
  const fields = exported as Record<string, unknown>
  for (const field of Object.keys(fields)) {
    if (!FIELDS.includes(field)) {
      const message = `unknown field \`${field}\`; a config has ${listed(FIELDS)}`
      throw new ConfigError(file, message, { field })
    }
  }
  const root = dirname(file)
  const input = resolve(root, directory(file, fields, 'input'))
  const output = resolve(root, directory(file, fields, 'output'))
  if (!statSync(input, { throwIfNoEntry: false })?.isDirectory()) {
    throw new ConfigError(file, `\`input\` names no directory: ${input}`, {
      field: 'input',
    })
  }
  if (contains(output, input)) {
    const message = '`output` may not be the input directory or hold it'
    throw new ConfigError(file, message, { field: 'output' })
  }
  return {
    file,
    input,
    output,
    ...origin(file, fields.origin),
    served: served(file, fields.served),
    passthrough: globs(file, fields.passthrough),
    plugins: plugins(file, fields.plugins),
    dev: dev(file, fields.dev),
  }
}

/** Field names as a message lists them. */
const listed = (fields: readonly string[]) => fields.map((name) => `\`${name}\``).join(', ')

function directory(file: string, fields: Record<string, unknown>, field: string): string {
  const value = fields[field]
  if (value === undefined) {
    throw new ConfigError(file, `\`${field}\` is required`, { field })
  }
  if (typeof value !== 'string' || value === '') {
    throw new ConfigError(file, `\`${field}\` must be a non-empty string`, { field })
  }
  return value
}

/**
 * `{origin}`, the origin `value` names as the URL parser writes it, or nothing where it
 * is omitted: refused, as written, unless it is `http://` or `https://`, a host, and an
 * optional port and `/` — each other thing it holds refused for what it is, before the
 * URL parser, which would read past whitespace, a backslash or a missing `//`, sees it.
 */
function origin(file: string, value: unknown): { origin?: string } {
  if (value === undefined) return {}
  const refused = (problem: string) =>
    new ConfigError(
      file,
      `\`origin\` ${problem}: it is where the site is served, an http or https URL with ` +
        'nothing after its host and port, as `https://example.com`',
      { field: 'origin' }
    )
  if (typeof value !== 'string') throw refused('must be a string')
  const is = `is ${JSON.stringify(value)}, which`
  if (/\s/.test(value)) throw refused(`${is} holds whitespace`)
  const scheme = /^https?:\/\//i.exec(value)
  if (scheme === null) throw refused(`${is} does not start \`http://\` or \`https://\``)
  const rest = value.slice(scheme[0].length)
  const host = /^[^/?#]*/.exec(rest)![0]
  if (host.includes('@')) throw refused(`${is} holds credentials, which no origin has`)
  if (host.includes('\\')) throw refused(`${is} holds a backslash`)
  const after = rest.slice(host.length).replace(/^\//, '')
  if (after.startsWith('?')) throw refused(`${is} has a query, which no origin has`)
  if (after.startsWith('#')) throw refused(`${is} has a fragment, which no origin has`)
  if (after !== '') {
    throw refused(
      `${is} has a path, and Lemmon has no path prefix: a page's url is a path from ` +
        '`/` on the origin'
    )
  }
  if (!URL.canParse(value)) throw refused(`${is} the URL parser refuses`)
  return { origin: new URL(value).origin }
}

/**
 * The paths `value` lists, each as the URL parser writes a path, or none where it is
 * omitted: each refused, as written, unless it is a path from `/` that names neither a
 * host nor a query or fragment, and holds no whitespace, backslash, or `.` or `..`
 * segment, which the URL parser would read past or resolve away; and two refused that
 * the parser writes as one.
 */
function served(file: string, value: unknown): string[] {
  if (value === undefined) return []
  const refused = (problem: string) =>
    new ConfigError(
      file,
      `${problem}: \`served\` lists paths on the origin its host answers without a file ` +
        'the build writes, each a path from `/` as a link writes it, as `/@eric`',
      { field: 'served' }
    )
  if (!Array.isArray(value)) throw refused('`served` must be a list of paths')
  const paths = new Map<string, number>()
  value.forEach((path: unknown, index) => {
    const entry = `\`served[${index}]\``
    if (typeof path !== 'string') throw refused(`${entry} must be a string`)
    const is = `${entry} is ${JSON.stringify(path)}, which`
    if (path === '') throw refused(`${entry} is empty`)
    if (/\s/.test(path)) throw refused(`${is} holds whitespace`)
    if (path.includes('\\')) throw refused(`${is} holds a backslash`)
    if (!path.startsWith('/')) throw refused(`${is} does not start \`/\``)
    if (path.startsWith('//')) throw refused(`${is} starts \`//\`, which names a host`)
    if (path.includes('?')) throw refused(`${is} has a query`)
    if (path.includes('#')) throw refused(`${is} has a fragment`)
    if (path.split('/').some((segment) => /^(?:\.|%2e){1,2}$/i.test(segment))) {
      throw refused(`${is} has a \`.\` or \`..\` segment`)
    }
    const { pathname } = new URL(path, 'http://served.invalid')
    const earlier = paths.get(pathname)
    if (earlier !== undefined) {
      throw refused(`${is} names \`${pathname}\`, as \`served[${earlier}]\` does`)
    }
    paths.set(pathname, index)
  })
  return [...paths.keys()]
}

function globs(file: string, value: unknown): string[] {
  if (value === undefined) return []
  if (!Array.isArray(value)) {
    const message = '`passthrough` must be a list of glob strings'
    throw new ConfigError(file, message, { field: 'passthrough' })
  }
  value.forEach((glob: unknown, index) => {
    if (typeof glob !== 'string' || glob === '') {
      const message = `\`passthrough[${index}]\` must be a non-empty glob string`
      throw new ConfigError(file, message, { field: 'passthrough' })
    }
  })
  return value as string[]
}

function plugins(file: string, value: unknown): Plugin[] {
  try {
    return validatePlugins(value)
  } catch (error) {
    throw new ConfigError(file, (error as Error).message, { field: 'plugins' })
  }
}

function dev(file: string, value: unknown): LoadedConfig['dev'] {
  const refused = (message: string) => new ConfigError(file, message, { field: 'dev' })
  if (value === undefined) return { headers: {} }
  if (!isPlainObject(value)) throw refused('`dev` must be a plain object, `{…}`')
  for (const field of Object.keys(value)) {
    if (!DEV_FIELDS.includes(field)) {
      throw refused(`unknown field \`dev.${field}\`; \`dev\` has ${listed(DEV_FIELDS)}`)
    }
  }
  const { headers } = value
  if (headers === undefined) return { headers: {} }
  if (!isPlainObject(headers)) {
    throw refused(
      '`dev.headers` must be a plain object, `{…}`, of header names and their values'
    )
  }
  // Each name by its lowercased form, which is how a client compares them.
  const names = new Map<string, string>()
  for (const [name, text] of Object.entries(headers)) {
    try {
      validateHeaderName(name)
    } catch {
      throw refused(
        `\`dev.headers\` names \`${name}\`, which is no header name: a name is letters, ` +
          "digits, and any of ``!#$%&'*+-.^_`|~``"
      )
    }
    const folded = name.toLowerCase()
    if (isServerHeader(folded)) {
      throw refused(
        `\`dev.headers\` names \`${name}\`, which is the dev server's to send or leave out`
      )
    }
    const earlier = names.get(folded)
    if (earlier !== undefined) {
      throw refused(
        `\`dev.headers\` names \`${earlier}\` and \`${name}\`, which are one header`
      )
    }
    names.set(folded, name)
    if (typeof text !== 'string') {
      throw refused(`\`dev.headers['${name}']\` must be a string`)
    }
    try {
      validateHeaderValue(name, text)
    } catch {
      throw refused(
        `\`dev.headers['${name}']\` is no header value: a value holds only tabs and ` +
          'the characters from U+0020 to U+00FF but U+007F'
      )
    }
  }
  return { headers: headers as Record<string, string> }
}

/** Whether `name`, lowercased, is one of the dev server's. */
function isServerHeader(name: string): boolean {
  return (SERVER_HEADERS as readonly string[]).includes(name)
}

/**
 * Whether `value` is an object written as one, `{…}`: not a list, and not an instance
 * such as `new Headers()`, whose entries are not its own fields and would be read as
 * none.
 */
export function isPlainObject(value: unknown): value is Record<string, unknown> {
  if (typeof value !== 'object' || value === null) return false
  const prototype: unknown = Object.getPrototypeOf(value)
  return prototype === Object.prototype || prototype === null
}

/** Whether `path` is `dir` or inside it. */
export function contains(dir: string, path: string): boolean {
  const rel = relative(dir, path)
  return !(rel === '..' || rel.startsWith(`..${sep}`) || isAbsolute(rel))
}

/** `path` as a terminal shows it: relative to `cwd` when it is under it. */
export function display(cwd: string, path: string): string {
  return contains(cwd, path) ? relative(cwd, path) : path
}

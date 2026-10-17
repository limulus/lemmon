import { existsSync, statSync } from 'node:fs'
import { dirname, isAbsolute, relative, resolve, sep } from 'node:path'
import { pathToFileURL } from 'node:url'

import { validatePlugins, type Plugin } from './plugins.ts'

/**
 * What a site's `lemmon.config.ts` exports by default:
 *
 * ```ts
 * import type { Config } from '@limulus/lemmon-shell'
 *
 * export default {
 *   input: 'www',
 *   output: 'dist/www',
 *   passthrough: ['assets/**', 'robots.txt'],
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
   * Globs naming the files copied to the output as they are, matched against each
   * file's path relative to {@link input} with Eleventy's glob semantics: picomatch,
   * dotfiles included, case-insensitive. Unlike Eleventy, the walk does not follow a
   * symbolically linked directory, so no glob reaches the files under one. A file a
   * glob matches is passthrough even when it is a document or template. None when
   * omitted.
   */
  passthrough?: string[]
  /**
   * The site's plugins, whose mark handlers fill each page's marks (D42): what they
   * claim is declared to the core, and a page's decorators run in this order. None when
   * omitted, and a site with none is warned at every mark, since nothing will fill one.
   */
  plugins?: Plugin[]
}

/** A config as loaded: where it came from, and its directories made absolute. */
export interface LoadedConfig {
  /** The config file, absolute. */
  file: string
  /** {@link Config.input}, absolute. */
  input: string
  /** {@link Config.output}, absolute. */
  output: string
  passthrough: string[]
  plugins: Plugin[]
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
  'passthrough',
  'plugins',
] satisfies (keyof Config)[]

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
      const expected = FIELDS.map((name) => `\`${name}\``).join(', ')
      const message = `unknown field \`${field}\`; a config has ${expected}`
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
    passthrough: globs(file, fields.passthrough),
    plugins: plugins(file, fields.plugins),
  }
}

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

/** Whether `path` is `dir` or inside it. */
export function contains(dir: string, path: string): boolean {
  const rel = relative(dir, path)
  return !(rel === '..' || rel.startsWith(`..${sep}`) || isAbsolute(rel))
}

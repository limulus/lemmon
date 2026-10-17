import type { Diagnostic, Span } from '@limulus/lemmon-core'
import { relative, resolve } from 'node:path'
import { parseArgs } from 'node:util'

import { build, check, type BuildOptions, type CheckResult } from './build.ts'
import { ConfigError, contains, loadConfig, type LoadedConfig } from './config.ts'
import type { Environment } from './load.ts'
import { SiteError } from './program.ts'
import { SourceText } from './source-text.ts'

/**
 * Where the CLI writes, what it resolves paths against, and what a build reads from its
 * surroundings — `process`, from the bin.
 */
export interface Io {
  stdout: { write(text: string): unknown }
  stderr: { write(text: string): unknown }
  cwd(): string
  /** The environment variables a site's `env("…")` bindings read. */
  env: Environment
  /** The clock `build.time` is read from; the system's when omitted. */
  now?(): Date
}

export const USAGE = `usage: lem <command> [options]

commands:
  build             compile the site, render every page into the output directory, and
                    copy the passthrough files
  check             compile the site and render every page as build does, reporting
                    what it finds and writing nothing

options:
  --config <path>   the config file (default: lemmon.config.ts)
  -h, --help        print this help
`

/**
 * Runs `lem` with `args` — the arguments after the executable and script — and returns
 * the exit code: 0 on success, 1 when the command reported an error, 2 for a command
 * line it does not understand.
 */
export async function run(args: readonly string[], io: Io): Promise<number> {
  let parsed: ReturnType<typeof parse>
  try {
    parsed = parse(args)
  } catch (error) {
    return usage(io, (error as Error).message)
  }
  const { values, positionals } = parsed
  if (values.help === true) {
    io.stdout.write(USAGE)
    return 0
  }
  const [command, ...extra] = positionals
  if (command === undefined) return usage(io, 'no command given')
  if (command !== 'build' && command !== 'check') {
    return usage(io, `unknown command '${command}'`)
  }
  if (extra.length > 0) return usage(io, `unexpected argument '${extra[0]}'`)
  const configPath = resolve(io.cwd(), values.config ?? 'lemmon.config.ts')
  return await (command === 'build' ? buildCommand : checkCommand)(configPath, io)
}

function parse(args: readonly string[]) {
  return parseArgs({
    args: [...args],
    options: {
      config: { type: 'string' },
      help: { type: 'boolean', short: 'h' },
    },
    allowPositionals: true,
  })
}

function usage(io: Io, problem: string): number {
  io.stderr.write(`lem: ${problem}\n\n${USAGE}`)
  return 2
}

/** `lem build`: the site built into its output, and what it wrote. */
async function buildCommand(configPath: string, io: Io): Promise<number> {
  const ran = await reported(configPath, io, build)
  if (ran === undefined) return 1
  const { errors, result } = ran
  if (errors > 0) {
    io.stderr.write(`lem: ${plural(errors, 'error')}, so nothing was written\n`)
    return 1
  }
  if (result.failure !== undefined) {
    const { file, cause } = result.failure
    io.stderr.write(
      `lem: could not write ${display(io.cwd(), file)}: ${(cause as Error).message}\n` +
        'lem: every page rendered, and the output may hold what was written before it\n'
    )
    return 1
  }
  const { pages, produced, copied } = result
  io.stdout.write(
    `wrote ${files(pages, produced)}, copied ${plural(copied.length, 'file')}\n`
  )
  return 0
}

/** `lem check`: the site checked as a build would be, and nothing written. */
async function checkCommand(configPath: string, io: Io): Promise<number> {
  const ran = await reported(configPath, io, check)
  if (ran === undefined) return 1
  const { errors, result } = ran
  if (errors > 0) {
    io.stderr.write(`lem: ${plural(errors, 'error')}\n`)
    return 1
  }
  io.stdout.write(`checked ${files(result.pages, result.produced)}\n`)
  return 0
}

/**
 * Loads the config at `configPath`, runs `step` over the site it describes, and prints
 * every diagnostic the step reported: its result and how many of those were errors, or
 * `undefined` where the config or the site could not be read, which is printed too.
 */
async function reported<R extends CheckResult>(
  configPath: string,
  io: Io,
  step: (config: LoadedConfig, options: BuildOptions) => Promise<R>
): Promise<{ result: R; errors: number } | undefined> {
  let config: LoadedConfig
  let result: R
  try {
    config = await loadConfig(configPath)
    result = await step(config, { now: io.now?.() ?? new Date(), env: io.env })
  } catch (error) {
    if (!(error instanceof ConfigError || error instanceof SiteError)) throw error
    io.stderr.write(`lem: ${error.message}\n`)
    return undefined
  }
  for (const diagnostic of result.diagnostics) {
    io.stderr.write(`${formatted(diagnostic, result.texts, config.input, io.cwd())}\n`)
  }
  const errors = result.diagnostics.filter((d) => d.severity === 'error').length
  return { result, errors }
}

/**
 * The pages and produced files a build writes, counted: a build whose plugins wrote no
 * file says nothing of them.
 */
function files(pages: string[], produced: string[]): string {
  const and =
    produced.length === 0 ? '' : ` and ${plural(produced.length, 'produced file')}`
  return `${plural(pages.length, 'page')}${and}`
}

const plural = (n: number, noun: string) => `${n} ${noun}${n === 1 ? '' : 's'}`

/**
 * A diagnostic as a terminal shows it: `path:line:col: severity: message`, counting
 * lines and columns from 1 as an editor shows them, then the source its span covers,
 * framed from `texts` — the text the build read, which the span was computed against —
 * and each label on an indented line of its own. A path is the core's — relative to the
 * input directory `input` — or absolute, as the shell's own `ambient.lem` is. A
 * diagnostic in a file `texts` does not hold prints without a frame: none a build
 * reports today (`CheckResult.texts`).
 */
export function formatted(
  diagnostic: Diagnostic,
  texts: ReadonlyMap<string, string>,
  input: string,
  cwd: string
): string {
  const place = (path: string, span: Span) => {
    const { line, character } = span.range.start
    return `${display(cwd, resolve(input, path))}:${line + 1}:${character + 1}`
  }
  const { path, span, severity, message, labels } = diagnostic
  const text = texts.get(path)
  return [
    `${place(path, span)}: ${severity}: ${message}`,
    ...(text === undefined ? [] : new SourceText(text).frame(span.range)),
    ...labels.map((label) => `  ${place(label.path, label.span)}: ${label.message}`),
  ].join('\n')
}

/** `path` as a terminal shows it: relative to `cwd` when it is under it. */
function display(cwd: string, path: string): string {
  return contains(cwd, path) ? relative(cwd, path) : path
}

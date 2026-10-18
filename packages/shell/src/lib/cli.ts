import type { Diagnostic, Span } from '@limulus/lemmon-core'
import { strict as assert } from 'node:assert'
import { resolve } from 'node:path'
import { parseArgs } from 'node:util'

import { build, check, CoreTrapped, type BuildOptions, type CheckResult } from './build.ts'
import { ConfigError, display, loadConfig, type LoadedConfig } from './config.ts'
import { dev } from './dev.ts'
import type { Environment } from './load.ts'
import { SiteError } from './program.ts'
import { SourceText } from './source-text.ts'

/**
 * Where the CLI writes, what it resolves paths against, and what a build reads from its
 * surroundings — `process`'s, from the bin.
 */
export interface Io {
  stdout: { write(text: string): unknown }
  stderr: { write(text: string): unknown }
  cwd(): string
  /** The environment variables a site's `env("…")` bindings read. */
  env: Environment
  /** The clock `build.time` is read from; the system's when omitted. */
  now?(): Date
  /**
   * What stops `lem dev`, armed as it is called — the bin's Ctrl-C — and called by that
   * command alone, so that until then an interrupt stops `lem` as it would any program.
   */
  interrupted(): AbortSignal
}

export const USAGE = `usage: lem <command> [options]

commands:
  build             compile the site, render every page into the output directory, and
                    copy the passthrough files
  check             compile the site and render every page as build does, reporting
                    what it finds and writing nothing
  dev               build the site, serve the output, and build it again on every
                    change, reloading the pages open on it

options:
  --config <path>   the config file (default: lemmon.config.ts)
  --port <n>        the port dev serves at, on 127.0.0.1 (default: 8080)
  -h, --help        print this help
`

/** The port `lem dev` serves at by default: Eleventy's, which limulus.net's dev used. */
const PORT = 8080

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
  if (command !== 'build' && command !== 'check' && command !== 'dev') {
    return usage(io, `unknown command '${command}'`)
  }
  if (extra.length > 0) return usage(io, `unexpected argument '${extra[0]}'`)
  const configPath = resolve(io.cwd(), values.config ?? 'lemmon.config.ts')
  if (command !== 'dev') {
    if (values.port !== undefined) return usage(io, '--port is an option of lem dev')
    const step = command === 'build' ? buildCommand : checkCommand
    return await untrapped(io, step(configPath, io))
  }
  const port = values.port ?? String(PORT)
  if (!/^\d{1,5}$/.test(port) || Number(port) > 65535) {
    return usage(io, `--port takes a port from 0 to 65535, not '${port}'`)
  }
  return await devCommand(configPath, Number(port), io)
}

function parse(args: readonly string[]) {
  return parseArgs({
    args: [...args],
    options: {
      config: { type: 'string' },
      port: { type: 'string' },
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
  const config = await configured(configPath, io)
  if (config === undefined) return 1
  return (await built(config, io)) ? 0 : 1
}

/**
 * Builds the site `config` describes, printing what `lem build` prints — what the build
 * reported, then what it wrote or why it wrote nothing, or not all: whether it built
 * with no error and every file written.
 */
async function built(config: LoadedConfig, io: Io): Promise<boolean> {
  const ran = await reported(config, io, build)
  if (ran === undefined) return false
  const { errors, result } = ran
  if (errors > 0) {
    io.stderr.write(`lem: ${plural(errors, 'error')}, so nothing was written\n`)
    return false
  }
  if (result.failure !== undefined) {
    const { file, cause } = result.failure
    io.stderr.write(
      `lem: could not write ${display(io.cwd(), file)}: ${(cause as Error).message}\n` +
        'lem: every page rendered, and the output may hold what was written before it\n'
    )
    return false
  }
  const { pages, produced, copied } = result
  io.stdout.write(
    `wrote ${files(pages, produced)}, copied ${plural(copied.length, 'file')}\n`
  )
  return true
}

/** `lem check`: the site checked as a build would be, and nothing written. */
async function checkCommand(configPath: string, io: Io): Promise<number> {
  const config = await configured(configPath, io)
  if (config === undefined) return 1
  const ran = await reported(config, io, check)
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
 * What `command` exits with, or 1 where the core trapped, which is said as `lem dev`
 * says it: in the words of the `CoreTrapped`.
 */
async function untrapped(io: Io, command: Promise<number>): Promise<number> {
  try {
    return await command
  } catch (error) {
    if (!(error instanceof CoreTrapped)) throw error
    io.stderr.write(`lem: ${error.message}\n`)
    return 1
  }
}

/** `lem dev`: the site built, served, and built again on every change. */
async function devCommand(configPath: string, port: number, io: Io): Promise<number> {
  const config = await configured(configPath, io)
  if (config === undefined) return 1
  return await dev({ config, port, io, build: () => built(config, io) })
}

/** The config at `configPath`, or `undefined` where it cannot be loaded, said why. */
async function configured(configPath: string, io: Io): Promise<LoadedConfig | undefined> {
  try {
    return await loadConfig(configPath)
  } catch (error) {
    // Every way a config can be wrong is a `ConfigError`; anything else is rethrown.
    assert(error instanceof ConfigError, error as Error)
    io.stderr.write(`lem: ${error.message}\n`)
    return undefined
  }
}

/**
 * Runs `step` over the site `config` describes and prints every diagnostic it
 * reported: its result and how many of those were errors, or `undefined` where the
 * site has no bindings file, which is printed too.
 */
async function reported<R extends CheckResult>(
  config: LoadedConfig,
  io: Io,
  step: (config: LoadedConfig, options: BuildOptions) => Promise<R>
): Promise<{ result: R; errors: number } | undefined> {
  let result: R
  try {
    result = await step(config, { now: io.now?.() ?? new Date(), env: io.env })
  } catch (error) {
    if (!(error instanceof SiteError)) throw error
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

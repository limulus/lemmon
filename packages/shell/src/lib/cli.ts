import type { Diagnostic, Span } from '@limulus/lemmon-core'
import { relative, resolve } from 'node:path'
import { parseArgs } from 'node:util'

import { build, type BuildResult } from './build.ts'
import { ConfigError, contains, loadConfig, type LoadedConfig } from './config.ts'
import type { Environment } from './load.ts'
import { SiteError } from './program.ts'

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
  if (command !== 'build') return usage(io, `unknown command '${command}'`)
  if (extra.length > 0) return usage(io, `unexpected argument '${extra[0]}'`)
  return await buildCommand(resolve(io.cwd(), values.config ?? 'lemmon.config.ts'), io)
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

async function buildCommand(configPath: string, io: Io): Promise<number> {
  let config: LoadedConfig
  let result: BuildResult
  try {
    config = await loadConfig(configPath)
    result = await build(config, { now: io.now?.() ?? new Date(), env: io.env })
  } catch (error) {
    if (!(error instanceof ConfigError || error instanceof SiteError)) throw error
    io.stderr.write(`lem: ${error.message}\n`)
    return 1
  }
  for (const diagnostic of result.diagnostics) {
    io.stderr.write(`${located(diagnostic, config, io.cwd())}\n`)
  }
  const errors = result.diagnostics.filter((d) => d.severity === 'error').length
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
  // A build whose plugins wrote no file says nothing of them.
  const and =
    produced.length === 0 ? '' : ` and ${plural(produced.length, 'produced file')}`
  io.stdout.write(
    `wrote ${plural(pages.length, 'page')}${and}, copied ${plural(copied.length, 'file')}\n`
  )
  return 0
}

const plural = (n: number, noun: string) => `${n} ${noun}${n === 1 ? '' : 's'}`

/**
 * A diagnostic as `path:line:col: severity: message`, counting lines and columns from 1
 * as an editor shows them, with each label on an indented line of its own. A path is
 * the core's — relative to the input directory — or absolute, as the shell's own
 * `ambient.lem` is.
 */
function located(diagnostic: Diagnostic, config: LoadedConfig, cwd: string): string {
  const place = (path: string, span: Span) => {
    const { line, character } = span.range.start
    return `${display(cwd, resolve(config.input, path))}:${line + 1}:${character + 1}`
  }
  const { path, span, severity, message, labels } = diagnostic
  return [
    `${place(path, span)}: ${severity}: ${message}`,
    ...labels.map((label) => `  ${place(label.path, label.span)}: ${label.message}`),
  ].join('\n')
}

/** `path` as a terminal shows it: relative to `cwd` when it is under it. */
function display(cwd: string, path: string): string {
  return contains(cwd, path) ? relative(cwd, path) : path
}

import type { Diagnostic, Span } from '@limulus/lemmon-core'
import { join, relative, resolve } from 'node:path'
import { parseArgs } from 'node:util'

import { ConfigError, contains, loadConfig } from './config.ts'
import { discover, type Site } from './site.ts'
import type { EntryKind } from './walk.ts'

/** Where the CLI writes, and what it resolves paths against — `process`, from the bin. */
export interface Io {
  stdout: { write(text: string): unknown }
  stderr: { write(text: string): unknown }
  cwd(): string
}

export const USAGE = `usage: lem <command> [options]

commands:
  build             discover the site: walk the input, read every page's front matter,
                    and report what was found

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
  return await build(resolve(io.cwd(), values.config ?? 'lemmon.config.ts'), io)
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

async function build(configPath: string, io: Io): Promise<number> {
  let site: Site
  try {
    site = await discover(await loadConfig(configPath))
  } catch (error) {
    if (!(error instanceof ConfigError)) throw error
    io.stderr.write(`lem: ${error.message}\n`)
    return 1
  }
  io.stdout.write(summary(site, io.cwd()))
  for (const diagnostic of site.diagnostics) {
    io.stderr.write(`${located(diagnostic, site, io.cwd())}\n`)
  }
  return site.diagnostics.some((d) => d.severity === 'error') ? 1 : 0
}

/** What discovery found: the files by kind, then each page with its url. */
function summary(site: Site, cwd: string): string {
  const counts: Record<EntryKind, number> = {
    document: 0,
    template: 0,
    passthrough: 0,
    ignored: 0,
  }
  let pageTemplates = 0
  for (const entry of site.entries) {
    counts[entry.kind]++
    if (entry.kind === 'template' && entry.page) pageTemplates++
  }
  const plural = (n: number, noun: string) => `${n} ${noun}${n === 1 ? '' : 's'}`
  const lines = [
    `${display(cwd, site.config.input)}: ${plural(counts.document, 'document')}, ` +
      `${plural(counts.template, 'template')} (${pageTemplates} with front matter), ` +
      `${counts.passthrough} passthrough, ${counts.ignored} ignored`,
  ]
  const urls = [...site.pages].map(([path, page]) => {
    const url = page.loaded.value.url
    if (url === undefined) return [path, '(generator)']
    return [path, typeof url === 'string' ? url : JSON.stringify(url)]
  })
  if (urls.length > 0) {
    const width = Math.max(...urls.map(([, url]) => url.length))
    lines.push(
      '',
      'pages:',
      ...urls.map(([path, url]) => `  ${url.padEnd(width)}  ${path}`)
    )
  }
  return `${lines.join('\n')}\n`
}

/**
 * A diagnostic as `path:line:col: severity: message`, counting lines and columns from 1
 * as an editor shows them, with each label on an indented line of its own.
 */
function located(diagnostic: Diagnostic, site: Site, cwd: string): string {
  const place = (path: string, span: Span) => {
    const { line, character } = span.range.start
    return `${display(cwd, join(site.config.input, path))}:${line + 1}:${character + 1}`
  }
  const { path, span, severity, message, labels } = diagnostic
  return [
    `${place(path, span)}: ${severity}: ${message}`,
    ...labels.map((label) => `  ${place(label.path, label.span)}: ${label.message}`),
  ].join('\n')
}

/** `path` as a terminal shows it: relative to `cwd` when it is under it. */
function display(cwd: string, path: string): string {
  return contains(cwd, path) ? relative(cwd, path) || '.' : path
}

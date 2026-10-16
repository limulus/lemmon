import {
  loadCore,
  type Core,
  type Diagnostic,
  type JsonValue,
  type Span,
} from '@limulus/lemmon-core'
import { strict as assert } from 'node:assert'

import type { LoadedConfig } from './config.ts'
import { envelope, type Environment } from './load.ts'
import {
  compileSite,
  FILE_START,
  hasError,
  pageSpan,
  type CompiledPage,
} from './program.ts'
import { discover } from './site.ts'
import {
  OutputFiles,
  outputFile,
  write,
  WriteError,
  type Collision,
  type Output,
} from './write.ts'

/** What a build takes from its surroundings, so that a spec can fix both. */
export interface BuildOptions {
  /** When the build runs: `build.time`. */
  now: Date
  /** The environment variables `env("…")` bindings read. */
  env: Environment
}

/** What a build did: the files it wrote and copied, and everything it reported. */
export interface BuildResult {
  /**
   * Each page's file, relative to the output directory, in the order rendered: every
   * page the core loaded, in its plan's order, then every page no record typed, in
   * path order. Empty when an error or a failed write stopped the build.
   */
  pages: string[]
  /** Each passthrough file copied, by its path under the input and the output both. */
  copied: string[]
  diagnostics: Diagnostic[]
  /**
   * The file the file system refused, where writing failed: every page had been
   * rendered and checked, and the output may hold the files written before it.
   */
  failure?: WriteError
}

/**
 * Builds the site `config` describes into its output directory — every step of the
 * build pipeline this shell has (docs/architecture.md §Build pipeline): discover,
 * compile, load, plan, render each page, write it and copy the passthrough files. Any
 * error diagnostic, at any step, stops the build before it writes a file, since every
 * page is rendered and its output file checked against every other before the first
 * is written; warnings are reported and the build goes on. A write the file system
 * then refuses — a file an earlier build left in the way, which nothing removes — is
 * reported as the result's `failure`, and the output may hold what was written before
 * it.
 *
 * A page whose record the core loaded renders through its layout. A page no glob held
 * and no `type:` named has no schema, so it renders its own output with `page` bound
 * to its front matter as read (D30, D37): `page` is bound as sent, one value per load,
 * so each such page is loaded on its own before it renders. Each of those loads
 * re-sends and re-validates the whole envelope, so their cost grows with the untyped
 * pages times the size of the site: 18 of 25 pages on the limulus-dot-net port branch
 * cost about 0.35 s in all.
 *
 * @throws {SiteError} when the input has no `site.lem`.
 */
export async function build(
  config: LoadedConfig,
  options: BuildOptions
): Promise<BuildResult> {
  const site = await discover(config)
  const diagnostics = [...site.diagnostics]
  const passthrough = site.entries
    .filter((entry) => entry.kind === 'passthrough')
    .map((entry) => entry.path)
  const failed = { pages: [], copied: [], diagnostics }
  if (hasError(diagnostics)) return failed
  const core = await loadCore()
  try {
    const compiled = await compileSite(core, site)
    diagnostics.push(...compiled.diagnostics)
    if (compiled.program === undefined) return failed
    const { program } = compiled
    const generator = `Lemmon ${core.version}`
    const loaded = await envelope(
      config.input,
      program,
      { time: options.now, generator },
      options.env
    )
    diagnostics.push(...loaded.diagnostics)
    if (loaded.data === undefined) return failed
    const { data } = loaded
    diagnostics.push(...core.loadData(data))
    if (hasError(diagnostics)) return failed
    const renderer = new Renderer(core, program.pages, passthrough, diagnostics)
    for (const planned of core.plan()) {
      assert(planned.url !== undefined, 'the shell’s host names the url field')
      renderer.render(planned.path, planned.url)
    }
    for (const [path, compiledPage] of program.pages) {
      if (compiledPage.record !== undefined) continue
      const { value } = compiledPage.page.loaded
      const ambient = { ...data.ambient, page: value }
      diagnostics.push(...core.loadData({ ...data, ambient }))
      renderer.render(path, value.url)
    }
    if (hasError(diagnostics)) return failed
    try {
      await write(config.input, config.output, renderer.outputs, passthrough)
    } catch (error) {
      return { ...failed, failure: error as WriteError }
    }
    return {
      pages: renderer.outputs.map((output) => output.file),
      copied: passthrough,
      diagnostics,
    }
  } finally {
    core.free()
  }
}

/** Renders pages into memory, refusing any two whose files cannot both be written. */
class Renderer {
  readonly outputs: Output[] = []
  readonly #core: Core
  readonly #pages: Map<string, CompiledPage>
  readonly #diagnostics: Diagnostic[]
  readonly #files = new OutputFiles()

  constructor(
    core: Core,
    pages: Map<string, CompiledPage>,
    passthrough: string[],
    diagnostics: Diagnostic[]
  ) {
    this.#core = core
    this.#pages = pages
    this.#diagnostics = diagnostics
    // Each passthrough file is where the walk found it, so no two collide in a way
    // the input's own file system allowed; a page is checked against them all.
    for (const path of passthrough) this.#files.claim(path, { path })
  }

  /**
   * Renders the page at `path` to the file its `url` names, or reports why it cannot
   * be written: a url that names no file, or a file another page or a passthrough
   * file writes, or one that cannot stand beside it on disk. A failed render reports
   * its runtime diagnostic.
   */
  render(path: string, url: JsonValue): void {
    const page = this.#pages.get(path)
    assert(page !== undefined, 'every planned page was compiled by this build')
    const at = page.page.loaded.spans.url ?? pageSpan(page.page)
    const file = typeof url === 'string' ? outputFile(url) : undefined
    if (file === undefined) {
      this.#diagnostics.push({
        path,
        span: at,
        severity: 'error',
        message:
          `the url ${JSON.stringify(url)} names no file in the output directory: a ` +
          "page's url is a path from `/`, without an empty, `.` or `..` segment, a " +
          'percent-encoded `/` or `\\`, a query, or a fragment',
        labels: [],
      })
      return
    }
    const collision = this.#files.claim(file, { path, span: at })
    if (collision !== undefined) {
      this.#diagnostics.push(collided(path, at, file, collision))
      return
    }
    const rendered = this.#core.render({ path })
    this.#diagnostics.push(...rendered.diagnostics)
    // The marks and fragments are set aside: no handler resolves a mark yet (∆F4y), so
    // each keeps the default content it already holds in the page, and nothing the
    // page collected is bundled.
    if (rendered.html !== null) this.outputs.push({ file, html: rendered.html })
  }
}

/**
 * The error at the page at `path`, whose url at `span` names `file`, for the claim it
 * collides with, labelled where that claim was made: at the other page's url, or at the
 * start of the passthrough file.
 */
function collided(
  path: string,
  span: Span,
  file: string,
  collision: Collision
): Diagnostic {
  const { writer } = collision
  const who =
    writer.span === undefined
      ? `the passthrough file \`${writer.path}\``
      : `\`${writer.path}\``
  const other = `\`${collision.file}\``
  const message = {
    same: `\`${file}\` is written by ${who} already`,
    fold:
      `\`${file}\` differs from ${other}, which ${who} writes, only in case or ` +
      'Unicode form, and a file system may hold the two as one file',
    inside: `\`${file}\` would be inside ${other}, which ${who} writes as a file`,
    holds: `\`${file}\` would be a file, and ${who} writes ${other} inside it`,
  }[collision.how]
  return {
    path,
    span,
    severity: 'error',
    message,
    labels: [
      {
        path: writer.path,
        span: writer.span ?? FILE_START,
        message: `the first to write there, as ${other}`,
      },
    ],
  }
}

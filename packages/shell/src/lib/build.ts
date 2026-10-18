import {
  loadCore,
  type Core,
  type Diagnostic,
  type JsonValue,
  type Label,
  type Mark,
  type Span,
} from '@limulus/lemmon-core'
import { strict as assert } from 'node:assert'
import { Buffer } from 'node:buffer'

import type { LoadedConfig } from './config.ts'
import { envelope, type Environment } from './load.ts'
import { resolvePage, type Plugin, type ProducedFile } from './plugins.ts'
import { compileSite, FILE_START, hasError, pageSpan, type Program } from './program.ts'
import { discover } from './site.ts'
import {
  OutputFiles,
  outputFile,
  write,
  WriteError,
  type Collision,
  type Output,
  type Writer,
} from './write.ts'

/** What a build takes from its surroundings, so that a spec can fix both. */
export interface BuildOptions {
  /** When the build runs: `build.time`. */
  now: Date
  /** The environment variables `env("…")` bindings read. */
  env: Environment
}

/**
 * What a check found: the files a build would write and copy, and everything it
 * reported.
 */
export interface CheckResult {
  /**
   * Each page's file, relative to the output directory, in the order rendered: every
   * page the core loaded, in its plan's order, then every page no record typed, in
   * path order. Empty when an error stopped the build.
   */
  pages: string[]
  /**
   * Each file a plugin's producer wrote, relative to the output directory, in the order
   * first written — once, however many pages wrote it. Empty when the build stopped.
   */
  produced: string[]
  /**
   * Each passthrough file copied, or that a build would copy, by its path under the
   * input and the output both.
   */
  copied: string[]
  diagnostics: Diagnostic[]
  /**
   * The text of every file the build read, once each, by the path its diagnostics name
   * it by: each document and template the walk found, the shell's own `ambient.lem`,
   * and each data file a binding read. Every diagnostic's own path is among them, since
   * each is placed in a file the build read; a label may name a passthrough file, which
   * no step reads. These are the texts the spans were computed against, so a span is
   * shown from here rather than from the file read again, which may have changed since.
   */
  texts: Map<string, string>
}

/**
 * A call into the core that never returned, which leaves the `.wasm` instance it ran in
 * unable to run anything again in this process: a trap, or an error raised inside the
 * instance, such as V8's stack overflow (∆ugN). Its `cause` is what the call threw.
 * Known by the core refusing to be freed after it, since the call still holds it.
 */
export class CoreTrapped extends Error {
  constructor(cause: unknown) {
    super(`the core failed with ${String(cause)}, and cannot run again in this process`, {
      cause,
    })
    this.name = 'CoreTrapped'
  }
}

/** What a build did: what a check finds, with the files written and copied. */
export interface BuildResult extends CheckResult {
  /**
   * The file the file system refused, where writing failed: every page had been
   * rendered and checked, and the output may hold the files written before it. The
   * files the result lists are then empty.
   */
  failure?: WriteError
}

/**
 * Builds the site `config` describes into its output directory — every step of the
 * build pipeline this shell has (docs/architecture.md §Build pipeline): discover,
 * compile, load, plan, render each page and resolve its marks with the config's
 * plugins, write it and the files its producers wrote, and copy the passthrough files.
 * Any error diagnostic, at any step, stops the build before it writes a file, since
 * every page is rendered and every output file checked against every other before the
 * first is written; warnings are reported and the build goes on. A write the file
 * system then refuses — a file an earlier build left in the way, which nothing removes —
 * is reported as the result's `failure`, and the output may hold what was written before
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
 * @throws {CoreTrapped} when a call into the core never returned.
 */
export async function build(
  config: LoadedConfig,
  options: BuildOptions
): Promise<BuildResult> {
  const { result, rendered } = await renderSite(config, options)
  if (rendered === undefined) return result
  try {
    const { outputs, produced } = rendered
    await write(config.input, config.output, outputs, produced, result.copied)
  } catch (error) {
    const stopped = { pages: [], produced: [], copied: [] }
    return { ...result, ...stopped, failure: error as WriteError }
  }
  return result
}

/**
 * Checks the site `config` describes as {@link build} builds it, every step but the
 * writing: every page is rendered and its marks resolved, and every output file
 * checked against every other, so a check reports what a build would, the runtime
 * errors and the collisions among them, and the output directory is not touched.
 *
 * @throws {SiteError} when the input has no `site.lem`.
 * @throws {CoreTrapped} when a call into the core never returned.
 */
export async function check(
  config: LoadedConfig,
  options: BuildOptions
): Promise<CheckResult> {
  return (await renderSite(config, options)).result
}

/**
 * Every step of the build before the writing, in one core freed before it returns:
 * what a check found, and the pages and produced files rendered into memory unless an
 * error stopped it.
 */
async function renderSite(
  config: LoadedConfig,
  options: BuildOptions
): Promise<{ result: CheckResult; rendered?: Pick<Renderer, 'outputs' | 'produced'> }> {
  const site = await discover(config)
  const diagnostics = [...site.diagnostics]
  const texts = new Map<string, string>()
  for (const entry of site.entries) {
    if (entry.kind === 'document' || entry.kind === 'template') {
      texts.set(entry.path, entry.text)
    }
  }
  const passthrough = site.entries
    .filter((entry) => entry.kind === 'passthrough')
    .map((entry) => entry.path)
  const failed = { result: { pages: [], produced: [], copied: [], diagnostics, texts } }
  if (hasError(diagnostics)) return failed
  const core = await loadCore()
  let threw = false
  try {
    const compiled = await compileSite(core, site, texts)
    diagnostics.push(...compiled.diagnostics)
    if (compiled.program === undefined) return failed
    const { program } = compiled
    const generator = `Lemmon ${core.version}`
    const loaded = await envelope(
      config.input,
      program,
      { time: options.now, generator },
      options.env,
      texts
    )
    diagnostics.push(...loaded.diagnostics)
    if (loaded.data === undefined) return failed
    const { data } = loaded
    diagnostics.push(...core.loadData(data))
    if (hasError(diagnostics)) return failed
    const renderer = new Renderer(core, program, config.plugins, passthrough, diagnostics)
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
    const result = {
      pages: renderer.outputs.map((output) => output.file),
      produced: renderer.produced.map((output) => output.file),
      copied: passthrough,
      diagnostics,
      texts,
    }
    const { outputs, produced } = renderer
    return { result, rendered: { outputs, produced } }
  } catch (error) {
    threw = true
    throw released(core, error)
  } finally {
    if (!threw) core.free()
  }
}

/**
 * `core` freed after a step threw `error`, and what the build throws for it: `error`
 * itself, or a {@link CoreTrapped} where the core refuses to be freed. It refuses only
 * while a call into it still holds it, one that `error` ended inside the instance — and
 * only after the glue has taken it off its finalization registry, which would otherwise
 * free it in that instance whenever the collector reached it, a throw no caller could
 * catch.
 */
function released(core: Core, error: unknown): unknown {
  try {
    core.free()
    return error
  } catch {
    return new CoreTrapped(error)
  }
}

/**
 * Renders pages into memory, their marks resolved, with the files their producers wrote,
 * refusing any two files that cannot both be written.
 */
class Renderer {
  readonly outputs: Output[] = []
  readonly produced: Output[] = []
  readonly #core: Core
  readonly #program: Program
  readonly #plugins: readonly Plugin[]
  readonly #diagnostics: Diagnostic[]
  readonly #files = new OutputFiles()
  /**
   * Each file a producer wrote, by its path, with who wrote it first and the mark that
   * call was given — one object for every file one producer call writes.
   */
  readonly #written = new Map<string, { bytes: Uint8Array; writer: Writer; mark: Mark }>()

  constructor(
    core: Core,
    program: Program,
    plugins: readonly Plugin[],
    passthrough: string[],
    diagnostics: Diagnostic[]
  ) {
    this.#core = core
    this.#program = program
    this.#plugins = plugins
    this.#diagnostics = diagnostics
    // Each passthrough file is where the walk found it, so no two collide in a way
    // the input's own file system allowed; a page or a produced file is checked
    // against them all, and against each other in whichever order they come.
    for (const path of passthrough) this.#files.claim(path, { path })
  }

  /**
   * Renders the page at `path` to the file its `url` names, its marks resolved, or
   * reports why it cannot be written: a url that names no file, or a file another page
   * or a passthrough file writes, or one that cannot stand beside it on disk. A failed
   * render reports its runtime diagnostic, resolving reports what the handlers did
   * wrong, and each file its producers wrote is checked as the page's own file is.
   */
  render(path: string, url: JsonValue): void {
    const page = this.#program.pages.get(path)
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
          `page's url ${URL_RULE}`,
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
    if (rendered.html === null) return
    // Resolved as soon as it renders, before any output transform could see the page
    // (D42).
    const { payloads } = this.#program
    const resolved = resolvePage(rendered, { path, plugins: this.#plugins, payloads })
    this.#diagnostics.push(...resolved.diagnostics)
    this.outputs.push({ file, bytes: resolved.bytes })
    for (const produced of resolved.files) this.#produce(path, produced)
  }

  /**
   * Takes the file a producer wrote while the page at `page` resolved, or reports why
   * it cannot be written, at the producer's mark: its url names no file, another
   * producer wrote other bytes there, or a page or a passthrough file claims it, or a
   * file that cannot stand beside it. The same bytes another producer wrote at the same
   * file are taken as written already — the stylesheet two pages share — and other
   * bytes from the same call are the producer writing the file twice.
   */
  #produce(page: string, { url, bytes, plugin, mark }: ProducedFile): void {
    const { path, span } = mark
    const lead = `the \`${plugin}\` plugin writes a file for this mark on \`${page}\``
    const file = outputFile(url)
    if (file === undefined) {
      this.#diagnostics.push({
        path,
        span,
        severity: 'error',
        message:
          `${lead} at the url ${JSON.stringify(url)}, which names no file in the output ` +
          `directory: a file's url ${URL_RULE}`,
        labels: [],
      })
      return
    }
    const writer: Writer = { path, span, producer: { plugin, page } }
    const first = this.#written.get(file)
    if (first !== undefined) {
      if (Buffer.compare(first.bytes, bytes) === 0) return
      // One call's files share its first mark, which is where both would point.
      const twice = first.mark === mark
      this.#diagnostics.push({
        path,
        span,
        severity: 'error',
        message: twice
          ? `the \`${plugin}\` plugin writes \`${file}\` twice with different content ` +
            `for this mark on \`${page}\``
          : `${lead}, and ${who(first.writer)} writes other content to \`${file}\``,
        labels: twice ? [] : [label(first.writer, file)],
      })
      return
    }
    const collision = this.#files.claim(file, writer)
    if (collision !== undefined) {
      this.#diagnostics.push(collided(path, span, file, collision, lead))
      return
    }
    this.#written.set(file, { bytes, writer, mark })
    this.produced.push({ file, bytes })
  }
}

/** What a url must be to name a file in the output directory. */
const URL_RULE =
  'is a path from `/`, without an empty, `.` or `..` segment, a percent-encoded `/` or ' +
  '`\\`, a query, or a fragment'

/**
 * How a message names `writer`: a page by its path, a passthrough file as one, and a
 * producer's file by its plugin and the page it was written for.
 */
function who(writer: Writer): string {
  if (writer.producer !== undefined) {
    const { plugin, page } = writer.producer
    return `the \`${plugin}\` plugin, for \`${page}\`,`
  }
  return writer.span === undefined
    ? `the passthrough file \`${writer.path}\``
    : `\`${writer.path}\``
}

/**
 * The label at `writer`, the first to claim `file`: at a page's url, a producer's mark,
 * or the start of a passthrough file.
 */
function label(writer: Writer, file: string): Label {
  return {
    path: writer.path,
    span: writer.span ?? FILE_START,
    message: `the first to write there, as \`${file}\``,
  }
}

/**
 * The error at `span` in `path` — a page's url, or the mark whose producer wrote the
 * file, which `lead` then says — for `file` and the claim it collides with, labelled
 * where that claim was made.
 */
function collided(
  path: string,
  span: Span,
  file: string,
  collision: Collision,
  lead?: string
): Diagnostic {
  const by = who(collision.writer)
  const other = `\`${collision.file}\``
  const message = {
    same: `\`${file}\` is written by ${by} already`,
    fold:
      `\`${file}\` differs from ${other}, which ${by} writes, only in case or ` +
      'Unicode form, and a file system may hold the two as one file',
    inside: `\`${file}\` would be inside ${other}, which ${by} writes as a file`,
    holds: `\`${file}\` would be a file, and ${by} writes ${other} inside it`,
  }[collision.how]
  return {
    path,
    span,
    severity: 'error',
    message: lead === undefined ? message : `${lead}, and ${message}`,
    labels: [label(collision.writer, collision.file)],
  }
}

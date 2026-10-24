import {
  loadCore,
  type Core,
  type Diagnostic,
  type JsonValue,
  type Label,
  type Mark,
  type PageRef,
  type Span,
} from '@limulus/lemmon-core'
import { strict as assert } from 'node:assert'
import { Buffer } from 'node:buffer'

import type { LoadedConfig } from './config.ts'
import { hasError } from './diagnostics.ts'
import { forEntry } from './front-matter.ts'
import { checkLinks } from './link-check.ts'
import { envelope, withBodies, type Environment } from './load.ts'
import {
  pageName,
  resolvePage,
  type Generated,
  type Plugin,
  type ProducedFile,
} from './plugins.ts'
import { compileSite, FILE_START, pageSpan, type Program } from './program.ts'
import { provided, type AssetFile } from './providers.ts'
import { rebaseLinks } from './rebase.ts'
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
   * Each file a plugin's asset hook built, relative to the output directory, in the
   * order the plugins and their files are listed. Empty when the build stopped.
   */
  assets: string[]
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
   * it by: each document — as the plugins' transforms left it, which is what compiled —
   * template and misnamed file the walk found, the shell's own `ambient.lem`, and each
   * data file a binding read. Every diagnostic's own path is among them, since each is
   * placed in a file the build read, but for the config file's: a plugin's asset hook
   * that failed, or a file it built that cannot be written, is placed at the start of the
   * config, which lists the plugin and is never read as text. A label may name the config
   * too, or a passthrough file, which no step reads. These are the texts the spans were
   * computed against, so a span is shown from here rather than from the file read
   * again, which may have changed since.
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
 * build pipeline this shell has (docs/architecture.md §Build pipeline): discover and
 * transform each document with the config's plugins, build their assets, compile, load,
 * plan, render each document's body and load again with the bodies in `build.bodies`
 * (D46), render each page and resolve its marks with the plugins, check every link in
 * what it would write (D47), write the pages, the files their asset hooks and producers
 * built, and copy the passthrough files.
 * Any error diagnostic, at any step, stops the build before it writes a file, since
 * every page is rendered, every output file checked against every other, and every link
 * checked before the first is written; warnings are reported and the build goes on. A
 * write the file system then refuses — a file an earlier build left in the way, which
 * nothing removes — is reported as the result's `failure`, and the output may hold what
 * was written before it.
 *
 * A page whose record the core loaded renders through its layout. A page no glob held
 * and no `type:` named has no schema, so it renders its own output with `page` bound
 * to its front matter as read (D30, D37): `page` is bound as sent, one value per load,
 * so each such page is loaded on its own before it renders. Each of those loads
 * re-sends and re-validates the whole envelope, the bodies included, so their cost grows
 * with the untyped pages times the size of the site: 18 of 25 pages on the
 * limulus-dot-net port branch cost about 0.35 s in all while they were untyped.
 *
 * @throws {SiteError} when the input has no `site.lem`.
 * @throws {CoreTrapped} when a call into the core never returned.
 */
export async function build(
  config: LoadedConfig,
  options: BuildOptions
): Promise<BuildResult> {
  const { result, rendered } = await renderSite(config, options, true)
  if (rendered === undefined) return result
  try {
    const { outputs, assets, produced } = rendered
    const built = [...assets, ...produced]
    await write(config.input, config.output, outputs, built, result.copied)
  } catch (error) {
    const stopped = { pages: [], assets: [], produced: [], copied: [] }
    return { ...result, ...stopped, failure: error as WriteError }
  }
  return result
}

/**
 * Checks the site `config` describes as {@link build} builds it, every step but the
 * writing: every document is transformed, the plugins' assets are built, every page is
 * rendered and its marks resolved, every output file checked against every other, and
 * every link checked, so a check reports what a build would, the runtime errors, the
 * collisions and the broken links among them, and the output directory is not touched.
 * The asset hooks are told the build writes nothing.
 *
 * @throws {SiteError} when the input has no `site.lem`.
 * @throws {CoreTrapped} when a call into the core never returned.
 */
export async function check(
  config: LoadedConfig,
  options: BuildOptions
): Promise<CheckResult> {
  return (await renderSite(config, options, false)).result
}

/** What a build that no error stopped holds in memory to write. */
interface Rendered {
  outputs: Output[]
  assets: Output[]
  produced: Output[]
}

/**
 * Every step of the build before the writing, in one core freed before it returns:
 * what a check found, and the pages and the files the plugins built, in memory, unless
 * an error stopped it — for a build that `writes` its output, or for a check.
 */
async function renderSite(
  config: LoadedConfig,
  options: BuildOptions,
  writes: boolean
): Promise<{ result: CheckResult; rendered?: Rendered }> {
  const site = await discover(config)
  const diagnostics = [...site.diagnostics]
  const texts = new Map<string, string>()
  for (const entry of site.entries) {
    if ('text' in entry) texts.set(entry.path, entry.text)
  }
  const passthrough = site.entries
    .filter((entry) => entry.kind === 'passthrough')
    .map((entry) => entry.path)
  const documents = new Set(
    site.entries.filter((entry) => entry.kind === 'document').map((entry) => entry.path)
  )
  const failed = {
    result: { pages: [], assets: [], produced: [], copied: [], diagnostics, texts },
  }
  if (hasError(diagnostics)) return failed
  const built = await provided(config, { writes, env: options.env })
  diagnostics.push(...built.diagnostics)
  // Each passthrough file is where the walk found it, so no two collide in a way the
  // input's own file system allowed; an asset file is checked against them all and
  // against each other, and a page or a produced file against all of those.
  const files = new OutputFiles()
  for (const path of passthrough) files.claim(path, { path })
  const assets = claimed(files, built.files, config.file, diagnostics)
  if (hasError(diagnostics)) return failed
  const core = await loadCore()
  let threw = false
  try {
    const compiled = await compileSite(core, site, texts, built.manifest)
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
    diagnostics.push(...core.loadData(loaded.data))
    if (hasError(diagnostics)) return failed
    const renderer = new Renderer(core, program, config, files, diagnostics)
    // The body pass (D46): each document the load made renders its body alone, under a
    // `build.bodies` that is still empty, and the second load binds them all, so every
    // page renders after it. An error in it stops the build here, so that a page reading
    // a body never reports the body's error again as a missing key.
    const bodies: Record<string, string> = {}
    for (const { path, url } of planned(core)) {
      if (!documents.has(path)) continue
      const body = renderer.body(path, url)
      if (body !== undefined) bodies[url] = body
    }
    if (hasError(diagnostics)) return failed
    const data = withBodies(loaded.data, bodies)
    diagnostics.push(...core.loadData(data))
    if (hasError(diagnostics)) return failed
    for (const page of planned(core)) renderer.render(page)
    for (const [path, compiledPage] of program.pages) {
      if (compiledPage.record !== undefined) continue
      const { value } = compiledPage.page.loaded
      const ambient = { ...data.ambient, page: value }
      diagnostics.push(...core.loadData({ ...data, ambient }))
      renderer.render({ path, url: value.url })
    }
    if (hasError(diagnostics)) return failed
    const { outputs, produced } = renderer
    // Last, over every byte the build would write: the links in it (D47).
    diagnostics.push(
      ...checkLinks({
        outputs: [...outputs, ...assets, ...produced],
        files,
        manifest: built.manifest,
        mappedBy: built.mappedBy,
        origin: config.origin,
        served: config.served,
        config: config.file,
      })
    )
    if (hasError(diagnostics)) return failed
    const result = {
      pages: renderer.outputs.map((output) => output.file),
      assets: assets.map((output) => output.file),
      produced: renderer.produced.map((output) => output.file),
      copied: passthrough,
      diagnostics,
      texts,
    }
    return { result, rendered: { outputs, assets, produced } }
  } catch (error) {
    threw = true
    throw released(core, error)
  } finally {
    if (!threw) core.free()
  }
}

/** Every page the last load made, in its plan's order, each with its url. */
function planned(core: Core): (PageRef & { url: string })[] {
  return core.plan().map((page) => {
    assert(page.url !== undefined, 'the shell’s host names the url field')
    return { ...page, url: page.url }
  })
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
 * The files the plugins' asset hooks built, each claimed in `files` for the file its url
 * names, or reported in `diagnostics` at the start of `config`, the config file: a url
 * that names no file, or a file a passthrough file or another asset file writes, or one
 * that cannot stand beside it on disk.
 */
function claimed(
  files: OutputFiles,
  assets: AssetFile[],
  config: string,
  diagnostics: Diagnostic[]
): Output[] {
  const outputs: Output[] = []
  for (const { url, bytes, plugin } of assets) {
    const lead = `the \`${plugin}\` plugin's \`assets\` hook returns a file`
    const file = outputFile(url)
    if (file === undefined) {
      diagnostics.push({
        path: config,
        span: FILE_START,
        severity: 'error',
        message:
          `${lead} at the url ${JSON.stringify(url)}, which names no file in the output ` +
          `directory: a file's url ${URL_RULE}`,
        labels: [],
      })
      continue
    }
    const collision = files.claim(file, { path: config, assets: { plugin } })
    if (collision !== undefined) {
      diagnostics.push(
        collided(config, FILE_START, file, collision, `${lead} at \`${url}\``)
      )
      continue
    }
    outputs.push({ file, url, bytes })
  }
  return outputs
}

/**
 * Renders pages into memory, their marks resolved, with the files their producers wrote,
 * refusing any two files that cannot both be written; and the bodies of documents, for
 * the body pass (D46).
 */
class Renderer {
  readonly outputs: Output[] = []
  readonly produced: Output[] = []
  readonly #core: Core
  readonly #program: Program
  readonly #plugins: readonly Plugin[]
  /** The site's origin, which a body's links are made absolute against, if it has one. */
  readonly #origin?: string
  readonly #diagnostics: Diagnostic[]
  /** Every output file claimed so far: the passthrough and asset files to start with. */
  readonly #files: OutputFiles
  /**
   * Each file a producer wrote, by its path, with who wrote it first and the mark that
   * call was given — one object for every file one producer call writes.
   */
  readonly #written = new Map<string, { bytes: Uint8Array; writer: Writer; mark: Mark }>()

  constructor(
    core: Core,
    program: Program,
    { plugins, origin }: Pick<LoadedConfig, 'plugins' | 'origin'>,
    files: OutputFiles,
    diagnostics: Diagnostic[]
  ) {
    this.#core = core
    this.#program = program
    this.#plugins = plugins
    this.#origin = origin
    this.#files = files
    this.#diagnostics = diagnostics
  }

  /**
   * Renders the page at `path` — its page `index`, for a generator's — to the file its
   * `url` names, its marks resolved, or reports why it cannot be written: a url that
   * names no file, or a file another page, a passthrough file or an asset hook writes,
   * or one that cannot stand beside it on disk. A failed render reports its runtime
   * diagnostic, resolving reports what the handlers did wrong, and each file its
   * producers wrote is checked as the page's own file is. A generated page is placed at
   * its generator's `for:` entry, which gives every page of it its url, and a runtime
   * diagnostic of its render is labelled there with the item it was made from, so that
   * a fault every page shares is told apart page by page (D43).
   */
  render({ path, index, url }: PageRef & { url: JsonValue }): void {
    const page = this.#program.pages.get(path)
    assert(page !== undefined, 'every planned page was compiled by this build')
    let generated: Generated | undefined
    let at = page.page.loaded.spans.url ?? pageSpan(page.page)
    if (index !== undefined) {
      assert(page.page.for !== undefined, 'a generated page has its generator’s key')
      const { list, span } = forEntry(page.page.for)
      generated = { index, list }
      at = span
    }
    const file = typeof url === 'string' ? outputFile(url) : undefined
    if (file === undefined) {
      const of = generated === undefined ? '' : ` of ${pageName(path, generated)}`
      this.#diagnostics.push({
        path,
        span: at,
        severity: 'error',
        message:
          `the url ${JSON.stringify(url)}${of} names no file in the output directory: ` +
          `a page's url ${URL_RULE}`,
        labels: [],
      })
      return
    }
    const collision = this.#files.claim(file, { path, span: at, generated })
    if (collision !== undefined) {
      // Every page of a generator stands at one place, so the message says which.
      const lead =
        generated === undefined
          ? undefined
          : `${pageName(path, generated)} is at the url ${JSON.stringify(url)}`
      this.#diagnostics.push(collided(path, at, file, collision, lead))
      return
    }
    const rendered = this.#core.render({ path, index })
    for (const diagnostic of rendered.diagnostics) {
      if (generated === undefined) {
        this.#diagnostics.push(diagnostic)
        continue
      }
      const label = { path, span: at, message: `rendering ${pageName(path, generated)}` }
      this.#diagnostics.push({ ...diagnostic, labels: [...diagnostic.labels, label] })
    }
    if (rendered.html === null) return
    // Resolved as soon as it renders, before any output transform could see the page
    // (D42).
    const { payloads } = this.#program
    const marks = { path, generated, plugins: this.#plugins, payloads }
    const resolved = resolvePage(rendered, marks)
    this.#diagnostics.push(...resolved.diagnostics)
    // A url that names a file is a string.
    this.outputs.push({ file, url: url as string, bytes: resolved.bytes })
    const subject = { page: path, generated }
    for (const produced of resolved.files) this.#produce(subject, produced)
  }

  /**
   * The body of the document at `path`, whose page is at `url`, rendered alone (D46) —
   * its own output, its layout never called — with its marks resolved by the handlers a
   * page's are, and its links rebased for reading on another page: made absolute against
   * the site's origin, or root-relative where it names none; or `undefined`, where it
   * failed, or where `url` names no file, which the page's render reports. It renders
   * before any body exists, so a runtime error is labelled at its front matter as
   * rendering the body then. Resolving it reports only errors, since its page resolves
   * the same marks and warns there; each file its producers wrote is taken as a page's
   * is, named as the body's. A link it cannot rebase is kept as written and warned at,
   * at its front matter, as is a `srcset` candidate the rebase dropped.
   */
  body(path: string, url: string): string | undefined {
    // A body is keyed by its page's url, which the page's render refuses, and reports.
    if (outputFile(url) === undefined) return undefined
    const page = this.#program.pages.get(path)
    assert(page !== undefined, 'every planned page was compiled by this build')
    const at = pageSpan(page.page)
    const named = pageName(path, undefined, true)
    const rendered = this.#core.render({ path, body: true })
    const label = { path, span: at, message: `rendering ${named}, before any body exists` }
    for (const diagnostic of rendered.diagnostics) {
      this.#diagnostics.push({ ...diagnostic, labels: [...diagnostic.labels, label] })
    }
    if (rendered.html === null) return undefined
    const { payloads } = this.#program
    const marks = { path, body: true, plugins: this.#plugins, payloads }
    const resolved = resolvePage(rendered, marks)
    this.#diagnostics.push(...resolved.diagnostics)
    const subject = { page: path, body: true }
    for (const produced of resolved.files) this.#produce(subject, produced)
    const { body, unrebased } = rebaseLinks(UTF8.decode(resolved.bytes), url, this.#origin)
    for (const problem of unrebased) {
      const message =
        problem.kind === 'refused'
          ? `${named} links to ${JSON.stringify(problem.url)} in \`${problem.at}\`, ` +
            "which the URL parser refuses even against its page's url, so the body keeps " +
            'it as written'
          : `${named} holds \`${problem.at}\` as ${JSON.stringify(problem.srcset)}, ` +
            `whose descriptor \`${problem.descriptor}\` does not parse, so its candidate is ` +
            'dropped from the body as the others are rebased, as Eleventy drops it'
      this.#diagnostics.push({ path, span: at, severity: 'warning', message, labels: [] })
    }
    return body
  }

  /**
   * Takes the file a producer wrote while `subject` — a page, which of a generator's
   * pages, or a document's body — resolved, or reports why it cannot be written, at the
   * producer's mark: its url names no file, another producer wrote other bytes there, or
   * a page, a passthrough file or an asset hook's file claims it, or a file that cannot
   * stand beside it. The same bytes another producer wrote at the same file are taken as
   * written already — the stylesheet two pages share, or a body and its page — and other
   * bytes from the same call are the producer writing the file twice.
   */
  #produce(
    subject: { page: string; generated?: Generated; body?: boolean },
    { url, bytes, plugin, mark }: ProducedFile
  ): void {
    const { path, span } = mark
    const named = pageName(subject.page, subject.generated, subject.body)
    const lead = `the \`${plugin}\` plugin writes a file for this mark on ${named}`
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
    const writer: Writer = { path, span, producer: { plugin, ...subject } }
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
            `for this mark on ${named}`
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
    this.produced.push({ file, url, bytes })
  }
}

/**
 * A body's bytes as text: UTF-8, since the core writes it and a handler's bytes that are
 * not are refused, so a stray byte is a bug; a leading U+FEFF is the body's own.
 */
const UTF8 = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true })

/** What a url must be to name a file in the output directory. */
const URL_RULE =
  'is a path from `/`, without an empty, `.` or `..` segment, a percent-encoded `/` or ' +
  '`\\`, a query, or a fragment'

/**
 * How a message names `writer`: a page by its path — a generated one by its item too —
 * a passthrough file as one, an asset hook's file by its plugin, and a producer's file
 * by its plugin and the page it was written for.
 */
function who(writer: Writer): string {
  if (writer.producer !== undefined) {
    const { plugin, page, generated, body } = writer.producer
    return `the \`${plugin}\` plugin, for ${pageName(page, generated, body)},`
  }
  if (writer.assets !== undefined) {
    return `the \`${writer.assets.plugin}\` plugin's \`assets\` hook`
  }
  return writer.span === undefined
    ? `the passthrough file \`${writer.path}\``
    : pageName(writer.path, writer.generated)
}

/**
 * The label at `writer`, the first to claim `file`: at a page's url, a producer's mark,
 * or the start of a passthrough file or of the config listing an asset hook's plugin.
 */
function label(writer: Writer, file: string): Label {
  return {
    path: writer.path,
    span: writer.span ?? FILE_START,
    message: `the first to write there, as \`${file}\``,
  }
}

/**
 * The error at `span` in `path` — a page's url, the mark whose producer wrote the file,
 * or the start of the config listing an asset hook's plugin, which `lead` then says —
 * for `file` and the claim it collides with, labelled where that claim was made.
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

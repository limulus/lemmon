import type { OutputFile } from 'esbuild'
import { realpath } from 'node:fs/promises'
import { basename, dirname, extname, join, posix, relative, resolve, sep } from 'node:path'
import picomatch from 'picomatch'

import { encoder, type Plugin } from './plugins.ts'
import { files } from './walk.ts'

/** How a site shapes what {@link esbuild} bundles, and where it ships it. */
export interface EsbuildOptions {
  /**
   * The entry points, as globs from the input directory, matched as `passthrough` globs
   * are — dotfiles included, case aside — against the files a build walks: every
   * `index.ts` and `worker.ts` at any depth when omitted (D09).
   */
  entryPoints?: readonly string[]
  /**
   * The imports left for the browser to load, as esbuild's `external` reads them:
   * `['/lib/*']` when omitted (D09), so that a script imports another entry's bundle by
   * its URL — `'/lib/x/index.XXXXXXXX.js'` (D10).
   */
  external?: readonly string[]
  /**
   * Where each entry's bundle is written, as esbuild's `entryNames` reads it, from the
   * output directory: `'[dir]/[name].[hash]'` when omitted, beside where its source is
   * in the input, named by a hash of its bytes.
   */
  entryNames?: string
  /**
   * Where a file a bundle imports — a `.wasm` module — is written, as esbuild's
   * `assetNames` reads it: `'assets/immutable/esbuild/[name].[hash]'` when omitted,
   * under the `/assets/immutable/` a CDN may cache forever (D13).
   */
  assetNames?: string
  /**
   * The environment variables the scripts read, each as `process.env.NAME`, which esbuild
   * replaces with its value in the build's environment as a string literal. None when
   * omitted. One the environment lacks fails the hook, naming it.
   */
  env?: readonly string[]
}

/**
 * The shell's esbuild plugin (D09, D10, D45), listed in a site's config beside
 * `bundler()`: an `assets` hook that bundles the site's client scripts with esbuild
 * 0.24.0 — pinned exactly, since another release can write the same bundle differently,
 * and a bundle's bytes name it — and returns each bundle as a file, with a manifest that
 * maps each entry's URL as its source names it to the URL it ships at:
 * `/lib/x/index.js` to `/lib/x/index.ABCD2345.js`, so `asset("/lib/x/index.js")` prints
 * the hashed URL.
 *
 * Each entry under the input is bundled on its own — minified, as an ES module, with a
 * source map beside it — and a `.wasm` it imports is instantiated as a module
 * (`esbuild-plugin-wasm` 1.1.0, pinned too). esbuild runs from the project's root, the
 * directory of the config file, wherever `lem` was started, since its hashes name the
 * sources from there.
 *
 * Then each bundle's placeholders are resolved, as limulus.net's Eleventy build resolves
 * them (D10): a string naming `….XXXXXXXX.js` names the entry at that path, found from
 * `/` where it starts with `/` and from the bundle's own logical path where it does not,
 * and the file name the entry ships under is spliced into the string as written — unless
 * `entryNames` has moved the bundles so that it would name another file, when the string
 * becomes the URL the entry ships at, relative to the bundle's own where it was relative.
 * This follows the hashing, so the placeholder's target is not in its bundle's name. A
 * placeholder naming no entry fails the hook, naming the source that wrote it.
 *
 * esbuild's errors fail the hook, which the build reports at the config's start with
 * esbuild's message; its warnings are written to standard error as esbuild words them.
 * Every build bundles again, `lem dev`'s rebuilds among them, and `lem check` too, since
 * the manifest is what `asset()` compiles against. esbuild itself is loaded the first
 * time the hook runs, so a site that does not list the plugin never loads it.
 */
export function esbuild(options: EsbuildOptions = {}): Plugin {
  const {
    entryPoints = ['**/index.ts', '**/worker.ts'],
    external = ['/lib/*'],
    entryNames = '[dir]/[name].[hash]',
    assetNames = 'assets/immutable/esbuild/[name].[hash]',
    env = [],
  } = options
  const isEntry = picomatch([...entryPoints], { dot: true, nocase: true })
  return {
    name: 'esbuild',
    assets: async (context) => {
      const { env: environment } = context
      // As esbuild resolves each source, so that the paths in its maps and hashes are
      // the same however the project's own path was spelled: `/tmp` is `/private/tmp`.
      const [root, input, output] = await Promise.all(
        [context.root, context.input, context.output].map(real)
      )
      const define: Record<string, string> = {}
      for (const name of env) {
        const value = environment[name]
        if (value === undefined) {
          throw new Error(
            `the environment has no \`${name}\`, which the scripts read as ` +
              `\`process.env.${name}\``
          )
        }
        define[`process.env.${name}`] = JSON.stringify(value)
      }
      const entries = (await files({ input, output })).filter((path) => isEntry(path))
      // Loaded here rather than where this module is, which every value a config imports
      // from the shell loads, so that only a site listing this plugin loads esbuild.
      const [{ build, formatMessages }, { wasmLoader }] = await Promise.all([
        import('esbuild'),
        import('esbuild-plugin-wasm'),
      ])
      const { metafile, outputFiles, warnings } = await build({
        absWorkingDir: root,
        assetNames,
        bundle: true,
        define,
        entryNames,
        entryPoints: entries.map((path) => resolve(input, path)),
        external: [...external],
        format: 'esm',
        logLevel: 'silent',
        metafile: true,
        minify: true,
        outbase: input,
        outdir: output,
        plugins: [wasmLoader()],
        sourcemap: true,
        write: false,
      })
      if (warnings.length > 0) {
        process.stderr.write((await formatMessages(warnings, { kind: 'warning' })).join(''))
      }
      // A path esbuild names, absolute or from the root, as the URL it is written at.
      const urlOf = (file: string) =>
        `/${relative(output, resolve(root, file)).split(sep).join('/')}`
      // A source as esbuild names one in its messages: its path from the root.
      const named = (file: string) =>
        relative(root, resolve(root, file)).split(sep).join('/')
      const manifest: Record<string, string> = {}
      // The source each logical path was claimed for, and each bundle by its URL.
      const claimed = new Map<string, string>()
      const bundles = new Map<string, Bundle>()
      const claim = (path: string, source: string, file: string) => {
        const first = claimed.get(path)
        if (first !== undefined) {
          throw new Error(
            `\`${first}\` and \`${source}\` are both bundled as \`${path}\`, and an ` +
              'asset ships at one URL'
          )
        }
        claimed.set(path, source)
        manifest[path] = urlOf(file)
      }
      for (const [file, { entryPoint, cssBundle }] of Object.entries(metafile.outputs)) {
        if (entryPoint === undefined) continue
        const source = named(entryPoint)
        const logical = relative(input, resolve(root, entryPoint)).split(sep).join('/')
        const stem = `/${logical.slice(0, logical.length - extname(logical).length)}`
        const path = `${stem}${extname(file)}`
        claim(path, source, file)
        bundles.set(urlOf(file), { path, source })
        // The styles its script imports, which a page links as it loads the script.
        if (cssBundle !== undefined) claim(`${stem}.css`, source, cssBundle)
      }
      const byUrl = new Map(outputFiles.map((file) => [urlOf(file.path), file]))
      const built: Record<string, Uint8Array> = {}
      for (const { path: file, contents, text } of outputFiles) {
        const url = urlOf(file)
        const bundle = bundles.get(url)
        if (bundle === undefined) {
          built[url] = contents
          continue
        }
        // Where a placeholder naming no entry was written: in the source its map shows
        // holding it, or else — a `define` put it there — in the bundle.
        const where = (written: string) =>
          located(written, byUrl.get(`${url}.map`)!, root) ??
          `the bundle of \`${bundle.source}\``
        built[url] = encoder.encode(placed(text, url, bundle, manifest, where))
      }
      return { manifest, files: built }
    },
  }
}

/**
 * `path` with every symbolic link in it resolved, as esbuild resolves a source's: what
 * does not exist yet — an output directory never written — is kept as written, under
 * what it is in.
 */
async function real(path: string): Promise<string> {
  const rest: string[] = []
  for (;;) {
    try {
      return join(await realpath(path), ...rest)
    } catch {
      rest.unshift(basename(path))
      path = dirname(path)
    }
  }
}

/** An entry's bundle: its logical path, and its source's path from the root. */
interface Bundle {
  path: string
  source: string
}

/**
 * Where `written` first stands in a source the map `map` holds, counted as esbuild counts
 * the places it reports: the source's path from `root`, its line from 1, and its column
 * from 0, in bytes. `undefined` where no source holds it.
 */
function located(written: string, map: OutputFile, root: string): string | undefined {
  const { sources, sourcesContent } = JSON.parse(map.text) as {
    sources: string[]
    // `null` for a source a dependency's own map names without its text.
    sourcesContent: (string | null)[]
  }
  const index = sourcesContent.findIndex((content) => content?.includes(written))
  if (index === -1) return undefined
  const content = sourcesContent[index]!
  const before = content.slice(0, content.indexOf(written)).split('\n')
  const column = encoder.encode(before[before.length - 1]).length
  const source = relative(root, resolve(dirname(map.path), sources[index]))
  return `${source.split(sep).join('/')}:${before.length}:${column}`
}

/**
 * A string naming a placeholder: `"/lib/x/index.XXXXXXXX.js"` or `'./worker.XXXXXXXX.js'`,
 * as limulus.net's Eleventy build matches one, the path before the placeholder captured.
 */
const PLACEHOLDER = /(["'])([^"']+)\.X{8}\.js\1/g

/**
 * `text`, the bundle shipped at `url` of the entry `bundle` describes, with every
 * placeholder in it resolved to the entry it names in `manifest`: the file name that entry
 * ships under spliced in as written, as Eleventy splices the hash in, which keeps the
 * string's length and so the map's columns — or, where that would name another file
 * because `entryNames` moved the bundles, the entry's URL, written relative to `url`
 * where the placeholder was relative, `./` where it stays in that directory.
 *
 * @throws {Error} at what `where` says of a placeholder naming no entry.
 */
function placed(
  text: string,
  url: string,
  bundle: Bundle,
  manifest: Record<string, string>,
  where: (written: string) => string
): string {
  return text.replace(PLACEHOLDER, (_match, quote: string, before: string) => {
    const absolute = before.startsWith('/')
    const path = posix.join(absolute ? '/' : posix.dirname(bundle.path), `${before}.js`)
    const target = manifest[path]
    if (target === undefined) {
      const written = `${before}.XXXXXXXX.js`
      throw new Error(
        `${where(written)}: the placeholder \`${written}\` names no entry point: none ` +
          `ships at \`${path}\``
      )
    }
    const spliced = `${before.slice(0, before.lastIndexOf('/') + 1)}${posix.basename(target)}`
    if ((absolute ? spliced : posix.join(posix.dirname(url), spliced)) === target) {
      return `${quote}${spliced}${quote}`
    }
    if (absolute) return `${quote}${target}${quote}`
    const from = posix.relative(posix.dirname(url), target)
    return `${quote}${from.startsWith('../') ? from : `./${from}`}${quote}`
  })
}

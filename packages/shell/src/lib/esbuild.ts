import type { OutputFile } from 'esbuild'
import { createHash } from 'node:crypto'
import { realpath } from 'node:fs/promises'
import { basename, dirname, extname, join, posix, relative, resolve, sep } from 'node:path'
import picomatch from 'picomatch'

import { encoder, takesOnly, type Plugin } from './plugins.ts'
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

const OPTIONS: readonly string[] = [
  'entryPoints',
  'external',
  'assetNames',
  'env',
] satisfies (keyof EsbuildOptions)[]

/**
 * The shell's esbuild plugin (D09, D10, D45), listed in a site's config beside
 * `bundler()`: an `assets` hook that bundles the site's client scripts with esbuild
 * 0.24.0 — pinned exactly, since another release can write the same bundle differently,
 * and a bundle's bytes name it — and returns each bundle as a file, with a manifest that
 * maps each entry's URL as its source names it to the URL it ships at:
 * `/lib/x/index.js` to `/lib/x/index.ABCD2345.js`, so `asset("/lib/x/index.js")` prints
 * the hashed URL.
 *
 * Each entry under the input is bundled on its own, written beside its source — minified,
 * as an ES module, with a source map beside it — and a `.wasm` it imports is instantiated
 * as a module (`esbuild-plugin-wasm` 1.1.0, pinned too). esbuild runs from the project's
 * root, the directory of the config file, wherever `lem` was started, so that its
 * messages name each source from there.
 *
 * Then each bundle is named as limulus.net's Eleventy build names it, by what it ships
 * as. Its placeholders are resolved first (D10): a string naming `….XXXXXXXX.js` names
 * the entry at that path, found from `/` where it starts with `/` and from the bundle's
 * own logical path where it does not, and the file name that entry ships under — so it
 * is named first — is spliced into the string as written. The root's path, wherever a
 * map holds it — `esbuild-plugin-wasm` names its module by an absolute path, in the map's
 * sources and in the code it writes — becomes the way to the root from the map, as every
 * other source is named. Then `[hash]` is the first eight base32 characters of the
 * SHA-256 of the bundle — the comment naming its map still holding the placeholder — and
 * of its map, so a change to a worker renames every bundle that names it, and no name
 * depends on where the project is, so long as esbuild writes that path into the map as it
 * is: one holding a `"`, a `\` or a character outside ASCII is escaped there, and stays
 * absolute. A placeholder naming no entry, or naming its own bundle, even through others,
 * fails the hook at the source that wrote it.
 *
 * esbuild's errors fail the hook, which the build reports at the config's start with
 * esbuild's message; its warnings are written to standard error as esbuild words them.
 * Every build bundles again, `lem dev`'s rebuilds among them, and `lem check` too, since
 * the manifest is what `asset()` compiles against. esbuild itself is loaded the first
 * time the hook runs, so a site that does not list the plugin never loads it.
 *
 * Throws a `TypeError`, which the config load reports, for an option it does not take, so
 * that a misspelling is not ignored where a config runs with its types stripped.
 */
export function esbuild(options: EsbuildOptions = {}): Plugin {
  takesOnly('esbuild', options, OPTIONS)
  const {
    entryPoints = ['**/index.ts', '**/worker.ts'],
    external = ['/lib/*'],
    assetNames = 'assets/immutable/esbuild/[name].[hash]',
    env = [],
  } = options
  const isEntry = picomatch([...entryPoints], { dot: true, nocase: true })
  return {
    name: 'esbuild',
    assets: async (context) => {
      const { env: environment } = context
      // As esbuild resolves each source, so that the paths in its maps, and so the
      // names, are the same however the project's own path was spelled: `/tmp` is
      // `/private/tmp`.
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
        // Beside its source, with the placeholder where its hash goes until it is named
        // by what it ships as, as limulus.net's Eleventy build has it, so that the comment
        // naming its map carries the placeholder into its hash. esbuild's own hash after
        // it, cut off before anything is hashed, keeps two entries apart that would share
        // a path — `index.ts` and `index.tsx` — so that `claim` names both sources,
        // where esbuild would refuse them first, naming an output file.
        entryNames: `[dir]/[name].${UNNAMED}.[hash]`,
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
      const sourceOf = (file: string) =>
        relative(root, resolve(root, file)).split(sep).join('/')
      // What ships under each logical path, in the order esbuild lists the entries, and
      // each entry's own bundle by its logical path, which is what a placeholder names.
      const bundles: Bundle[] = []
      const scripts = new Map<string, Bundle>()
      const claim = (bundle: Bundle) => {
        const first = bundles.find(({ path }) => path === bundle.path)
        if (first !== undefined) {
          throw new Error(
            `\`${first.source}\` and \`${bundle.source}\` are both bundled as ` +
              `\`${bundle.path}\`, and an asset ships at one URL`
          )
        }
        bundles.push(bundle)
      }
      for (const [file, { entryPoint, cssBundle }] of Object.entries(metafile.outputs)) {
        if (entryPoint === undefined) continue
        const source = sourceOf(entryPoint)
        const logical = relative(input, resolve(root, entryPoint)).split(sep).join('/')
        const stem = `/${logical.slice(0, logical.length - extname(logical).length)}`
        const path = `${stem}${extname(file)}`
        const bundle = { path, source, url: urlOf(file), entry: true }
        claim(bundle)
        scripts.set(path, bundle)
        // The styles its script imports, which a page links as it loads the script.
        if (cssBundle !== undefined) {
          claim({ path: `${stem}.css`, source, url: urlOf(cssBundle), entry: false })
        }
      }
      const byUrl = new Map(outputFiles.map((file) => [urlOf(file.path), file]))
      const built: Record<string, Uint8Array> = {}
      // Writes `text`, which esbuild wrote at `url` with its map beside it, named by what
      // the two ship as; returns the URL it ships at.
      const ship = (url: string, text: string): string => {
        const map = byUrl.get(`${url}.map`)!
        // esbuild-plugin-wasm names a module by its absolute path, which would name the
        // bundle by where the checkout is; relative to the map, as each other source is,
        // and `./` from a map in the root itself rather than `/`.
        const relocated = map.text.replaceAll(
          `${root}/`,
          `${relative(dirname(map.path), root) || '.'}/`
        )
        // Without esbuild's hash: a dot and eight characters, as many as the placeholder.
        const extension = posix.extname(url)
        const placeheld = `${url.slice(0, -`.${UNNAMED}${extension}`.length)}${extension}`
        // esbuild ends a bundle with the comment naming its map, renamed with the bundle.
        const cut = text.lastIndexOf('sourceMappingURL=')
        const [code, comment] = [text.slice(0, cut), text.slice(cut)]
        const named = (to: string) =>
          code + comment.replace(posix.basename(url), () => posix.basename(to))
        // The placeholder in the file name alone, the last: a directory may be spelled
        // like one.
        const at = placeheld.lastIndexOf(UNNAMED)
        const hash = contentHash(named(placeheld), relocated)
        const final = placeheld.slice(0, at) + hash + placeheld.slice(at + UNNAMED.length)
        built[final] = encoder.encode(named(final))
        built[`${final}.map`] = encoder.encode(relocated)
        return final
      }
      // The URL each bundle ships at, by its logical path, once it is named, and the
      // bundles being named, each naming the next.
      const shipped = new Map<string, string>()
      const naming: string[] = []
      const name = (bundle: Bundle): string => {
        const done = shipped.get(bundle.path)
        if (done !== undefined) return done
        naming.push(bundle.path)
        const map = byUrl.get(`${bundle.url}.map`)!
        // Where a placeholder was written: in the source its map shows holding it, or
        // else — a `define` put it there — in the bundle.
        const where = (written: string) =>
          located(written, map, root) ?? `the bundle of \`${bundle.source}\``
        const text = placed(byUrl.get(bundle.url)!.text, bundle.path, (path, written) => {
          const target = scripts.get(path)
          if (target === undefined) {
            throw new Error(
              `${where(written)}: the placeholder \`${written}\` names no entry point: ` +
                `none ships at \`${path}\``
            )
          }
          if (naming.includes(path)) {
            const cycle = [...naming.slice(naming.indexOf(path)), path]
            throw new Error(
              `${where(written)}: the placeholder \`${written}\` closes a cycle, ` +
                `${cycle.map((each) => `\`${each}\``).join(' -> ')}, and a bundle's ` +
                'name is a hash of the names in it'
            )
          }
          return name(target)
        })
        naming.pop()
        const url = ship(bundle.url, text)
        shipped.set(bundle.path, url)
        return url
      }
      const manifest: Record<string, string> = {}
      for (const bundle of bundles) {
        manifest[bundle.path] = bundle.entry
          ? name(bundle)
          : ship(bundle.url, byUrl.get(bundle.url)!.text)
      }
      // What else esbuild wrote — the `.wasm` — ships as esbuild named it.
      const rewritten = new Set(bundles.flatMap(({ url }) => [url, `${url}.map`]))
      for (const { path, contents } of outputFiles) {
        const url = urlOf(path)
        if (!rewritten.has(url)) built[url] = contents
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

/**
 * A bundle esbuild wrote: its logical path, its source's path from the root, the URL
 * esbuild wrote it at — `XXXXXXXX` where its hash goes, and esbuild's own after the
 * name — and whether it is an entry's own bundle rather than the styles an entry's
 * script imports.
 */
interface Bundle {
  path: string
  source: string
  url: string
  entry: boolean
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

/** The placeholder, where a bundle's hash goes until it is named. */
const UNNAMED = 'XXXXXXXX'

/** RFC 4648's base32 alphabet, each character's index its value. */
const BASE32 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567'

/**
 * What names a bundle, as limulus.net's Eleventy build names one: the first eight
 * characters of the base32 of the SHA-256 of `texts` as UTF-8, one after another — the
 * digest's first 40 bits, five to a character.
 */
function contentHash(...texts: string[]): string {
  const sha256 = createHash('sha256')
  for (const text of texts) sha256.update(text, 'utf8')
  const digits = sha256.digest().readUIntBE(0, 5).toString(32).padStart(8, '0')
  return [...digits].map((digit) => BASE32[parseInt(digit, 32)]).join('')
}

/**
 * `text`, the bundle at the logical path `path`, with every placeholder in it resolved to
 * the URL `shipped` gives for the entry it names, as `shipped(path, written)`, the entry's
 * logical path and the placeholder as written: the file name that entry ships under
 * spliced in as written, as Eleventy splices the hash in, which keeps the string's length
 * and so the map's columns. Every bundle ships beside its source, so the spliced string
 * names the file the entry ships as.
 */
function placed(
  text: string,
  path: string,
  shipped: (path: string, written: string) => string
): string {
  return text.replace(PLACEHOLDER, (_match, quote: string, before: string) => {
    // A join closes up an empty segment that a browser keeps, so a placeholder holding
    // one names its path as written, which no entry ships at: spliced in, it would name
    // another URL, or with a leading `//` another host. Dot segments resolve alike.
    const target = shipped(
      before.includes('//')
        ? `${before}.js`
        : posix.join(before.startsWith('/') ? '/' : posix.dirname(path), `${before}.js`),
      `${before}.${UNNAMED}.js`
    )
    const spliced = `${before.slice(0, before.lastIndexOf('/') + 1)}${posix.basename(target)}`
    return `${quote}${spliced}${quote}`
  })
}

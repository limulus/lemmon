import { spawnSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { mkdirSync, realpathSync, symlinkSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it, vi } from 'vitest'

import { build } from './build.ts'
import { esbuild, type EsbuildOptions } from './esbuild.ts'
import { claimsOf, validatePlugins, type AssetsContext } from './plugins.ts'
import {
  configOf,
  ENV,
  LAYOUTS_LEM,
  PASSTHROUGH,
  project,
  SITE,
  tree,
} from '../mocks/sites.ts'
import { tempDir } from '../mocks/temp-dir.ts'

/** A bundle's `[hash]`: eight characters of base32, never the placeholder it replaces. */
const HASH = '(?!X{8})[A-Z2-7]{8}'

/** The smallest WebAssembly module there is: the magic number and the version. */
const WASM = '\0asm\u0001\0\0\0'

/**
 * A project holding `files` under its input, `www`, and the context its asset hook is
 * given: the project's directory its root, `dist` its output.
 */
function site(
  files: Record<string, string>,
  env: Record<string, string> = {}
): AssetsContext {
  const root = project(files)
  return { root, input: join(root, 'www'), output: join(root, 'dist'), writes: true, env }
}

/**
 * What {@link esbuild}'s hook, made with `options`, builds in `context`: its manifest, and
 * its files as text.
 */
async function bundled(context: AssetsContext, options?: EsbuildOptions) {
  const { manifest = {}, files = {} } = await esbuild(options).assets!(context)
  const decoder = new TextDecoder()
  const text = Object.entries(files).map(([url, bytes]) => [
    url,
    decoder.decode(bytes as Uint8Array),
  ])
  return { manifest, files: Object.fromEntries(text) as Record<string, string> }
}

/** The file at `url` in `files`, which must hold it. */
function at(files: Record<string, string>, url: string): string {
  const text = files[url]
  if (text === undefined) throw new Error(`no file at \`${url}\``)
  return text
}

/** The file name in `url`. */
const nameIn = (url: string) => url.split('/').pop()!

describe('esbuild()', () => {
  it('should be a plugin with an asset hook alone, named `esbuild`', () => {
    const plugin = esbuild()

    expect(validatePlugins([plugin])).toEqual([plugin])
    expect(plugin.name).toBe('esbuild')
    expect(claimsOf([plugin])).toEqual([])
  })

  it('should refuse an option it does not take, naming those it does', () => {
    const options = { entryNames: '[dir]/[name].[hash]' } as unknown as EsbuildOptions

    expect(() => esbuild(options)).toThrow(
      new TypeError(
        '`esbuild()` cannot take `entryNames`, which is none of its options: ' +
          '`entryPoints`, `external`, `assetNames`, `env`'
      )
    )
    // Every one it does take, at once.
    expect(() =>
      esbuild({ entryPoints: ['*.ts'], external: [], assetNames: '[name]', env: ['A'] })
    ).not.toThrow()
  })

  describe('its asset hook', () => {
    it('should bundle each `index.ts` and `worker.ts`, mapping each to the URL it ships at', async () => {
      const { manifest, files } = await bundled(
        site({
          'lib/a/index.ts': "import { twice } from './twice.ts'\nconsole.log(twice(2))\n",
          'lib/a/twice.ts': 'export const twice = (n: number): number => n * 2\n',
          'lib/a/worker.ts': 'self.postMessage(1)\n',
          'feed/index.md': '# Not a script\n',
        })
      )

      expect(manifest).toEqual({
        '/lib/a/index.js': expect.stringMatching(
          new RegExp(`^/lib/a/index\\.${HASH}\\.js$`)
        ),
        '/lib/a/worker.js': expect.stringMatching(
          new RegExp(`^/lib/a/worker\\.${HASH}\\.js$`)
        ),
      })
      const index = manifest['/lib/a/index.js']
      expect(Object.keys(files).sort()).toEqual(
        [
          index,
          `${index}.map`,
          manifest['/lib/a/worker.js'],
          `${manifest['/lib/a/worker.js']}.map`,
        ].sort()
      )
      // Bundled, minified, and pointing at its map, which names the sources from it.
      expect(at(files, index)).toMatch(/^var \w=\w=>\w\*2;console\.log\(\w\(2\)\);\n/)
      expect(at(files, index)).toContain(`//# sourceMappingURL=${nameIn(index)}.map\n`)
      expect(JSON.parse(at(files, `${index}.map`))).toMatchObject({
        sources: ['../../../www/lib/a/twice.ts', '../../../www/lib/a/index.ts'],
      })
    })

    it('should name a bundle the same in every build, wherever the project is', async () => {
      const files = {
        'lib/a/index.ts':
          "import { instance } from './empty.wasm'\nconsole.log(instance)\n",
        'lib/a/empty.wasm': WASM,
      }
      const context = site(files)

      const first = await bundled(context)
      expect(await bundled(context)).toEqual(first)
      // Named by what it ships as, which names its sources from itself: neither the
      // project's own path nor the root esbuild runs from is in it.
      expect(await bundled(site(files))).toEqual(first)
      expect(await bundled({ ...context, root: context.input })).toEqual(first)
    })

    it('should name a module in a map from the map, not by the project’s path', async () => {
      const context = site({
        'lib/a/index.ts':
          "import { instance } from './empty.wasm'\nconsole.log(instance)\n",
        'lib/a/empty.wasm': WASM,
      })

      const { manifest, files } = await bundled(context)

      // esbuild-plugin-wasm names its module by an absolute path, in the map's sources
      // and in the code it writes for it, which the map holds too.
      const map = at(files, `${manifest['/lib/a/index.js']}.map`)
      expect(JSON.parse(map)).toMatchObject({
        sources: [
          'wasm-module:../../../www/lib/a/empty.wasm',
          '../../../www/lib/a/index.ts',
        ],
      })
      expect(map).toContain('import wasmModule from \\"../../../www/lib/a/empty.wasm\\";')
      expect(map).not.toContain(realpathSync(context.root))
    })

    it('should name a module from a map in the root itself', async () => {
      const context = site({
        'index.ts':
          "import { instance } from '../dist/empty.wasm'\nconsole.log(instance)\n",
      })
      const output = join(context.root, 'dist')
      mkdirSync(output)
      writeFileSync(join(output, 'empty.wasm'), WASM)

      const { manifest, files } = await bundled({ ...context, root: output, output })

      // From the map's own directory, not from `/`.
      const map = at(files, `${manifest['/index.js']}.map`)
      expect(JSON.parse(map)).toMatchObject({
        sources: ['wasm-module:./empty.wasm', '../www/index.ts'],
      })
      expect(map).toContain('import wasmModule from \\"./empty.wasm\\";')
    })

    it('should name a bundle as limulus.net’s Eleventy build names it', async () => {
      const context = site({
        'lib/a/index.ts': "new Worker(new URL('./worker.XXXXXXXX.js', import.meta.url))\n",
        'lib/a/worker.ts':
          "import { instance } from './empty.wasm'\nconsole.log(instance)\n",
        'lib/a/empty.wasm': WASM,
      })

      const { manifest, files } = await bundled(
        { ...context, output: join(context.root, 'dist', 'www') },
        { assetNames: 'assets/immutable/11ty/esbuild/[name].[hash]' }
      )

      // What limulus.net's 11ty/esbuild.ts, as of its d1dc734, writes for these sources
      // into dist/www.
      expect(manifest).toEqual({
        '/lib/a/index.js': '/lib/a/index.JP4OVLGG.js',
        '/lib/a/worker.js': '/lib/a/worker.JMWDZIAD.js',
      })
      expect(Object.keys(files).sort()).toEqual([
        '/assets/immutable/11ty/esbuild/empty.FV2RY22A.wasm',
        '/lib/a/index.JP4OVLGG.js',
        '/lib/a/index.JP4OVLGG.js.map',
        '/lib/a/worker.JMWDZIAD.js',
        '/lib/a/worker.JMWDZIAD.js.map',
      ])
      expect(at(files, '/lib/a/index.JP4OVLGG.js')).toBe(
        'new Worker(new URL("./worker.JMWDZIAD.js",import.meta.url));\n' +
          '//# sourceMappingURL=index.JP4OVLGG.js.map\n'
      )
    })

    it('should take a bundle’s name from the SHA-256 of its text, as it is named, and its map', async () => {
      const { manifest, files } = await bundled(
        site({ 'lib/a/index.ts': 'console.log(7)\n' })
      )

      // A digest whose first five bits are zero, which name the first character too.
      const url = manifest['/lib/a/index.js']
      expect(url).toBe('/lib/a/index.AXXY6SC2.js')
      const [, hash] = nameIn(url).split('.')
      // The comment naming its map still holds the placeholder while it is hashed.
      const unnamed = at(files, url).replace(nameIn(url), 'index.XXXXXXXX.js')
      expect(unnamed).toBe('console.log(7);\n//# sourceMappingURL=index.XXXXXXXX.js.map\n')
      const digest = createHash('sha256')
        .update(unnamed + at(files, `${url}.map`))
        .digest()
      // RFC 4648's base32 of the digest's first 40 bits, five to a character.
      const bits = [...digest.subarray(0, 5)]
        .map((byte) => byte.toString(2).padStart(8, '0'))
        .join('')
      const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567'
      expect(hash).toBe(
        bits
          .match(/.{5}/g)!
          .map((five) => alphabet[parseInt(five, 2)])
          .join('')
      )
    })

    it('should build the same files however the project’s path is spelled, through a link or not', async () => {
      const context = site({
        'lib/a/index.ts': "import './b.ts'\nconsole.log(1)\n",
        'lib/a/b.ts': '',
      })
      const link = join(tempDir(), 'link')
      symlinkSync(context.root, link)

      const linked = await bundled({
        ...context,
        root: link,
        input: join(link, 'www'),
        output: join(link, 'dist'),
      })

      // esbuild reads each source by its real path, so the hook hands it every
      // directory by its real path too, or the map would climb out of the link to name
      // them, and the hashes would differ with it.
      expect(linked).toEqual(await bundled(context))
    })

    it('should find entries as the walk finds files, matched as passthrough globs are', async () => {
      const context = site({
        'Lib/Index.ts': 'console.log(1)\n',
        '.well-known/worker.ts': 'console.log(2)\n',
        'node_modules/x/index.ts': 'console.log(3)\n',
        'dist/old/index.ts': 'console.log(4)\n',
      })

      const { manifest } = await bundled({
        ...context,
        output: join(context.input, 'dist'),
      })

      // Case aside and dotfiles included; never a `node_modules` or the output.
      expect(Object.keys(manifest)).toEqual(['/.well-known/worker.js', '/Lib/Index.js'])
    })

    it('should build nothing where no file is an entry', async () => {
      expect(await bundled(site({ 'lib/a/helper.ts': 'export {}\n' }))).toEqual({
        manifest: {},
        files: {},
      })
    })

    it('should map the styles an entry imports beside it', async () => {
      const { manifest, files } = await bundled(
        site({
          'lib/a/index.ts': "import './a.css'\n",
          'lib/a/a.css': '.a { color: red }\n',
        })
      )

      // Named by what it ships as, too, with its map beside it.
      const css = manifest['/lib/a/index.css']
      expect(css).toMatch(new RegExp(`^/lib/a/index\\.${HASH}\\.css$`))
      expect(at(files, css)).toBe(
        `.a{color:red}\n/*# sourceMappingURL=${nameIn(css)}.map */\n`
      )
      expect(JSON.parse(at(files, `${css}.map`))).toMatchObject({
        sources: ['../../../www/lib/a/a.css'],
      })
    })

    it('should instantiate a `.wasm` module an entry imports, written under `/assets/immutable/esbuild/`', async () => {
      const { manifest, files } = await bundled(
        site({
          'lib/a/index.ts':
            "import { instance } from './empty.wasm'\nconsole.log(instance)\n",
          'lib/a/empty.wasm': WASM,
        })
      )

      // Named as esbuild names it, by its bytes.
      const [url] = Object.keys(files).filter((file) => file.endsWith('.wasm'))
      expect(url).toMatch(new RegExp(`^/assets/immutable/esbuild/empty\\.${HASH}\\.wasm$`))
      expect(at(files, url)).toBe(WASM)
      const bundle = at(files, manifest['/lib/a/index.js'])
      expect(bundle).toContain(`"../../${url.slice(1)}"`)
      expect(bundle).toContain('WebAssembly.instantiate')
    })

    it('should take its entries, externals and asset names from its options', async () => {
      const { manifest, files } = await bundled(
        site({
          'app.ts':
            "import '/vendor/x.js'\nimport { instance } from './m.wasm'\nconsole.log(instance)\n",
          'm.wasm': WASM,
          'lib/a/index.ts': 'console.log(1)\n',
        }),
        { entryPoints: ['*.ts'], external: ['/vendor/*'], assetNames: 'media/[name]' }
      )

      const app = manifest['/app.js']
      expect(manifest).toEqual({
        '/app.js': expect.stringMatching(new RegExp(`^/app\\.${HASH}\\.js$`)),
      })
      expect(Object.keys(files).sort()).toEqual([app, `${app}.map`, '/media/m.wasm'])
      expect(at(files, app)).toMatch(/^import"\/vendor\/x\.js";/)
    })

    it('should refuse two entries bundled at one logical path', async () => {
      const context = site({
        'lib/a/index.ts': 'console.log(1)\n',
        'lib/a/index.tsx': 'console.log(2)\n',
      })

      await expect(
        bundled(context, { entryPoints: ['**/index.ts', '**/index.tsx'] })
      ).rejects.toThrow(
        new Error(
          '`www/lib/a/index.ts` and `www/lib/a/index.tsx` are both bundled as ' +
            '`/lib/a/index.js`, and an asset ships at one URL'
        )
      )
    })

    it('should refuse an entry’s styles where another entry is bundled at their path', async () => {
      const context = site({
        'lib/a/index.ts': "import './a.css'\n",
        'lib/a/a.css': '.a { color: red }\n',
        'lib/a/index.css': '.b { color: blue }\n',
      })

      await expect(
        bundled(context, { entryPoints: ['**/index.ts', '**/index.css'] })
      ).rejects.toThrow(
        new Error(
          '`www/lib/a/index.css` and `www/lib/a/index.ts` are both bundled as ' +
            '`/lib/a/index.css`, and an asset ships at one URL'
        )
      )
    })

    describe('with `env`', () => {
      it('should replace `process.env.NAME` with the value of each variable it names', async () => {
        const { manifest, files } = await bundled(
          site(
            {
              'lib/a/index.ts':
                'console.log(process.env.TOKEN, process.env.EMPTY, process.env.HOME)\n',
            },
            { TOKEN: 'a"b', EMPTY: '', HOME: '/home' }
          ),
          { env: ['TOKEN', 'EMPTY'] }
        )

        // One it does not name stays a read of the browser's `process`, as written.
        expect(at(files, manifest['/lib/a/index.js'])).toMatch(
          /^console\.log\('a"b',"",process\.env\.HOME\);/
        )
      })

      it('should fail on a variable the environment lacks, naming it', async () => {
        const context = site({ 'lib/a/index.ts': 'console.log(process.env.TOKEN)\n' })

        await expect(bundled(context, { env: ['TOKEN'] })).rejects.toThrow(
          new Error(
            'the environment has no `TOKEN`, which the scripts read as `process.env.TOKEN`'
          )
        )
      })
    })

    describe('a placeholder', () => {
      it('should name the entry at its path from `/`, written as the URL it ships at', async () => {
        const { manifest, files } = await bundled(
          site({
            'lib/a/index.ts': "import '/lib/b/index.XXXXXXXX.js'\n",
            'lib/b/index.ts': "console.log('b')\n",
          })
        )

        // `/lib/*` is left to the browser, which loads `b`'s bundle by its hashed URL.
        expect(at(files, manifest['/lib/a/index.js'])).toMatch(
          new RegExp(`^import"${manifest['/lib/b/index.js']}";\n`)
        )
      })

      describe('relative to the bundle’s own path', () => {
        it('should take the file name its entry ships under, spliced in as written', async () => {
          const { manifest, files } = await bundled(
            site({
              'lib/a/index.ts':
                "new Worker(new URL('./worker.XXXXXXXX.js', import.meta.url))\n" +
                "new Worker(new URL('worker.XXXXXXXX.js', import.meta.url))\n" +
                "new Worker(new URL('../b/index.XXXXXXXX.js', import.meta.url))\n" +
                "import('/lib/b/index.XXXXXXXX.js')\n",
              'lib/a/worker.ts': 'self.postMessage(1)\n',
              'lib/b/index.ts': "console.log('b')\n",
            })
          )

          // As Eleventy splices the hash in: each string as written, the same length.
          const worker = nameIn(manifest['/lib/a/worker.js'])
          const b = manifest['/lib/b/index.js']
          expect(at(files, manifest['/lib/a/index.js'])).toContain(
            `new Worker(new URL("./${worker}",import.meta.url));` +
              `new Worker(new URL("${worker}",import.meta.url));` +
              `new Worker(new URL("../b/${nameIn(b)}",import.meta.url));import("${b}");`
          )
        })
      })

      it('should be resolved before its bundle is named, so a loader is renamed with its worker', async () => {
        const context = site({
          'lib/a/index.ts':
            "new Worker(new URL('./worker.XXXXXXXX.js', import.meta.url))\n",
          'lib/a/worker.ts': 'self.postMessage(1)\n',
        })
        const before = await bundled(context)
        writeFileSync(join(context.input, 'lib/a/worker.ts'), 'self.postMessage(2)\n')

        const after = await bundled(context)

        // The loader's source is unchanged, but the name in it is the new worker's, so it
        // ships under a new name too: a cache keeping every hashed name for a year never
        // hands out the old loader for the new worker's page.
        const worker = after.manifest['/lib/a/worker.js']
        expect(worker).not.toBe(before.manifest['/lib/a/worker.js'])
        expect(after.manifest['/lib/a/index.js']).not.toBe(
          before.manifest['/lib/a/index.js']
        )
        expect(at(after.files, after.manifest['/lib/a/index.js'])).toContain(
          `new URL("./${nameIn(worker)}",import.meta.url)`
        )
      })

      it('should fail where it closes a cycle, at the source that wrote it', async () => {
        const context = site({
          'lib/a/index.ts': "import '/lib/b/index.XXXXXXXX.js'\n",
          'lib/b/index.ts': "import '/lib/c/index.XXXXXXXX.js'\n",
          'lib/c/index.ts': "// Back to b.\nimport '/lib/b/index.XXXXXXXX.js'\n",
        })

        // Each would be named by the other's name, so neither can be; the cycle alone is
        // named, not the bundle that led into it.
        await expect(bundled(context)).rejects.toThrow(
          new Error(
            'www/lib/c/index.ts:2:8: the placeholder `/lib/b/index.XXXXXXXX.js` closes a ' +
              'cycle, `/lib/b/index.js` -> `/lib/c/index.js` -> `/lib/b/index.js`, and a ' +
              "bundle's name is a hash of the names in it"
          )
        )
      })

      it('should fail where it names its own bundle', async () => {
        const context = site({
          'lib/a/worker.ts':
            "new Worker(new URL('./worker.XXXXXXXX.js', import.meta.url))\n",
        })

        await expect(bundled(context)).rejects.toThrow(
          new Error(
            'www/lib/a/worker.ts:1:20: the placeholder `./worker.XXXXXXXX.js` closes a ' +
              "cycle, `/lib/a/worker.js` -> `/lib/a/worker.js`, and a bundle's name is a " +
              'hash of the names in it'
          )
        )
      })

      it('should fail where it names no entry, at the source that wrote it', async () => {
        const context = site({
          'lib/a/index.ts': "import { start } from './start.ts'\nstart()\n",
          'lib/a/start.ts':
            '// Starts the worker.\nexport const start = () =>\n' +
            "  new Worker(new URL('./gone.XXXXXXXX.js', import.meta.url))\n",
        })

        // Placed as esbuild places its own errors: from the root, a column from 0.
        await expect(bundled(context)).rejects.toThrow(
          new Error(
            'www/lib/a/start.ts:3:22: the placeholder `./gone.XXXXXXXX.js` names no entry ' +
              'point: none ships at `/lib/a/gone.js`'
          )
        )
      })

      it('should be spliced in as written where it climbs, as a browser resolves it alike', async () => {
        const context = site({
          'lib/a/index.ts':
            "new Worker(new URL('/lib/a/../b/worker.XXXXXXXX.js', import.meta.url))\n",
          'lib/b/worker.ts': 'self.postMessage(1)\n',
        })

        const { files, manifest } = await bundled(context)

        const worker = manifest['/lib/b/worker.js']
        expect(at(files, manifest['/lib/a/index.js'])).toContain(
          `new URL("/lib/a/../b/${nameIn(worker)}",import.meta.url)`
        )
      })

      it('should fail where an empty segment keeps it from naming the entry a join would', async () => {
        const context = site({
          'lib/a/index.ts':
            "new Worker(new URL('/lib//b/worker.XXXXXXXX.js', import.meta.url))\n",
          'lib/b/worker.ts': 'self.postMessage(1)\n',
        })

        // A browser asks for `/lib//b/…`, which is not where `lib/b/worker.ts` ships.
        await expect(bundled(context)).rejects.toThrow(
          new Error(
            'www/lib/a/index.ts:1:20: the placeholder `/lib//b/worker.XXXXXXXX.js` names ' +
              'no entry point: none ships at `/lib//b/worker.js`'
          )
        )
      })

      it('should fail at the source that wrote it past one whose text the map lacks', async () => {
        // A dependency's own map, naming its source without the source's text.
        const map = { version: 3, sources: ['dep.coffee'], names: [], mappings: 'AAAA' }
        const inline = Buffer.from(JSON.stringify(map)).toString('base64')
        const context = site({
          'lib/a/dep.js':
            'export const f = () => 1\n' +
            `//# sourceMappingURL=data:application/json;base64,${inline}\n`,
          'lib/a/index.ts':
            "import { f } from './dep.js'\n" +
            "console.log(f(), new URL('./gone.XXXXXXXX.js', import.meta.url))\n",
        })

        await expect(bundled(context)).rejects.toThrow(
          new Error(
            'www/lib/a/index.ts:2:26: the placeholder `./gone.XXXXXXXX.js` names no entry ' +
              'point: none ships at `/lib/a/gone.js`'
          )
        )
      })

      it('should fail at the bundle where no source wrote it', async () => {
        const context = site(
          { 'lib/a/index.ts': 'import(process.env.SCRIPT)\n' },
          { SCRIPT: '/lib/gone/index.XXXXXXXX.js' }
        )

        await expect(bundled(context, { env: ['SCRIPT'] })).rejects.toThrow(
          new Error(
            'the bundle of `www/lib/a/index.ts`: the placeholder ' +
              '`/lib/gone/index.XXXXXXXX.js` names no entry point: none ships at ' +
              '`/lib/gone/index.js`'
          )
        )
      })

      it('should be a string closed by the quote that opened it', async () => {
        const text = 'console.log("/lib/gone/index.XXXXXXXX.js\'")\n'
        const { manifest, files } = await bundled(site({ 'lib/a/index.ts': text }))

        // Quoted by neither quote alone, so nothing names an entry and nothing fails.
        expect(at(files, manifest['/lib/a/index.js'])).toMatch(
          /^console\.log\("\/lib\/gone\/index\.XXXXXXXX\.js'"\);/
        )
      })
    })

    describe('what esbuild reports', () => {
      it('should fail with esbuild’s errors, each at its source from the root', async () => {
        const context = site({ 'lib/a/index.ts': "import 'missing'\n" })

        await expect(bundled(context)).rejects.toThrow(
          'Build failed with 1 error:\nwww/lib/a/index.ts:1:7: ERROR: Could not resolve "missing"'
        )
      })

      it('should write esbuild’s warnings to standard error, as esbuild words them, and build', async () => {
        const stderr = vi.spyOn(process.stderr, 'write').mockImplementation(() => true)
        try {
          const clean = await bundled(site({ 'lib/a/index.ts': 'console.log(1)\n' }))
          expect(stderr).not.toHaveBeenCalled()

          const { manifest } = await bundled(
            site({ 'lib/a/index.ts': 'console.log({ a: 1, a: 2 })\n' })
          )

          expect(Object.keys(clean.manifest)).toEqual(Object.keys(manifest))
          expect(stderr).toHaveBeenCalledTimes(1)
          const [written] = stderr.mock.calls[0]
          expect(written).toContain('[WARNING] Duplicate key "a" in object literal')
          expect(written).toContain('www/lib/a/index.ts:1:20:')
        } finally {
          stderr.mockRestore()
        }
      })

      it('should load esbuild only when the hook runs, and keep esbuild itself silent', () => {
        // In a process of its own, since esbuild's own log, were it on, would be written
        // by its service straight to the standard error it inherits, which no spy sees.
        const child = `
          import { createRequire } from 'node:module'
          const require = createRequire(import.meta.url)
          const loaded = () =>
            Object.keys(require.cache).some((file) => file.includes('/esbuild/lib/main.js'))
          const [shell, warned, failing] = process.argv.slice(1)
          const { esbuild } = await import(shell)
          console.log('imported:', loaded())
          const at = (root) =>
            ({ root, input: root + '/www', output: root + '/dist', writes: false, env: {} })
          const { assets } = esbuild()
          await assets(at(warned))
          await assets(at(failing)).catch((error) => console.log('failed:', error.message))
          console.log('bundled:', loaded())
        `
        const warned = project({ 'lib/a/index.ts': 'console.log({ a: 1, a: 2 })\n' })
        const failing = project({ 'lib/a/index.ts': "import 'missing'\n" })
        const shell = new URL('../index.ts', import.meta.url).href

        const { status, stdout, stderr } = spawnSync(
          process.execPath,
          [
            '--conditions=source',
            '--input-type=module',
            '--eval',
            child,
            shell,
            warned,
            failing,
          ],
          { encoding: 'utf8' }
        )

        expect(status).toBe(0)
        // Importing the shell loads no esbuild; a site listing the plugin does.
        expect(stdout).toBe(
          'imported: false\n' +
            'failed: Build failed with 1 error:\n' +
            'www/lib/a/index.ts:1:7: ERROR: Could not resolve "missing"\n' +
            'bundled: true\n'
        )
        // The warning once, as the hook writes it, and the error only in its failure.
        expect(stderr.split('Duplicate key "a"')).toHaveLength(2)
        expect(stderr).not.toContain('Could not resolve')
      })
    })
  })

  describe('in a build', () => {
    it('should ship each bundle at the URL `asset()` prints for its logical path', async () => {
      const layouts = LAYOUTS_LEM.replace(
        '<main>',
        '<script type="module" src="<lem.asset("/lib/app/index.js")>" async></script>\n  <main>'
      )
      const dir = project(
        { ...SITE, 'layouts.lem': layouts, 'lib/app/index.ts': 'console.log(1)\n' },
        PASSTHROUGH
      )

      const result = await build(configOf(dir, PASSTHROUGH, [esbuild()]), {
        now: new Date(0),
        env: ENV,
      })

      expect(result.diagnostics).toEqual([])
      const [url] = result.assets.filter((file) => file.endsWith('.js'))
      expect(url).toMatch(new RegExp(`^lib/app/index\\.${HASH}\\.js$`))
      const written = tree(join(dir, 'dist'))
      expect(written[url]).toBe(
        `console.log(1);\n//# sourceMappingURL=${url.split('/').pop()}.map\n`
      )
      expect(written['posts/a/index.html']).toContain(
        `<script type="module" src="/${url}" async></script>`
      )
    })
  })
})

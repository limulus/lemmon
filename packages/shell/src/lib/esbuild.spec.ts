import { spawnSync } from 'node:child_process'
import { symlinkSync, writeFileSync } from 'node:fs'
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

/** esbuild's `[hash]`: eight characters of base32. */
const HASH = '[A-Z2-7]{8}'

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

describe('esbuild()', () => {
  it('should be a plugin with an asset hook alone, named `esbuild`', () => {
    const plugin = esbuild()

    expect(validatePlugins([plugin])).toEqual([plugin])
    expect(plugin.name).toBe('esbuild')
    expect(claimsOf([plugin])).toEqual([])
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
      const name = index.split('/').pop()!
      expect(at(files, index)).toMatch(/^var \w=\w=>\w\*2;console\.log\(\w\(2\)\);\n/)
      expect(at(files, index)).toContain(`//# sourceMappingURL=${name}.map\n`)
      expect(JSON.parse(at(files, `${index}.map`))).toMatchObject({
        sources: ['../../../www/lib/a/twice.ts', '../../../www/lib/a/index.ts'],
      })
    })

    it('should name a bundle the same in every build, by its bytes and the root it is built from', async () => {
      const context = site({ 'lib/a/index.ts': 'console.log(1)\n' })

      const first = await bundled(context)
      expect(await bundled(context)).toEqual(first)
      // esbuild hashes the sources by their paths from where it runs, so the root moves
      // every name, and the bytes stay as they were.
      const elsewhere = await bundled({ ...context, root: context.input })
      const [url] = Object.values(first.manifest)
      const [moved] = Object.values(elsewhere.manifest)
      expect(moved).not.toBe(url)
      expect(at(elsewhere.files, moved).replace(/\w+\.js\.map/, '')).toBe(
        at(first.files, url).replace(/\w+\.js\.map/, '')
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

      const css = manifest['/lib/a/index.css']
      expect(css).toMatch(new RegExp(`^/lib/a/index\\.${HASH}\\.css$`))
      expect(at(files, css)).toContain('.a{color:red}')
    })

    it('should instantiate a `.wasm` module an entry imports, written under `/assets/immutable/esbuild/`', async () => {
      // The smallest module there is: the magic number and the version.
      const wasm = '\0asm\u0001\0\0\0'
      const { manifest, files } = await bundled(
        site({
          'lib/a/index.ts':
            "import { instance } from './empty.wasm'\nconsole.log(instance)\n",
          'lib/a/empty.wasm': wasm,
        })
      )

      const [url] = Object.keys(files).filter((file) => file.endsWith('.wasm'))
      expect(url).toMatch(new RegExp(`^/assets/immutable/esbuild/empty\\.${HASH}\\.wasm$`))
      expect(at(files, url)).toBe(wasm)
      const bundle = at(files, manifest['/lib/a/index.js'])
      expect(bundle).toContain(`"../../${url.slice(1)}"`)
      expect(bundle).toContain('WebAssembly.instantiate')
    })

    it('should take its entries, externals and names from its options', async () => {
      const { manifest, files } = await bundled(
        site({
          'app.ts':
            "import '/vendor/x.js'\nimport { instance } from './m.wasm'\nconsole.log(instance)\n",
          'm.wasm': '\0asm\u0001\0\0\0',
          'lib/a/index.ts': 'console.log(1)\n',
        }),
        {
          entryPoints: ['*.ts'],
          external: ['/vendor/*'],
          entryNames: 'js/[name]',
          assetNames: 'media/[name]',
        }
      )

      expect(manifest).toEqual({ '/app.js': '/js/app.js' })
      expect(Object.keys(files).sort()).toEqual([
        '/js/app.js',
        '/js/app.js.map',
        '/media/m.wasm',
      ])
      expect(at(files, '/js/app.js')).toMatch(/^import"\/vendor\/x\.js";/)
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
        const files = {
          'lib/a/index.ts':
            "new Worker(new URL('./worker.XXXXXXXX.js', import.meta.url))\n" +
            "new Worker(new URL('worker.XXXXXXXX.js', import.meta.url))\n" +
            "new Worker(new URL('../b/index.XXXXXXXX.js', import.meta.url))\n" +
            "import('/lib/b/index.XXXXXXXX.js')\n",
          'lib/a/worker.ts': 'self.postMessage(1)\n',
          'lib/b/index.ts': "console.log('b')\n",
        }
        const name = (url: string) => url.split('/').pop()!

        it('should take the file name its entry ships under, spliced in as written', async () => {
          const { manifest, files: built } = await bundled(site(files))

          // As Eleventy splices the hash in: each string as written, the same length.
          const worker = name(manifest['/lib/a/worker.js'])
          const b = manifest['/lib/b/index.js']
          expect(at(built, manifest['/lib/a/index.js'])).toContain(
            `new Worker(new URL("./${worker}",import.meta.url));` +
              `new Worker(new URL("${worker}",import.meta.url));` +
              `new Worker(new URL("../b/${name(b)}",import.meta.url));import("${b}");`
          )
        })

        it('should be the URL its entry ships at where `entryNames` moved the bundles', async () => {
          const context = site(files)
          const flat = await bundled(context, { entryNames: '[name].[hash]' })
          const deeper = await bundled(context, { entryNames: '[dir]/x/[name].[hash]' })

          // Written as found where that still names the file, from the bundle's own URL
          // where it would not, and absolute where it was.
          const b = flat.manifest['/lib/b/index.js']
          const worker = name(flat.manifest['/lib/a/worker.js'])
          expect(at(flat.files, flat.manifest['/lib/a/index.js'])).toContain(
            `new Worker(new URL("./${worker}",import.meta.url));` +
              `new Worker(new URL("${worker}",import.meta.url));` +
              `new Worker(new URL("./${name(b)}",import.meta.url));import("${b}");`
          )
          const moved = deeper.manifest['/lib/b/index.js']
          expect(moved).toMatch(new RegExp(`^/lib/b/x/index\\.${HASH}\\.js$`))
          expect(at(deeper.files, deeper.manifest['/lib/a/index.js'])).toContain(
            `new Worker(new URL("../../b/x/${name(moved)}",import.meta.url));` +
              `import("${moved}");`
          )
        })
      })

      it('should be resolved after hashing, so a bundle is named by the placeholder', async () => {
        const context = site({
          'lib/a/index.ts':
            "new Worker(new URL('./worker.XXXXXXXX.js', import.meta.url))\n",
          'lib/a/worker.ts': 'self.postMessage(1)\n',
        })
        const before = await bundled(context)
        writeFileSync(join(context.input, 'lib/a/worker.ts'), 'self.postMessage(2)\n')

        const after = await bundled(context)

        // As limulus.net's Eleventy build names it: a new worker, and its loader's name
        // unchanged, though the URL in it is the new worker's.
        const loader = before.manifest['/lib/a/index.js']
        expect(after.manifest['/lib/a/worker.js']).not.toBe(
          before.manifest['/lib/a/worker.js']
        )
        expect(after.manifest['/lib/a/index.js']).toBe(loader)
        expect(at(after.files, loader)).not.toBe(at(before.files, loader))
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

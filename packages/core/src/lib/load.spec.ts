import { readFileSync } from 'node:fs'
import { mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { setTimeout as sleep } from 'node:timers/promises'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { setFlagsFromString } from 'node:v8'
import { runInNewContext } from 'node:vm'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { WasmBoundary } from './boundary.ts'
import { createCore, loadCore, packageVersion, stagingDirectory } from './load.ts'

const stagingDir = new URL('../../artifacts/wasm/', import.meta.url)

describe('stagingDirectory()', () => {
  it('should find the staged artifact by walking up from a module', () => {
    expect(stagingDirectory().href).toBe(stagingDir.href)
  })

  it('should find it from the built layout as well as the source one', () => {
    // The bug this guards: counting `../`s from src/lib/ sends the built
    // dist/esm/lib/ copy looking under dist/ instead of the package's root.
    const built = new URL('../../dist/esm/lib/load.js', import.meta.url)
    expect(stagingDirectory(built).href).toBe(stagingDir.href)
  })

  it('should name the nearest candidate when nothing is staged above it', () => {
    expect(stagingDirectory(new URL('file:///nowhere/at/all.js')).href).toBe(
      'file:///nowhere/at/artifacts/wasm/'
    )
  })
})

const decoder = new TextDecoder()

describe('loadCore()', () => {
  it('should load the staged artifact found from this module', async () => {
    const core = await loadCore()
    expect(core.compile([{ path: 'page.lem', text: 'hi' }]).diagnostics).toEqual([])
    expect(decoder.decode(core.render({ path: 'page.lem' }).html!)).toBe('hi')
  })

  it('should accept a directory as a path string, trailing slash or not', async () => {
    const withSlash = fileURLToPath(stagingDir)
    expect(await loadCore(withSlash)).toBeDefined()
    expect(await loadCore(withSlash.replace(/\/$/, ''))).toBeDefined()
  })

  it('should accept a directory as a URL', async () => {
    const core = await loadCore(stagingDir)
    expect(core.compile([{ path: 'page.lem', text: 'hi' }]).diagnostics).toEqual([])
  })

  it('should say how to stage the artifact when it is not there', async () => {
    await expect(loadCore('/no/such/dir')).rejects.toThrow(/npm run rust:wasm/)
  })

  it('should let a failure inside the glue module surface as itself', async () => {
    // Advice to stage the artifact would be the wrong diagnosis: it is staged, it just
    // does not load.
    const dir = await mkdtemp(join(tmpdir(), 'lemmon-glue-'))
    await writeFile(join(dir, 'lemmon-core.js'), 'throw new Error("glue is broken")\n')
    await writeFile(join(dir, 'lemmon-core_bg.wasm'), '')
    await expect(loadCore(dir)).rejects.toThrow(/glue is broken/)
  })
})

describe('createCore()', () => {
  it('should give every call its own resident core over one instantiation', async () => {
    const glue = (await import(new URL('lemmon-core.js', stagingDir).href)) as WasmBoundary
    const bytes = await readFile(new URL('lemmon-core_bg.wasm', stagingDir))
    const first = createCore(glue, bytes)
    const second = createCore(glue, bytes)
    first.compile([{ path: 'page.lem', text: 'hi' }])
    expect(decoder.decode(first.render({ path: 'page.lem' }).html!)).toBe('hi')
    // The second core shares the instantiated module but none of the resident state.
    expect(() => second.render({ path: 'page.lem' })).toThrow(/`compile` first/)
    second.free()
    first.free()
  })

  it("should name the core by this package's version, not the one its glue reports", () => {
    // A stand-in glue, so that the version it reports differs from the manifest's: the
    // real one reports the crates' 0.0.0, which this checkout's manifest also says.
    const glue = {
      initSync() {},
      version: () => '9.9.9-the-crates',
      Core: class {},
    } as unknown as WasmBoundary
    const manifest = JSON.parse(
      readFileSync(new URL('../../package.json', import.meta.url), 'utf8')
    ) as { version: string }
    expect(createCore(glue, new Uint8Array()).version).toBe(manifest.version)
  })
})

describe('packageVersion()', () => {
  it('should read the version of the nearest package.json above a module', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'lemmon-manifest-'))
    await writeFile(join(dir, 'package.json'), '{ "version": "1.2.3" }\n')
    await mkdir(join(dir, 'dist', 'esm', 'lib'), { recursive: true })
    const module = pathToFileURL(join(dir, 'dist', 'esm', 'lib', 'load.js'))
    expect(packageVersion(module)).toBe('1.2.3')
  })

  it('should say where it looked when no package.json is above it', () => {
    expect(() => packageVersion(new URL('file:///nowhere/at/all.js'))).toThrow(
      'No package.json at or above file:///nowhere/at/all.js'
    )
  })
})

describe('createCore(), in an instance a call has trapped', () => {
  // A trap poisons the instance the glue module holds; re-evaluating the module gives
  // whatever runs next a fresh one.
  afterEach(() => {
    vi.resetModules()
  })

  it('should construct no core, leaving the collector none to free in it', async () => {
    vi.resetModules()
    const glue = (await import(new URL('lemmon-core.js', stagingDir).href)) as WasmBoundary
    const bytes = await readFile(new URL('lemmon-core_bg.wasm', stagingDir))
    const core = createCore(glue, bytes)
    // An operator chain long enough to overflow the `.wasm` stack (∆ugN).
    const deep = [{ path: 'deep.lem', text: `<lem.${'1 + '.repeat(2999)}1>\n` }]
    expect(() => core.compile(deep)).toThrow(WebAssembly.RuntimeError)
    // The call that trapped still holds the core; freeing it is refused, after the glue
    // has taken it off its finalization registry.
    expect(() => {
      core.free()
    }).toThrow(/borrowed/)

    expect(() => createCore(glue, bytes)).toThrow(WebAssembly.RuntimeError)
    // A core constructed before the trap would be freed now, in the trapped instance,
    // and what that throws would fail the run.
    setFlagsFromString('--expose-gc')
    const gc = runInNewContext('gc') as () => void
    for (let i = 0; i < 3; i++) {
      gc()
      await sleep(20)
    }
  })
})

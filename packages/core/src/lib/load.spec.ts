import { mkdtemp, readFile, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

import type { WasmBoundary } from './boundary.ts'
import { createCore, loadCore, stagingDirectory } from './load.ts'

const stagingDir = new URL('../../../../artifacts/wasm/', import.meta.url)

describe('stagingDirectory()', () => {
  it('should find the staged artifact by walking up from a module', () => {
    expect(stagingDirectory().href).toBe(stagingDir.href)
  })

  it('should find it from the built layout as well as the source one', () => {
    // The bug this guards: counting `../`s from src/lib/ sends the built
    // dist/esm/lib/ copy looking under packages/ instead of the repo root.
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
    expect(core.compile([{ path: 'page.lem', text: 'hi' }])).toEqual([])
    expect(decoder.decode(core.render({ path: 'page.lem' }).html!)).toBe('hi')
  })

  it('should accept a directory as a path string, trailing slash or not', async () => {
    const withSlash = fileURLToPath(stagingDir)
    expect(await loadCore(withSlash)).toBeDefined()
    expect(await loadCore(withSlash.replace(/\/$/, ''))).toBeDefined()
  })

  it('should accept a directory as a URL', async () => {
    const core = await loadCore(stagingDir)
    expect(core.compile([{ path: 'page.lem', text: 'hi' }])).toEqual([])
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
})

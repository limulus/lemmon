import { existsSync, readFileSync } from 'node:fs'
import { readFile } from 'node:fs/promises'
import { pathToFileURL } from 'node:url'

import type { WasmBoundary } from './boundary.ts'
import { Core } from './core.ts'

// The staged artifact set's names — the wasm-bindgen `--out-name` convention that
// `scripts/stage-wasm.ts` produces. Spelled here rather than imported from
// `scripts/lib/wasm-artifact.ts` because package sources cannot reach outside their
// package's `rootDir`.
const GLUE_FILENAME = 'lemmon-core.js'
const WASM_FILENAME = 'lemmon-core_bg.wasm'

const STAGING_SUBPATH = 'artifacts/wasm/'

/**
 * The staging directory {@link loadCore} reads by default: the nearest `artifacts/wasm/`
 * at or above `from` that holds a staged artifact — this package's own, which it
 * publishes — and otherwise the nearest place one would go, so a failure can name where
 * it looked.
 *
 * Searched rather than counted in `../`s because this module runs from two depths:
 * `src/lib/` under vitest and `dist/esm/lib/` once built.
 */
export function stagingDirectory(from: URL = new URL(import.meta.url)): URL {
  const nearest = new URL(STAGING_SUBPATH, new URL('./', from))
  let dir = new URL('./', from)
  for (;;) {
    const candidate = new URL(STAGING_SUBPATH, dir)
    if (existsSync(new URL(GLUE_FILENAME, candidate))) return candidate
    const parent = new URL('../', dir)
    if (parent.href === dir.href) return nearest
    dir = parent
  }
}

/**
 * The version in the nearest `package.json` at or above `from`: this package's own,
 * which a release sets before it publishes, so a published core names the release it
 * shipped in. The `.wasm` cannot know it, since the release picks it after the `.wasm`
 * is built and tested.
 *
 * Searched rather than counted in `../`s for the reason {@link stagingDirectory} is.
 */
export function packageVersion(from: URL = new URL(import.meta.url)): string {
  let dir = new URL('./', from)
  while (!existsSync(new URL('package.json', dir))) {
    const parent = new URL('../', dir)
    if (parent.href === dir.href)
      throw new Error(`No package.json at or above ${from.href}`)
    dir = parent
  }
  const manifest = JSON.parse(readFileSync(new URL('package.json', dir), 'utf8')) as {
    version: string
  }
  return manifest.version
}

/**
 * Instantiates the glue over the given bytes (or compiled module) and returns a fresh
 * {@link Core}. Instantiation is per glue module and idempotent, so every call after
 * the first reuses the instance and just constructs another resident core.
 */
export function createCore(
  glue: WasmBoundary,
  wasm: BufferSource | WebAssembly.Module
): Core {
  glue.initSync({ module: wasm })
  // Asked before a core is constructed, as the cheapest call there is: in an instance an
  // earlier call trapped, it is the first to trap again, and a core constructed before it
  // would be left for the glue's finalizer to free in that instance, a throw no caller
  // could catch. What it answers is the crates' version, which no release changes.
  glue.version()
  return new Core(new glue.Core(), packageVersion())
}

/**
 * Loads the staged artifact set from `dir` — found by walking up from this module
 * unless given — and returns a fresh {@link Core}.
 */
export async function loadCore(dir?: URL | string): Promise<Core> {
  const base = resolveDir(dir)
  let glue: WasmBoundary
  let wasm: Buffer
  try {
    glue = (await import(new URL(GLUE_FILENAME, base).href)) as WasmBoundary
    wasm = await readFile(new URL(WASM_FILENAME, base))
  } catch (cause) {
    // Only a *missing* artifact gets the staging advice. Anything else — a glue module
    // that throws as it evaluates, a runtime that rejects the module — is a different
    // problem, and saying "run rust:wasm" about it would send the reader the wrong way.
    if (!isMissing(cause)) throw cause
    throw missingArtifact(base.href, cause)
  }
  return createCore(glue, wasm)
}

function resolveDir(dir: URL | string | undefined): URL {
  if (dir === undefined) return stagingDirectory()
  if (typeof dir === 'string') return pathToFileURL(dir.endsWith('/') ? dir : `${dir}/`)
  return dir
}

function isMissing(cause: unknown): boolean {
  const code = (cause as NodeJS.ErrnoException | undefined)?.code
  return code === 'ENOENT' || code === 'ERR_MODULE_NOT_FOUND'
}

function missingArtifact(where: string, cause?: unknown): Error {
  return new Error(
    `No staged WASM artifact under ${where} — run \`npm run rust:wasm\` first.`,
    { cause }
  )
}

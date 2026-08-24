import { existsSync } from 'node:fs'
import { readFile } from 'node:fs/promises'
import { pathToFileURL } from 'node:url'

import type { WasmBoundary } from './boundary.ts'
import { Core } from './core.ts'

// The staged artifact set's names — the wasm-bindgen `--out-name` convention that
// `scripts/stage-wasm.ts` produces. Spelled here rather than imported from
// `scripts/wasm-artifact.ts` because package sources cannot reach outside their
// package's `rootDir`.
const GLUE_FILENAME = 'lemmon-core.js'
const WASM_FILENAME = 'lemmon-core_bg.wasm'

const STAGING_SUBPATH = 'artifacts/wasm/'

/**
 * The staging directory {@link loadCore} reads by default: the nearest `artifacts/wasm/`
 * at or above `from` that holds a staged artifact — the repo root's, in a checkout —
 * and otherwise the nearest place one would go, so a failure can name where it looked.
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
 * Instantiates the glue over the given bytes (or compiled module) and returns a fresh
 * {@link Core}. Instantiation is per glue module and idempotent, so every call after
 * the first reuses the instance and just constructs another resident core.
 */
export function createCore(
  glue: WasmBoundary,
  wasm: BufferSource | WebAssembly.Module
): Core {
  glue.initSync({ module: wasm })
  return new Core(new glue.Core())
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

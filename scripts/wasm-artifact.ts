import { join } from 'node:path'

/**
 * Where the staged `.wasm` lives: built once by the `wasm` CI job, uploaded, and
 * downloaded by every job that consumes it. Provisional — `∆TNT` decides where the
 * artifact finally lands inside `@limulus/lemmon-core` and what it is called there.
 */
export const STAGING_DIR = 'artifacts/wasm'

/**
 * Cargo's own output name for the `lemmon-wasm` cdylib, kept as-is to avoid inventing a
 * naming convention `∆TNT` is the one to settle.
 */
export const WASM_FILENAME = 'lemmon_wasm.wasm'

export const STAGED_WASM = join(STAGING_DIR, WASM_FILENAME)

/** Every function the boundary is expected to expose to the shell. */
export const EXPECTED_EXPORTS = ['version', 'describe_error']

/**
 * wasm-bindgen suffixes generated import shims with a 16-hex-digit hash that moves with
 * the crate version, so names are compared with it stripped.
 */
const SHIM_HASH = /_[0-9a-f]{16}$/

export const withoutShimHash = (name: string): string => name.replace(SHIM_HASH, '')

/**
 * The only imports a pure core may reference, by hash-stripped name.
 *
 * Deliberately checked by *name*, not by import module: on `wasm32-unknown-unknown` there
 * is no WASI module to catch, and every host capability reached through `js-sys`/`web-sys`
 * arrives as one more name under `__wbindgen_placeholder__`. Checking modules would let
 * `js_sys::Date::now()` — a clock, exactly what invariant 1 forbids — through untouched;
 * it shows up here as `__wbg_now`.
 *
 * The list fails closed: a name that is not here is either a new wasm-bindgen runtime
 * internal (verify that is what it is, then add it) or a host capability that must be
 * removed. `∆TNT` will need to revisit this once `wasm-bindgen-cli` runs, since the CLI
 * rewrites the placeholder imports into its own JS shim.
 */
export const ALLOWED_IMPORT_NAMES = [
  '__wbindgen_describe',
  '__wbindgen_externref_table_set_null',
  '__wbindgen_externref_table_grow',
  // wasm-bindgen's panic path. Throwing is not a host capability.
  '__wbg___wbindgen_throw',
]

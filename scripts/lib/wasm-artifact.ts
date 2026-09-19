/**
 * The staged wasm artifact's names — where it lives, what it must export, and which
 * imports a pure core may reference (invariant 1) — with the two predicates that turn
 * a module's actual names into `npm run wasm:check`'s verdict. The script around them
 * is I/O: read the bytes, compile, print. The deciding is here, where a spec can feed
 * it a violation.
 */

import { join } from 'node:path'

/**
 * Where the staged wasm-bindgen output lives: built once by the `wasm` CI job,
 * uploaded, and downloaded by every job that consumes it. Locally, `npm run rust:wasm`
 * populates it and everything wasm-consuming (vitest, the bench harness) reads from it.
 * The set is `wasm-bindgen --target web` output — environment-agnostic ESM glue whose
 * caller supplies the bytes (`initSync`) — so the same files serve Node today and a
 * browser playground later. A home inside the published `@limulus/lemmon-core` package
 * waits until something publishes.
 */
export const STAGING_DIR = 'artifacts/wasm'

/** The `--out-name` given to wasm-bindgen: the artifact set's shared basename. */
export const OUT_NAME = 'lemmon-core'

/** The processed `.wasm`, named by wasm-bindgen's `_bg` convention. */
export const WASM_FILENAME = `${OUT_NAME}_bg.wasm`

/** The ESM glue module consumers import: `initSync({ module })` plus the `Core` class. */
export const GLUE_FILENAME = `${OUT_NAME}.js`

export const STAGED_WASM = join(STAGING_DIR, WASM_FILENAME)
export const STAGED_GLUE = join(STAGING_DIR, GLUE_FILENAME)

/**
 * Every export the boundary is expected to expose — the `Core` class surface as
 * wasm-bindgen names it, plus the runtime internals its glue calls. Regenerated from
 * the actual module whenever the surface changes; `npm run wasm:check` fails on a
 * missing name.
 */
export const EXPECTED_EXPORTS = [
  'version',
  'core_new',
  'core_compile',
  'core_loadData',
  'core_plan',
  'core_render',
  '__wbg_core_free',
]

/**
 * Which expected exports `names` — a module's actual export surface — does not carry,
 * in {@link EXPECTED_EXPORTS} order. Names beyond the expected ones are not a
 * violation: the module also carries wasm-bindgen's allocator and table entry points,
 * which nothing here needs to enumerate.
 */
export const missingExports = (names: string[]): string[] => {
  const present = new Set(names)
  return EXPECTED_EXPORTS.filter((name) => !present.has(name))
}

/**
 * wasm-bindgen suffixes generated import shims with a 16-hex-digit hash that moves with
 * the crate version, so names are compared with it stripped.
 */
const SHIM_HASH = /_[0-9a-f]{16}$/

const withoutShimHash = (name: string): string => name.replace(SHIM_HASH, '')

/**
 * The only imports a pure core may reference, by hash-stripped name.
 *
 * Deliberately checked by *name*, not by import module: on `wasm32-unknown-unknown`
 * there is no WASI module to catch, and every host capability reached through
 * `js-sys`/`web-sys` arrives as one more named shim from the glue. Checking modules
 * would let `js_sys::Date::now()` — a clock, exactly what invariant 1 forbids — through
 * untouched; it would show up here as `__wbg_now`.
 *
 * The list fails closed: a name that is not here is either a new wasm-bindgen runtime
 * internal (verify that is what it is, then add it) or a host capability that must be
 * removed. Since `wasm-bindgen-cli` runs in staging, these are the names of the
 * `./lemmon-core_bg.js` glue's own runtime functions — each one verified against the
 * generated glue, none reaching outside it.
 */
const ALLOWED_IMPORT_NAMES = [
  // The error path: `JsError::new` builds the JS `Error` a shim throws. Neither is a
  // host capability.
  '__wbg_Error',
  '__wbg___wbindgen_throw',
  // Glue-internal externref table setup, run once at `initSync`.
  '__wbindgen_init_externref_table',
]

/**
 * Which of `names` — a module's actual import names — are not allowlisted, hash
 * stripped and de-duplicated in first-seen order, so two shims for the same function
 * are reported once. A non-empty result is the violation `npm run wasm:check` refuses
 * on: either a host capability reached invariant 1's pure core, or wasm-bindgen added
 * a runtime internal.
 *
 * A name whose suffix the stripping does not recognize is reported whole. That is the
 * intended failure: if wasm-bindgen's hash format moves, every shim name stops
 * matching the allowlist and the check fails closed rather than passing blind.
 */
export const unexpectedImports = (names: string[]): string[] => [
  ...new Set(
    names
      .map((name) => withoutShimHash(name))
      .filter((name) => !ALLOWED_IMPORT_NAMES.includes(name))
  ),
]

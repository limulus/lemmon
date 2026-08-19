import { readFile } from 'node:fs/promises'

import {
  ALLOWED_IMPORT_NAMES,
  EXPECTED_EXPORTS,
  STAGED_WASM,
  withoutShimHash,
} from './wasm-artifact.ts'

// Runs against the artifact the `wasm` job built, on every Node version in the matrix.
// Building the `.wasm` is Node-independent, but *accepting* it is not: which WebAssembly
// proposals a runtime enables moves between Node majors, so this is where a core that
// outran its runtimes would be caught.
const bytes = await readFile(STAGED_WASM)
const module = await WebAssembly.compile(bytes)

const exports = new Set(WebAssembly.Module.exports(module).map(({ name }) => name))
const missing = EXPECTED_EXPORTS.filter((name) => !exports.has(name))
if (missing.length > 0) {
  throw new Error(`${STAGED_WASM} is missing expected exports: ${missing.join(', ')}`)
}

const unexpected = [
  ...new Set(
    WebAssembly.Module.imports(module)
      .map(({ name }) => withoutShimHash(name))
      .filter((name) => !ALLOWED_IMPORT_NAMES.includes(name))
  ),
]
if (unexpected.length > 0) {
  throw new Error(
    `${STAGED_WASM} imports ${unexpected.join(', ')}, which is not on the allowlist in ` +
      `scripts/wasm-artifact.ts. Either a dependency reached for a host capability — a ` +
      `clock, filesystem, network, or RNG, which invariant 1 forbids — or wasm-bindgen ` +
      `added a runtime internal. Confirm which before allowlisting it.`
  )
}

console.log(
  `${STAGED_WASM} (${bytes.length} bytes) compiles on node ${process.version}; ` +
    `exports ${EXPECTED_EXPORTS.join(', ')}; imports nothing outside wasm-bindgen`
)

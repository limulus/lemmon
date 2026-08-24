import { execFileSync } from 'node:child_process'
import { existsSync, mkdirSync, readdirSync, rmSync, statSync } from 'node:fs'
import { join } from 'node:path'

import { OUT_NAME, STAGED_WASM, STAGING_DIR } from './wasm-artifact.ts'

const TARGET = 'wasm32-unknown-unknown'
const PROFILE = 'release'
// Cargo's own output name for the `lemmon-wasm` cdylib, before wasm-bindgen renames it.
const CDYLIB = 'lemmon_wasm.wasm'

interface CargoMetadata {
  target_directory: string
  packages: { dependencies: { name: string; req: string }[] }[]
}

// Ask cargo rather than assuming `./target`: CARGO_TARGET_DIR is commonly redirected.
const metadata = execFileSync('cargo', ['metadata', '--format-version', '1', '--no-deps'], {
  encoding: 'utf8',
})
const { target_directory: targetDirectory, packages } = JSON.parse(
  metadata
) as CargoMetadata

// The CLI must match the `wasm-bindgen` crate — it refuses to run on a skew — so the
// install hint reads the pin rather than repeating it, and cannot go stale when the
// workspace bumps.
const bindgenVersion = packages
  .flatMap((pkg) => pkg.dependencies)
  .find((dep) => dep.name === 'wasm-bindgen')
  ?.req.replace(/^[^0-9]*/, '')

const built = join(targetDirectory, TARGET, PROFILE, CDYLIB)
if (!existsSync(built)) {
  throw new Error(
    `No ${PROFILE} ${TARGET} build at ${built} — run \`npm run rust:wasm\`, which builds ` +
      `before it stages.`
  )
}

// This script owns clearing the staging dir. Doing it here rather than with `del-cli` in
// `rust:wasm` is what keeps the CI wasm job free of `npm ci` — it needs a cargo
// toolchain, the wasm-bindgen CLI, and a new enough Node, nothing from node_modules.
rmSync(STAGING_DIR, { force: true, recursive: true })
mkdirSync(STAGING_DIR, { recursive: true })

// `--target web` produces environment-agnostic ESM glue: the caller supplies the bytes
// (`initSync`), which keeps loading injectable for tests and reusable in a browser.
try {
  execFileSync(
    'wasm-bindgen',
    ['--target', 'web', '--out-dir', STAGING_DIR, '--out-name', OUT_NAME, built],
    { stdio: 'inherit' }
  )
} catch (error) {
  if ((error as NodeJS.ErrnoException).code === 'ENOENT') {
    throw new Error(
      `wasm-bindgen is not on PATH — install it with ` +
        `\`cargo install wasm-bindgen-cli --version ${bindgenVersion} --locked\``
    )
  }
  throw error
}

const staged = readdirSync(STAGING_DIR).sort().join(', ')
console.log(
  `staged ${staged} in ${STAGING_DIR}/ ` +
    `(${statSync(STAGED_WASM).size} byte .wasm) from ${built}`
)

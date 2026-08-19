import { execFileSync } from 'node:child_process'
import { copyFileSync, existsSync, mkdirSync, rmSync, statSync } from 'node:fs'
import { join } from 'node:path'

import { STAGED_WASM, STAGING_DIR, WASM_FILENAME } from './wasm-artifact.ts'

const TARGET = 'wasm32-unknown-unknown'
const PROFILE = 'release'

// Ask cargo rather than assuming `./target`: CARGO_TARGET_DIR is commonly redirected.
const metadata = execFileSync('cargo', ['metadata', '--format-version', '1', '--no-deps'], {
  encoding: 'utf8',
})
const targetDirectory = (JSON.parse(metadata) as { target_directory: string })
  .target_directory

const built = join(targetDirectory, TARGET, PROFILE, WASM_FILENAME)
if (!existsSync(built)) {
  throw new Error(
    `No ${PROFILE} ${TARGET} build at ${built} — run \`npm run rust:wasm\`, which builds ` +
      `before it stages.`
  )
}

rmSync(STAGING_DIR, { force: true, recursive: true })
mkdirSync(STAGING_DIR, { recursive: true })
copyFileSync(built, STAGED_WASM)

console.log(`staged ${STAGED_WASM} (${statSync(STAGED_WASM).size} bytes) from ${built}`)

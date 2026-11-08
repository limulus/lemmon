import { execFileSync, spawnSync } from 'node:child_process'
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'

import {
  builtPageFaults,
  packFaults,
  SITE,
  staleLicenses,
  type Packed,
} from './lib/packed.ts'

// Packs both packages as `npm publish` would, installs the tarballs into an empty project
// outside this repository, and builds a site there with the installed `lem`: what a site
// installing the published packages does, short of the registry. It needs `npm run build`
// and a staged `.wasm` first, and the network for the third-party dependencies.

const CORE = 'packages/core'
const SHELL = 'packages/shell'

// npm hands a script its own configuration as `npm_*` variables; the npm run here acts
// on other directories, so none of that is for it.
const env = Object.fromEntries(
  Object.entries(process.env).filter(([name]) => !name.toLowerCase().startsWith('npm_'))
)

function fail(faults: string[], kept?: string): never {
  for (const fault of faults) console.error(`test:pack: ${fault}`)
  if (kept !== undefined) console.error(`test:pack: left ${kept} as it was`)
  process.exit(1)
}

const read = (path: string) => readFileSync(path, 'utf8')
const shellManifest = JSON.parse(read(join(SHELL, 'package.json'))) as {
  dependencies: Record<string, string>
}

const licenses = staleLicenses(read('LICENSE'), {
  '@limulus/lemmon-core': read(join(CORE, 'LICENSE')),
  '@limulus/lemmon-shell': read(join(SHELL, 'LICENSE')),
})
if (licenses.length > 0) fail(licenses)

const dir = mkdtempSync(join(tmpdir(), 'lemmon-pack-'))
const packed = JSON.parse(
  execFileSync(
    'npm',
    [
      'pack',
      '--json',
      '--pack-destination',
      dir,
      '--workspace',
      CORE,
      '--workspace',
      SHELL,
    ],
    { encoding: 'utf8', env }
  )
) as (Packed & { filename: string; size: number; entryCount: number })[]
const faults = packFaults(packed, shellManifest.dependencies['@limulus/lemmon-core'])
if (faults.length > 0) fail(faults, dir)

const site = join(dir, 'site')
for (const [path, text] of Object.entries(SITE)) {
  mkdirSync(dirname(join(site, path)), { recursive: true })
  writeFileSync(join(site, path), text)
}
const tarballs = packed.map(({ filename }) => join(dir, filename))
const installed = spawnSync('npm', ['install', '--no-audit', '--no-fund', ...tarballs], {
  cwd: site,
  encoding: 'utf8',
  env,
})
if (installed.status !== 0) {
  fail([`npm install exited ${installed.status}:\n${installed.stderr}`], dir)
}

const built = spawnSync(join(site, 'node_modules', '.bin', 'lem'), ['build'], {
  cwd: site,
  encoding: 'utf8',
  env,
})
if (built.status !== 0) {
  fail([`lem build exited ${built.status}:\n${built.stdout}${built.stderr}`], dir)
}
const version = packed[0].version
const page = builtPageFaults(read(join(site, 'out', 'index.html')), version)
if (page.length > 0) fail(page, dir)

rmSync(dir, { force: true, recursive: true })
const sizes = packed.map(
  ({ name, entryCount, size }) =>
    `${name} (${entryCount} files, ${Math.round(size / 1024)} kB)`
)
console.log(
  `test:pack: packed ${sizes.join(' and ')} at ${version}, installed both into an empty ` +
    `project, and built a page there with the installed lem`
)

/**
 * What the published packages must hold for an installed copy to run, with the
 * predicates that turn `npm pack`'s listing, the manifests, and the page an installed
 * `lem` built into `npm run test:pack`'s verdict. The script around them is I/O: pack,
 * install, build. The deciding is here, where a spec can feed it a violation.
 */

/** A package as `npm pack --json` lists it. */
export interface Packed {
  name: string
  version: string
  files: { path: string }[]
}

/**
 * Each package's files without which an installed copy cannot run: the build its
 * `exports` and `bin` name, the `.wasm` the core's loader looks for beside its own
 * directory, the shell's ambient declarations, and the license npm packs only from a
 * package's own directory.
 */
export const REQUIRED_FILES: Readonly<Record<string, readonly string[]>> = {
  '@limulus/lemmon-core': [
    'LICENSE',
    'package.json',
    'dist/esm/index.js',
    'dist/types/index.d.ts',
    'artifacts/wasm/lemmon-core.js',
    'artifacts/wasm/lemmon-core_bg.wasm',
  ],
  '@limulus/lemmon-shell': [
    'LICENSE',
    'package.json',
    'ambient.lem',
    'bin/lem.js',
    'dist/esm/index.js',
    'dist/esm/lib/cli.js',
    'dist/types/index.d.ts',
  ],
}

/**
 * What keeps the packed set from shipping, one sentence each; none when it can. The
 * packages release in lockstep, so they carry one version, and the shell depends on
 * exactly that core.
 */
export function packFaults(packed: readonly Packed[], shellNeedsCore: string): string[] {
  const faults: string[] = []
  for (const [name, required] of Object.entries(REQUIRED_FILES)) {
    const pack = packed.find((candidate) => candidate.name === name)
    if (pack === undefined) {
      faults.push(`${name} was not packed`)
      continue
    }
    const paths = new Set(pack.files.map(({ path }) => path))
    for (const path of required) {
      if (!paths.has(path)) faults.push(`${name} is missing ${path}`)
    }
  }
  const versions = [...new Set(packed.map(({ version }) => version))]
  if (versions.length > 1) {
    faults.push(`the packages are at ${versions.join(' and ')}, not one version`)
  }
  const core = packed.find(({ name }) => name === '@limulus/lemmon-core')
  if (core !== undefined && shellNeedsCore !== core.version) {
    faults.push(
      `the shell depends on @limulus/lemmon-core ${shellNeedsCore}, ` +
        `but the core is at ${core.version}`
    )
  }
  return faults
}

/** Each package whose `LICENSE` is not the repository's, by name. */
export function staleLicenses(
  root: string,
  copies: Readonly<Record<string, string>>
): string[] {
  return Object.entries(copies)
    .filter(([, text]) => text !== root)
    .map(([name]) => `${name}'s LICENSE is not the repository's`)
}

/**
 * The page the check's site builds, as an installed `lem` must write it: its body,
 * the highlighting the config's plugin adds, and the version the build names itself
 * with, which must be the release's rather than the crates' `0.0.0`.
 */
export function builtPageFaults(html: string, version: string): string[] {
  const faults: string[] = []
  const generator = `<meta name="generator" content="Lemmon ${version}">`
  if (!html.includes(generator)) faults.push(`the page does not hold ${generator}`)
  if (!html.includes('>Installed</h1>')) faults.push('the page lost its heading')
  if (!html.includes('<code class="hljs">')) faults.push('the code block is unhighlighted')
  return faults
}

/**
 * The site the check builds with the installed packages: a config importing a plugin
 * from the shell, as a site's does, a page type whose layout prints the generator, and
 * one document with a code block for that plugin.
 */
export const SITE: Readonly<Record<string, string>> = {
  'package.json': '{ "private": true, "type": "module" }\n',
  'lemmon.config.ts': `import { highlighter } from '@limulus/lemmon-shell'

export default { input: 'www', output: 'out', plugins: [highlighter()] }
`,
  'www/site.lem': `<lem-type Note extends Page>
  layout: (Note, html): html = note
</lem-type>

<lem-fn note(n: Note, body: html)>
  <html><head><meta name="generator" content="<lem.build.generator>"></head>
  <body><lem-if n.draft>(a draft) </lem-if><lem.body></body></html>
</lem-fn>

<lem-let notes: [Note] from "**/*.md" />
`,
  'www/index.md': '# Installed\n\n```js\nconst answer = 42\n```\n',
}

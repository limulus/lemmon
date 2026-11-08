/**
 * The decisions `npm run test:pack` makes, fed listings shaped as `npm pack --json`
 * writes them, each with one fault planted: a file knocked out, a package missing, the
 * versions apart, the shell pinned to another core. The required lists appear only as
 * inputs, since comparing one against a copy of itself would pass forever.
 */

import { describe, expect, it } from 'vitest'

import {
  builtPageFaults,
  packFaults,
  REQUIRED_FILES,
  staleLicenses,
  type Packed,
} from './packed.ts'

/** Each package packed at `version` with every file it needs, and one more. */
function packedAt(version: string): Packed[] {
  return Object.entries(REQUIRED_FILES).map(([name, paths]) => ({
    name,
    version,
    files: [...paths, 'README.md'].map((path) => ({ path })),
  }))
}

describe('packFaults()', () => {
  it('should find nothing wrong with a complete set at one version', () => {
    expect(packFaults(packedAt('1.2.3'), '1.2.3')).toEqual([])
  })

  it('should name a required file a package lacks', () => {
    const packed = packedAt('1.2.3')
    const core = packed.find(({ name }) => name === '@limulus/lemmon-core')!
    core.files = core.files.filter(({ path }) => !path.endsWith('.wasm'))
    expect(packFaults(packed, '1.2.3')).toEqual([
      '@limulus/lemmon-core is missing artifacts/wasm/lemmon-core_bg.wasm',
    ])
  })

  it('should name a package that was not packed', () => {
    const packed = packedAt('1.2.3').filter(({ name }) => name !== '@limulus/lemmon-shell')
    expect(packFaults(packed, '1.2.3')).toEqual(['@limulus/lemmon-shell was not packed'])
  })

  it('should refuse packages at two versions', () => {
    const [core, shell] = packedAt('1.2.3')
    expect(packFaults([core, { ...shell, version: '1.2.4' }], '1.2.3')).toEqual([
      'the packages are at 1.2.3 and 1.2.4, not one version',
    ])
  })

  it('should refuse a shell that depends on another core than the one packed', () => {
    expect(packFaults(packedAt('1.2.3'), '0.0.0')).toEqual([
      'the shell depends on @limulus/lemmon-core 0.0.0, but the core is at 1.2.3',
    ])
  })

  it('should not ask what the shell depends on when no core was packed', () => {
    const packed = packedAt('1.2.3').filter(({ name }) => name !== '@limulus/lemmon-core')
    expect(packFaults(packed, '0.0.0')).toEqual(['@limulus/lemmon-core was not packed'])
  })
})

describe('staleLicenses()', () => {
  it('should pass copies that match the repository’s', () => {
    expect(staleLicenses('MIT\n', { core: 'MIT\n', shell: 'MIT\n' })).toEqual([])
  })

  it('should name a copy that has drifted', () => {
    expect(
      staleLicenses('MIT 2027\n', { core: 'MIT 2027\n', shell: 'MIT 2026\n' })
    ).toEqual(["shell's LICENSE is not the repository's"])
  })
})

describe('builtPageFaults()', () => {
  const page =
    '<html><head><meta name="generator" content="Lemmon 1.2.3"></head>\n' +
    '<body><h1 id="installed">Installed</h1>\n<pre><code class="hljs">const answer = 42</code></pre>' +
    '</body></html>'

  it('should pass the page an installed lem writes', () => {
    expect(builtPageFaults(page, '1.2.3')).toEqual([])
  })

  it('should refuse a page named for another version, as the crates’ 0.0.0 would be', () => {
    expect(builtPageFaults(page.replace('1.2.3', '0.0.0'), '1.2.3')).toEqual([
      'the page does not hold <meta name="generator" content="Lemmon 1.2.3">',
    ])
  })

  it('should refuse a page whose body or highlighting went missing', () => {
    expect(
      builtPageFaults('<meta name="generator" content="Lemmon 1.2.3">', '1.2.3')
    ).toEqual(['the page lost its heading', 'the code block is unhighlighted'])
  })
})

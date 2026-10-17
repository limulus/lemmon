import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'

import { outputFile, write } from './write.ts'
import { tree } from '../mocks/sites.ts'
import { tempDir } from '../mocks/temp-dir.ts'

describe('outputFile()', () => {
  it('should write a url ending in `/` as its directory’s `index.html`', () => {
    expect(outputFile('/')).toBe('index.html')
    expect(outputFile('/feed/the-rebound/')).toBe('feed/the-rebound/index.html')
  })

  it('should write any other url as the file it names', () => {
    expect(outputFile('/feed.xml')).toBe('feed.xml')
    expect(outputFile('/feed/x/index.html')).toBe('feed/x/index.html')
  })

  it('should name the file by what each segment’s percent-encoding stands for', () => {
    expect(outputFile('/notes/a%20b/')).toBe('notes/a b/index.html')
    expect(outputFile('/caf%C3%A9.html')).toBe('café.html')
  })

  it('should name no file for a url that does not stay inside the output', () => {
    for (const url of [
      'feed/',
      'https://limulus.net/',
      '/feed/?page=2',
      '/feed/#top',
      '//',
      '/a//b/',
      '/../x/',
      '/a/./b',
      '/a/%2E%2E/b',
      '/a%2Fb/',
      '/a%5Cb/',
      '/%E0%A4%A/',
    ]) {
      expect(outputFile(url), url).toBeUndefined()
    }
  })
})

describe('write()', () => {
  it('should write each page’s bytes, creating its directories', async () => {
    const root = tempDir({ 'www/.keep': '' })
    // Bytes the core returned, written without being decoded and encoded again.
    const bytes = new Uint8Array([0x63, 0x61, 0x66, 0xc3, 0xa9, 0xff])

    await write(
      join(root, 'www'),
      join(root, 'dist'),
      [{ file: 'a/b/index.html', bytes }],
      []
    )

    expect([...readFileSync(join(root, 'dist/a/b/index.html'))]).toEqual([...bytes])
  })

  it('should copy each passthrough file to its own path, and leave the rest', async () => {
    const root = tempDir({
      'www/assets/css/site.css': 'main {}\n',
      'www/robots.txt': 'User-agent: *\n',
    })
    mkdirSync(join(root, 'dist'))
    writeFileSync(join(root, 'dist/old.html'), 'old')

    await write(
      join(root, 'www'),
      join(root, 'dist'),
      [],
      ['assets/css/site.css', 'robots.txt']
    )

    expect(tree(join(root, 'dist'))).toEqual({
      'assets/css/site.css': 'main {}\n',
      'robots.txt': 'User-agent: *\n',
      'old.html': 'old',
    })
  })
})

import type { Span } from '@limulus/lemmon-core'
import { copyFile, mkdir, writeFile } from 'node:fs/promises'
import { dirname, join } from 'node:path'

import type { Generated } from './plugins.ts'

/**
 * The file a page at `url` is written to, relative to the output directory and with
 * `/` separators — a url ending in `/` is a directory, written as its `index.html`
 * (`/feed/the-rebound/` → `feed/the-rebound/index.html`, `/` → `index.html`), and any
 * other names its file (`/feed.xml` → `feed.xml`) — or `undefined` where the url names
 * no file inside the output: not a path from `/`, a query or a fragment, a `.` or `..`
 * or empty segment, or a segment whose percent-decoding holds a separator.
 *
 * Each segment is percent-decoded, since a `url` value is encoded — a space as `%20`,
 * non-ASCII as its UTF-8 bytes — and a file is named by what the encoding stands for,
 * as a server maps a request back to one.
 */
export function outputFile(url: string): string | undefined {
  if (!url.startsWith('/') || /[?#]/.test(url)) return undefined
  const segments = url.slice(1).split('/')
  if (url.endsWith('/')) segments[segments.length - 1] = 'index.html'
  const decoded: string[] = []
  for (const segment of segments) {
    let name: string
    try {
      name = decodeURIComponent(segment)
    } catch {
      return undefined
    }
    if (name === '' || name === '.' || name === '..' || /[/\\]/.test(name)) {
      return undefined
    }
    decoded.push(name)
  }
  return decoded.join('/')
}

/**
 * What writes an output file: a page, a passthrough file copied as it is, a plugin's
 * asset hook, or a plugin's producer, for a page.
 */
export interface Writer {
  /**
   * The page's path, or the passthrough file's, relative to the input directory — or the
   * file the producer's mark stands in, or the config file, absolute, which lists the
   * plugin whose asset hook built the file.
   */
  path: string
  /**
   * Where the page's url stands, or the producer's mark; none for a passthrough file or
   * an asset hook's, which are placed at the file's start.
   */
  span?: Span
  /** Which of its generator's pages the page is, where a generator made it (D43). */
  generated?: Generated
  /** The plugin whose asset hook built the file. */
  assets?: { plugin: string }
  /**
   * The plugin whose producer wrote the file, and the page it was written for — which of
   * a generator's pages, where a generator made it, or its body alone, where the body
   * pass rendered it (D46).
   */
  producer?: { plugin: string; page: string; generated?: Generated; body?: boolean }
}

/** An output file another writer has claimed in a way that rules this one out. */
export interface Collision {
  /** The file the earlier writer claimed. */
  file: string
  writer: Writer
  /**
   * How the two collide: the `same` file; files that differ only in case or Unicode
   * form, which a file system may `fold` to one; or a file that would be a directory
   * the other is written `inside`, or that `holds` the other inside it.
   */
  how: 'same' | 'fold' | 'inside' | 'holds'
}

/** One claim on an output file: the file, as its writer named it, and the writer. */
export interface Claim {
  file: string
  writer: Writer
}

/**
 * The claim {@link OutputFiles.find} found for a file, and whether it is on that file
 * `exact`ly, or on one that differs from it only in case or Unicode form.
 */
export interface Found extends Claim {
  exact: boolean
}

/**
 * The output files a build will write, each claimed by its writer before any is
 * written, so that two which cannot both stand on disk are refused while nothing has
 * been written yet.
 *
 * Files are compared folded — NFC, lowercased — because the file system may fold case
 * and Unicode form (APFS does both), and the walk and the globs already match without
 * case: the build refuses two files it could not tell apart rather than leave the last
 * writer to win. A file is also refused where it would be a directory another file is
 * written inside, or would hold one, since no path can be both.
 */
export class OutputFiles {
  /** Each claimed file, by its folded path. */
  readonly #files = new Map<string, Claim>()
  /** Each directory a claimed file is written inside, by its folded path. */
  readonly #directories = new Map<string, Claim>()

  /** Claims `file` for `writer`, or returns the claim it collides with. */
  claim(file: string, writer: Writer): Collision | undefined {
    const folded = file.normalize('NFC').toLowerCase()
    const same = this.#files.get(folded)
    if (same !== undefined) return { ...same, how: same.file === file ? 'same' : 'fold' }
    const held = this.#directories.get(folded)
    if (held !== undefined) return { ...held, how: 'holds' }
    const directories = parents(folded)
    for (const directory of directories) {
      const above = this.#files.get(directory)
      if (above !== undefined) return { ...above, how: 'inside' }
    }
    const claim = { file, writer }
    this.#files.set(folded, claim)
    for (const directory of directories) {
      if (!this.#directories.has(directory)) this.#directories.set(directory, claim)
    }
    return undefined
  }

  /**
   * The claim on `file`, or on the file that differs from it only in case or Unicode
   * form, if any — what a link to `file` reaches (D47). A directory a file is written
   * inside is no claim.
   */
  find(file: string): Found | undefined {
    const claim = this.#files.get(file.normalize('NFC').toLowerCase())
    return claim === undefined ? undefined : { ...claim, exact: claim.file === file }
  }
}

/** Every directory `file` is written inside: `a` and `a/b` for `a/b/c`. */
function parents(file: string): string[] {
  const segments = file.split('/')
  return segments.slice(1).map((_, i) => segments.slice(0, i + 1).join('/'))
}

/**
 * A rendered page, or a file a plugin built — its asset hook or its producer — and the
 * file it is written to.
 */
export interface Output {
  /** Relative to the output directory, with `/` separators. */
  file: string
  /**
   * The url it is served at, as its writer named it: a page's from its record or its
   * path, a plugin's as the plugin wrote it. Its links are resolved against it (D47).
   */
  url: string
  /** The page as the core returned it with its marks resolved, or what was built. */
  bytes: Uint8Array
}

/**
 * A file the file system would not let a build write — one an earlier build left in
 * the way, which nothing removes, or a directory it cannot create — naming the file.
 */
export class WriteError extends Error {
  /** The file, absolute. */
  readonly file: string

  constructor(file: string, cause: Error) {
    super(`could not write ${file}: ${cause.message}`, { cause })
    this.name = 'WriteError'
    this.file = file
  }
}

/**
 * Writes each page to its file under `output`, then each file a plugin `built` — an
 * asset hook's, then a producer's, as the build lists them — and copies each
 * passthrough file — a path relative to `input` — to the same path under `output`,
 * creating directories as needed. The bytes are written as they are, and nothing
 * already in `output` is removed: a file no page, plugin or passthrough names is left
 * where it is.
 *
 * @throws {WriteError} at the first file the file system refuses; the files before it
 *   are written.
 */
export async function write(
  input: string,
  output: string,
  pages: Output[],
  built: Output[],
  passthrough: string[]
): Promise<void> {
  const target = (path: string): string => join(output, ...path.split('/'))
  const attempt = async (file: string, put: () => Promise<void>): Promise<void> => {
    try {
      await mkdir(dirname(file), { recursive: true })
      await put()
    } catch (cause) {
      throw new WriteError(file, cause as Error)
    }
  }
  for (const page of [...pages, ...built]) {
    const file = target(page.file)
    await attempt(file, () => writeFile(file, page.bytes))
  }
  for (const path of passthrough) {
    const file = target(path)
    await attempt(file, () => copyFile(join(input, ...path.split('/')), file))
  }
}

import type { Span } from '@limulus/lemmon-core'
import { copyFile, mkdir, writeFile } from 'node:fs/promises'
import { dirname, join } from 'node:path'

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

/** What writes an output file: a page, or a passthrough file copied as it is. */
export interface Writer {
  /** The page's path, or the passthrough file's, relative to the input directory. */
  path: string
  /** Where the page's url stands; none for a passthrough file. */
  span?: Span
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

/** One claim on an output file. */
interface Claim {
  file: string
  writer: Writer
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
}

/** Every directory `file` is written inside: `a` and `a/b` for `a/b/c`. */
function parents(file: string): string[] {
  const segments = file.split('/')
  return segments.slice(1).map((_, i) => segments.slice(0, i + 1).join('/'))
}

/** A rendered page and the file it is written to. */
export interface Output {
  /** Relative to the output directory, with `/` separators. */
  file: string
  /** The page as the core returned it, with its marks resolved. */
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
 * Writes each page to its file under `output`, and copies each passthrough file —
 * a path relative to `input` — to the same path under `output`, creating directories
 * as needed. The bytes are written as they are, and nothing already in `output` is
 * removed: a file no page or passthrough names is left where it is.
 *
 * @throws {WriteError} at the first file the file system refuses; the files before it
 *   are written.
 */
export async function write(
  input: string,
  output: string,
  pages: Output[],
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
  for (const page of pages) {
    const file = target(page.file)
    await attempt(file, () => writeFile(file, page.bytes))
  }
  for (const path of passthrough) {
    const file = target(path)
    await attempt(file, () => copyFile(join(input, ...path.split('/')), file))
  }
}

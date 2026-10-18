import { readdir, readFile, stat } from 'node:fs/promises'
import { join, relative, sep } from 'node:path'
import picomatch from 'picomatch'

import { contains, type LoadedConfig } from './config.ts'
import { findFrontMatter } from './front-matter.ts'

/** A markdown document (`.md`): always a page, read with its text. */
export interface DocumentEntry {
  kind: 'document'
  path: string
  text: string
}

/**
 * A template (`.lem`), read with its text. One that opens with front matter is a page
 * template (D41); one without holds definitions, or is a file the host reads bindings
 * from.
 */
export interface TemplateEntry {
  kind: 'template'
  path: string
  text: string
  page: boolean
}

/** A file a passthrough glob matched, which a build copies as it is. */
export interface PassthroughEntry {
  kind: 'passthrough'
  path: string
}

/**
 * Anything else — a data file, a script, Eleventy's own templates — which the walk
 * does not read. A data file is read when a `from` binding names it.
 */
export interface IgnoredEntry {
  kind: 'ignored'
  path: string
}

/**
 * One file under the input directory. Its path is relative to that directory, with `/`
 * separators: the path the core knows the file by.
 */
export type Entry = DocumentEntry | TemplateEntry | PassthroughEntry | IgnoredEntry

/** What the walk makes of a file. */
export type EntryKind = Entry['kind']

/** Names the walk passes over, at any depth, as Eleventy ignores them by default. */
const SKIPPED = new Set(['node_modules', '.git'])

/**
 * Whether the walk passes over `path`, an absolute path under the config's input: the
 * output directory and what is in it, where the input holds it, and anything named
 * `node_modules` or `.git` — a worktree's `.git` is a file — and what is in it, at any
 * depth. `lem dev` watches the input through it too, so that a change the walk would
 * not see starts no rebuild.
 */
export function skipped(
  config: Pick<LoadedConfig, 'input' | 'output'>,
  path: string
): boolean {
  if (contains(config.output, path)) return true
  return relative(config.input, path)
    .split(sep)
    .some((name) => SKIPPED.has(name))
}

/**
 * Every file under the config's input directory, sorted by path, each classified: a
 * file a passthrough glob matches is passthrough whatever its extension; otherwise
 * `.md` is a document, `.lem` a template, and anything else is ignored. The walk passes
 * over a `node_modules` or `.git`, and the output directory when it is inside the
 * input, so a build never reads what an earlier one wrote ({@link skipped}). A
 * symbolic link counts as the file it names, and one that names nothing fails the
 * walk; a linked directory is not followed.
 */
export async function walk(
  config: Pick<LoadedConfig, 'input' | 'output' | 'passthrough'>
): Promise<Entry[]> {
  const isPassthrough = picomatch(config.passthrough, { dot: true, nocase: true })
  const files: string[] = []
  const visit = async (dir: string): Promise<void> => {
    for (const dirent of await readdir(dir, { withFileTypes: true })) {
      const file = join(dir, dirent.name)
      if (skipped(config, file)) continue
      if (dirent.isDirectory()) {
        await visit(file)
      } else if (
        dirent.isFile() ||
        (dirent.isSymbolicLink() && (await stat(file)).isFile())
      ) {
        files.push(relative(config.input, file).split(sep).join('/'))
      }
    }
  }
  await visit(config.input)
  files.sort((a, b) => (a < b ? -1 : 1))
  return await Promise.all(
    files.map(async (path): Promise<Entry> => {
      if (isPassthrough(path)) return { kind: 'passthrough', path }
      if (path.endsWith('.md')) return { kind: 'document', path, text: await read(path) }
      if (path.endsWith('.lem')) {
        const text = await read(path)
        return { kind: 'template', path, text, page: findFrontMatter(text) !== undefined }
      }
      return { kind: 'ignored', path }
    })
  )

  function read(path: string): Promise<string> {
    return readFile(join(config.input, ...path.split('/')), 'utf8')
  }
}

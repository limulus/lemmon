import type {
  DataInput,
  Diagnostic,
  FromBinding,
  JsonValue,
  LoadedValue,
} from '@limulus/lemmon-core'
import { readFile } from 'node:fs/promises'
import { join } from 'node:path'

import { contains } from './config.ts'
import { hasError } from './diagnostics.ts'
import type { CompiledPage, Program } from './program.ts'
import { readData } from './yaml-values.ts'

/** What a build knows about itself: the value `build` is bound to. */
export interface BuildInfo {
  /** When the build ran, sent as `Date.toISOString()` writes it: RFC 3339, in UTC. */
  time: Date
  /** What built the site: `Lemmon 0.0.0`. */
  generator: string
}

/** The environment variables a site's `env("…")` bindings read — `process.env`. */
export type Environment = Record<string, string | undefined>

/** What a build hands `loadData`, unless a source could not be fulfilled, and why. */
export interface Envelope {
  data?: DataInput
  diagnostics: Diagnostic[]
}

/**
 * The envelope that loads `program` (step 4 of the build pipeline, D37): `build` as the
 * shell's own value, with no bodies yet — the value the body pass renders under, before
 * {@link withBodies} adds them (D46) — every `from` binding fulfilled by its kind, and in
 * `pages` every page no glob holds whose `type:` named its record — a generator with its
 * `for:` key, which the core expands into its pages (D43).
 *
 * A glob's binding is the front matter of the pages it holds, in path order; a data
 * file is read from the input directory and parsed with positions; an environment
 * value is the variable's text. A data file outside the input or not there, an
 * environment variable that is not set, and a plugin — which nothing runs yet — are
 * errors at the binding, and a data file that does not parse is an error in the file;
 * each leaves no envelope, since the core refuses one that fulfils fewer bindings than
 * the program declares.
 *
 * A data file is read once in a build: its text is recorded in `texts` by the path its
 * diagnostics name it by, its binding's literal, and a path `texts` holds already — a
 * file the walk read, or another binding's — is taken from there, so every diagnostic
 * in one file is placed against one text.
 */
export async function envelope(
  input: string,
  program: Program,
  build: BuildInfo,
  env: Environment,
  texts: Map<string, string>
): Promise<Envelope> {
  const diagnostics: Diagnostic[] = []
  const sources: Record<string, LoadedValue | LoadedValue[]> = {}
  const compiled = [...program.pages.values()]
  for (const source of program.sources) {
    const at = (message: string): void => {
      diagnostics.push({
        path: source.path,
        span: source.span,
        severity: 'error',
        message: `\`${source.name}\` loads ${described(source)}, ${message}`,
        labels: [],
      })
    }
    if (source.kind === 'plugin') {
      at('and plugins are not supported yet (∆wOn)')
    } else if (source.kind === 'env') {
      const value = env[source.literal]
      if (value === undefined) at('which is not set')
      else sources[source.name] = { value }
    } else if (source.collects !== undefined) {
      sources[source.name] = compiled
        .filter((page) => page.binding === source.name)
        .map(entryOf)
    } else {
      const file = join(input, ...source.literal.split('/'))
      // Like a glob, a data file's path is from the input directory, and stays in it.
      if (!contains(input, file)) {
        at('which is outside the input directory')
        continue
      }
      let text = texts.get(source.literal)
      if (text === undefined) {
        try {
          text = await readFile(file, 'utf8')
        } catch (error) {
          const { code, message } = error as NodeJS.ErrnoException
          at(
            code === 'ENOENT'
              ? 'which is not a file in the input directory'
              : `which cannot be read: ${message}`
          )
          continue
        }
        texts.set(source.literal, text)
      }
      const read = readData(source.literal, text)
      diagnostics.push(...read.diagnostics)
      if (read.loaded !== undefined) sources[source.name] = read.loaded
    }
  }
  if (hasError(diagnostics)) return { diagnostics }
  const pages = compiled
    .filter((page) => page.record !== undefined && page.binding === undefined)
    .map(entryOf)
  const ambient = {
    build: { time: build.time.toISOString(), generator: build.generator, bodies: {} },
  }
  return { data: { ambient, sources, pages }, diagnostics }
}

/**
 * `data`, an {@link envelope}, with `bodies` as `build.bodies`: each document's finished
 * body by its page's url, which the second load binds for every page to read (D46).
 */
export function withBodies(data: DataInput, bodies: Record<string, string>): DataInput {
  const build = data.ambient?.build as Record<string, JsonValue>
  return { ...data, ambient: { ...data.ambient, build: { ...build, bodies } } }
}

/**
 * A compiled page's entry in the envelope: its front matter as loaded, with its `for:`
 * key beside it where it has one (D43) — a glob's member too, which the core refuses at
 * the key, since no glob can hold a generator.
 */
function entryOf({ page }: CompiledPage): LoadedValue {
  return page.for === undefined ? page.loaded : { ...page.loaded, for: page.for }
}

/** What a binding loads, as its `from` source is written. */
function described(source: FromBinding): string {
  switch (source.kind) {
    case 'env':
      return `the environment variable \`${source.literal}\``
    case 'plugin':
      return `\`plugin("${source.literal}")\``
    case 'file':
      return `\`${source.literal}\``
  }
}

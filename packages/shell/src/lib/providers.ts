import type { Diagnostic } from '@limulus/lemmon-core'
import { dirname } from 'node:path'

import { isPlainObject, type LoadedConfig } from './config.ts'
import { findFrontMatter, opensFrontMatter, type Block, type Page } from './front-matter.ts'
import {
  encoder,
  failure,
  fields,
  isContent,
  isName,
  settle,
  shown,
  threw,
  type Assets,
  type AssetsContext,
  type Plugin,
} from './plugins.ts'
import { FILE_START, pageSpan } from './program.ts'

/** A document's text as the plugins' transforms left it, and what they did wrong. */
export interface Transformed {
  text: string
  diagnostics: Diagnostic[]
}

/**
 * The text of the document at `path` — `text`, whose front matter read as `page` — once
 * each of `plugins`' {@link Plugin.transformDocument} has run over its body, in the
 * order listed, each given the body the one before returned. The front matter is kept
 * byte for byte, so its spans hold, and the core finds the same block: the same YAML,
 * closed by the same line — which, where its `---` ended the file, ends with the line
 * ending the body starts with.
 *
 * A transform that throws, or returns anything but a string the core can be sent, or a
 * body that would make other front matter of the text ({@link moved}), is an error at
 * the document's front matter, or at its start where it has none, and the document
 * keeps the text it had before that transform, which no later transform is given.
 */
export function transformed(
  path: string,
  text: string,
  page: Page,
  plugins: readonly Plugin[]
): Transformed {
  const transforms = plugins.filter((plugin) => plugin.transformDocument !== undefined)
  if (transforms.length === 0) return { text, diagnostics: [] }
  const block = findFrontMatter(text)
  const prefix = text.slice(0, block?.end ?? 0)
  const frontMatter = frozen(structuredClone(page.loaded.value))
  let body = text.slice(prefix.length)
  for (const plugin of transforms) {
    let made: unknown
    const problem = failure(() => {
      made = plugin.transformDocument!.call(plugin, { path, frontMatter, body })
      if (typeof made !== 'string') {
        return `it returned ${shown(made)} rather than the document's body, a string`
      }
      if (/\p{Cs}/u.test(made)) {
        return 'it returned a body holding a lone surrogate, which has no UTF-8 form'
      }
      return moved(text, block, prefix + made)
    })
    if (problem !== undefined) {
      settle(made)
      const diagnostic: Diagnostic = {
        path,
        span: pageSpan(page),
        severity: 'error',
        message:
          `the \`${plugin.name}\` plugin's \`transformDocument\` failed on this ` +
          `document: ${problem}`,
        labels: [],
      }
      return { text: prefix + body, diagnostics: [diagnostic] }
    }
    body = made as string
  }
  return { text: prefix + body, diagnostics: [] }
}

/**
 * Why `next` — `text` with its body replaced — holds other front matter than `text`,
 * whose block is `block`, if it does: the same YAML closed by the same line, or, where
 * `text` closes none, no block and the same first line. A body can change a closed
 * block only where its closing `---` ended the file, by not starting with a newline; a
 * document with no closed block is the body alone.
 */
function moved(text: string, block: Block | undefined, next: string): string | undefined {
  const after = findFrontMatter(next)
  if (block !== undefined) {
    if (after?.inner.end === block.inner.end) return undefined
    return (
      "it returned a body that does not start with a newline, and the front matter's " +
      'closing `---` ends the file, so that line would no longer close it: start the ' +
      'body with a newline'
    )
  }
  const opens = opensFrontMatter(text)
  if (opensFrontMatter(next) === opens && after === undefined) return undefined
  if (!opens) {
    return (
      'it returned a body opening with a `---` line, which would read as front matter ' +
      'the document does not have'
    )
  }
  const how = after === undefined ? 'drops the `---` line opening' : 'closes'
  return (
    `it returned a body that ${how} the front matter the document leaves unclosed, ` +
    'which the core reports as it stands'
  )
}

/**
 * `value` with every object in it frozen, so that writing to any of it throws in
 * strict-mode code, and changes nothing elsewhere.
 */
function frozen<T>(value: T): T {
  if (typeof value === 'object' && value !== null) {
    for (const inner of Object.values(value)) frozen(inner)
    Object.freeze(value)
  }
  return value
}

/** A file an {@link Plugin.assets} hook built, for the build to claim and write. */
export interface AssetFile {
  /** The url the hook named, as it named it. */
  url: string
  bytes: Uint8Array
  /** The plugin whose hook built it. */
  plugin: string
}

/** What the plugins' asset hooks built: the manifest and files, and what went wrong. */
export interface Provided {
  /** Every plugin's manifest, merged in the order the plugins are listed. */
  manifest: Record<string, string>
  /** Every plugin's files, in the order the plugins and their files are listed. */
  files: AssetFile[]
  diagnostics: Diagnostic[]
}

const ASSETS_FIELDS: readonly string[] = ['manifest', 'files'] satisfies (keyof Assets)[]

/**
 * Calls each of the config's plugins' {@link Plugin.assets} hooks, in the order listed,
 * awaiting each, and merges what they built — for a build that `writes` its output, or
 * for a check that does not, in the environment `env`, which the hooks are given as one
 * frozen copy.
 *
 * Each failure is an error at the start of the config file, which lists the plugin:
 * a hook that throws or rejects, or returns anything but {@link Assets} — a manifest
 * that does not map non-empty strings to non-empty strings, files that are not a string
 * or bytes by url, a field it does not have — and a logical path two plugins both map,
 * naming both. A hook that fails adds nothing, and the others still run.
 */
export async function provided(
  config: LoadedConfig,
  { writes, env }: Pick<AssetsContext, 'writes' | 'env'>
): Promise<Provided> {
  const result: Provided = { manifest: {}, files: [], diagnostics: [] }
  const frozenEnv = Object.freeze({ ...env })
  const mappedBy = new Map<string, string>()
  const at = (message: string) => {
    result.diagnostics.push({
      path: config.file,
      span: FILE_START,
      severity: 'error',
      message,
      labels: [],
    })
  }
  for (const plugin of config.plugins) {
    if (plugin.assets === undefined) continue
    const failed = `the \`${plugin.name}\` plugin's \`assets\` hook failed`
    const { input, output } = config
    const context = { root: dirname(config.file), input, output, writes, env: frozenEnv }
    let made: unknown
    try {
      made = await plugin.assets.call(plugin, context)
    } catch (error) {
      at(`${failed}: ${threw(error)}`)
      continue
    }
    const problem = unbuilt(made)
    if (problem !== undefined) {
      at(`${failed}: ${problem}`)
      continue
    }
    const { manifest = {}, files = {} } = made as Assets
    for (const [path, url] of Object.entries(manifest)) {
      const first = mappedBy.get(path)
      if (first === undefined) {
        mappedBy.set(path, plugin.name)
        result.manifest[path] = url
        continue
      }
      at(
        `the plugins \`${first}\` and \`${plugin.name}\` both map the asset \`${path}\`; ` +
          'an asset ships at one URL'
      )
    }
    for (const [url, content] of Object.entries(files)) {
      const bytes = typeof content === 'string' ? encoder.encode(content) : content
      result.files.push({ url, bytes, plugin: plugin.name })
    }
  }
  return result
}

/** What is wrong with `made` as an asset hook's result, if anything. */
function unbuilt(made: unknown): string | undefined {
  if (!isPlainObject(made)) {
    return `it returned ${shown(made)} rather than a plain object, \`{manifest?, files?}\``
  }
  for (const field of Object.keys(made)) {
    if (!ASSETS_FIELDS.includes(field)) {
      return (
        `it returned \`${field}\`, which this shell does not read; it returns ` +
        fields(ASSETS_FIELDS)
      )
    }
  }
  const { manifest, files } = made
  for (const [field, value, of] of [
    ['manifest', manifest, 'logical paths and URLs'],
    ['files', files, 'urls and contents'],
  ] as const) {
    if (value !== undefined && !isPlainObject(value)) {
      return (
        `its \`${field}\` is ${shown(value)} rather than a plain object, \`{…}\`, ` +
        `of ${of}`
      )
    }
  }
  for (const [path, url] of Object.entries(manifest ?? {})) {
    if (!isName(path)) {
      return (
        `its \`manifest\` names the logical path ${JSON.stringify(path)}, where a ` +
        'path is a non-empty string with a UTF-8 form'
      )
    }
    if (!isName(url)) {
      const what = typeof url === 'string' ? JSON.stringify(url) : shown(url)
      return (
        `its \`manifest\` maps \`${path}\` to ${what}, where it maps a path to the URL ` +
        'the asset ships at, a non-empty string with a UTF-8 form'
      )
    }
  }
  for (const [url, content] of Object.entries(files ?? {})) {
    if (!isContent(content)) {
      return (
        `its \`files\` holds ${shown(content)} at \`${url}\`, where a file is a string ` +
        'or bytes'
      )
    }
  }
  return undefined
}

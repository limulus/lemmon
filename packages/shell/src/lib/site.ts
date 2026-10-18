import type { Diagnostic } from '@limulus/lemmon-core'
import { dirname, relative, sep } from 'node:path'

import type { LoadedConfig } from './config.ts'
import { readPage, type Page } from './front-matter.ts'
import { transformed } from './providers.ts'
import { walk, type Entry } from './walk.ts'

/** A site as discovery finds it, before anything is compiled. */
export interface Site {
  config: LoadedConfig
  /**
   * Every file under the input directory, classified, sorted by path: each document's
   * text as the config's plugins' transforms left it.
   */
  entries: Entry[]
  /**
   * Every page whose front matter read cleanly — each document, and each template that
   * opens with front matter — by path, in path order.
   */
  pages: Map<string, Page>
  /**
   * What reading the pages' front matter, and transforming the documents, reported, in
   * path order.
   */
  diagnostics: Diagnostic[]
}

/**
 * Discovers the site `config` describes (step 1 of the build pipeline): walks the input
 * and reads every page's front matter, each page's `inputPath` its path from the
 * config's directory — `www/feed/the-rebound/index.md`. A page whose front matter has
 * an error is left out of {@link Site.pages}, and the error is in
 * {@link Site.diagnostics}.
 *
 * Each document whose front matter read then goes through the config's plugins'
 * document transforms, the first half of step 2 (D44), and its entry holds the text
 * they left, which is what compiles; a transform's failure is in
 * {@link Site.diagnostics} too. A page template is not transformed.
 */
export async function discover(config: LoadedConfig): Promise<Site> {
  const entries = await walk(config)
  const inputDir = relative(dirname(config.file), config.input).split(sep).join('/')
  const pages = new Map<string, Page>()
  const diagnostics: Diagnostic[] = []
  for (const entry of entries) {
    if (entry.kind === 'document' || (entry.kind === 'template' && entry.page)) {
      const read = readPage(entry.path, entry.text, { inputDir })
      diagnostics.push(...read.diagnostics)
      if (read.page === undefined) continue
      pages.set(entry.path, read.page)
      if (entry.kind !== 'document') continue
      const { text, diagnostics: failed } = transformed(
        entry.path,
        entry.text,
        read.page,
        config.plugins
      )
      entry.text = text
      diagnostics.push(...failed)
    }
  }
  return { config, entries, pages, diagnostics }
}

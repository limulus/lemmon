import type { Diagnostic } from '@limulus/lemmon-core'
import { dirname, relative, sep } from 'node:path'

import type { LoadedConfig } from './config.ts'
import { readPage, type Page } from './front-matter.ts'
import { walk, type Entry } from './walk.ts'

/** A site as discovery finds it, before anything is compiled. */
export interface Site {
  config: LoadedConfig
  /** Every file under the input directory, classified, sorted by path. */
  entries: Entry[]
  /**
   * Every page whose front matter read cleanly — each document, and each template that
   * opens with front matter — by path, in path order.
   */
  pages: Map<string, Page>
  /** What reading the pages' front matter reported, in path order. */
  diagnostics: Diagnostic[]
}

/**
 * Discovers the site `config` describes (step 1 of the build pipeline): walks the input
 * and reads every page's front matter, each page's `inputPath` its path from the
 * config's directory — `www/feed/the-rebound/index.md`. A page whose front matter has
 * an error is left out of {@link Site.pages}, and the error is in
 * {@link Site.diagnostics}.
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
      if (read.page !== undefined) pages.set(entry.path, read.page)
    }
  }
  return { config, entries, pages, diagnostics }
}

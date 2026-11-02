import type { HLJSApi, Language } from 'highlight.js'
import { createRequire } from 'node:module'

import { takesOnly, type Plugin } from './plugins.ts'

/** How a site narrows what {@link highlighter} claims. */
export interface HighlighterOptions {
  /**
   * Language names and aliases it leaves to another plugin — `['rust', 'rs']` for a site
   * whose own producer highlights Rust — each one it would otherwise claim. Each is a key
   * as a code block writes it, so a language's other names stay claimed until they are
   * listed too. None when omitted.
   */
  exclude?: readonly string[]
}

/**
 * The shell's highlighter (D42), listed in a site's config beside `bundler()`: a `code`
 * producer that highlights a code block with highlight.js 11.9.0 — pinned exactly, since
 * another release's grammars mark the same code differently — in the language its key
 * names, and writes it as the highlight function in markdown-it's documentation does,
 * which limulus.net's Eleventy build uses:
 *
 * ```html
 * <pre><code class="hljs"><span class="hljs-keyword">let</span> x = …
 * </code></pre>
 * ```
 *
 * The classes are highlight.js's, so any of its themes styles the block. It claims every
 * language highlight.js ships, by name and by alias — `ts` and `typescript`, `txt` and
 * `plaintext` — but those `options.exclude` lists, each matched as written: highlight.js
 * ignores case, and a claim does not. A code block in a language it does not claim, and
 * one naming none, keep the block the core wrote, and a language no plugin claims is
 * warned at.
 *
 * Throws a `TypeError`, which the config load reports, for an option it does not take,
 * for an `exclude` that is not a list — a string would be read as its characters, `'r'`
 * un-claiming R — and for an excluded key it would not have claimed, so a misspelling
 * does not leave the language claimed.
 */
export function highlighter(options: HighlighterOptions = {}): Plugin {
  takesOnly('highlighter', options, ['exclude'] satisfies (keyof HighlighterOptions)[])
  const { exclude: excluded = [] } = options
  if (!Array.isArray(excluded)) {
    throw new TypeError('`exclude` of `highlighter()` must be a list of language names')
  }
  // Loaded here rather than where this module is, which every value a config imports
  // from the shell loads, so that only a site listing this plugin loads highlight.js;
  // and required, since the keys it claims are known when the config loads.
  const hljs = createRequire(import.meta.url)('highlight.js') as HLJSApi
  const languages = new Set(
    hljs
      .listLanguages()
      .flatMap((name) => [name, ...((hljs.getLanguage(name) as Language).aliases ?? [])])
  )
  const exclude = new Set(excluded)
  for (const key of exclude) {
    if (!languages.has(key)) {
      throw new TypeError(
        `\`highlighter()\` cannot exclude \`${key}\`, which is no language or ` +
          'alias highlight.js has'
      )
    }
  }
  return {
    name: 'highlighter',
    marks: [
      {
        name: 'code',
        keys: [...languages].filter((key) => !exclude.has(key)),
        payload: 'Code',
        keyedBy: 'language',
        produce: ({ marks }) =>
          marks.map(({ key, payload }) => {
            const { code } = payload as { code: string }
            // A producer listing keys is given only the marks carrying one of them.
            const { value } = hljs.highlight(code, {
              language: key as string,
              ignoreIllegals: true,
            })
            return `<pre><code class="hljs">${value}</code></pre>`
          }),
      },
    ],
  }
}

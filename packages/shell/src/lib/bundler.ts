import { createHash } from 'node:crypto'

import type { Plugin, PluginHandler } from './plugins.ts'

/**
 * The url path the `styles` bundles are written under: inside `/assets/immutable/`,
 * which a CDN may cache forever, since a bundle's name changes whenever its bytes do.
 */
const BUNDLES = '/assets/immutable/bundle/'

/**
 * The shell's default plugin (D42), listed in a site's config as `plugins: [bundler()]`:
 * producers for the names the collect tags collect under, so that a layout's
 * `mark("styles")`, `mark("head")` and `mark("scripts")` place what a page's components
 * collected. A site that would produce one of those names itself leaves this plugin out
 * rather than listing two producers for a mark, which its config load refuses.
 *
 * - `styles`: what the page's `lem-style` blocks collected, in collection order, joined
 *   by newlines, is written as one stylesheet named by its bytes — the first ten
 *   characters of their SHA-256 in base64url, Eleventy's bundle form — under
 *   `/assets/immutable/bundle/`, so pages that collect the same styles share one file.
 *   Every `styles` mark becomes the `<link>` to it, and nothing where the page collected
 *   no styles.
 * - `scripts`: every `scripts` mark becomes a module `<script>` for each URL the page's
 *   `lem-script` tags collected, joined by newlines.
 * - `head`: every `head` mark becomes what the page's `lem-head` blocks collected, joined
 *   by newlines.
 */
export function bundler(): Plugin {
  return { name: 'bundler', marks: [styles, scripts, head] }
}

const styles: PluginHandler = {
  name: 'styles',
  produce: ({ marks, fragments, write }) => {
    if (fragments.length === 0) return marks.map(() => '')
    const css = fragments.map((fragment) => fragment.text).join('\n')
    // base64url, not base64: a `/` in the name would write the bundle a directory down.
    const name = createHash('sha256').update(css, 'utf8').digest('base64url').slice(0, 10)
    const url = `${BUNDLES}${name}.css`
    write(url, css)
    return marks.map(() => `<link rel="stylesheet" href="${url}">`)
  },
}

const scripts: PluginHandler = {
  name: 'scripts',
  produce: ({ marks, fragments }) => {
    // A fragment is the URL as `<lem.…>` prints one, escaped for an attribute already.
    const html = fragments
      .map((fragment) => `<script type="module" src="${fragment.text}"></script>`)
      .join('\n')
    return marks.map(() => html)
  },
}

const head: PluginHandler = {
  name: 'head',
  produce: ({ marks, fragments }) => {
    const html = fragments.map((fragment) => fragment.text).join('\n')
    return marks.map(() => html)
  },
}

import type { Mark } from '@limulus/lemmon-core'

import type { Plugin, PluginHandler } from '../lib/plugins.ts'

const decoder = new TextDecoder()

/**
 * A highlighter for `ts` alone, which reads the code from the payload the shell fills in
 * rather than from the page, and claims the record and key field a code block carries.
 */
export const highlighter: Plugin = {
  name: 'highlighter',
  marks: [
    {
      name: 'code',
      keys: ['ts'],
      payload: 'Code',
      keyedBy: 'language',
      produce: ({ marks }) =>
        marks.map(
          (mark) =>
            `<pre class="ts">${(mark.payload as { code: string }).code.trim()}</pre>`
        ),
    },
  ],
}

/**
 * The record a `callout` mark carries, which the site declares while a plugin cannot ship
 * its own `.lem`: a site's `site.lem` appends it.
 */
export const CALLOUT_LEM = `<lem-type Callout>
  kind: string
  text: string
</lem-type>
`

/** A producer for template marks, dispatched by the `kind` field of their payload. */
export const callouts: Plugin = {
  name: 'callouts',
  marks: [
    {
      name: 'callout',
      keys: ['note'],
      payload: 'Callout',
      keyedBy: 'kind',
      produce: ({ marks }) =>
        marks.map(
          (mark) =>
            `<aside class="${mark.key}">${(mark.payload as { text: string }).text}</aside>`
        ),
    },
  ],
}

/** A decorator of `name` that wraps whatever content a mark has in `tag`. */
export function wrapping(tag: string, name = 'code', keys?: string[]): PluginHandler {
  return {
    name,
    keys,
    decorate: (_: Mark, content: Uint8Array) =>
      `<${tag}>${decoder.decode(content)}</${tag}>`,
  }
}

/** A keyless decorator over every code block: a copy button, after a fashion. */
export const copy: Plugin = { name: 'copy', marks: [wrapping('copy')] }

/**
 * A decorator over the `sh` code blocks alone, which names the field a `code` mark is
 * keyed by, as a handler listing keys must where no other handler of the name does.
 */
export const shell: Plugin = {
  name: 'shell',
  marks: [{ ...wrapping('sh', 'code', ['sh']), payload: 'Code', keyedBy: 'language' }],
}

/** A `styles` producer that inlines what the page collected, in collection order. */
export const inline: Plugin = {
  name: 'inline',
  marks: [
    {
      name: 'styles',
      produce: ({ marks, fragments }) =>
        marks.map(() => `<style>${fragments.map((f) => f.text).join('\n')}</style>`),
    },
  ],
}

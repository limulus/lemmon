import type { Mark } from '@limulus/lemmon-core'
import hljs from 'highlight.js'
import { spawnSync } from 'node:child_process'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'

import { build } from './build.ts'
import { highlighter } from './highlighter.ts'
import { claimsOf, validatePlugins, type Plugin } from './plugins.ts'
import { FILE_START } from './program.ts'
import { configOf, ENV, PASSTHROUGH, project, SITE, tree } from '../mocks/sites.ts'
import { at } from '../mocks/spans.ts'

/** The keys `plugin` claims `code` for. */
function keysOf(plugin: Plugin): readonly string[] {
  const [claim] = claimsOf([plugin])
  return claim.keys ?? []
}

/**
 * What the highlighter makes of code blocks, each `[language, code]` — one replacement
 * per block, in order.
 */
function highlighted(blocks: [string, string][]) {
  const handler = highlighter().marks?.[0]
  if (handler?.produce === undefined) throw new Error('no producer for `code`')
  const marks: Mark[] = blocks.map(([language, code]) => ({
    name: 'code',
    key: language,
    payload: { language, info: language, code },
    payloadId: '0000000000000000',
    start: 0,
    end: 0,
    path: 'post.md',
    span: FILE_START,
  }))
  const page = new Uint8Array()
  const context = { name: 'code', marks, fragments: [], page, body: false, write: () => {} }
  return handler.produce(context)
}

/** A site's own producer of `code` for Rust, which the highlighter must make room for. */
const rust: Plugin = {
  name: 'rust',
  marks: [
    {
      name: 'code',
      keys: ['rust', 'rs'],
      payload: 'Code',
      keyedBy: 'language',
      produce: ({ marks }) => marks.map(() => '<pre>rust</pre>'),
    },
  ],
}

describe('highlighter()', () => {
  it('should produce `code`, keyed by the language of the `Code` it carries', () => {
    const plugin = highlighter()

    expect(validatePlugins([plugin])).toEqual([plugin])
    expect(plugin.name).toBe('highlighter')
    expect(claimsOf([plugin])).toEqual([
      { name: 'code', keys: expect.any(Array), payload: 'Code', keyedBy: 'language' },
    ])
  })

  it('should claim every language highlight.js has, by name and by alias, once each', () => {
    const keys = keysOf(highlighter())

    expect(keys).toEqual(expect.arrayContaining(hljs.listLanguages()))
    expect(keys).toEqual(
      expect.arrayContaining(['ts', 'typescript', 'rs', 'rust', 'sh', 'bash', 'txt'])
    )
    // `ls` and `ml` are each the alias of two languages.
    expect(new Set(keys).size).toBe(keys.length)
    expect(keys.filter((key) => hljs.getLanguage(key) === undefined)).toEqual([])
  })

  it('should load highlight.js only when it is called, not when the shell is imported', () => {
    // In a process of its own, since this one has loaded highlight.js to compare with.
    const child = `
      import { createRequire } from 'node:module'
      const require = createRequire(import.meta.url)
      const loaded = () =>
        Object.keys(require.cache).some((file) => file.includes('/highlight.js/lib/'))
      const { highlighter } = await import(process.argv[1])
      console.log('imported:', loaded())
      highlighter()
      console.log('called:', loaded())
    `
    const shell = new URL('../index.ts', import.meta.url).href

    const { status, stdout } = spawnSync(
      process.execPath,
      ['--conditions=source', '--input-type=module', '--eval', child, shell],
      { encoding: 'utf8' }
    )

    expect(status).toBe(0)
    expect(stdout).toBe('imported: false\ncalled: true\n')
  })

  describe('a code block', () => {
    it('should be highlighted in `pre code.hljs`, as limulus.net’s markdown-it writes it', () => {
      expect(highlighted([['ts', "let x = '<b>'\n"]])).toEqual([
        '<pre><code class="hljs"><span class="hljs-keyword">let</span> x = ' +
          '<span class="hljs-string">&#x27;&lt;b&gt;&#x27;</span>\n</code></pre>',
      ])
    })

    it('should be highlighted in the language its key names, each in order', () => {
      expect(
        highlighted([
          ['rs', 'fn main() {}\n'],
          ['typescript', 'const a = 1\n'],
        ])
      ).toEqual([
        '<pre><code class="hljs"><span class="hljs-keyword">fn</span> ' +
          '<span class="hljs-title function_">main</span>() {}\n</code></pre>',
        '<pre><code class="hljs"><span class="hljs-keyword">const</span> a = ' +
          '<span class="hljs-number">1</span>\n</code></pre>',
      ])
    })

    it('should be escaped as plain text where it names `txt`', () => {
      expect(highlighted([['txt', 'a < b & "c"\n']])).toEqual([
        '<pre><code class="hljs">a &lt; b &amp; &quot;c&quot;\n</code></pre>',
      ])
    })

    it('should be highlighted past what its language’s grammar holds illegal', () => {
      // Rust's grammar holds `</` illegal, where highlight.js would otherwise give up and
      // return the block escaped, with no spans at all.
      expect(highlighted([['rust', 'fn main() { let x = 1; } </\n']])).toEqual([
        '<pre><code class="hljs"><span class="hljs-keyword">fn</span> ' +
          '<span class="hljs-title function_">main</span>() { ' +
          '<span class="hljs-keyword">let</span> <span class="hljs-variable">x</span> = ' +
          '<span class="hljs-number">1</span>; } &lt;/\n</code></pre>',
      ])
    })
  })

  describe('with `exclude`', () => {
    it('should claim every key but those it lists, leaving them to another plugin', () => {
      const keys = keysOf(highlighter({ exclude: ['rust', 'rs'] }))

      expect(keys).not.toContain('rust')
      expect(keys).not.toContain('rs')
      expect(keys).toContain('ts')
      expect(() =>
        validatePlugins([highlighter({ exclude: ['rust', 'rs'] }), rust])
      ).not.toThrow()
    })

    it('should be refused beside another producer of a key it does not exclude', () => {
      expect(() => validatePlugins([highlighter(), rust])).toThrow(
        'the plugins `highlighter` and `rust` both produce `code` for `rust`; a mark has ' +
          'one producer (D42)'
      )
    })

    it('should refuse an option it does not take, as a misspelling of `exclude`', () => {
      const options = { exlude: ['rust'] } as unknown as { exclude: string[] }

      expect(() => highlighter(options)).toThrow(
        new TypeError(
          '`highlighter()` cannot take `exlude`, which is none of its options: `exclude`'
        )
      )
    })

    it('should refuse an `exclude` that is not a list, whose characters would be keys', () => {
      const refusal = new TypeError(
        '`exclude` of `highlighter()` must be a list of language names'
      )

      // `'rs'` would refuse `s`, and `'r'` would un-claim R without a word.
      for (const exclude of ['rs', 'r']) {
        expect(() => highlighter({ exclude: exclude as unknown as string[] })).toThrow(
          refusal
        )
      }
    })

    it('should refuse a key highlight.js does not have, matched as written', () => {
      expect(() => highlighter({ exclude: ['rust', 'Rust'] })).toThrow(
        new TypeError(
          '`highlighter()` cannot exclude `Rust`, which is no language or alias ' +
            'highlight.js has'
        )
      )
    })
  })

  describe('in a build', () => {
    const post =
      '---\ntitle: A\n---\n\n```ts\nlet x = 1\n```\n\n```\nplain\n```\n\n```rsut\nx\n```\n'

    it('should highlight the languages it claims, leaving the rest as written', async () => {
      const dir = project({ ...SITE, 'posts/a.md': post }, PASSTHROUGH)
      const config = configOf(dir, PASSTHROUGH, [highlighter()])
      const result = await build(config, { now: new Date(0), env: ENV })

      expect(result.diagnostics).toEqual([
        expect.objectContaining({
          path: 'posts/a.md',
          span: at(post, '```«rsut»'),
          severity: 'warning',
          // The site's signal: the language warned at with what it would claim instead.
          message: 'no producer claims `rsut` for `code`; did you mean `rust`?',
        }),
      ])
      expect(tree(join(dir, 'dist'))['posts/a/index.html']).toContain(
        '<pre><code class="hljs"><span class="hljs-keyword">let</span> x = ' +
          '<span class="hljs-number">1</span>\n</code></pre>\n' +
          '<pre><code>plain\n</code></pre>\n' +
          '<pre><code class="language-rsut">x\n</code></pre>\n'
      )
    })
  })
})

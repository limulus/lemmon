import { loadCore, type Diagnostic, type MarkContext } from '@limulus/lemmon-core'
import { describe, expect, it } from 'vitest'

import {
  claimsOf,
  resolvePage,
  validatePlugins,
  type Plugin,
  type PluginHandler,
} from './plugins.ts'
import { callouts, copy, highlighter, inline, shell, wrapping } from '../mocks/plugins.ts'
import { at } from '../mocks/spans.ts'

const decoder = new TextDecoder()

describe('claimsOf()', () => {
  it('should claim a producer’s name with its keys, payload and key field', () => {
    expect(claimsOf([highlighter, callouts])).toEqual([
      { name: 'code', keys: ['ts'], payload: 'Code', keyedBy: 'language' },
      { name: 'callout', keys: ['note'], payload: 'Callout', keyedBy: 'kind' },
    ])
  })

  it('should claim a decorator’s name with an empty key list, whatever keys it lists', () => {
    // A claim with no list covers every key, and would silence the warning at a code
    // block's language no producer covers.
    expect(claimsOf([copy, shell])).toStrictEqual([
      { name: 'code', keys: [], payload: undefined, keyedBy: undefined },
      { name: 'code', keys: [], payload: 'Code', keyedBy: 'language' },
    ])
  })

  it('should claim a keyless producer’s name with an empty key list', () => {
    expect(claimsOf([inline])).toStrictEqual([
      { name: 'styles', keys: [], payload: undefined, keyedBy: undefined },
    ])
  })

  it('should claim nothing for no plugins, or plugins without handlers', () => {
    expect(claimsOf([])).toEqual([])
    expect(claimsOf([{ name: 'a' }, { name: 'b', marks: [] }])).toEqual([])
  })
})

describe('the plugin claims, against the core', () => {
  // The config load restates the core's rule for one name's payload and key field so
  // that its refusal can name the plugins. Each list here is one the two must judge
  // alike: refused at load exactly where its claims make the core's declaration throw.
  const note = (extra: Partial<PluginHandler>): PluginHandler => ({
    name: 'note',
    decorate: (_, content) => content,
    ...extra,
  })
  const cases: [string, boolean, Plugin[]][] = [
    [
      'two plugins naming different payloads',
      false,
      [
        { name: 'a', marks: [note({ payload: 'Note' })] },
        { name: 'b', marks: [note({ payload: 'Callout' })] },
      ],
    ],
    [
      'one plugin naming two payloads',
      false,
      [{ name: 'a', marks: [note({ payload: 'Note' }), note({ payload: 'Callout' })] }],
    ],
    [
      'two key fields',
      false,
      [
        { name: 'a', marks: [note({ payload: 'Note', keyedBy: 'kind' })] },
        { name: 'b', marks: [note({ payload: 'Note', keyedBy: 'type' })] },
      ],
    ],
    [
      'a key field with no payload',
      false,
      [{ name: 'a', marks: [note({ keyedBy: 'k' })] }],
    ],
    [
      'a payload beside a handler naming none',
      true,
      [
        { name: 'a', marks: [note({ payload: 'Note' })] },
        { name: 'b', marks: [note({ produce: () => [] })] },
      ],
    ],
    [
      'a key field beside a handler naming none',
      true,
      [
        { name: 'a', marks: [note({ payload: 'Note', keyedBy: 'kind' })] },
        { name: 'b', marks: [note({ payload: 'Note' })] },
      ],
    ],
    [
      'a key the core cannot be sent',
      false,
      [
        {
          name: 'a',
          marks: [
            note({ keys: ['\uD800'], payload: 'Note', keyedBy: 'kind', produce: () => [] }),
          ],
        },
      ],
    ],
    [
      'two names each with its own payload',
      true,
      [
        {
          name: 'a',
          marks: [
            note({ payload: 'Note' }),
            { ...note({}), name: 'aside', payload: 'Aside' },
          ],
        },
      ],
    ],
  ]

  for (const [label, expected, plugins] of cases) {
    it(`should ${expected ? 'take' : 'refuse'} ${label} at load and in the core alike`, async () => {
      const core = await loadCore()
      const loads = succeeds(() => validatePlugins(plugins))
      const declares = succeeds(() =>
        core.compile([], { ambient: {}, marks: claimsOf(plugins) })
      )

      expect({ loads, declares }).toEqual({ loads: expected, declares: expected })
    })
  }
})

/** Whether `call` returns rather than throwing. */
function succeeds(call: () => unknown): boolean {
  try {
    call()
    return true
  } catch {
    return false
  }
}

describe('resolvePage()', () => {
  // Two `styles` marks with a default, and one `head` mark the other plugin fills: a
  // handler that fails leaves its own marks as they were, and every other mark resolved.
  const PAGE =
    '<lem-let d><link rel="stylesheet" href="/x.css"></lem-let>\n' +
    '<lem.mark("styles", default: d)>|<lem.mark("styles", default: d)>|<lem.mark("head")>\n'
  const FIRST = at(PAGE, '«mark("styles", default: d)»>|<lem.mark("styles"')
  const KEPT = '<link rel="stylesheet" href="/x.css">'
  const head: Plugin = { name: 'head', marks: [{ name: 'head', produce: () => ['<H>'] }] }

  /** `PAGE` rendered and resolved with `plugins`, as the build would for `page.lem`. */
  async function resolved(
    plugins: Plugin[]
  ): Promise<{ html: string; diagnostics: Diagnostic[] }> {
    const core = await loadCore()
    const compiled = core.compile([{ path: 'page.lem', text: PAGE }], {
      ambient: {},
      marks: claimsOf(plugins),
    })
    expect(compiled.diagnostics).toEqual([])
    core.loadData({})
    const page = { path: 'page.lem', plugins, payloads: new Map() }
    const { bytes, diagnostics } = resolvePage(core.render({ path: 'page.lem' }), page)
    return { html: decoder.decode(bytes), diagnostics }
  }

  /** A plugin whose `styles` producer is `produce`. */
  const styles = (produce: unknown): Plugin => ({
    name: 'styles',
    marks: [{ name: 'styles', produce: produce as PluginHandler['produce'] }],
  })

  it('should fill every mark with what its producer made', async () => {
    // A plugin listing no handlers is listed all the same, and answers nothing.
    const plugins = [styles(() => ['<a>', '<b>']), { name: 'none' }, head]
    const { html, diagnostics } = await resolved(plugins)

    expect(html).toBe('<a>|<b>|<H>\n')
    expect(diagnostics).toEqual([])
  })

  describe('a producer that fails', () => {
    const failures: [string, unknown, string][] = [
      [
        'throws an error',
        () => {
          throw new TypeError('boom')
        },
        'it threw TypeError: boom',
      ],
      [
        'throws a string',
        () => {
          const thrown: unknown = 'boom'
          throw thrown
        },
        'it threw boom',
      ],
      [
        'throws what is not an error',
        () => {
          const thrown: unknown = { reason: 'boom' }
          throw thrown
        },
        'it threw an object',
      ],
      [
        'returns a promise',
        // Rejected as well, which the build settles: nothing is left unhandled.
        () => Promise.reject(new Error('later')),
        'it returned a promise rather than a list of replacements',
      ],
      [
        'returns a promise for each mark',
        () => [Promise.reject(new Error('later')), Promise.reject(new Error('later'))],
        'it returned a promise as a replacement, which is neither a string nor bytes',
      ],
      [
        'miscounts with promises',
        () => [Promise.reject(new Error('later'))],
        'it returned 1 replacement for 2 marks',
      ],
      [
        'returns a thenable for a mark',
        () => [{ then: () => undefined }, '<b>'],
        'it returned a promise as a replacement, which is neither a string nor bytes',
      ],
      [
        'returns a string',
        () => KEPT,
        'it returned a string rather than a list of replacements',
      ],
      ['miscounts', () => ['<a>'], 'it returned 1 replacement for 2 marks'],
      [
        'returns undefined for a mark',
        () => [undefined, '<b>'],
        'it returned undefined as a replacement, which is neither a string nor bytes',
      ],
      [
        'returns a number for a mark',
        () => ['<a>', 5],
        'it returned 5 as a replacement, which is neither a string nor bytes',
      ],
      [
        'returns an object for a mark',
        () => [{}, {}],
        'it returned an object as a replacement, which is neither a string nor bytes',
      ],
      [
        'returns a function for a mark',
        () => [() => '<a>', '<b>'],
        'it returned a function as a replacement, which is neither a string nor bytes',
      ],
      [
        'returns null for a mark',
        () => ['<a>', null],
        'it returned null as a replacement, which is neither a string nor bytes',
      ],
    ]

    for (const [label, produce, problem] of failures) {
      it(`should warn at its first mark and keep its marks’ defaults when it ${label}`, async () => {
        const { html, diagnostics } = await resolved([styles(produce), head])

        expect(html).toBe(`${KEPT}|${KEPT}|<H>\n`)
        expect(diagnostics).toEqual([
          {
            path: 'page.lem',
            span: FIRST,
            severity: 'warning',
            message:
              "the `styles` plugin's producer for `styles` failed on `page.lem`, so the " +
              `marks it answers keep their default content: ${problem}`,
            labels: [],
          },
        ])
      })
    }
  })

  describe('a decorator that fails', () => {
    const failures: [string, unknown, string][] = [
      [
        'throws',
        () => {
          throw new Error('boom')
        },
        'it threw Error: boom',
      ],
      [
        'returns nothing',
        () => undefined,
        'it returned undefined rather than a string or bytes',
      ],
      [
        'returns a promise',
        () => Promise.reject(new Error('later')),
        'it returned a promise rather than a string or bytes',
      ],
      [
        'returns a list of promises',
        () => [Promise.reject(new Error('later'))],
        'it returned an object rather than a string or bytes',
      ],
    ]

    for (const [label, decorate, problem] of failures) {
      it(`should warn at the mark and keep its content so far when it ${label}`, async () => {
        const failing: Plugin = {
          name: 'failing',
          marks: [{ name: 'head', decorate: decorate as PluginHandler['decorate'] }],
        }
        const after: Plugin = { name: 'after', marks: [wrapping('i', 'head')] }
        const { html, diagnostics } = await resolved([
          styles(() => ['', '']),
          head,
          failing,
          after,
        ])

        // The decorator after it still runs, over the producer's content.
        expect(html).toBe('||<i><H></i>\n')
        expect(diagnostics).toEqual([
          expect.objectContaining({
            path: 'page.lem',
            span: at(PAGE, 'mark("head")'),
            severity: 'warning',
            message:
              "the `failing` plugin's decorator for `head` failed on `page.lem`, so this " +
              `mark keeps the content it had: ${problem}`,
          }),
        ])
      })
    }
  })

  it('should call a handler with its own object as `this`', async () => {
    // A handler written as a class: its methods on the prototype, its state private, so
    // the config load takes it and the build calls it on itself.
    class Linker {
      readonly name = 'styles'
      readonly #link = '<l>'

      produce({ marks }: MarkContext) {
        return marks.map(() => this.#link)
      }

      decorate(_: unknown, content: Uint8Array) {
        return `${decoder.decode(content)}${this.#link.toUpperCase()}`
      }
    }
    const plugins = validatePlugins([{ name: 'methods', marks: [new Linker()] }])

    expect((await resolved([...plugins, head])).html).toBe('<l><L>|<l><L>|<H>\n')
  })
})

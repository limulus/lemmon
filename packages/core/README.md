# @limulus/lemmon-core

The compiler and renderer of [Lemmon](https://github.com/limulus/lemmon), a typed,
tag-based template language, built from Rust to WebAssembly and wrapped in a typed
JavaScript API. The compiler touches no files, network or clock: you hand it source text
and data, and it hands back HTML and diagnostics, the same bytes for the same inputs every
time.

> **Early development.** Lemmon is young and changing fast. Expect frequent major
> releases, each with significant breaking changes.

Building a site? Use [`@limulus/lemmon-shell`][shell], whose `lem` command drives this
package. Use the core directly to run Lemmon inside your own program: another build tool,
a live preview, an editor integration.

## Install

```sh
npm install @limulus/lemmon-core
```

It needs Node 24 or later.

## Compile, load, render

```ts
import { loadCore } from '@limulus/lemmon-core'

const core = await loadCore()

const files = [
  { path: 'site.lem', text: '<lem-type Site>\n  name: string\n</lem-type>\n' },
  { path: 'page.lem', text: '<h1>Welcome to <lem.site.name></h1>\n' },
]
// Every value a template may read, and its type: here `site`, a `Site`.
const host = { ambient: { site: 'Site' } }

core.compile(files, host).diagnostics // []
core.loadData({ ambient: { site: { name: 'limulus.net' } } }) // []

const { html } = core.render({ path: 'page.lem' })
new TextDecoder().decode(html!) // '<h1>Welcome to limulus.net</h1>\n'
```

The core knows no value by name. The host declares each one a template may read, typed by
ordinary Lemmon declarations that compile with the templates. A template is checked
against those types when it compiles, and data when it loads:

```ts
core.compile([{ path: 'page.lem', text: '<h1><lem.site.nmae></h1>\n' }]).diagnostics
// [{ path: 'page.lem', severity: 'error', span: { start: 9, end: 18, range: … },
//    message: '`Site` has no field `nmae`; did you mean `name`?', labels: [] }]

core.loadData({ ambient: { site: { name: 42 } } })
// [{ path: 'site.lem', severity: 'error', span: { start: 18, end: 22, range: … },
//    message: 'expected `string` for `site.name`, got `int`', labels: [] }]
```

## The API

A `Core` is resident: it keeps the compiled program and its data between calls, so a host
compiles and loads once and then renders every page, and after an edit compiles again only
the files that changed.

| Call                    | What it does                                                   |
| ----------------------- | -------------------------------------------------------------- |
| `compile(files, host?)` | Compiles `.lem` templates and `.md` documents into the program |
| `loadData(input)`       | Loads the data templates read, checked against declared types  |
| `plan()`                | Lists the pages the loaded data made, in the order they loaded |
| `render(page)`          | Renders one file to bytes, with its marks and its diagnostics  |
| `free()`                | Releases the core's memory                                     |

- `compile` takes `{ path, text }` for each file and returns
  `{ diagnostics, payloads, sources }`: the problems found, each code block's contents,
  and the files and data the program asks the host to load. The `host` declaration is
  kept until a call passes another, and a compile that installs a new program drops the
  loaded data, so load again before rendering.
- `render` takes `{ path }`, with the `index` `plan()` gave a page a generator made, and
  returns `{ html, marks, fragments, diagnostics }`. `html` is the page's UTF-8 bytes,
  and `null` where the render failed.
- `loadCore(dir?)` loads the WebAssembly and returns a `Core`, and
  `createCore(glue, bytes)` makes one from WebAssembly you load yourself. `core.version`
  names the core the WebAssembly was built from.

Each exported type documents its fields in its doc comment, so an editor shows the
contract as you write the call.

## Diagnostics

Nothing wrong in a template, a document or its data throws. Each problem comes back as a
`Diagnostic`, `{ path, span, severity, message, labels }`. Its span holds byte offsets
into the source and the same range as Language Server Protocol positions, so an editor can
underline it as it is. A compile with warnings alone still installs its program; one with
an error keeps the previous program in place, so the last good version still renders.

A throw from a `Core` method is a bug in the caller, such as rendering a path that was
never compiled, or loading data before any compile has succeeded. The exception is a trap
inside the WebAssembly, such as a stack overflow on an expression thousands of operators
long: it poisons the instance, so no core in that process can run again, and the process
has to start over.

## Marks

A render leaves a mark, a named range of the page holding its default content, around
every Markdown code block and wherever a template calls `mark()`.
`resolveMarks(result, handlers)` fills them: a producer makes a mark's content, and a
decorator changes the content it has. A code block's mark is named `code`, keyed by its
language, and names its code by the `payloadId` that `compile` reported it under:

```ts
import { resolveMarks } from '@limulus/lemmon-core'

const post = { path: 'post.md', text: '# Notes\n\n```sh\nnpm install <pkg>\n```\n' }
const { payloads } = core.compile([post])
const code = new Map(payloads.map((payload) => [payload.id, payload.code]))
const escape = (text: string) => text.replaceAll('&', '&amp;').replaceAll('<', '&lt;')
const shell = (id: string) => `<pre class="shell">${escape(code.get(id)!)}</pre>`

const { html } = resolveMarks(core.render({ path: 'post.md' }), [
  {
    name: 'code',
    keys: ['sh'],
    produce: ({ marks }) => marks.map((mark) => shell(mark.payloadId!)),
  },
])
```

A mark no handler answers keeps the default content the core wrote, here
`<pre><code class="language-sh">`, and `resolveMarks` lists it in `unclaimed`.

## Documentation

- [The WASM API][api] in the architecture doc: every call and what crosses it, in full
- [The language reference][language] and [Writing Lemmon][skill], the author's guide

## License

[MIT](https://github.com/limulus/lemmon/blob/main/LICENSE)

[api]: https://github.com/limulus/lemmon/blob/main/docs/architecture.md#wasm-api-limuluslemmon-core
[language]: https://github.com/limulus/lemmon/blob/main/docs/language.md
[shell]: https://github.com/limulus/lemmon/tree/main/packages/shell#readme
[skill]: https://github.com/limulus/lemmon/blob/main/agents/skills/lemmon/SKILL.md

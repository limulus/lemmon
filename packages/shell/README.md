# @limulus/lemmon-shell

The Node/TypeScript shell around the Lemmon core (see `docs/architecture.md` §Shape and
§Build pipeline): the `lem` CLI and what it drives. `lem build` reads a site, compiles and
renders every page through the WASM core, and writes HTML and passthrough files to the
output directory.

```ts
// lemmon.config.ts
import type { Config } from '@limulus/lemmon-shell'

export default {
  input: 'www',
  output: 'dist/www',
  passthrough: ['assets/**/*', '**/*.{png,svg,jpg,jpeg,xsd,xsl,txt,pdf}'],
} satisfies Config
```

`input` and `output` are directories relative to the config file; `passthrough` globs are
relative to `input`, with Eleventy's glob semantics (picomatch, dotfiles included,
case-insensitive); `plugins` lists the site's plugins, none when omitted.

## Plugins

A plugin is `{name, marks}`: a name its errors and warnings call it by, and the mark
handlers it brings (D42). Any other field is refused when the config loads, since this
shell runs no other hook. A handler answers one mark name:

```ts
import type { Config } from '@limulus/lemmon-shell'

export default {
  input: 'www',
  output: 'dist/www',
  plugins: [
    {
      name: 'callouts',
      marks: [
        {
          name: 'callout', // what `mark("callout", …)` places
          keys: ['note', 'tip'], // the keys it produces, read from the payload's…
          keyedBy: 'kind', // …`kind` field
          payload: 'Callout', // the record a mark carries, declared in the site's `.lem`
          produce: ({ marks }) => marks.map((mark) => `<aside>${mark.key}</aside>`),
        },
      ],
    },
  ],
} satisfies Config
```

A **producer** (`produce`) runs once per page, over every mark of its name it answers, with
the fragments the page collected under that name and the page as the core returned it,
and returns one replacement per mark — a string or bytes. One that lists `keys` answers
the marks carrying one of them, and one that lists none answers the marks with no key, so
there is no wildcard producer. A **decorator** (`decorate`) runs per mark, over the content
so far, on the marks carrying a key it lists or on every mark of its name where it lists
none. A code block is a `code` mark keyed by its language, and its handlers see its
payload, `{language, info, code}`, as `mark.payload`.

The claims are declared to the core at compile, so a `mark()` name no plugin claims is a
warning at the name, with a did-you-mean, and so is a literal key no producer lists; a site
listing no plugins is warned at every mark. A decorator's keys choose what it decorates,
not what is covered, so a `code` block in a language only a decorator lists is warned at
too. Refused at config load, naming both plugins: two producers that would answer one mark
(one name, and one key or none), and two handlers naming different `payload`s or
`keyedBy` fields for one name. Refused too: `keys` for a name no handler of it names
`keyedBy` for — a `mark()` call's key is read from that field, so for `code` a handler says
`payload: 'Code', keyedBy: 'language'` — an `async` handler, since the build waits for no
promise, and a function where a plugin belongs, a plugin factory not called.

## `lem build [--config <path>]`

1. **Discover.** Walks `input` — never entering a `node_modules` or `.git` directory, the
   output, or a symbolically linked directory — and classifies each file: a `passthrough`
   glob's match is copied as it is, `.md` is a document, `.lem` a template (a page when
   it opens with front matter, D41), and anything else is left alone until a `from`
   binding names it. Each page's front matter is read with the `yaml` package, with a
   span for every field path: `type:` and `for:` are read and stripped, `url` derived
   where the front matter gives none (`feed/x/index.md` at `/feed/x/`, `index.lem` at
   `/`), and `inputPath` added (the path from the config's directory,
   `www/feed/x/index.md`).
2. **Compile**, in two calls. First the shell's own `ambient.lem` — `Page`, `Build` and
   `Code` (docs/language.md §Ambient types) — and every template, under the declaration
   `site` → the bindings of `site.lem`, `page` → `Page`, `build` → `Build`, pages
   rendered through `layout` and placed by `url`. Page templates are in this call too,
   without their records, so that what one defines is in sight of the layouts. The core
   reports the program's `from` bindings and the record each glob collects; every page
   is matched against those globs (with the passthrough glob semantics, so case aside),
   and then all pages — documents and page templates — compile in one call, each as its
   own `type:` or as the record of the glob that holds it. A page two globs match is an
   error at the page. Documents compile only in the second call, so what a document
   (`.md`) defines is invisible to every template, with front matter or without. The
   first call's declaration carries the plugins' claims, so the core warns at the marks.
3. **Load.** `build` is `{time, generator}` — the clock as RFC 3339 in UTC, and `Lemmon`
   with the core's version. A glob's binding is the front matter of the pages it holds;
   a data file (JSON or YAML) is read from `input` with positions, and one whose path
   leads outside `input` is an error at its binding; `env("X")` is the variable's text.
   Every page no glob holds whose `type:` named its record is carried on its own.
4. **Plan and render.** Every loaded page renders through its layout. A page no glob
   holds and no `type:` names has no schema: it renders its own output, with `page` its
   front matter as read, loaded on its own before it renders. Each of those loads
   re-sends and re-validates the whole envelope, so the cost grows with the untyped
   pages times the size of the site: 18 of 25 pages on the limulus-dot-net port branch
   cost about 0.35 s in all.
5. **Resolve.** As each page renders, its marks are filled by the plugins' handlers, in
   the order the plugins and their handlers are listed. A handler that throws, or returns
   anything but its content, is a warning at the mark, which keeps the content it had —
   the default for a producer's marks, the content so far for a decorator's — so the page
   is correct either way. What a page collects under a name nothing on it marks is a
   warning at the block that collected it, since nothing writes it.
6. **Write.** A url ending in `/` is written as `<output>/<url>/index.html`, any other as
   the file it names; each passthrough file is copied to its own path. Nothing in the
   output is removed. Before the first file is written, every page has been rendered and
   its file checked against every other page's and every passthrough file's: the same
   file, the same but for case or Unicode form (APFS holds those as one), and a file
   where another needs a directory are each an error at the later page, labelled at the
   first.

It prints every diagnostic as `path:line:col: severity: message`, each label on an
indented line beneath, and `wrote N pages, copied M files`. Any error, at any step, stops
the build before it writes a file, and `lem` exits 1; warnings are printed and the build
goes on. A write the file system then refuses — a file an earlier build left where a
directory is needed, say — is reported as `lem: could not write <file>: …`, exit 1, and
the output may hold what was written before it. A site needs a `site.lem` without front
matter in `input`, even an empty one: without it no glob matches anything.

### Not yet

- **The default plugin and produced files.** The shell ships no `styles`, `scripts` or
  `head` producer and no highlighter, so only a site's own plugins fill a mark, and a
  producer writes no file of its own — no CSS bundle: ∆F4y, and ∆Nxd for highlighting.
  No `asset()` resolves, since the declaration carries no manifest: ∆d1N.
- **Computed mark keys.** A key a template computes and no producer lists keeps the
  default content, unreported: ∆CxX.
- **Plugins.** A plugin's only hook is `marks`, and a `from plugin("…")` binding is an
  error at the binding: ∆c4j.
- **Generators.** A page whose front matter has `for:` is an error at the key: ∆99b.
- **Drafts.** `draft: true` is built like any page; `--drafts` and leaving drafts out
  are ∆1rD, as is `lem check`.
- **Second phase.** A feed renders with the other pages, reading none of their output:
  ∆zu0.
- **The rest of the pipeline.** No output transform (dprint, ∆Oyi), no integrity check
  (∆yOJ), no dev server (∆Vhi), and no worker pool or incremental rebuild: every build
  compiles and renders everything in one core.

## Comparing rendered HTML

`renderedPage(html, {display})` parses a page with parse5 and reduces it to what a browser
renders: attributes by value, comments dropped, and text after the UA stylesheet's
white-space processing, so indentation and blank lines between blocks are invisible while
a space between two inline runs, or anything in a `pre`, still counts (D28). It knows no
site CSS; `display` declares the custom elements a site's stylesheet makes `block` or
`flex`. `compareRendered(ours, theirs, classes)` walks two such pages and returns every
difference with its path, how many nodes matched, and how often each forgiveness class
fired, comparing the doctype and the document mode it sets as well. Within one parent it
stops at the first child it cannot pair or whose text differs, since everything after it
would misalign; a pair whose attributes differ is reported and its siblings still compared.

A forgiveness class names one kind of accepted difference, through any of five hooks.
Four are consulted only where the walk would otherwise report a difference: `attr` for one
differing attribute of a matched element (so several classes may each explain part of one
element), `text` for a differing text run, and `theirsOnly` and `oursOnly` for an element
one side lacks. The fifth, `subtree`, is offered every matched pair and may take its
children over, comparing pairs inside it with more classes in effect; it returns how many
differences it forgave, 0 for none, and a hook that has compared or reported anything may
not then decline. Each hook sees the matched pairs enclosing the
difference. This repo's fidelity harness for the-rebound (`scripts/render-the-rebound.ts`)
and limulus.net's `npm run parity` use it, each holding every class to an expected count.

## Running from a checkout

The `lem` bin is `bin/lem.ts`, which Node runs directly, under the `source` export
condition so that this package and `@limulus/lemmon-core` load from their `src/`. A site
can therefore depend on a checkout with `file:` and see every edit without a build:

```json
{
  "devDependencies": { "@limulus/lemmon-shell": "file:../lemmon/packages/shell" }
}
```

npm links the directory and leaves its dependencies to the checkout's own `node_modules`,
`@limulus/lemmon-core` among them, so nothing is fetched; the core finds the staged
`.wasm` in the checkout's `artifacts/wasm/`, so run `npm run rust:wasm` there first.
Importing the package from a site's own code needs the condition too
(`node --conditions=source`, and `customConditions: ["source"]` for its typecheck), or a
`npm run build` here first.

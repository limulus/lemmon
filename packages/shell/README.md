# @limulus/lemmon-shell

The Node/TypeScript shell around the Lemmon core (see `docs/architecture.md` §Shape and
§Build pipeline): the `lem` CLI and what it drives. `lem build` reads a site, runs its
plugins' document transforms and asset hooks, compiles and renders every page through the
WASM core, fills each page's marks with its plugins, and writes HTML, the files the plugins
built, and passthrough files to the output directory; `lem check` does all of it but the
writing; and `lem dev` builds the site, serves the output, and builds it again on every
change, reloading the pages open on it.

```ts
// lemmon.config.ts
import { bundler, type Config } from '@limulus/lemmon-shell'

export default {
  input: 'www',
  output: 'dist/www',
  passthrough: ['assets/**/*', '**/*.{png,svg,jpg,jpeg,xsd,xsl,txt,pdf}'],
  plugins: [bundler()],
  dev: {
    headers: {
      'Cross-Origin-Opener-Policy': 'same-origin',
      'Cross-Origin-Embedder-Policy': 'require-corp',
    },
  },
} satisfies Config
```

`input` and `output` are directories relative to the config file; `passthrough` globs are
relative to `input`, with Eleventy's glob semantics (picomatch, dotfiles included,
case-insensitive); `plugins` lists the site's plugins, none when omitted. `dev.headers`
are the headers `lem dev`'s server adds to every response it writes itself, as the
site's host sends them — the two above make a page cross-origin isolated — none when
omitted. Refused at load: a name or value Node would not send, two names equal but for
case, and a header that is the server's (`SERVER_HEADERS` in `src/lib/config.ts`) — one
it sends, such as `Content-Type`, `Cache-Control`, `Location` and Node's framing, or one
that would describe the body other than as it is sent: `Content-Encoding`, and `Trailer`,
which Node refuses to write beside a length. So neither side overrides the other.

## The default plugin

`bundler()` is the shell's own plugin, listed like any other. It produces the three names
the collect tags collect under, none of them keyed, so a layout places what a page's
components collected:

- `styles`: what the page's `lem-style` blocks collected, joined by newlines in the order
  collected, is written to `/assets/immutable/bundle/<name>.css`, its name the first ten
  characters of its SHA-256 in base64url — Eleventy's bundle form — and every
  `<lem.mark("styles")>` becomes `<link rel="stylesheet" href="…">` to it. Pages that
  collect the same styles share one file; a page that collects none gets no `<link>`.
- `scripts`: every `<lem.mark("scripts")>` becomes a `<script type="module" src="…">` per
  URL the page's `lem-script` tags collected. Each arrives as its `asset()` prints,
  escaped for an attribute already, so `scripts` does not escape it again.
- `head`: every `<lem.mark("head")>` becomes what the page's `lem-head` blocks collected.

A site that produces one of those names itself leaves `bundler()` out, since two producers
for one mark are refused at config load. It ships no `.lem`, so there is no
`<lem.styles()>` wrapper: a layout writes `<lem.mark("styles")>`.

## Plugins

A plugin is `{name, transformDocument?, assets?, marks?}`: a name its errors and warnings
call it by, a document transform and an asset hook that run before anything compiles
(D44), and the mark handlers that fill each page's marks after it renders (D42). Any other
field is refused when the config loads, since this shell runs no other hook. The types
are exported: `Plugin`, `DocumentSource`, `AssetsContext`, `Assets`, `PluginHandler` and
`ProducerContext`.

### Document transforms

`transformDocument(document)` is given each document (`.md`) whose front matter read —
never a page template, which has no markdown and whose definitions every layout compiles
against — as `{path, frontMatter, body}`: its path
from the input directory, its front matter as the core loads it (`type:` and `for:` read
off, `url` and `inputPath` derived), and the text after the front matter. It returns the
body the document compiles with:

```ts
import type { Plugin } from '@limulus/lemmon-shell'

/**
 * Appends the `[link]` definition a link post's markdown cites, after a blank line: a
 * definition cannot interrupt a paragraph, which a body ending without a newline ends in.
 */
export const linkPosts: Plugin = {
  name: 'link-posts',
  transformDocument: ({ frontMatter, body }) =>
    typeof frontMatter.linkUrl === 'string'
      ? `${body}\n\n[link]: ${frontMatter.linkUrl}\n`
      : body,
}
```

The transforms run in the order the plugins are listed, each over the body the one before
returned, and the front matter is kept byte for byte: it is read before any transform runs,
and it is frozen, so a transform writing to it throws — in strict-mode code, which an ES
module such as `lemmon.config.ts` is; elsewhere the write is ignored. The text they leave is
what the core compiles, and what `lem build` frames a diagnostic in the document against, so
a transform that appends keeps every line and column the author's file has; one that changes
anything earlier moves them. A transform returns as it runs, so an `async` one is refused at
config load. One that throws, or returns anything but a string — a promise among them — or a
string holding a lone surrogate, which has no UTF-8 form, or a body that would give the text
other front matter — a `---` first line in a document that had none, or one closing a block
nothing closed — is an error at the document's front matter, or its start where it has none,
and the build stops before it compiles. Where the closing `---` is the file's last line,
with no newline after it, the body must start with one, which ends that line; the message
says so.

### Assets

`assets(context)` is called once per build, after the transforms and before anything
compiles, and the build waits for it: it may be `async`. The context is `{input, output,
writes, env}`: the input and output directories, absolute; whether the build writes —
`false` for `lem check`; and the build's environment, the values `env("…")` bindings
read, as a frozen copy — so a hook reads a token from it rather than from `process.env`,
and a spec can hand it one. It returns `{manifest?, files?}`:

```ts
import { readFile } from 'node:fs/promises'
import { join } from 'node:path'
import type { Plugin } from '@limulus/lemmon-shell'

/** A script shipped under a hashed name, as a bundler would name what it builds. */
export const scripts: Plugin = {
  name: 'scripts',
  assets: async ({ input }) => ({
    // What `asset("lib/app.js")` and an `[asset]` field resolve to: a finished URL.
    manifest: { 'lib/app.js': '/lib/app.1a2b3c4d.js' },
    // What the build writes with the pages, at the file each url names in the output.
    files: { '/lib/app.1a2b3c4d.js': await readFile(join(input, 'lib/app.js')) },
  }),
}
```

`manifest` maps each logical path — what `asset("…")` names — to the URL the asset ships
at, both non-empty strings, since the core does no path arithmetic (D38); every plugin's
manifest is declared to the core with the others', so `asset()` resolves at compile and an
unmapped path is a compile error with a did-you-mean. `files` maps each url — a path from
`/`, mapped to a file as a page's url is — to a string, written as UTF-8, or bytes. A hook
that throws or rejects, or returns anything else — a field it does not have, a manifest
or files that are not plain objects of those — and a logical path two plugins both map
are each an error at the start of `lemmon.config.ts`, printed without a frame, and the
build stops before it compiles. The files are claimed after the passthrough files and
before any page: a url that names no file, or a file a passthrough file or another
plugin's hook writes — the same file, the same but for case or Unicode form, or a file
where the other needs a directory — is an error at the config's start; a page or produced
file that collides with one is an error at the page's url or the producer's mark,
labelled at the config. `lem check` calls the hook too, and writes none of its files.

`lem dev` reads the config once, so a plugin object outlives every build, and each build
calls its hooks again: anything a plugin caches when its factory runs must stay true
across rebuilds.

### Mark handlers

A handler answers one mark name:

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

A producer may also write files of its own — the stylesheet its marks link to — through its
context's `write(url, content)`, which takes a url as a page's url is, a path from `/`, and
a string or bytes, and works only while the producer runs. A file is queued, checked as a
page's file is, and written after the pages; two pages whose producers write the same bytes
to one file write it once. A url or content of the wrong kind, a url naming no file, and
other content where this producer or another page's wrote already are each an error at the
producer's mark. A clash with a page, a passthrough file, an asset hook's file or another
produced file — the same file, the same but for case or Unicode form, or a file where the
other needs a directory — is an error at the later writer, a page's url or a producer's
mark, labelled at the first.
`write` never throws, and what a producer that then fails had written is dropped,
unremarked.

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
   `www/feed/x/index.md`). Each document then goes through the plugins' transforms.
2. **Assets.** Each plugin's `assets` hook, in order; the manifests merge onto the
   declaration the compile is given, and the files are claimed.
3. **Compile**, in two calls. First the shell's own `ambient.lem` — `Page`, `Build` and
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
4. **Load.** `build` is `{time, generator}` — the clock as RFC 3339 in UTC, and `Lemmon`
   with the core's version. A glob's binding is the front matter of the pages it holds;
   a data file (JSON or YAML) is read from `input` with positions, and one whose path
   leads outside `input` is an error at its binding; `env("X")` is the variable's text.
   Every page no glob holds whose `type:` named its record is carried on its own.
5. **Plan and render.** Every loaded page renders through its layout. A page no glob
   holds and no `type:` names has no schema: it renders its own output, with `page` its
   front matter as read, loaded on its own before it renders. Each of those loads
   re-sends and re-validates the whole envelope, so the cost grows with the untyped
   pages times the size of the site: 18 of 25 pages on the limulus-dot-net port branch
   cost about 0.35 s in all.
6. **Resolve.** As each page renders, its marks are filled by the plugins' handlers, in
   the order the plugins and their handlers are listed. A handler that throws, or returns
   anything but its content, is a warning at the mark, which keeps the content it had —
   the default for a producer's marks, the content so far for a decorator's — so the page
   is correct either way. What a page collects under a name nothing on it marks is a
   warning at the block that collected it, since nothing writes it.
7. **Write.** A url ending in `/` is written as `<output>/<url>/index.html`, any other as
   the file it names; then each file an asset hook built; then each file a producer
   wrote, the bundler's stylesheets among them; and each passthrough file is copied to
   its own path. Nothing in the output is removed. Before the first file is written,
   every page has been rendered and every file checked against every other — page,
   asset, produced, and passthrough: the same file, the same but for case or Unicode form
   (APFS holds those as one), and a file where another needs a directory are each an
   error at the later writer, a page's url or a producer's mark — or the config's start,
   for an asset hook's file — labelled at the first.

It prints every diagnostic as `path:line:col: severity: message`, then each line its span
covers, numbered, with `^` beneath what the span covers there — a span over more than five
lines shows its first three and its last, and a line over 100 UTF-16 units a window of it,
cut with `…` — and then each label on an indented line:

```text
www/layouts.lem:2:15: error: `Post` has no field `titel`; did you mean `title`?
> 2 |   <title><lem.p.titel></title>
    |               ^^^^^^^
lem: 1 error, so nothing was written
```

The lines are the text the build read, which the span was computed against, not the file
read again. With no error it prints `wrote N pages, A asset files and P produced files,
copied M files`, each kind of plugin-built file counted only where a plugin built one.
Any error, at any step, stops the build before it writes a file, and `lem` exits 1;
warnings are printed and the build goes on. A write the file system then refuses — a file
an earlier build left where a directory is needed, say — is reported as
`lem: could not write <file>: …`, exit 1, and the output may hold what was written before
it. A site needs a `site.lem` without front matter in `input`, even an empty one: without
it no glob matches anything.

### Not yet

- **Highlighting.** The shell ships no highlighter, so a code block naming a language
  keeps comrak's plain block and is warned at: ∆Nxd.
- **Assets.** The shell ships no esbuild plugin, so an `asset()` resolves only where a
  site's own plugin's `assets` hook maps it: ∆d1N.
- **Computed mark keys.** A key a template computes and no producer lists keeps the
  default content, unreported: ∆CxX.
- **Plugins.** A plugin has no data hook and no `lem` command, it ships no `.lem` — so
  no wrapper such as `<lem.styles()>` — and a `from plugin("…")` binding is an error at
  the binding: ∆wOn.
- **Generators.** A page whose front matter has `for:` is an error at the key: ∆99b.
- **Drafts.** `draft: true` is built like any page; `--drafts` and leaving drafts out
  are ∆Han, in Someday/Maybe.
- **Second phase.** A feed renders with the other pages, reading none of their output:
  ∆zu0.
- **The rest of the pipeline.** No output transform (∆fwn), no integrity check
  (∆yOJ), and no worker pool or incremental rebuild (∆xBO): every build compiles and
  renders everything in one core, `lem dev`'s rebuilds among them.

## `lem check [--config <path>]`

`lem build`'s steps 1–6 and the checks step 7 makes before its first write: every
document is transformed, the asset hooks run — told the build writes nothing — every page
is rendered and its marks resolved, and every file checked against every other, so it
reports every diagnostic a build would — render errors and collisions among them — in the
same form, and never creates or touches the output directory. A write the file system
would refuse is therefore found only by `lem build`. It ends with `lem: N errors` and exit
1, or `checked N pages, A asset files and P produced files` and exit 0, each kind of
plugin-built file again counted only where a plugin built one.

## `lem dev [--config <path>] [--port <n>]`

Watches `input`, builds the site as `lem build` does, and serves the output at
`http://127.0.0.1:<port>/` — 8080 unless `--port` names another, 0 for one the system
picks — then builds the whole site again each time changes settle, once 50 ms pass with no
other: the window in which chokidar, which watches, drops a second write to one file. The
watch passes over what the walk does: `node_modules`, `.git`, and the output where the
input holds it; and as the walk, it follows no linked directory, so a link to the output
does not rebuild for every file a build writes. A linked file is watched as the link: an
edit reaches a build through the file it names only where that file is under the input
too. On a two-post site, a post edited, then given an error and fixed:

```text
wrote 2 pages, copied 0 files
built in 22 ms
serving dist at http://127.0.0.1:8080/
restart lem dev after changing lemmon.config.ts
wrote 2 pages, copied 0 files
rebuilt in 7 ms
www/posts/first.md:6:6: error: `Post` has no field `byline`
> 6 | <lem.page.byline>
    |      ^^^^^^^^^^^
lem: 1 error, so nothing was written
rebuilt in 7 ms
wrote 2 pages, copied 0 files
rebuilt in 6 ms
```

Each build prints what `lem build` would, then how long it took. One with no error and
every file written tells every page open on the server to reload. One with an error
reloads nothing, since the error stopped it before it wrote, so a browser keeps the last
good pages; nor does one the file system refused partway. A first build that fails is
served all the same, its pages answered with a 404 page carrying the reload client, so
that the fix reloads them. A build that throws — on a file the walk cannot read, such as
the dangling link an editor's lock file is — and a directory the watch cannot read are
printed, and `lem dev` goes on. A call into the core that never returns ends it, exit 1,
saying to restart it, since every later build would load the instance it poisoned;
`lem build` and `lem check` print the first of these lines for it, and exit 1:

```text
lem: the core failed with RangeError: Maximum call stack size exceeded, and cannot run again in this process
lem: restart lem dev to build again
```

The config is read once, so a change to `lemmon.config.ts` needs a restart. Ctrl-C stops
it once the build in flight is done, reloading nothing for that build, exit 0, and
Ctrl-\ ends it at once. A port it cannot listen at, one in use say, is exit 1.

Neither `lem dev` nor `lem build` removes anything from the output, so the file of a page
deleted or moved stays there, and `lem dev` goes on serving it, until the output is
cleaned.

The server listens at the literal `127.0.0.1`, since a `localhost` listen binds `::1`
alone on macOS, where another server at `127.0.0.1` on the same port would go unreported,
and refuses with a 403 a request naming any host but `127.0.0.1` or `localhost` at its
port, so that a page elsewhere whose name has been pointed here cannot read it. It answers
GET and HEAD from the output as the build wrote it — `/feed/` is `feed/index.html`, a
directory asked for without its `/` is redirected to the url with one, and anything else
is 404, a small page carrying the reload client — with a `Content-Type` from `mime-types`
by extension, `Cache-Control: no-cache`, and the config's `dev.headers` on every response;
every other refusal is plain text. An HTML page gets
`<script type="module" src="/.lemmon/reload.js"></script>` before its last `</body>` in
the bytes sent, the file on disk left as built, so the parity harness can read a
`lem dev` output — once the output has been cleaned and built again, since a page
deleted since the last clean build is still there. That client, a file so that a page
allowing only `script-src 'self'` still runs it, opens a WebSocket to `/.lemmon/reload`
and reloads on any message — a WebSocket rather than server-sent events, which hold one
of a browser's six HTTP/1.1 connections to a host for every tab open on it. It opens one
again every second after it closes, and reloads once one opens, so a restarted `lem dev`
reloads the pages still open on it.

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

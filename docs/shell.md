# The Lemmon shell

The reference for `@limulus/lemmon-shell`, the Node/TypeScript shell around the Lemmon core
(`docs/architecture.md` §Shape and §Build pipeline): the `lem` CLI and what it drives. The
package's README introduces it; this is what it does, in full. `lem build` reads a site,
runs its plugins' document transforms and asset hooks, compiles and renders every page
through the WASM core, fills each page's marks with its plugins, checks every link in what
it would write, and writes HTML, the files the plugins built, and passthrough files to the
output directory; `lem check` does all of it but the writing; and `lem dev` builds the
site, serves the output, and rebuilds it on every change, reloading the pages open on it.

```ts
// lemmon.config.ts
import { bundler, esbuild, highlighter, type Config } from '@limulus/lemmon-shell'

export default {
  input: 'www',
  output: 'dist/www',
  origin: 'https://limulus.net',
  served: ['/@eric'],
  passthrough: ['assets/**/*', '**/*.{png,svg,jpg,jpeg,xsd,xsl,txt,pdf}'],
  plugins: [bundler(), highlighter(), esbuild()],
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
case, and a header that is the server's (`SERVER_HEADERS` in
`packages/shell/src/lib/config.ts`) — one it sends, such as `Content-Type`,
`Cache-Control`, `Location` and Node's framing, or one that would describe the body other
than as it is sent: `Content-Encoding`, and `Trailer`, which Node refuses to write beside
a length. So neither side overrides the other.

`origin` is where the site is served, an `http` or `https` URL with nothing after its host
and port, as `https://limulus.net`; it is checked as written, and whitespace, a backslash, a
path, a query, a fragment or credentials are each refused for what they are, since a page's
url is a path from `/` and Lemmon has no path prefix. A document's body is made absolute
against it before a feed reads it (step 5); with none, its links are made paths from `/`.
The link check (step 7) reads it to tell the site's own absolute links — on its host, by
`http` or `https` — from another host's, which it skips; with none, it skips every
absolute link.

`served` lists the paths on the origin that the site's host answers without a file the
build writes — a redirect or a rewrite, as limulus.net's CloudFront function sends
`/@eric` to Mastodon — each a path from `/` as a link writes it, matched exactly: `/@eric`
is not `/@eric/`, nor `/@eric/posts`. A link to one is taken as answered, and nothing else
reads the list. Each is refused at load, for what it is, unless it starts with one `/` and
holds no whitespace, backslash, query, fragment, or `.` or `..` segment, and two that the
URL parser writes as one path (`/café` and `/caf%C3%A9`) are refused as one. None when
omitted. One that names a file the build writes is a warning at the config's start, since
a link there, a fragment into it among them, then goes unchecked.

## The shell's plugins

### `bundler()`

`bundler()` is the shell's default plugin, listed like any other. It produces the three
names the collect tags collect under, none of them keyed, so a layout places what a page's
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

### `highlighter(options?)`

`highlighter()` highlights code blocks with highlight.js, pinned at exactly 11.9.0, since
another release's grammars mark the same code differently — a dependency of about 5.3 MB
unpacked, 1,535 files, with none of its own. It produces `code` for every
language highlight.js has, by name and by alias — `typescript` and `ts`, `plaintext` and
`txt` — and writes each block as markdown-it's documented highlight function does:

```html
<pre><code class="hljs"><span class="hljs-keyword">let</span> x = <span class="hljs-number">1</span>
</code></pre>
```

The spans carry highlight.js's classes, so any of its themes styles them; the shell ships
none, so a site links one, as limulus.net links `hljs-github.css`. A key matches as written,
so a fence opening ```` ```TS ```` is claimed by no producer, though highlight.js would
read it: it keeps comrak's `<pre><code class="language-TS">`, and is warned at with a
did-you-mean, `ts`. So is any language highlight.js lacks; a block naming none keeps
comrak's block unremarked.

`options.exclude`, its one option — any other is refused when the config loads, as
`esbuild()` refuses one — lists keys it leaves to another plugin, each one it would claim
— an unknown one is refused too — and each matched as written, so a language's other names
stay claimed until listed too:

```ts
plugins: [bundler(), highlighter({ exclude: ['rust', 'rs'] }), rustHighlighter()]
```

highlight.js loads its 192 grammars, about 30 ms, when the config calls `highlighter()`,
since the keys it claims are known when the config loads; importing the package loads
none, so a site that does not list it never pays for them. It compiles a language's
grammar the first time a block names it, and highlighting is not cached: on the
limulus-dot-net port branch its eight blocks take about 8 ms in the first build and 1 ms
in each rebuild.

### `esbuild(options?)`

`esbuild()` bundles a site's client scripts in its `assets` hook (D09, D10, D45), with
esbuild pinned at exactly 0.24.0, since another release can write the same bundle
differently and a bundle's bytes name it — a dependency of 148 KB of JavaScript and one
native binary of about 9.3 MB for the platform — and `esbuild-plugin-wasm` pinned at 1.1.0,
28 KB, which writes code of its own into a bundle. Every `index.ts` and `worker.ts` under
the input, found as the build's walk finds files, is an entry, bundled on its own:
minified, an ES module with its source map beside it, an import of `/lib/*` left for the
browser, and a `.wasm` it imports instantiated as a module. Each bundle is written beside
its source, named by a hash of what it ships as — `www/lib/x/index.ts` at
`/lib/x/index.ABCD2345.js` — and a file a bundle imports, the `.wasm`, under
`/assets/immutable/esbuild/`, named as esbuild names it. The manifest maps
each entry's URL as its source names it to the URL its bundle ships at, so a layout writes

```lem
<script type="module" src="<lem.asset("/lib/x/index.js")>" async></script>
```

and a front matter field typed `asset` names one the same way, `script: /lib/x/index.js`.
The styles an entry's imports gather are a bundle too, mapped as `/lib/x/index.css`; two
bundles at one such path — two entries, or an entry's styles and a CSS entry — fail the
hook, naming both sources.

A script names another entry — to import it, or to start it as a worker — by a
placeholder, the entry's URL with `XXXXXXXX` where the hash goes (D10):
`import '/lib/y/index.XXXXXXXX.js'`, `new URL('./worker.XXXXXXXX.js', import.meta.url)`.
Before a bundle is named, each placeholder in a string names the entry at its path, found
from the bundle's own logical path where it is relative, and the file name that entry
ships under — so that entry is named first — is spliced in as written, as limulus.net's
Eleventy build splices the hash in: `'worker.XXXXXXXX.js'` becomes `"worker.ABCD2345.js"`,
the same length, so the map's columns hold. A `.` or `..` segment resolves as a browser
resolves it, but a path holding an empty segment, `'/lib//y/…'`, names no entry, since the
browser would ask for another URL than the entry's. One naming no entry fails the hook at
the source that wrote it, from the root with its line and column as esbuild places its own
errors — or at the bundle, where a `define` put it there — and so does one naming its own
bundle, directly or through the bundles it names, since each would need the other's name
to be named.

Then the bundle is named as that build names one: by the first eight base32 characters of
the SHA-256 of the bundle — its last line still naming its map `index.XXXXXXXX.js.map` —
and of its map, in which the project's own path, which `esbuild-plugin-wasm` writes for
its module, is first made relative to the map. So an edit to a worker alone renames
every bundle that names it, which a CDN holding each hashed name for a year needs, and no
name depends on where the project is, so long as esbuild writes that path into the map as
it is: one holding a `"`, a `\` or a character outside ASCII is escaped there, and stays
absolute, as it does in Eleventy's build. Eleventy's build also writes an unhashed
`/lib/x/index.js` re-exporting each bundle; this one does not, since nothing loads one.

Each option is optional, and one it does not take is refused when the config loads:
`entryPoints`, globs from the input matched as `passthrough` globs are; `external` and
`assetNames`, as esbuild reads them; and `env`, the environment variables the scripts read
as `process.env.NAME`, each replaced by its value in the build's environment — the one
`lem` runs in, so a site keeping a value in a `.env` file loads it in its config,
`import 'dotenv/config'` — and one the environment lacks fails the hook, naming it.
limulus.net keeps Eleventy's asset directory, which a worker's bundle names, and its
map's token:

```ts
esbuild({
  assetNames: 'assets/immutable/11ty/esbuild/[name].[hash]',
  env: ['MAPBOX_PUBLIC_TOKEN'],
})
```

esbuild runs from the config file's directory, wherever `lem` was started, so that its
messages name each source from there, and the hook resolves the directories through
symbolic links first, as esbuild resolves the sources. On the limulus-dot-net port branch
its 37 files — 18 bundles, their maps and a `.wasm` — are byte for byte Eleventy's, with
the `11ty/` asset names above, and so are a copy's elsewhere.
esbuild's errors fail the hook, an error at the config's start carrying esbuild's message,
file and position; its warnings are written to standard error as esbuild words them, and
the build goes on. Every build bundles again, `lem check` too, since `asset()` compiles
against the manifest: on the port branch that is about 0.2 s of each `lem dev` rebuild,
0.37 s against 0.17 s without it. esbuild is loaded the first time the hook runs, so a
site that does not list the plugin never loads it.

## Plugins

A plugin is `{name, transformDocument?, assets?, marks?}`: a name its errors and warnings
call it by, a document transform and an asset hook that run before anything compiles
(D44), and the mark handlers that fill each page's marks after it renders (D42). Any other
field is refused when the config loads, since this shell runs no other hook. The types
are exported: `Plugin`, `DocumentSource`, `AssetsContext`, `Assets`, `PluginHandler`,
`ProducerContext` and `DecoratorContext`.

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
compiles, and the build waits for it: it may be `async`. The context is `{root, input,
output, writes, env}`: the directory of the config file, which `input` and `output` are
relative to — where a tool whose output depends on its working directory, as esbuild's
hashes do, runs from — and the input and output directories, all absolute; whether the
build writes —
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
payload, `{language, info, code}`, as `mark.payload`. Content returned as bytes must be
UTF-8, as a page is; other bytes are a failure, as any other answer that is no content is.

Handlers run for each document's body, which the body pass resolves for `build.bodies`
before any page renders (D46), and then again for its page: `body` on a producer's
context, and on the object a decorator is given after the content, says which.

A producer may also write files of its own — the stylesheet its marks link to — through its
context's `write(url, content)`, which takes a url as a page's url is, a path from `/`, and
a string or bytes, and works only while the producer runs. A file is queued, checked as a
page's file is, and written after the pages; two pages whose producers write the same bytes
to one file write it once, and so do a body and its page. A document that places
`mark("styles")` in its own text therefore gets a bundle of its body's fragments, which
only `build.bodies` links to, beside its page's. A url or content of the wrong kind, a url
naming no file, and other content where this producer or another page's wrote already are
each an error at the producer's mark. A clash with a page, a passthrough file, an asset hook's file or another
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
   it opens with front matter, D41), either in another case — `index.MD` — an error at
   the file's start naming it in lowercase, since the core knows a document by a
   lowercase `.md` alone, and anything else is left alone until a `from` binding names
   it. Each page's front matter is read with the `yaml` package, with a span for every
   field path: `type:` and `for:` are read and stripped, `url` derived where the front
   matter gives none (`feed/x/index.md` at `/feed/x/`, `index.lem` at `/`), and
   `inputPath` added (the path from the config's directory, `www/feed/x/index.md`). Each
   document then goes through the plugins' transforms.
2. **Assets.** Each plugin's `assets` hook, in order; the manifests merge onto the
   declaration the compile is given, and the files are claimed.
3. **Compile**, in two calls. First the shell's own `ambient.lem` — `Page`, `Build` and
   `Code` (docs/language.md §Ambient types) — and every template, under the declaration
   `site` → the bindings of `site.lem`, `page` → `Page`, `build` → `Build`, pages
   rendered through `layout` and placed by `url`, and a generated page's neighbours
   filled into `prev` and `next` — so a page type that declares either declares it as
   an optional of itself or a type it extends, with no default. Page templates are in
   this call too, without their records, so that what one defines is in sight of the
   layouts. The core
   reports the program's `from` bindings and the record each glob collects; every page
   is matched against those globs (with the passthrough glob semantics, so case aside),
   and then all pages — documents and page templates — compile in one call, each as its
   own `type:` or as the record of the glob that holds it. A page two globs match is an
   error at the page, and so is one no glob holds that names no `type:`, since nothing
   would check its front matter or give it a layout (D48): it is placed at its front
   matter, or at the file's start where it has none, and names both fixes — a `type:`,
   or a glob in `site.lem` that collects it. One with `for:` is refused at the key
   instead, in D43's words: a template's generator has no type to make its pages of, and
   a document is no generator at all. A refused page is left out of the second call, and
   the build stops before it; a document whose front matter never closes is not refused
   but compiled without a record, so that the core reports the unclosed front matter.
   Documents compile only in the second call, so what a document (`.md`) defines is
   invisible to every template, with front matter or without. The first call's
   declaration carries the plugins' claims, so the core warns at the marks.
4. **Load.** `build` is `{time, generator, bodies}` — the clock as RFC 3339 in UTC,
   `Lemmon` with the core's version, and no bodies yet, `{}`. A glob's binding is the
   front matter of the pages it holds; a data file (JSON or YAML) is read from `input`
   with positions, and one whose path leads outside `input` is an error at its binding;
   `env("X")` is the variable's text.
   Every page no glob holds is carried on its own, typed by its `type:`, a generator
   with its `for:` key beside its front matter — the value as read, with spans for the
   whole of it, its entry, and the list alone — which the core checks and expands into
   one page per item (D43). A glob's member carries its key too, which the core refuses,
   since no glob can hold a generator.
5. **Plan and render.** After the body pass, below, every loaded page renders through
   its layout, in the plan's order: a generator's pages each by its index, at the url its
   type's default gave it.
   A diagnostic names a generated page by its item — `` `photos/photo.lem` for
   `site.gallery[3]` `` — and a render's runtime error on one is labelled so at the
   generator's `for:` entry.

   **The body pass** (D46) runs first. Each document the load made — each page the plan
   lists whose file is a `.md` — renders its body alone, its own output without its layout,
   and its marks are resolved as step 6 resolves a page's: every handler runs for a
   document's body and then again for its page, told which by its context's `body` (a
   producer's context, a decorator's third argument). It reports only errors, since the page
   resolves the same marks and warns there; a file a producer writes for it is checked and
   written as a page's is, named for "the body of" the document, so a document that places
   `mark("styles")` in its own text gets a bundle of its own fragments, which only
   `build.bodies` links to. Its links are then made absolute against `origin`: each url
   step 7 reads in an element's attributes, and a `<base href>`, is stripped of ASCII
   whitespace; one the URL parser reads on its own is then kept, and any other is resolved
   against the page's url, its query kept; where the config names no `origin`, it is made a
   path from `/` instead, which names the same thing from any page of the site, and a url
   naming a host of its own is kept. A url that is only a fragment is kept where its element
   embeds what it names — any SVG element's `href` or `xlink:href` but an `a`'s, as in
   `<use href="#icon">`, and a `style` attribute's urls, as in `url(#clip)` — since it names
   part of the body itself, which a reader of the body would not load from the origin; an
   `<a href="#fn1">` is resolved like any other. A `<style>`'s text is left as written,
   though step 7 reads it. The body is parsed with parse5, and only an attribute holding a
   url that changes is written anew, with only that url changed in its value, so a
   `srcset`'s descriptors and commas, a `ping`'s spacing and the rest of a `style` say what
   they said; the attribute is double-quoted and its character references decoded, and every
   byte of the body outside such attributes stays as rendered. A url in CSS is written as a
   single-quoted string, and a refresh's url has its quotes percent-encoded, since HTML's
   refresh steps end a quoted url at its quote and a page's url holds `'` as it is. A url
   the URL parser refuses is kept as written and warned at, at the document's front matter.
   A `srcset` is read by HTML's rules with `@prettier/parse-srcset` 3 (11 KB of JavaScript,
   no dependencies), which refuses the whole value where any candidate's descriptors do not
   parse — HTML drops that candidate alone — so such a `srcset` is kept as written, its urls
   unrebased, and the pass says nothing of it: step 7 refuses it, as an error, on the page
   that prints the body. A document whose url names no file is left to its page's render,
   which reports it. Then the core loads again with every body a string in `build.bodies`,
   by its page's url as `url.text()` writes it, so a feed template prints
   `<lem.build.bodies[post.url.text()]>`, escaped once. While the bodies render the map is
   empty: a document whose own body indexes it fails there, at the read, labelled at its
   front matter as rendering the body before any body exists, and a computed binding in
   `site.lem` runs at both loads. An error in the pass stops the build before the second
   load, and one in the second load before any page renders.
6. **Resolve.** As each page renders, its marks are filled by the plugins' handlers, in
   the order the plugins and their handlers are listed. A handler that throws, or returns
   anything but its content, is a warning at the mark, which keeps the content it had —
   the default for a producer's marks, the content so far for a decorator's — so the page
   is correct either way. What a page collects under a name nothing on it marks is a
   warning at the block that collected it, since nothing writes it.
7. **Check** (D47). Once every page has rendered and resolved without an error, every
   link in what the build would write is checked against the files it writes: in each page,
   asset file and produced file whose name ends `.html`, `.css` or `.xml`, read as its
   kind is read — HTML parsed with parse5 as a browser parses it, CSS tokenized with
   `@csstools/css-tokenizer` 4 (CSS Syntax 3; 18 KB of JavaScript), XML parsed with
   `@xmldom/xmldom` 0.9 (about 280 KB), neither with dependencies of its own — and every
   url an asset hook's manifest maps. In HTML a link is a url in an attribute that holds
   one, on the elements of `@11ty/posthtml-urls`' table (`href`, `src`, each `srcset`
   candidate, `poster`, `cite`, `action`, `formaction`, `data`, and each of a `ping`'s urls,
   split at whitespace as HTML splits them), resolved against the first `<base href>`; a
   refreshing `meta`'s url; any SVG element's `href` or `xlink:href` (`use`, `image`,
   `feImage`, `a`); an Open Graph url's `content` (`og:url`, and `og:image`, `og:video` and
   `og:audio` in each spelling); and each `url()` and `@import` in a `<style>` or a `style`
   attribute — what the body pass rebases, but a `<style>`'s. A `srcset` that
   `@prettier/parse-srcset` refuses is an error, once per page for each element, attribute
   and value, as an XML file whose links cannot be read is (below): none of its links can be
   checked, while a browser drops only the candidate at fault and loads the rest. In CSS a
   link is a `url()`, and an `@import`'s string, against the stylesheet's own url, so a
   `data:` URI holding `url(%23clip)` is one link that is skipped. In XML it is an
   `xml-stylesheet` instruction's `href` before the root, any element's `href` or
   `xlink:href`, and the text of an RSS `link` without one, an RSS channel image's `url`, a
   sitemap `loc` or `*_loc`, and a `guid` unless `isPermaLink="false"`, each against its
   nearest `xml:base`; an Atom `id` is no link, and a feed's escaped content is checked on
   its page. An XML file is an error where the parser reports any error or warning in it but
   a U+FFFD, where it declares an encoding other than UTF-8, or where it is not UTF-8, since
   none of its links can be read; the parser expands no entity a document declares.
   JavaScript, an inline script's text, an SVG paint's `url(#…)`, an HTML page whose name
   does not end `.html`, and passthrough files of any kind are not read. Every tree is
   walked with a stack of its own, so no nesting overflows the call stack.

   A link the URL parser refuses, one in a scheme other than `http` and `https`, one on
   another host than `origin`'s, and a path `served` lists are skipped; `http://` and
   `https://` on the origin's host are both the site's own. Any other names the file its
   path maps to, as a page's url does, which the build must write exactly: a file that
   differs only in case or Unicode form is no match, since a host that matches names
   exactly serves nothing there, though macOS's file system and so `lem dev` would serve
   it; nor is a directory whose `index.html` it writes — `/tils` is not `/tils/`, whose
   relative links resolve elsewhere. A fragment into an HTML page the build writes must
   name an element of it, by its `id` or an `a`'s `name`, as written or percent-decoded,
   with the empty fragment and `top` the top and a `:~:` text directive left off — so one
   only a script would satisfy, an id it inserts or a route such as `#/settings`, is an
   error, which nothing exempts yet; one into anything else is not checked. Each fault is
   an error, once per page per url it resolves to, naming each way it is written there and
   every attribute it stands in, with how often:

   ```text
   www/b.md:2:1: error: `b.md` links to "/a/#fn2" in `<a href>`, and `a/index.html` has no element whose `id`, or an `a` whose `name`, is `fn2`
   > 2 | title: B
       | ^^^^^^^^
     www/a.md:2:1: the writer of `a/index.html`
   ```

   It stands at the page's url key, or its front matter where it has none, a generated
   page's `for:` entry, a producer's mark for its file, or the config's start for an asset
   file or a manifest url, and is labelled at the writer of the file a near miss names, or
   of the page a missing fragment points into. A manifest url that names no file is said
   once, at the config, and no page that prints it is faulted for it again. A `served`
   path that names a file the build writes is a warning at the config's start, since a
   link there goes unchecked. On the limulus-dot-net port branch the check reads about
   4,100 links in about 65 ms in `lem build` and 45 ms in a `lem dev` rebuild. `lem dev`
   writes nothing while a page links to a page the site does not have, until it does.
8. **Write.** A url ending in `/` is written as `<output>/<url>/index.html`, any other as
   the file it names; then each file an asset hook built; then each file a producer
   wrote, the bundler's stylesheets among them; and each passthrough file is copied to
   its own path. Nothing in the output is removed. Before the first file is written,
   every page has been rendered, every link checked (step 7), and every file checked
   against every other — page, asset, produced, and passthrough: the same file, the same
   but for case or Unicode form (APFS holds those as one), and a file where another needs
   a directory are each an error at the later writer, a page's url or a producer's mark —
   or the config's start, for an asset hook's file — labelled at the first.

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
it. A site needs a `site.lem` without front matter in `input`, since its globs type every
page that names no `type:` of its own: without it no glob matches anything.

### Still to build

- **Computed mark keys.** A key a template computes and no producer lists keeps the
  default content, unreported: `∆CxX`.
- **Plugins.** A plugin has no data hook and no `lem` command, it ships no `.lem` — so
  no wrapper such as `<lem.styles()>` — and a `from plugin("…")` binding is an error at
  the binding: `∆wOn`.
- **Drafts.** `draft: true` is built like any page; `--drafts` and leaving drafts out
  are `∆Han`, in Someday/Maybe.
- **The rest of the pipeline.** No output transform (`∆fwn`), and no worker pool or
  incremental rebuild (`∆xBO`): every build compiles and renders everything in one core,
  `lem dev`'s rebuilds among them.

## `lem check [--config <path>]`

`lem build`'s steps 1–7 and the checks step 8 makes before its first write: every
document is transformed, the asset hooks run — told the build writes nothing — every page
is rendered and its marks resolved, every file checked against every other, and every link
checked, so it reports every diagnostic a build would — render errors, collisions and
broken links among them — in the same form, and never creates or touches the output
directory. A write the file system would refuse is therefore found only by `lem build`.
It ends with `lem: N errors` and exit 1, or `checked N pages, A asset files and P
produced files` and exit 0, each kind of plugin-built file again counted only where a
plugin built one.

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

The config is read once, so a change to `lemmon.config.ts`, or to a plugin module it
imports, needs a restart. Ctrl-C stops it once the build in flight is done, reloading
nothing for that build, exit 0, and Ctrl-\ ends it at once. A port it cannot listen at,
one in use say, is exit 1.

Neither `lem dev` nor `lem build` removes anything from the output, so the file of a page
deleted or moved stays there, and `lem dev` goes on serving it, until the output is
cleaned. So do the bundles `esbuild()` wrote before: each edit to a script ships it under a
new hashed name, and the old ones stay beside it.

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

## Running from a checkout

The `lem` bin is `packages/shell/bin/lem.ts`, which Node runs directly, under the `source`
export condition so that this package and `@limulus/lemmon-core` load from their `src/`. A
site can therefore depend on a checkout with `file:` and see every edit without a build:

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
`npm run build` in the checkout first.

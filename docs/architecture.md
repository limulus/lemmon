# Lemmon architecture

Working distillation of the "Shell & Core" artifact for implementation sessions. Decision
IDs (D01…) refer to `docs/decisions/README.md`.

## Shape

Two halves, one contract:

- **Core** — Rust workspace compiled to WebAssembly: scanner, parser, bytecode compiler
  (which is also the type checker, D31), VM, document-mode segmenter/splicer, and comrak
  for markdown. Pure:
  `(program, data, page) → (page bytes, marks and fragments, diagnostics)`. No I/O, clock,
  or env.
- **Shell** — Node/TypeScript: content walker, YAML front matter (with positions), plugin
  host, esbuild driver, worker pool, dev server (the site's `dev.headers`, live reload),
  output writer (hashing conventions), CLI (`lem`). Everything site- or host-specific is
  site code: limulus.net's sprite and link-post appender are plugins beside its config,
  and its photos and revisions load as typed data files its own scripts write (D12, as
  built). Markdown is _not_ a plugin (D08).

The same `.wasm` runs in the build, the LSP (Node process), a browser playground, and —
later, unchanged — a Lambda (`ContentSource` seam, D14).

## WASM API (`@limulus/lemmon-core`)

```ts
class Core {
  compile(files: {path, text, record?}[], host?: HostDecl):
      {diagnostics, payloads, sources}
      // changed files only; per-file units, cached (D25). `record` names a document's
      // schema; `host` names the ambient values and their types (D30); `payloads` are
      // the code blocks' `{id, language, info, code}`, each once (D42); `sources` the
      // program's `from` bindings, `{name, kind, literal, collects?, path, span}`
  loadData(input: DataInput): Diagnostic[]            // records, data files, build info
  upsertRecord(collection, record) / removeRecord(id) // dev server + future on-demand
  plan(): {path, index?, url?}[]                      // every loaded page (D43)
  render(page: PageRef): { html (bytes), marks, fragments, deps }
      // a page through its entry, or its body alone (D46); marks are named ranges the
      // host's handlers fill, fragments what the collect tags gathered under each name
      // (D42); deps ∆xBO
  exportModule(): Uint8Array / importModule(bytes)    // precompiled program (Lambda cold start)
  analyze(path, offset): { diagnostics, completions, hover, definition }  // LSP
  format(path): string
}
// HostDecl = {ambient: {[name]: TypeName | {bindingsOf: path} | null},
//              document?: AmbientName, entry?: FieldName,
//              url?: FieldName, neighbours?: [FieldName, FieldName],  (D43)
//              assets?: {[logicalPath]: url},   (D36, the manifest D38)
//              marks?: {name, keys?, payload?, keyedBy?}[]} (the claims, D42)
// reserved optional host import (D08): renderMarkdown?(text: string): string
```

Boundary rules: few large calls; program and data stay resident in the instance; bulk data
crosses as one UTF-8 JSON buffer (rmp-serde if profiling demands) — except a render's
page, which crosses as bytes beside a JSON header of its marks and fragments, since
escaping the page into that JSON and parsing it back is more than half of a `render` call
(the spike's page: 108 µs to 49 µs) and a mark's offsets name the page's own bytes (D42);
one instance per worker thread, nothing shared. Whole-site data ≪ 10 MB. The core embeds
no highlighter (D42): removing syntect took the staged `.wasm` from 4.21 MB to 2.26 MB.

Ambient declarations (D30, D36): the core knows no type or value by name. The host passes
its environment with `compile` — the declarations as Lemmon source (the shell ships
`ambient.lem`, declaring `Page`, `Build`, and `Code`) and `host`, which binds each
ambient value name to a declared type, to the top-level bindings of a file (`site` →
`site.lem`), or to nothing at all, which names the value without typing it. `document` says which of them
a file's `record` retypes, and `entry` names the field a page is rendered through
(`layout`). `url` names the field `plan()` reports a page's url from, and `neighbours`
the two fields the loader fills on a generated page with the pages beside it (D43): each
is checked at link, with the url and entry fields, since none of those field names is a
fact of any unit. The declaration is resident like the units, so a
call that changes nothing about the environment omits it, and what each unit was
compiled against is one of its facts: a host that rebinds a name leaves the units that
read it stale, named for recompiling, as a changed signature does (D25). A core given no
declaration has no ambient values, and `page` in a template is an undefined name.

Each document or page template carries its `record`: the element type of the `from`
binding whose glob matched it, or the type a `type:` key names, which the checker
requires to extend that element type — or, where no glob matched, the document value's
type. The unit depends on that type as a compile-time fact (D25). A document
without one compiles with `page` unknown-typed, which is how the spike renders today.

Code blocks are marks (D42). Every code block but a `math` fence, which comrak renders
before consulting any seam, is a `code` mark keyed by its language word, with the info
string and the literal as a payload the document reports once, by content hash, when it
compiles; the block's default content — comrak's own `<pre><code class="language-x">`,
escaped — stands in the page, and a render's mark names the range and the payload id.
The core's adapter fills comrak's highlighter seam with comrak's own markup and a
sentinel before each `<pre>`, which is how the element is found in the output. The core
embeds no grammars and highlights nothing: the host's producers do, after render, and
the claims they declare are what the compiler checks a language word against, with a
did-you-mean, recording the answer about that word as the document's compile-time fact.
A host that declares no producer for a language gets the block as comrak writes it and
a warning at the word.

∆TNT built the `compile`/`loadData`/`render` subset of that class (plus `version`), and
settled the spike shapes the sketch left open — each chosen so the full form is additive,
not a break:

- `compile` returns `{diagnostics, payloads, sources}`: the diagnostics about the files
  passed and any resident unit the relink found stale, and the payload of every code
  block the files that compiled write, `{id, language, info, code}`, each once (D42). A
  file that compiles beside one that does not goes resident without the program
  relinking and is not compiled again once the other is fixed, so its payloads are
  reported by the call that compiled it, relink or no; the host keeps them by id.
  `sources` is every `from` binding of the program a clean relink installed, in the
  order its bindings file declares them, and empty from a call that installed none:
  `{name, kind, literal, collects?, path, span}`, where `kind` is `file`, `plugin` or
  `env`, `literal` the string the source names, and `collects` the record a glob's
  pages are — present exactly when the binding's type is a list of a record extending
  the document type, strictly, since a list of the document type itself is a compile
  error at the binding's type — so the core, which knows the types, tells a glob from a
  data file and the host does not repeat the rule. `path` and `span` place the
  binding's name, where a host reports a source it cannot fulfil (∆muD). ∆Qr2 adds
  `facts` beside them. A declaration whose claims cannot be merged — two naming
  different payloads or key fields for one name, or a key field with no payload — is a
  thrown caller bug.
- `DataInput` is `{ambient, sources?}` (D37, ∆iCW): `ambient` is bare JSON by any name
  the host bound — an untyped host's values pass through as sent — and `sources` fulfils
  each `from` binding by name: a list of `{path, value, spans}` for a glob, one such for
  a data file, `{value}` for a plugin or env source, where `spans` maps a field path to a
  complete boundary span the host computed from the YAML it parsed; the shell strips the
  `type:` key it consumed. The core validates every value against the program, runs the
  defaults and computed bindings, tags each record, and keeps the result resident — the
  store every render binds — returning the load diagnostics, each placed by the host's
  positions or at the declaration that expected the value. The store lives with the
  program it was loaded against: a load that reports an error leaves the previous data
  bound, stale until fixed, as a failed compile leaves the program, and a compile that
  installs a new program drops it — `loadData` again before `render`. A load runs
  against the program that linked, under the declaration it linked with; an envelope
  the program cannot be loaded from — a name the host does not bind, a source no `from`
  binding asked for, a document never compiled, a load before any compile has succeeded
  or after the declaration changed without one succeeding — is a thrown caller bug. The
  asset manifest rides on the declaration, not on the envelope: `asset("…")` is answered
  while compiling (D10, D38), so it has to be there by then, and a rebuild that re-hashes
  an asset is a change to the declaration. So do the mark claims (D42), for the same
  reason: a document's language words are checked while it compiles. A file passed to
  `compile` may carry `recordSpan`, the byte offsets of the `type:` key's value, so a
  diagnostic about the record points at the key; offsets that are not a span of the
  file's text are refused as a caller bug, since a line index panics on them.
- `PageRef` is `{path, index?, body?}`: a path names a document or page template, `index`
  one page of a generator, counted from zero in its list's order, and `body` asks for a
  loaded page's own output alone, its entry field never called (D46). `plan()` lists every
  loaded page with the url the host-named field holds (D43): each collection's in the
  order the loader builds them, then the envelope's `pages` in their order, a generator
  expanded where it stands, `index` present exactly on a generated page. `DataInput`
  gains `pages`, `{path, value, spans}` for every page no glob holds, and any page's
  entry may carry its `for:` pair as the shell parsed it, `for: {value, spans,
  valueSpans}` — `spans` keyed from the value as any table is, and `valueSpans` each
  entry's value alone, so a diagnostic about the list underlines the list. A glob's
  member may carry one too, which the load refuses at the key. A page the envelope did
  not carry is not loaded: `plan()` omits it, and rendering it is a thrown caller bug, as
  is rendering a generator's path with no `index`, an `index` past the pages it made, or
  an `index` for a file that is no generator. A message names a generated page by its
  item, in the language's own index syntax — `` `photos/photo.lem` for
  `site.gallery[3]` `` — in the core and the shell alike.
- `render` returns a four-byte little-endian length, that many bytes of
  `{marks, fragments, diagnostics}` as JSON, then the page's UTF-8 bytes. A failed render
  sends the diagnostic, null marks and fragments, and no page section at all, which is
  what tells it from a page that rendered empty. A mark is `{name, key?, payload?,
  payloadId?, start, end, path, span}`, its offsets naming the page's bytes: `key` is
  what a producer is chosen by, `payload` the JSON a `mark()` call built, `payloadId`
  the id of the payload `compile` reported for a code block, and a field with nothing to
  say is absent. A payload's JSON is what JSON has a form for, with markup, a URL, an
  asset and a datetime (RFC 3339) as strings, a duration as its whole seconds, and a
  record as the object of its fields in declared order; the VM refuses, at the call, a
  payload holding a mark, a function, or a float that is not a finite number. A
  fragment is `{text, path, span}`. `deps` is omitted until ∆xBO, not stubbed. A loaded
  page renders with the host's document value bound to its own record from the store,
  its file's output passed as `body` to the entry field's function, in one run of the
  VM; any other file — a template with no front matter, or a document or page template
  no collection matched and no `type:` named — renders its own output, reading the value
  the envelope carried, if it did, as sent (D37). With `body`, a loaded page renders as
  far as that output and stops: what its layout would be given, with the marks and
  fragments standing in it and its `uniqueId`s counted as the page counts them, since the
  page renders its body first; a file that is no page renders as it does without it.
- A `Diagnostic` is `{path, span: {start, end, range}, severity, message, labels}`. The
  span carries byte offsets (invariant 8) *and* `range`, the same two points as
  `{line, character}` LSP positions — zero-based, `character` in UTF-16 code units. The
  core computes them because it holds the bytes; the shell and the language server would
  otherwise each re-encode the source to turn a byte offset into a UTF-16 column.
  `severity` is `error` or `warning` — a program is still produced
  when the only diagnostics are warnings — and `labels` are the secondary spans, each
  `{path, span, message}`, pointing at the same kind of thing the primary span does (a
  name, not a whole definition) (∆VNk). A `code` would be an additive field; nothing
  consumes one yet. `compile`'s diagnostics describe the files passed to that call, plus
  any resident unit the relink found stale; a resident file's warnings are reported once,
  by the call that compiled it, so a shell that shows warnings keeps them per path.
- Errors split by kind: anything wrong with a template or document is a returned
  diagnostic; a thrown `JsError` is reserved for caller bugs — a malformed JSON envelope,
  a `compile` naming the same file twice, rendering before a compile has succeeded,
  rendering a path the program does not have — which have no source span to point at.
- `compile` really does take changed files only: units stay resident in the `Core`, each
  call compiles what it is given against the resident export index and relinks. A file
  that fails keeps its previous unit and the previous module — stale until fixed, failure
  reported, and the index goes back to describing the units that are actually resident.
  Deleting a file has no boundary call yet (nothing needs it before the shell exists);
  the link step's unresolved-import diagnostic is what makes adding one safe.
- A definition of a name a resident file already exports is refused: the offering file is
  the one blamed, whatever its path sorts as, and it yields no unit — so the collision is
  reported once, by the call that introduced it, and the last good program still renders.
  A shell that shows errors keeps them per path, as it does warnings.
- A caller left stale by a signature it never saw change is a link diagnostic pointing at
  the caller, not a silent relink: named arguments are reordered against the parameter
  list at compile time, so recompiling only the callee would otherwise swap a caller's
  arguments with nothing reported. That check is what makes `Unit::facts` load-bearing
  rather than bookkeeping, and it names exactly the file the shell must recompile.

## Build pipeline

1. **Discover** (shell) — walk input, classify (document/template/passthrough/ignored),
   parse front matter with positions, load `lemmon.config.ts`. A data file is read when a
   `from` binding names it.
2. **Providers** (shell) — plugin data hooks; each plugin's `transformDocument` over
   every document's body (the link-post reference appender), in plugin order, the text it
   leaves what compiles and what diagnostics are framed against; and each plugin's
   `assets` hook (the esbuild pipeline), awaited once per build, whose merged manifest is
   handed to `compile` on the host declaration because `asset()` is answered there (D10,
   D38) — its values the URLs assets shipped at, since the core does no path arithmetic
   — and whose files are written with the pages (D44).
3. **Compile** (core) — every changed `.lem`/`.md` → its own bytecode unit, then a link step
   joins the units into the module (D25, §Separate compilation); document prose rendered by
   comrak here and stored in the document's unit (markdown never runs at render time),
   each code block a `code` mark whose payload `compile` reports once, by content hash
   (D42).
   Type checking is not a pass of its own: the walk that lowers an expression is the walk
   that types it (D31), so a type error is a compile diagnostic like any other and a file
   with one yields no unit.
   Templates without front matter compile first, `site.lem` among them: the shell matches
   documents and page templates — `.lem` files with front matter (D41) — against the
   `from` globs that `site.lem` reports, which are the `sources` that first `compile`
   returns with the record each collects, then compiles each with its `record` (D30).
4. **Load** (core) — validate records against schemas, run the defaults and computed
   bindings, resolve reference fields (D37) and `[asset]` fields, the latter against the
   manifest the declaration carried into step 3; then load the pages no glob holds,
   expand each generator where it stands into one page per item of the list its `for:`
   pair names — the pair checked against the program, the front matter validated once,
   the item preset into its field and the type's defaults run for each — with its
   neighbours filled once every page of it is built, and refuse two pages at one url
   (D43). Failures point at the YAML positions the shell sent with the data.
5. **Plan** (core) — list every loaded page with its url, from the field the host named
   (D43); deps come with ∆xBO. A file with no record — a document or
   page template no glob matched and no `type:` named — is no page: `plan()` omits it,
   and the shell renders it with `render({path})` at the url it derives.
6. **Render × N** (core) — first the body pass (shell, D46): each document the load made
   renders its body alone, `render({path, body: true})`, its marks resolved as step 7
   resolves a page's and its links made absolute against the config's `origin`, and the
   shell loads again with every body a string in `build.bodies`, by its page's url. Then
   every page: body → the entry function → the page as bytes + its marks (name, range,
   payload id, span) + the fragments collected under each name + dependency set. Parallel
   via shell workers.
7. **Resolve** (shell) — for each mark, the producer for its name and key, then its
   decorators in plugin order, over the page as the core returned it; a code block's
   handlers given the payload `compile` reported, kept by id (D42). The handlers are
   the plugins' (§Shell plugin API), the shell's `bundler()` and `highlighter()` among
   them where the site lists them.
8. **Check** (shell) — every link in the HTML, CSS and XML the build would write, and
   every url the asset hooks mapped, against the files it writes, fragments included
   (D47).
9. **Write** (shell) — HTML, the files the asset hooks built — the script bundles
   `esbuild()` builds — the files producers wrote — the per-page CSS bundles `bundler()`
   writes under `/assets/immutable/bundle/` — and passthrough.

What `lem build` (∆muD) runs is steps 1–9, step 2 but its data hooks, in one core on one
thread. `lem check` runs the same and stops before the first write, so it reports every
diagnostic a build would, render errors, output collisions and broken links included; it
never touches the output directory, so a write the file system refuses is found only by
`lem build`.
`lem dev` runs `lem build`'s steps over the whole site again each time changes to the
input settle, serves the output, and reloads the pages open on it after a build with no
error and every file written; it reads `lemmon.config.ts` once, so a change to that needs
a restart. Step 3's first call compiles every `.lem` — page templates too, without their
records, so a layout sees what they define — and its second the pages as
their records; a document compiles only in the second, so no template sees what a
document defines. Every page is rendered and its output file checked against every
other — the same file, the same but for case or Unicode form, or a file where another
needs a directory — and every link in what it would write checked (step 8, below) before
the first is written, so any error stops the build before it writes; a write the file
system then refuses is reported, and the output may hold what was written before it. A
page no glob matched and no `type:` named renders with `page` its own front matter, each
such page loaded on its own before it renders, since a load binds one document value;
each of those loads re-sends and re-validates the whole envelope, every body in
`build.bodies` among it, so the cost grows with the untyped pages times the size of the
site — 18 of 25 pages on the limulus-dot-net port branch cost about 0.35 s in all while
they were untyped; it types every page now. Step 7 runs as each
page renders, with the handlers of the plugins `lemmon.config.ts` lists, whose claims step
3's first call declared — an empty list for a site listing none, which warns at every mark.
A plugin list two producers would answer one mark under, or whose handlers name two
payloads or key fields for one name, is refused at config load, naming both plugins, and
so are keys for a name no handler keys by a payload field; a handler that throws, or
returns anything but its content, is a warning at its mark, which keeps the content it
had; and fragments a page collects under a name nothing on it marks are a warning at the
first of them. A producer writes a file of its own through its context's
`write(url, content)`; step 9 writes the pages, then the files the asset hooks built,
then those, then the passthrough files. A url or content of the wrong kind, a url naming
no file, and other bytes where a producer wrote already are each an error at the
producer's mark; the same bytes two pages' producers write to one file are written once.
A produced file is checked as a page's is, against every page, passthrough file, asset
hook's file and other produced file — the same file, the same but for case or Unicode
form, a file where the other needs a directory — an error at the later writer, a page's
url or a producer's mark, labelled at the first. A producer that fails
writes nothing. The shell's default plugin, `bundler()`, is listed like any other —
`plugins: [bundler()]` — and produces `styles`, `scripts` and `head`, each without keys:
what a page's `lem-style` blocks collected, joined by newlines in collection order, is
written to `/assets/immutable/bundle/<name>.css`, its name the first ten characters of its
SHA-256 in base64url (Eleventy's bundle form), and every `styles` mark becomes the
`<link>` to it, or nothing where the page collected no styles; every `scripts` mark
becomes a module `<script>` per URL the `lem-script` tags collected; and every `head`
mark the `lem-head` fragments, joined by newlines. Pages that collect the same styles
share one bundle. The shell's highlighter, `highlighter()`, is listed the same way —
`plugins: [bundler(), highlighter()]` — and produces `code` for every language
highlight.js 11.9.0 has, by name and by alias (`typescript` and `ts`, `plaintext` and
`txt`), each key matched as written, so `TS` is none of them; its `exclude` option leaves
the keys it lists to another plugin's producer. Each block it claims becomes
`<pre><code class="hljs">…</code></pre>` with highlight.js's spans inside — the markup
of markdown-it's documented highlight function, which limulus.net uses — and a block in
any other language, or naming none, keeps comrak's. The version is pinned exactly, since
another release's grammars mark the same code differently. No output is cached by payload
id: highlighting limulus.net's eight blocks takes about 1 ms of a 160 ms rebuild, and
8 ms in the first build, which compiles the grammars.

Step 6's body pass (D46) runs once the load has succeeded. Each page `plan()` lists whose
file is a document (`.md`) renders with `render({path, body: true})` — its own output, its
layout never called, with its marks and fragments and its `uniqueId`s counted as its page
counts them — and its marks are resolved by the handlers a page's are, so a code block is
highlighted as on its page and a producer's file is checked and written as a page's is,
named for the body, the same bytes its page writes written once. Every handler so runs for a
document's body and then for its page, told which by `body` on a producer's context and in a
decorator's third argument; content it returns as bytes must be UTF-8, on a page or a body
alike, or it is a handler's failure, its mark keeping what it had. The pass reports only
errors: a handler that fails on a body, and fragments a body collects and nothing in it
places, are warned at by its page, which resolves the same marks over the same fragments.
Each body is decoded as UTF-8, a leading U+FEFF kept, and its links rebased for reading on
another page, by the rule Eleventy's HTML base plugin applies to a feed's content over the
attributes `@11ty/posthtml-urls` walks for it (D46): parsed with parse5 — `<noscript>`
content included, `<template>` content walked, a namespaced attribute such as `xlink:href`
left — and each url in an `href`, `src`, `srcset` candidate and the rest that the URL parser
cannot read on its own resolved against the page's url on the site's origin,
`lemmon.config.ts`'s `origin`, or, where it names none, made a path from `/`, a url naming a
host of its own kept; only an attribute that changes is rewritten, so the rest of the body
stays as rendered. A url the URL parser refuses is kept as written and warned at, at the
document's front matter, as is a `srcset` candidate the rewrite drops; a document whose url
names no file is left to its page's render, which reports it. Each body is kept by its
page's url, the string `plan()` reports, which is what `url.text()` writes; the shell then
loads again, `build.bodies` carrying them, where the first load carried `{}`. So a
document's own body reads an empty map in the pass — an index into it is a runtime error
labelled as rendering the body before any body exists — and its page, rendered after, reads
every body; a computed binding runs at both loads. An error in the pass stops the build
before the second load, so that no page, feed or binding reports a body's error again as the
body it lacks, and an error in the second load stops it before any page renders. Page
templates, generated pages and documents no record typed have no body in the map.

Step 8 (D47) runs once every page has rendered and resolved with no error, so that it
reports the author's fault and not one downstream of it: a page that failed has claimed its
file and holds no links. It reads, in memory, every page, asset file and produced file the
build would write, by its extension, walking each tree with a stack of its own: HTML parsed
with parse5 as a document with scripting off — the parse the body pass uses — whose links
are the attributes `@11ty/posthtml-urls` walks (the table the body pass rebases, a `srcset`
candidate by candidate, a `ping` split at whitespace as HTML splits it) with `<base href>`
setting the base rather than being checked, a refreshing `meta`'s url, any SVG element's
`href` and `xlink:href`, an Open Graph url property's `content`, and the `url()`s of a
`<style>` and a `style` attribute; CSS tokenized by `@csstools/css-tokenizer` (CSS Syntax
3), its `url()`s and `@import`s; XML parsed by `@xmldom/xmldom`, an `xml-stylesheet`
instruction's `href` before the root, any element's `href` and `xlink:href`, and the text of
an RSS `link` without `href`, an RSS channel image's `url`, a sitemap `loc` or `*_loc`, and
a permalink `guid`, each against its nearest `xml:base`. An XML file is an error where the
parser reports any error or warning in it (a U+FFFD aside), where it declares an encoding
other than UTF-8, or where it is not UTF-8, since its links cannot be read. An Atom `id` is
no link, and a feed's escaped content is its page's, checked there. JavaScript, an HTML page
whose file does not end `.html`, passthrough files and every other kind are targets only. A
link the URL parser refuses, one in another scheme than `http` or `https`, and one on
another host than the config's `origin` — every absolute one, where it names none — are
skipped, and so is a path the config lists as `served`, one its host answers without a file
(a redirect, a rewrite); the origin's host on either scheme is the site's own. Any other
names the file its path maps to as a page's url does, which a page, asset file, produced
file or passthrough file must claim exactly: one differing only in case or Unicode form is
an error that names it, since a host matching names exactly serves nothing there, and so is
a dot-less link to a directory whose `index.html` is written. A fragment into an HTML page
the build writes must name an element of it by HTML's rule — an `id`, or an `a`'s `name`, as
written or percent-decoded, `top` and the empty fragment the top, a `:~:` text directive
dropped — so one only a script satisfies is an error too; one into anything else is not
checked. Every url an asset hook's manifest maps must name a claimed file too, and one that
does not is said once, at the config, and not again at each page that prints it. Each fault
is an error, one per page per url it resolves to, naming each way it is written and every
attribute it stands in, at the page's writer — its url, its generator's `for:` entry, a
producer's mark, the config's start for an asset file or a manifest entry — labelled at the
writer of the file a near miss names, or of the page a missing fragment points into. A
`served` path the build writes a file at is a warning at the config's start. On the
limulus-dot-net port branch the check reads about 4,100 links in 59 pages and 10
stylesheets, in about 65 ms in `lem build` and 45 ms in a `lem dev` rebuild.

Step 2 runs once discovery has read the site, before anything compiles. Each document whose
front matter read — not a page template, which has no markdown and whose definitions every
layout compiles against — is handed to every plugin's `transformDocument` in the order
listed, as its path, its front matter as the core loads it (frozen, so a write to it throws
in strict-mode code and changes nothing) and its body, and the body each returns is the next
one's; the front matter is kept byte for byte, so its spans hold. A transform that appends
keeps every position the author's file has, and one that changes anything earlier moves
them. A transform is synchronous: an `async` one is refused at config load, and one that
throws or returns anything but a string the core can be sent — or a body that would give the
text other front matter, a `---` first line where it had none, or a first line other than a
newline after a closing `---` that ended the file — is an error at the document's front
matter, which stops the build before it compiles. Then each plugin's `assets` hook is
awaited, in order, given the project's root — the config file's directory, which esbuild
runs from, so that its messages name each source from there (D45) — the input and
output directories, whether the build writes (`lem check` does not) and the build's
environment, the values `env("…")` bindings read, and returning `{manifest?, files?}`;
the manifests merge, a logical path two plugins both map an error naming both. A hook
that throws or returns anything else is an error at the start of `lemmon.config.ts`,
which lists the plugin and is printed without a frame, since no step reads it as text.
The hooks' files are claimed after the passthrough files and before any page: one that
names no file, or collides with a passthrough file or another plugin's, is an error at the
config's start, and a page or produced file that collides with one is an error at that
page's url or mark, labelled at the config. Any error in step 2 stops the build before it
compiles. `lem dev` reads the config once, so a plugin object outlives every build, and
each build calls its hooks again. The shell's esbuild plugin, `esbuild()`, is one: it
bundles every `index.ts` and `worker.ts` under the input with esbuild 0.24.0 —
minified ES modules with source maps, `/lib/*` left to the browser, a `.wasm` import
instantiated by `esbuild-plugin-wasm` 1.1.0, each bundle beside its source as
`[dir]/[name].[hash]` — and maps each entry's URL as its source names it,
`/lib/x/index.js`, to the URL its bundle ships at. Each bundle is named by what it ships
as, as limulus.net's Eleventy build names one: each `XXXXXXXX` placeholder in it takes
the file name the entry it names ships under, that entry named first; its map names the
root's path relative to itself; and its `[hash]` is eight base32 characters of the
SHA-256 of the two, so a loader is renamed with its worker (D09, D10, D45). It bundles on
every build, about 0.2 s of a limulus.net rebuild. With the site's `11ty/` asset names
its files come out byte for byte Eleventy's, wherever the checkout is, so long as esbuild
writes its path into the map as it is: one holding a `"`, a `\` or a character outside
ASCII is escaped there, and stays absolute, on both sides.

- Step 2's data hooks are not yet run (∆wOn), so a `plugin("…")` source is an error at
  its binding.
- An output transform is not yet run (∆fwn): one would run over each page between steps
  7 and 8, since the check reads what is written.

Dev incrementality, not yet built (∆xBO, ∆Qr2), comes from two reported sets, and is
built only if a full rebuild of limulus.net takes over a second. Render-reported deps:
template edit → recompile one file, re-render dependents; content edit → `upsertRecord`,
re-render readers; TS edit → esbuild incremental, re-render referencing pages.
Compile-reported facts (D25): the shell keys each file's unit on its text plus the facts
it consumed, so a document edit recompiles that document and nothing else — its layouts
are unchanged units, not recompiled ones.

## Separate compilation (D25)

The compile unit is the file. Each `.lem`/`.md` compiles to a **unit** with its own constant
pool (document prose included) and symbolic cross-file references — named functions, types,
pages — and a link step resolves those to indices when it joins units into the `Module` the
VM runs. Linking is a pass over plain data; it is what makes relinking the whole site after
a one-file edit cheaper than recompiling one file.

A unit depends on its text and on the **compile-time facts** it consumed, nothing else:

- signatures and types of the project-flat and plugin names it references (not bodies);
- for a document, the record type the shell compiled it with (D30);
- what the host bound each ambient value it read to (D36);
- the asset-manifest entries `asset()` was checked against (D10);
- for a file that names a mark, or a document whose code names a language, what the
  host's claims said about that name and that key — the answer itself, not a digest of
  the claim, so a producer that starts covering one language leaves the files that named
  it stale and not the files that named another (D42).

The compiler reports those facts per file, and the shell's cache key is
`hash(text) + hash(facts)`. That is the guarantee: a content edit re-runs comrak and codegen
for that one document; the layouts and components it depends on are cache hits. A new photo
id changes no existing unit: a record named by id is read from its map at render, and no
unit's facts name an id (∆Hbw). Which pages then re-render — a render-time read of the
map, or a load-time `by` resolution — is not yet answered (∆xBO).
Checking is part of compiling a unit (D31): a file is re-checked exactly when it is
recompiled, which its text or its facts changing is what triggers.

∆TNT made the split real: `lemmon-vm` compiles per file (`prepare` → `ExportIndex` →
`compile_unit`) into a `Unit` — file-local constant pool, unit-local function indices,
imports symbolic by name — and `link` concatenates pools with offsets into the same
`Module` shape the VM always ran, so the VM itself never changed. The `ExportIndex` is
the project's one answer to which file defines each project-flat name (∆e5i): each file's
exports are offered to it in turn, a name another file already owns stays that file's,
and both `compile_unit` and `link` read that one answer rather than deriving their own.
A refused definition is reported where its file compiles, next to the stdlib-shadowing
warning that asks the index the same question; an import the index has no definition for
is a spanned link diagnostic. The index has a type namespace beside the functions
(∆cea): each file's `lem-type`/`lem-enum` declarations are offered under the same rule,
and annotations resolve to a name alone that the index gives a shape. Two kinds of fact
live on the unit — `Unit::facts`, the imported signatures (parameter names and types and
the return type a call was checked against, typed since ∆C8d), and `Unit::type_refs`,
the declarations of every foreign type the checking reached, each at the use that reached
it — so a callee whose type changed under a caller, or a type whose shape changed under a
consumer, is a link diagnostic naming the file to recompile, in that file's own order. Invariant 11 is a test:
editing a callee's body, or a declaration's default, leaves the consumer's unit
byte-identical. What the link step derives from every unit at once does change: since
∆HF0 it closes each unit's recorded reads through the calls they write, project-wide, so
a callee's body edit can reorder the loader's plans while changing no unit. `∆Qr2` makes the reported facts cross the boundary and the serialized
form (`exportModule`) first-class.

## VM

Stack machine; serializable
`Module { files, documents, constants, functions, declarations, exports, records, bindings }`
— the code, the project's type declarations, its project-flat functions by name, and
the loader's plans: each record's default functions and the bindings file's computed
bindings, in the order their recorded reads, closed through calls, dictate (D37,
∆HF0), and each record's `by` indexes, the ambient fields its strings resolve through
(∆eag) — linked from
per-file units (above). Values are immutable and Rc'd:
`Null | Bool | Int | Float | Str | Html | DateTime | Duration | Url | Asset | List | Map | Fn | Record | Neighbour`.
A neighbour field holds a `Neighbour`, a reference into the store's page table by
position, which a field read resolves to that page's record, so no record holds another
and a page's render takes the table beside the globals (D43); nothing else is given it,
since only a generated page's own record holds a reference. A payload holding one is
refused at the call that made it. While a page's defaults run, its neighbour fields
hold a `Neighbour` naming no page, which a read refuses in the compiler's words.
Records carry their declared name, and the module the declaration table that
`TypeTest` walks and the loader validates against, so the two cannot disagree about
what a name is; fields are read by name (by index is an optimisation nothing needs,
D35). Ops sketch:
`Const/Local/Global/Ambient`, `GetField/OptField/ Index`, `Call/CallBuiltin/CallValue`
(named args reordered at compile time; lambdas are closures over copied captures),
`Jump/JumpIfFalse` plus the short-circuit `JumpUnlessNull/JumpIfFalseKeep/JumpIfTrueKeep`,
`IterInit/IterNext`, `TypeTest`, `Emit` (type-directed) and `EmitText`, `Capture/EndCapture`
(slot bodies and `lem-let` blocks), `Collect` (a `lem-style`/`lem-head`/`lem-script`
body into the page's fragments under its name, dedup by content), `Mark` (its default
under a named range, with the key and payload the call gave), `Track` (dependency
recording), `Return`. A mark is a value as much as an op: `Mark` makes an `html` value
carrying a named range over its default content, which `Emit` appends and shifts, and
`render` returns beside the page (D42). Its key operand says where the key comes from —
a constant the compiler checked against the producers' keys, or a payload field read at
render — and its payload operand whether a record is popped from under the default. It is
an op rather than a builtin because a builtin cannot reach the file and span of its own
call, which is what a handler's diagnostic points at. Runtime errors carry spans
via the pc→span table. A builtin that takes a function (`map`, `filter`) runs it through a
re-entrant invoke inside its `CallBuiltin` step — the VM pushes the frame and runs its loop
again until that frame returns — so an error inside the closure keeps the closure's own
span (∆987). That re-entry costs native stack rather than heap, so it has a cap of its
own beside the frame count, sized against the megabyte of stack `crates/lemmon-wasm`'s
build script links the `.wasm` with; the parser's nesting cap is sized against the same
megabyte, and the boundary's ABI tests show both fit (D38, ∆vnI).

What ∆c8h built is the untyped subset of that: `Value` without the stdlib types, which
∆Q6L added, and no `Collect`, marked value, or `Track`. `Collect` and the marked value
arrived with the side channels; `Track` arrives with render-reported deps (∆xBO).
Records are checked by declaration since ∆cea, and since ∆PCK a record value is tagged
with its declared name (`Value::Record`, made by a map literal where a record is expected
and, later, by the loader at the boundary), which `TypeTest` reads; an enum value is its
variant's name as a string. The loader tags what it validates at `loadData` (D37, ∆iCW),
so a type test reaches loaded data; what still arrives untagged — an untyped host's
values, and `page` in a document no collection matched — is `any`, and a type test over
it is a render error (D35). The VM is the runtime for `any` (∆C8d): its type checks fire
only on values the checker let through as `any`, and stay for that reason.

Calibration, not promise: whole site tens of ms inside WASM; single page well under 1 ms of
VM time (that number is the Phase 0 exit criterion).

## Crates (verified Aug 2026)

| Role           | Choice                                                   | Notes                                                                                                                                                                                                                    |
| -------------- | -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Scanner        | `logos` 0.16                                             | `morph()` = text↔tag mode switch; a third, fields mode, lexes a `lem-type` body (D29)                                                                                                                                    |
| Parser         | hand-written RD + Pratt                                  | Chosen in the ∆7Ib bake-off over `lelwel` 0.10 (`docs/journal/∆7Ib-parser-bake-off.md`): LL(1) forced tag names into the token vocabulary and junk out of heads, costing recovery fidelity for ~700 fewer owned lines    |
| Syntax tree    | `rowan` 0.17 green/red CST                               | lossless + recovering; no incremental reparse needed (rust-analyzer's own conclusion). `lemmon_syntax::ast` is the typed view over it that the checker and codegen read; the CST stays for the formatter and diagnostics |
| Diagnostics    | structured JSON + `ariadne`/`annotate-snippets` renderer | spans first-class; `line-index` 0.1 (rust-analyzer's, sharing rowan's `text-size`) turns a byte offset into an LSP position at the boundary                                                                                                                                                                                                        |
| Markdown       | `comrak` 0.54                                            | in-core (D08); wasm32-clean. comrak with `default-features = false` + `attributes`, a heading adapter since its renderer ignores `{#id}`, and the core's own code-block adapter, which writes comrak's own markup behind a sentinel so each block becomes a `code` mark (D42). No highlighter: syntect, which D40 had highlight against the host's grammars, cost 1.95 MB of the staged `.wasm` and was removed by ∆7wK (4.21 → 2.26 MB) |
| WASM           | `wasm-bindgen` 0.2.127, CLI direct (no wasm-pack)        | `wasm-bindgen --target web` runs in `scripts/stage-wasm.ts`: caller-supplied bytes keep loading injectable and browser-reusable, and the repo owns staging, so wasm-pack’s own package layout would only fight it (∆TNT)                                                                                                                                                                                   |
| Dates          | `jiff` 0.2                                               | `default-features = false`, `alloc` only: no clock, no system zone, no tzdb, so invariant 1 is structural rather than reviewed. Parsing, the civil calendar, RFC 3339/2822 and strftime; a `datetime` is an instant and the offset it was written in (D38) |
| URL encoding   | `percent-encoding` 2.3                                   | one encoding pass behind `url()`. The WHATWG `url` crate was rejected: it refuses a relative reference like `/feed/` without a base, which is most of what a site composes, and pulls `idna`'s tables in (D38) |
| Latin folding  | `unicode-normalization` 0.1                              | canonical decomposition behind `slugify`, so a letter carrying a mark slugs as the letter. Transliteration was measured and rejected: `deunicode` cost 479 KB of the `.wasm` and `any_ascii` 582 KB, to answer a question no site in the corpus asks — and `@sindresorhus/slugify`'s answer applies German's `ü`→`ue` to every language's `ü` (D38) |
| Word counting  | `unicode-segmentation` 1.13                              | UAX #29 word boundaries behind `html.readingTime()`, so a script that does not space its words is counted rather than read as one long one. Splitting on whitespace would have made the core assume English |
| Payload ids    | `siphasher` 1.0                                          | SipHash-1-3 with zero keys behind a code block's payload id (D42): a document reports each payload once per compile and a render's mark names it by the id, and the algorithm is fixed for a host that caches producer output on the id across builds of the core, since `std`'s `DefaultHasher` may change between Rust releases. Already in the tree through comrak's `phf` |
| Suggestions    | `strsim` 0.11                                             | optimal string alignment behind `lemmon_syntax::nearest`, which owns the threshold deciding whether to suggest at all; shared so that every stage's did-you-mean reads the same, rather than each growing its own and drifting                                                                                |
| Corpus diffs   | `similar` 3.2                                             | the unified diff a failing corpus case reports, so what changed can be read from the failure instead of by blessing and diffing the result; dev-only, since `lemmon-corpus` is never anything but a dev-dependency                                                       |
| Serde          | `serde_json` (rmp-serde optional)                        | avoid serde-wasm-bindgen for bulk data. `preserve_order`, so a map keeps the key order the shell sent, which `lem-for` walks; `float_roundtrip`, so a number reads as the double the shell wrote. The default reader is fast and inexact: it read 4 of the 59 fractional numbers in limulus.net's photo data one unit in the last place off, and the page printed the neighbour. The exact reader costs 41 KB of the `.wasm`, beside the `core` one the compiler's float literals already link |
| Editor grammar | `tree-sitter` 0.26                                       | editor-only; do NOT link its C runtime into the core wasm; its `wasm` cargo feature means "host wasmtime", not "compile to wasm"                                                                                         |
| Native later   | `napi-rs` 3.x                                            | optional perf path behind the same TS interface (D11)                                                                                                                                                                    |
| Bundler        | `esbuild` 0.24.0 via JS API, `esbuild-plugin-wasm` 1.1.0 | the shell's `esbuild()` (D45): limulus.net's options port 1:1 (D09), and both are pinned exactly, since a bundle's bytes name it — 0.24.0 is what the site's Eleventy build runs, so its bundles come out byte for byte the same |

## Shell plugin API (sketch)

```ts
interface Plugin {
  name: string
  contentSources?(ctx): ContentSource[] // the on-demand seam (D14)
  data?(ctx): Promise<Record<string, unknown>> // typed via site.lem declarations
  transformDocument?(document: DocumentSource): string // a document's body, below
  assets?(context: AssetsContext): Assets | Promise<Assets> // manifest and files, below
  markdown?: MarkdownRenderer // reserved override (D08)
  transformOutput?(file, ctx): OutputFile
  marks?: PluginHandler[] // producers and decorators (D42), below
  emit?(ctx): Promise<OutputFile[]>
  commands?: Command[] // e.g. `lem photos add`
  watch?(ctx): AsyncIterable<Change>
}
```

Plugins ship `.lem` files (functions + types) bound via `lem-import` in `site.lem` (D23),
as the shell ships its own `ambient.lem` (D30); they cannot inject build-time JS into
the language or the core (D06). What they hand the core before it compiles — a document
transform's body, an asset hook's manifest and files (D44) — and what they do with a page
after the core returns it — an output transform, a mark handler — is the shell's trust
(D42): a producer makes a
mark's content from its payload and claims the keys it covers, a decorator transforms
whatever content a mark has, two producers on one name and key is a load error naming
both, and the shell's own `styles`/`scripts`/`head` producers ship as a default plugin
the site config lists, so a site can produce those names itself. A plugin that claims a
name ships a `lem-fn` wrapping `mark()` for it in its `.lem`, so a layout reads
`<lem.styles()>` and a component `<lem.diagram(…)>` (D42) — not yet (∆wOn): no plugin
ships `.lem`, the shell's default plugin included, so a layout writes `mark("styles")`.
A content API for out-of-build tools (`loadSite(config)`, reading loaded records back
from the core) is parked as ∆Gg3: limulus.net's revisions updater lists its articles with
a script of its own (∆shS).

As built, a site lists its plugins in `lemmon.config.ts` as `plugins`, and a plugin is
`{name, transformDocument?, assets?, marks?}` (D44, D42). Any other hook is refused at
config load, since the shell runs none of them:

- `data` and `commands`: not yet built (∆wOn).
- `transformOutput`: not yet built (∆fwn).
- `watch` and `emit`: not yet built (∆HzB).
- `contentSources` and `markdown`: reserved seams (D14, D08) that nothing has asked for.

The two hooks that run before compile (§Build pipeline, step 2):

```ts
interface DocumentSource {
  path: string // relative to the input directory: `links/x/index.md`
  frontMatter: { readonly [key: string]: JsonValue } // as the core loads it; frozen
  body: string // the text after the front matter, as the transform before left it
}

interface AssetsContext {
  root: string // the config file's directory, absolute: where esbuild runs from (D45)
  input: string // the input directory, absolute
  output: string // the output directory, absolute, where the files are written
  writes: boolean // false for `lem check`, which writes none of them
  env: Readonly<Environment> // what `env("…")` bindings read; a frozen copy
}

interface Assets {
  manifest?: Record<string, string> // logical path → the finished URL it ships at (D38)
  files?: Record<string, string | Uint8Array> // url → contents, written with the pages
}
```

A mark handler, and what a producer is given:

```ts
interface PluginHandler {
  name: string // the mark name it answers
  keys?: string[] // the keys it answers; a producer listing none answers keyless marks
  payload?: string // the record its marks carry, which a `.lem` file declares
  keyedBy?: string // the field of that record holding a mark's key; needs `payload`
  produce?(ctx: ProducerContext): (string | Uint8Array)[] // one per mark it answers
  decorate?(mark: Mark, content: Uint8Array, ctx: { body: boolean }): string | Uint8Array
}

interface ProducerContext {
  name: string
  marks: Mark[] // every mark of the name on the page that it answers
  fragments: Fragment[] // what the page collected under the name
  page: Uint8Array // the page as the core returned it
  body: boolean // a document's body in the body pass (D46), which runs before its page
  write(url: string, content: string | Uint8Array): void // a file beside the pages
}
```

The shell's default plugin is `bundler()`, exported from `@limulus/lemmon-shell`, whose
`styles`, `scripts` and `head` producers §Build pipeline describes; a site lists it as
`plugins: [bundler()]`, and one that produces those names itself leaves it out. Its
highlighter, `highlighter(options?)`, is a `code` producer listed beside it, and
`options.exclude` narrows its claim, so a site producing some languages itself lists both
(D42):

```ts
plugins: [bundler(), highlighter({ exclude: ['rust', 'rs'] }), rustHighlighter()]
```

Its esbuild plugin, `esbuild(options?)`, is an `assets` hook listed the same way, whose
options are a site's own entry globs, externals, entry and asset names, and the
environment variables its scripts read as `process.env.NAME` (D45).

## Editor tooling (day one, D16)

- `tree-sitter-lemmon`: embedded-template grammar; HTML injected into text, Lemmon injected
  into markdown lem-blocks. Corpus agreement with the Rust parser is CI-enforced.
- `@limulus/lemmon-lsp`: Node process loading the same `@limulus/lemmon-core` .wasm;
  diagnostics as you type, schema-driven completion, hover, go-to-def, rename, formatting,
  front-matter validation at YAML positions. Precedent: bash-language-server, VS Code
  anycode, Oso's Polar (Node host + wasm core); native `lsp-server` binary remains an escape
  hatch.

## Testing

- Golden corpus `tests/corpus/`: one `.lem` input per case plus one expectation file per
  stage, grouped by what a case is for (`syntax/` today; `render/` and `document/` follow)
  and by language feature below that. `lemmon-syntax` owns `.cst` (the lossless tree with
  its diagnostics trailer) and `.tags` (the tag-boundary projection); the VM adds `.html`.
  Tree and projection carry byte ranges, since `.tags` is what a tree-sitter grammar is
  held to; the trailers render one-based `line:col` instead, so a blessed diff can be
  matched to the source by eye.
  Whether a case parses clean is data — an empty diagnostics section — not a directory.
  `crates/lemmon-corpus` does discovery, blessing (`npm run corpus:bless`), and failure
  reporting for every stage. Gates: no orphaned expectations, and every `SyntaxKind` reached
  by some case.
- `.tags` is the tree-sitter agreement target: tags, close tags, text runs, and item-level
  error nodes, with heads, expressions, and `Block` wrappers elided — the boundaries a
  context-free grammar can be held to, and no more.
- Scanner property tests: text without `<lem.`/`<lem-`/`</lem-` round-trips byte-for-byte;
  escaping never emits `<`.
- cargo-fuzz on parser and VM (the LSP feeds them half-typed garbage).
- wasm-bindgen-test through Node for the real boundary.
- The limulus.net parity harness (architecture doc 3) is the end-to-end acceptance bar.
  It compares *rendered* HTML, never bytes (D28): the shell's `renderedPage` models
  the UA stylesheet's white-space processing so that everything D26 and D28 move is
  invisible to the comparison by construction, while a changed space inside `pre` or
  between two inline runs still fails (∆Ma4). `compareRendered` walks two such pages and
  consults a caller's forgiveness classes where it would report a difference — an
  attribute, a text run, an element one side lacks — and offers each matched pair to a
  class that may take its subtree over, counting what each forgives; `npm run parity` in
  the port branch holds every class to its expected count per page, as the fidelity
  harness does for the-rebound (∆Zo7).

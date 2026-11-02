# Checking what you wrote

`lem check` compiles every file of the site, loads its front matter and data against the
declared types, and renders every page — all `lem build` does but the writing — printing
each diagnostic as `path:line:col: severity: message`, the lines its span covers beneath
with `^` under what it covers, and its labels on indented lines:

```text
www/layouts.lem:2:15: error: `Post` has no field `titel`; did you mean `title`?
> 2 |   <title><lem.p.titel></title>
    |               ^^^^^^^
lem: 1 error
```

Any error makes it exit 1; with none it says how many pages it checked. It never touches
the output directory. `lem build` prints the same report, and writes nothing on an error.

Run it from the site's directory, the one holding `lemmon.config.ts`, through the lemmon
checkout's bin, which finds the core staged in that checkout (`npm run rust:wasm` there
rebuilds it):

```sh
node --conditions=source /path/to/lemmon/packages/shell/bin/lem.ts check
```

A page — a document, or a template with front matter — is typed the way the shell types it:
by its `type:` key, or else by the `from` glob in `site.lem` that matches its path from the
input directory. A page nothing types is an error at its front matter, naming both fixes.
The site needs a `site.lem` in its input directory, whose globs type every page that names
no `type:`.

What it reports, in the order it finds them:

- **Front matter that does not read**: bad YAML, a key given twice, a value JSON cannot
  carry. Nothing compiles past one.
- **Plugin hooks that fail**: a plugin's `transformDocument` that throws or returns
  anything but the document's body, at the document's front matter; and a plugin's
  `assets` hook that throws or returns anything but `{manifest?, files?}`, a logical path
  two plugins both map, or an asset file that collides with another output file, at the
  start of `lemmon.config.ts`, printed without source lines. Nothing compiles past one.
  The shell's `esbuild()` bundles the client scripts here, so a script esbuild cannot
  build, a `XXXXXXXX` placeholder naming no entry or its own bundle (even through other
  bundles, since each is named by the names in it), and an `env` variable that is not set
  are each one of these, esbuild's message giving the script's file and position.
  The text a transform leaves is what compiles: a diagnostic in what it appended points
  past the end of the file you wrote, and its frame shows the appended text.
- **Compile errors**: everything the checker finds, in every `.lem` first — page templates
  among them, compiled without their records so a layout sees what they define — then the
  pages the shell cannot type, before any compiles: one two globs match, one nothing types,
  and a generator with no `type:` — and then the pages, as their records. A template never
  sees what a document (`.md`) defines.
- **Mark warnings**: a `mark()` name no plugin the config lists claims, with a
  did-you-mean, and a literal key none of their producers lists. A site listing no plugins
  is warned at every mark. The shell's default plugin, `bundler()`, claims `styles`,
  `scripts` and `head`, and its highlighter, `highlighter()`, claims `code` for every
  language highlight.js has, by name and alias: with both listed, a code block warns at
  its word only where that names another language — `TS` among them, since a key matches
  as written.
- **Sources it cannot fulfil**: an environment variable that is not set, a data file outside
  the input directory or not there, and a `plugin("…")` source, which nothing runs yet
  (`∆wOn`) — each at its binding in `site.lem`; and a data file that does not parse, at the
  place in the data file.
- **Load errors**: a misspelled or missing front-matter key, a `by` reference its map lacks,
  a bad date in YAML, a data file with keys its type does not declare, two pages at one url.
  The [load mistakes](mistakes.md#while-loading-front-matter) are the common ones.
- **Render errors**: an `any` read that misses, a map key that is absent, a list index out
  of range — and a page whose url names no file, or a file another page, a passthrough file
  or a plugin's `assets` hook writes, or one a file system could not hold beside it: the
  same name but for case or Unicode form, or a file where the other needs a directory. A
  file a plugin's producer writes is checked the same way against pages, passthrough files,
  asset files and other produced files, an error at the later writer — a page's url or the
  producer's mark — labelled at the first; and at the producer's mark, a url or content of
  the wrong kind, a url naming no file, or other content where a producer wrote already.
- **Handler warnings**: a plugin's handler that throws, or returns anything but its
  content, at the mark, which keeps the content it had; and what a page collects that
  nothing on it places — a `lem-style` with no `mark("styles")` in its layout — at the block
  that collected it.
- **Broken links**, last, once nothing else failed: every link in the HTML, CSS and XML
  the build would write — `href`, `src`, `srcset`, a CSS `url()`, a feed's `<link>`, a
  sitemap's `<loc>`, an SVG `<use href="#…">` — and every url a plugin's `assets` hook
  maps, that names no file the build writes, matched exactly: `/Feed/` for `/feed/`, and
  `/tils` for `/tils/`, are errors that say so. A fragment must name an `id` (or an `a`'s
  `name`) in the page as built, so one only a script inserts fails, as does a footnote
  linked by number: comrak names it by its label, `#fn-label`. Each is reported once per
  page per url, at the page's `url` key or its front matter — the link may come from its
  layout, a component or its prose — or at a generator's `for:` entry, or at the mark of
  the producer whose file holds it; a manifest url, and a link in a file an asset hook
  built, at the start of `lemmon.config.ts`, printed without source lines, a manifest url
  once however many pages print it. An XML file the parser refuses, or that declares an
  encoding other than UTF-8, is an error too, since its links cannot be read. Links to
  other hosts, other schemes, and the paths `lemmon.config.ts` lists as `served` (a host's
  redirect, say) are not checked — a `served` path the build does write is a warning,
  since links to that page then go unchecked — nor are a feed's escaped entries, whose
  links are their pages'.

What it does not cover:

- **Computed mark keys.** A key a template computes is read at render, and one no producer
  lists keeps the default content without a word: not yet reported (`∆CxX`).

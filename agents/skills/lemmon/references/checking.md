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

A document is typed the way the shell types it: by its `type:` key, or else by the `from`
glob in `site.lem` that matches its path from the input directory. A document nothing types
compiles with `page` as `any` and its field reads go unchecked; that is the
page-without-a-record case, not a pass. The site needs a `site.lem` in its input directory,
even an empty one.

What it reports, in the order it finds them:

- **Front matter that does not read**: bad YAML, a key given twice, a value JSON cannot
  carry. Nothing compiles past one.
- **Compile errors**: everything the checker finds, in every `.lem` first — page templates
  among them, compiled without their records so a layout sees what they define — then in the
  pages as their records. A template never sees what a document (`.md`) defines.
- **Mark warnings**: a `mark()` name no plugin the config lists claims, with a
  did-you-mean, and a literal key none of their producers lists. A site listing no plugins
  is warned at every mark; `plugins: [bundler()]`, the shell's default plugin, claims
  `styles`, `scripts` and `head`. The shell ships no highlighter, so a code block naming
  a language warns too (not yet, ∆Nxd).
- **Sources it cannot fulfil**: an environment variable that is not set, a data file outside
  the input directory or not there, and a `plugin("…")` source, which nothing runs yet
  (∆wOn) — each at its binding in `site.lem`; and a data file that does not parse, at the
  place in the data file.
- **Load errors**: a misspelled or missing front-matter key, a `by` reference its map lacks,
  a bad date in YAML, a data file with keys its type does not declare, two pages at one url.
  The [load mistakes](mistakes.md#while-loading-front-matter) are the common ones.
- **Render errors**: an `any` read that misses, a map key that is absent, a list index out
  of range — and a page whose url names no file, or a file another page or a passthrough
  file writes, or one a file system could not hold beside it: the same name but for case or
  Unicode form, or a file where the other needs a directory. A file a plugin's producer
  writes is checked the same way against pages, passthrough files and other produced
  files, an error at the later writer — a page's url or the producer's mark — labelled at
  the first; and at the producer's mark, a url or content of the wrong kind, a url naming
  no file, or other content where a producer wrote already.
- **Handler warnings**: a plugin's handler that throws, or returns anything but its
  content, at the mark, which keeps the content it had; and what a page collects that
  nothing on it places — a `lem-style` with no `mark("styles")` in its layout — at the block
  that collected it.

What it does not cover:

- **Assets.** The declaration carries no manifest, so every `asset("…")` reports that the
  host declares none. That is expected while the esbuild pipeline is not yet built (∆d1N)
  and hands the shell no manifest.
- **Computed mark keys.** A key a template computes is read at render, and one no producer
  lists keeps the default content without a word: not yet reported (∆CxX).

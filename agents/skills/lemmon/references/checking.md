# Checking what you wrote

`lem build` is the check: it compiles every file of the site, loads its front matter and
data against the declared types, and renders every page, printing each diagnostic as
`path:line:col: severity: message` with its labels on indented lines beneath. Any error
stops it before it writes a file, and it exits 1; with none it says what it wrote.
`lem check`, the same report without the writing, is not yet built (∆1rD).

Run it from the site's directory, the one holding `lemmon.config.ts`, through the lemmon
checkout's bin, which finds the core staged in that checkout (`npm run rust:wasm` there
rebuilds it):

```sh
node --conditions=source /path/to/lemmon/packages/shell/bin/lem.ts build
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
- **Sources it cannot fulfil**: an environment variable that is not set, a data file outside
  the input directory or not there, and a `plugin("…")` source, which nothing runs yet
  (∆c4j) — each at its binding in `site.lem`; and a data file that does not parse, at the
  place in the data file.
- **Load errors**: a misspelled or missing front-matter key, a `by` reference its map lacks,
  a bad date in YAML, a data file with keys its type does not declare, two pages at one url.
  The [load mistakes](mistakes.md#while-loading-front-matter) are the common ones.
- **Render errors**: an `any` read that misses, a map key that is absent, a list index out
  of range — and a page whose url names no file, or a file another page or a passthrough
  file writes, or one a file system could not hold beside it: the same name but for case or
  Unicode form, or a file where the other needs a directory.

What it does not cover:

- **Assets.** The declaration carries no manifest, so every `asset("…")` reports that the
  host declares none. That is expected while the esbuild pipeline is not yet built (∆d1N)
  and hands the shell no manifest.
- **Marks.** It declares no mark handlers, so a misspelled `mark("stylez")` passes silently
  and every mark keeps its default content; the shell's handlers, not yet built (∆F4y), will
  warn at it.

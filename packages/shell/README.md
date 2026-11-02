# @limulus/lemmon-shell

`lem`, the command-line tool for [Lemmon](https://github.com/limulus/lemmon): a static site
generator whose templates are HTML with typed tags in them. It builds a directory of
Markdown documents and `.lem` templates into a static site, and checks every template,
every page's front matter and every link before it writes a file.

> **Early development.** Lemmon is young and changing fast. Expect frequent major
> releases, each with significant breaking changes.

## Install

```sh
npm install --save-dev @limulus/lemmon-shell
```

It needs Node 24 or later, which runs `lemmon.config.ts` as TypeScript itself. Give the
site's `package.json` `"type": "module"`, or Node warns on every run that it had to guess
the config is a module.

## A first site

Five files make a home page that lists the site's posts, and one post:

```text
lemmon.config.ts
www/site.lem
www/layouts.lem
www/index.lem
www/posts/hello.md
```

`lemmon.config.ts` says where the sources are and where the site goes:

```ts
import type { Config } from '@limulus/lemmon-shell'

export default {
  input: 'www',
  output: 'dist',
} satisfies Config
```

`site.lem` declares a type for each kind of page, and the collections. Every page type
extends `Page`, which gives it a `url`, and names the layout that renders it. Every file
matching `posts/*.md` is a `Post`:

```html
<lem-type Post extends Page>
  title: string
  date: datetime
  layout: (Post, html): html = postPage
</lem-type>

<lem-type Home extends Page>
  title: string
  layout: (Home, html): html = homePage
</lem-type>

<lem-let posts: [Post] from "posts/*.md" />
<lem-let recent: [Post] = posts.sortBy(|p| p.date).reverse() />
```

`layouts.lem` holds the layouts, which are functions from a page and its rendered body to
HTML:

```html
<lem-fn base(title: string, content: html)>
  <!doctype html>
  <html lang="en">
    <head>
      <meta charset="utf-8">
      <title><lem.title></title>
    </head>
    <body><lem.content></body>
  </html>
</lem-fn>

<lem-fn postPage(post: Post, body: html)>
  <lem-let content>
    <h1><lem.post.title></h1>
    <time datetime="<lem.post.date>"><lem.post.date.format("%B %-d, %Y")></time>
    <lem.body>
  </lem-let>
  <lem.base(post.title, content)>
</lem-fn>

<lem-fn homePage(home: Home, body: html)>
  <lem.base(home.title, body)>
</lem-fn>
```

`index.lem` is the home page. A `.lem` file with front matter is a page, and `type:` names
its type:

```html
---
type: Home
title: My site
---
<h1>My site</h1>
<ul>
  <lem-for post in site.recent>
    <li><a href="<lem.post.url>"><lem.post.title></a></li>
  </lem-for>
</ul>
```

`posts/hello.md` is a post. Its front matter must be a `Post`'s fields:

```md
---
title: Hello, world
date: 2026-11-01 09:00:00 -07:00
---

My first post, written in **Markdown**.
```

`npx lem build` writes `dist/index.html` and `dist/posts/hello/index.html`, and
`npx lem dev` serves them at <http://127.0.0.1:8080/>, rebuilding on every change.

A mistake stops the build before it writes anything, and is shown in the source that has
it. Here the post's date is wrong:

```text
www/posts/hello.md:3:1: error: `last Tuesday` is not a datetime: write a date (`2024-11-24`), or a date and time with an offset (`2024-11-24T18:35:00Z`)
> 3 | date: last Tuesday
    | ^^^^^^^^^^^^^^^^^^
lem: 1 error, so nothing was written
```

A field a template misspells stops it the same way, and so does printing a value that may
be missing without checking for it first. [Writing Lemmon][skill] is the guide to the
template language.

## Commands

| Command     | What it does                                                              |
| ----------- | ------------------------------------------------------------------------- |
| `lem build` | Builds the site into its output directory                                 |
| `lem check` | Everything `lem build` does except write, for CI or an editor's save hook |
| `lem dev`   | Serves the site and rebuilds it on every change, reloading open pages     |

- `--config <path>` names the config file, `lemmon.config.ts` in the current directory by
  default. `lem dev --port <n>` serves at another port than 8080.
- An error stops the build before it writes a file, and `lem` exits with 1. Warnings are
  printed and the build goes on.
- Every link in the HTML, CSS and XML a build makes is checked against the files it makes,
  `#fragment`s included, before anything is written. A broken link is an error.
- `lem dev` keeps serving the last good build while a change has an error, and reloads
  the open pages once it is fixed. It reads the config once, so restart it after changing
  `lemmon.config.ts` or a plugin it imports.
- Nothing is deleted from the output. A page you delete or move leaves its old file there
  until you clean the output directory.

## Configuration

`lemmon.config.ts` default-exports a `Config`. Only `input` and `output` are required:

```ts
import { bundler, esbuild, highlighter, type Config } from '@limulus/lemmon-shell'

export default {
  input: 'www',
  output: 'dist',
  origin: 'https://example.com',
  passthrough: ['images/**/*', 'favicon.ico'],
  plugins: [bundler(), highlighter(), esbuild()],
} satisfies Config
```

| Field         | What it is                                                              |
| ------------- | ----------------------------------------------------------------------- |
| `input`       | The directory of the site's sources, relative to the config file        |
| `output`      | The directory the site is written to, relative to the config file       |
| `origin`      | Where the site is served, as `https://example.com`                      |
| `passthrough` | Globs, relative to `input`, of files copied to the output unchanged     |
| `served`      | Paths the host answers itself, such as redirects, for the link check    |
| `plugins`     | The plugins the build runs, in the order their hooks run                |
| `dev.headers` | Headers `lem dev` adds to every response, as the site's host sends them |

`origin` makes the links in a feed's copy of each post absolute, and tells the link check
which absolute links are the site's own. Without it, the link check skips every absolute
link.

## Included plugins

The shell ships three plugins. A site lists the ones it uses in `plugins`.

**`bundler()`** gathers what a page's templates collect and places it where the layout
asks. `<lem-style>` blocks are joined into one stylesheet, placed by `<lem.mark("styles")>`
as a `<link>`, and named by a hash of its contents, so pages with the same styles share a
file. `<lem-script src = asset(…) />` tags become `<script type="module">` tags at
`<lem.mark("scripts")>`, and `<lem-head>` blocks are placed at `<lem.mark("head")>`.

**`highlighter(options?)`** highlights Markdown code blocks with
[highlight.js](https://highlightjs.org) 11.9.0, in every language it knows, by name or
alias as highlight.js spells them: `ts` is highlighted, `TS` is not. The output carries
highlight.js's classes, so any highlight.js theme styles it; link one from the layout.
`exclude` lists names to leave to another plugin, each alias on its own, as
`highlighter({ exclude: ['rust', 'rs'] })`. A code block naming a language no plugin
highlights is a warning, and keeps its plain `<pre><code>`.

**`esbuild(options?)`** bundles every `index.ts` and `worker.ts` under `input` with
[esbuild](https://esbuild.github.io) 0.24.0 into a minified ES module with a source map,
written to the output at its source's path under a name hashed from its contents:
`www/lib/app/index.ts` ships as `/lib/app/index.ABCD2345.js`. A template names a bundle by
its source's path, `.js` for `.ts`, and prints the URL it ships at:

```html
<script type="module" src="<lem.asset("/lib/app/index.js")>"></script>
```

A script names another bundle, to import it or start it as a worker, with `XXXXXXXX`
where the hash goes, as `new URL('./worker.XXXXXXXX.js', import.meta.url)`. The `env`
option lists environment variables the scripts read as `process.env.NAME`, each replaced
by its value at build time. [The reference][esbuild] lists the other options.

## Writing a plugin

A plugin is an object with a `name` and any of three hooks:

- `transformDocument({ path, frontMatter, body })` returns the Markdown a document
  compiles with. It runs before anything compiles.
- `assets({ root, input, output, writes, env })` returns files for the build to write,
  and a `manifest` mapping the paths templates pass to `asset()` to the URLs those files
  ship at. It may be `async`.
- `marks` lists handlers that fill the marks a page leaves for them: every Markdown code
  block, marked `code` with its language as the key, and every `<lem.mark("name")>` a
  template writes. A producer makes a mark's content; a decorator changes the content
  it has.

This one puts a Copy button after every code block, for the site's own script to wire up:

```ts
import type { Plugin } from '@limulus/lemmon-shell'

const decoder = new TextDecoder()

export const copyButtons: Plugin = {
  name: 'copy-buttons',
  marks: [
    {
      name: 'code',
      decorate: (mark, content) =>
        `<div class="code">${decoder.decode(content)}<button>Copy</button></div>`,
    },
  ],
}
```

A site lists it as it lists the included plugins, `plugins: [highlighter(), copyButtons]`,
and since a mark's decorators run after its producer, it wraps the block `highlighter()`
made. [Plugins][plugins] in the reference gives each hook's full contract.

## Documentation

- [Writing Lemmon][skill], the author's guide to `.lem` templates, `site.lem` and Markdown
  documents
- [The language reference][language]
- [The shell reference][reference]: every config field, plugin and build step in full
- [`@limulus/lemmon-core`][core], the compiler this package drives, for running Lemmon
  from your own code

## License

[MIT](https://github.com/limulus/lemmon/blob/main/LICENSE)

[core]: https://github.com/limulus/lemmon/tree/main/packages/core#readme
[esbuild]: https://github.com/limulus/lemmon/blob/main/docs/shell.md#esbuildoptions
[language]: https://github.com/limulus/lemmon/blob/main/docs/language.md
[plugins]: https://github.com/limulus/lemmon/blob/main/docs/shell.md#plugins
[reference]: https://github.com/limulus/lemmon/blob/main/docs/shell.md
[skill]: https://github.com/limulus/lemmon/blob/main/agents/skills/lemmon/SKILL.md

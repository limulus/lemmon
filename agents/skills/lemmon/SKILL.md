---
name: lemmon
description: >-
  How to write Lemmon that compiles first time: `.lem` templates, layouts and components,
  `site.lem` schemas and bindings, page templates, and `.md` documents with lem blocks. Use
  whenever writing, editing, reviewing or porting Lemmon source — including porting
  limulus.net's WebC/Eleventy layouts — whenever a file holds `<lem.` or `<lem-` tags, and
  whenever a Lemmon diagnostic needs explaining.
metadata:
  author: limulus
  version: '0.1.0'
---

# Writing Lemmon

A working author's reference distilled from `docs/language.md`, which is normative and holds
the depth (cited below as §). Where the two disagree the compiler wins, so check what you
write (`references/checking.md`). Never invent syntax: a form shown neither here nor in the
language doc is a question for Eric, not a guess. Porting WebC? Read
`references/porting-from-webc.md` as well.

## Rules every tag obeys

1. **Only `<lem.`, `<lem-` and `</lem-` interrupt text.** Every other `<` is HTML, which is
   opaque text: attributes, custom elements, `<script>`, comments. A trigger inside an HTML
   comment or a script still fires; write a literal one as `&lt;lem`.
2. **A head never contains `>`.** A tag ends at the first `>` outside a string, so ordering
   is `lt lte gt gte`, a lambda is `|a| expr`, an annotation is `: T`, and there is no `=>`,
   `->`, `|>` or `>=`. `<lem-if n > 0>` is `<lem-if n >` then text ` 0>`.
3. **The head alone decides void or block.** `= expr` makes `lem-let`/`lem-fn` void; `from`
   after a `lem-let`'s name makes the void source form; every other keyword tag is a block,
   closed bare (`</lem-fn>`). Write voids with ` />`. `<lem.…>` is always void and never
   slashed.
4. **`lem-*` is reserved.** An unknown `lem-*` tag is an error with a did-you-mean, never
   pass-through, so no custom element of yours may be named `lem-*`.
5. **One call form.** `<lem.f(args)>` calls. A body is passed by capturing it first (see
   Functions & components); there is no call-with-body (sugar for one is deferred, ∆M5x).
6. **No truthiness, no silent null.** A condition is a `bool` or an optional, and an
   optional is narrowed, read with `?.`, or defaulted with `??` before it is used.

## Tags

| Form                                  | Gotcha                                         |
| ------------------------------------- | ---------------------------------------------- |
| `<lem.expr>`                          | Prints or calls; parenthesise `<lem.(a ?? b)>` |
| `<lem-let x = e />`, `x: T = e`       | Immutable, block-scoped; unread warns          |
| `<lem-let x>…</lem-let>`              | Captures the rendered body as `html`           |
| `<lem-let x: T from src />`           | Top level of `site.lem` only; `: T` required   |
| `<lem-fn f(a: T, b: html)>…</lem-fn>` | Returns `html`; top level of a file only       |
| `<lem-fn f(a: T): R = e />`           | `: R` required: callers read only heads        |
| `<lem-if c>…</lem-if>`                | `c` is a `bool` or an optional; narrows        |
| `<lem-else-if c />`, `<lem-else />`   | Void separators inside the `lem-if`            |
| `<lem-for x in xs>`, `x, i in xs`     | `<lem-else />` inside is the empty case        |
| `<lem-for k, v in m>`                 | Over a map; loop names never warn              |
| `<lem-match s>…</lem-match>`          | Holds cases and whitespace only                |
| `<lem-case P>…</lem-case>`            | `T`, `T as t`, literal, `.variant`, `null`     |
| `<lem-default>…</lem-default>`        | Last; needed unless an enum is covered         |
| `<lem-type T>`, `T extends B`         | Fields one per line; top level only            |
| `<lem-enum E = a \| b />`             | Variants bare only here; `.a` elsewhere        |
| `<lem-style>…</lem-style>`            | Collected under `styles`, deduped; no head     |
| `<lem-head>…</lem-head>`              | Collected under `head`; never self-closed      |
| `<lem-script src = asset("x.js") />`  | An `asset` only, never a URL string            |
| `<lem-import …>`                      | Not yet supported (∆wOn)                       |

## Expressions

- **Literals:** `"text"`, `42`, `1.5`, `true`, `null`, `[a, b]`, `{key: v}`; every comma
  list takes one trailing comma. A `{…}` where a record is expected is that record: every
  non-optional field given, no others.
- **Strings are literal today.** The language doc describes `{…}` interpolation (its
  delimiter deferred), but the compiler does not implement it: `"Link: {title}"` is those
  characters. Join strings with `+`. Nothing turns a number into a string, so print one with
  its own `<lem.n>` beside the text.
- **Stdlib functions**, the whole ambient namespace: `raw(s)`, `datetime("2024-11-24")`,
  `url("/a/")`, `asset("css/site.css")`, `uniqueId("fig")`, `mark("styles")`. `asset` and
  `mark` take a literal; a literal `datetime` is checked while compiling.
- **Access:** `a.b` reads a field; `a?.b` reads through an optional (`a?.b?.c` for the next
  step); `xs[0]` and `m["key"]` index, and a map has no `.` fields. A missing index or key
  is a render error, not null: test with `m.has(k)` first.
- **Calls:** `f(x, nolink: true)` takes positional then named arguments; only a `lem-fn`
  called by name takes named ones (and `mark`'s `default:`). `<lem.f>` is the function
  value, not a call. Parameters have no defaults.
- **Methods:** `x.m()` asks `x`'s type only: a builtin method, or a field holding a
  function. A project `lem-fn` is never reached through a dot — `rendition(photo, "md")`,
  not `photo.rendition("md")` — and a method is always called: `s.length()`.
- **Lambdas:** `|p| p.date`, `|a, b| a lt b`, `|| 1`, typed from where they stand:
  `posts.map(|p| …)` types `p` from `posts`.
- **Operators:** `+ - * / %` (`+` joins two strings, never a string and a number); `== !=`
  (structural); `lt lte gt gte` on numbers, strings and datetimes; `&& || !` on `bool` only;
  `a ?? b` with `a` optional; `c ? x : y`; `x in xs`, `"k" in m`; `x is T` for a record
  type. Precedence is JavaScript's; `==`/`&&` spellings are provisional.

Builtin methods (§ Types):

- `string`: `trim()`, `lower()`, `upper()`, `slugify()`, `length()`, `split(sep)`,
  `replace(from, to)` (every occurrence; no regex), `startsWith(s)`, `endsWith(s)`,
  `contains(s)`.
- `[T]`: `first()`, `last()` and `find(p)` give `T?`; `length()`, `reverse()`,
  `contains(x)`, `take(n)`, `drop(n)`, `map(f)`, `filter(p)`, `any(p)`, `sortBy(key)`,
  `groupBy(key)`; `join(sep)` on `[string]`.
- `{string: V}`: `keys()`, `values()`, `has(key)`. `int`: `abs()`. `float`: `abs()`,
  `round()`, `floor()`, `fixed(places)`. `url`: `text()`. `html`: `readingTime()`.
- `datetime`: `format("%B %-d, %Y")` (strftime), `rfc3339()`, `rfc822()`, `year()`,
  `month()`, `day()`, read in the offset it was written in. `duration`: `minutes()`,
  `seconds()`.

## Types

`string int float bool datetime duration url html asset any`, `[T]`, `{string: T}` (keys are
always strings), `T?`, `(A, B): R`, and declared records and enums; `map`, `list` and
`function` are not type names. An `int` is not a `float` (provisional; the number model is
deferred), so write `1.0 / 3.0`.

- **Printing** takes `string` (escaped), `html` (verbatim), `int`, `float`, `datetime` (RFC
  3339), `url`, `asset` (its hashed URL), an enum (its variant's name, provisional) and
  `any`. A `bool`, `null`, list, map, record, function, `duration` or optional is a compile
  error: narrow it, `??` it, or call a method.
- **Narrowing:** `<lem-if x>` and `<lem-if x is T>` narrow `x` — a local, a parameter, an
  ambient value, or a plain-dot path off one (`post.hero`) — in that arm only: not in
  `lem-else`, not through `&&` or `!`, not the fields of what was narrowed.
  `<lem-case T as t>` narrows the same way.
- **Matching** is exhaustive only over an enum (every variant, and `null` for `E?`);
  anything else needs `<lem-default>` (the provisional reading; the rules are deferred).
- **`any`** opts out: accepted everywhere, checked by the VM at render.
- **`datetime`** is a moment plus its written offset: `2024-11-24` (midnight UTC) or
  `2024-11-24 11:35:00 -07:00`. No offset or a zone name is refused; no arithmetic.

## Records, enums and `site.lem`

The standard shell (`@limulus/lemmon-shell`, whose `lem build` writes the site) compiles
every file with its `ambient.lem`, which declares `Page` (`url: url`, `inputPath: string`,
`draft: bool = false`), `Build` (`time`, `generator`) and `Code`. It binds `page` (the page
being rendered), `build`, and `site`, the record of `site.lem`'s top-level bindings; a site
needs a `site.lem` in its input directory, even an empty one.

```lem site.lem
<lem-enum Placement = hero | left | right />

<lem-type Photo>
  title: string
  alt: string
  src: url
</lem-type>

<lem-type Post extends Page>
  title: string
  subhead: string?
  date: datetime
  author: string
  hero: Photo? by site.photos
  readingTime: duration = body.readingTime()
  layout: (Post, html): html = article
</lem-type>

<lem-type LinkPost extends Post>
  linkUrl: url
  linkTitle: string
  title: string = "Link: " + self.linkTitle
</lem-type>

<lem-type SitePage extends Page>
  title: string
  layout: (SitePage, html): html = sitePage
</lem-type>

<lem-let photos: {string: Photo} from "_data/photos.json" />
<lem-let posts: [Post] from "feed/*/index.md" />
<lem-let siteUrl: url from env("SITE_URL") />
<lem-let name: string = "limulus.net" />
<lem-let recent: [Post] = posts.sortBy(|p| p.date).reverse().take(5) />
```

- **Fields:** `name: Type`, then `by value.field`, then `= default`, one per line. A record
  has exactly its declared fields: an undeclared key in front matter or a data file is a
  load error, as is an omitted field neither optional nor defaulted. Records and enums are
  nominal and project-wide; `extends` takes one record.
- **Defaults** run once at load, when the data omits the field, and read only `self` and, on
  a `Page` type, `path` and `body` (its rendered prose) — never `site`, `page` or `build`,
  even through a `lem-fn`. A type whose defaults read `body` cannot type a page template. A
  subtype redeclares a field, same type, to give it a default.
- **`by site.photos`** resolves front matter `hero: upper-falls` through that map at load; a
  miss is a load error. Only a field of `site` (or another host value loaded first) may
  follow `by`.
- **`layout: (T, html): html`** is required where a type's base is `Page`, and inherited
  below. Its default names a `lem-fn`; front matter may name another of that type.
- **Enum variants:** `.hero` where a `Placement` is expected (an argument, an annotated
  binding, a return, the right of `==`/`!=`/`in`), `Placement.hero` anywhere, the bare
  string `hero` in front matter. A bare `hero` in an expression is a name. In a ternary keep
  the space, `c ? .hero : .left`: `?.` is one token.
- **`from` sources** stand only at the top level of `site.lem`, with `: T` and a literal: a
  glob, whose list type extends `Page` (`[Page]` itself is an error), collecting the
  documents and page templates it matches; a data-file path, inside the input directory;
  `env("NAME")`; or `plugin("name")`, not yet supported (∆wOn). A page belongs to one
  collection, and two globs matching one page is an error at the page.
- **Computed bindings** (`name: T = expr`) run at load in dependency order, read siblings by
  bare name and `build`, never `page`. Lists do not concatenate (no `+`, no `concat`), so
  one collection is one glob.
- **Declared, not inferred:** `site.lem`'s top-level bindings and expression-bodied
  `lem-fn`s carry their types, since other files are checked against heads alone (invariant
  11). Locals infer.
- **Front matter the shell reads itself:** `type:` names the record (a subtype of the
  collection's element type, or any `Page` type where no glob matched); `url:` is the one
  `Page` field a page may give (else the shell derives it, and `inputPath` always); `for:`
  makes a generator (D43; not yet expanded by anything, ∆99b).

## Functions & components

A component is a `lem-fn`, and its `html` parameters are slots, filled by capturing then
calling. Its styles go in a `lem-style` inside it, nested under a class named after the
function (D20), and are collected once however often it is called.

```lem components.lem
<lem-fn photo(p: Photo, placement: Placement)>
  <figure class="photo" data-placement="<lem.placement>">
    <img src="<lem.p.src>" alt="<lem.p.alt>">
    <figcaption><lem.p.title></figcaption>
  </figure>
  <lem-style>
    .photo { &[data-placement="right"] { float: right; } }
  </lem-style>
</lem-fn>

<lem-fn chipList(size: string, slot: html)>
  <ul class="chipList <lem.size>">
    <lem.slot>
  </ul>
</lem-fn>

<lem-fn chips(post: Post)>
  <lem-let items>
    <li><time datetime="<lem.post.date>"><lem.post.date.format("%B %-d, %Y")></time></li>
    <lem-match post>
      <lem-case LinkPost as link>
        <li><a href="<lem.link.linkUrl>"><lem.link.linkTitle></a></li>
      </lem-case>
      <lem-default>
        <li><lem.post.readingTime.minutes()> min read</li>
      </lem-default>
    </lem-match>
  </lem-let>
  <lem.chipList(size: "small", slot: items)>
</lem-fn>
```

A layout is a function of the record and the rendered body. It wraps another by capturing
its markup and passing it on, and places what components collected with `mark("styles")`,
`mark("head")` and `mark("scripts")`. Inside a layout or component `page` is typed `Page`,
so read the record through the parameter; and no parameter may be named `page`, `site` or
`build`.

Those three marks are filled by the shell's default plugin, which the site's
`lemmon.config.ts` lists as `plugins: [bundler()]`: `styles` becomes a `<link>` to the
page's collected CSS, written as one hashed file under `/assets/immutable/bundle/`. A site
listing no plugins is warned at every mark. Write the `mark()` call itself: a
`<lem.styles()>` wrapper is not yet shipped (∆wOn), so that name is undefined.

```lem layouts.lem
<lem-fn base(title: string, body: html)>
  <!doctype html>
  <html lang="en">
    <head>
      <title><lem.title> · <lem.site.name></title>
      <lem.mark("styles")>
      <lem.mark("head")>
    </head>
    <body><lem.body><lem.mark("scripts")></body>
  </html>
</lem-fn>

<lem-fn article(post: Post, body: html)>
  <lem-let main>
    <article>
      <lem-if post.hero>
        <lem.photo(post.hero, .hero)>
      </lem-if>
      <h1><lem.post.title></h1>
      <lem.chips(post)>
      <lem.body>
    </article>
  </lem-let>
  <lem.base(post.title, main)>
</lem-fn>

<lem-fn sitePage(p: SitePage, body: html)>
  <lem.base(p.title, body)>
</lem-fn>
```

**Whitespace** (D26–D28): write one statement per line with indented bodies, as
`tests/fidelity/the-rebound/site/` does. A `lem-let`, `lem-fn`, `lem-type`, `lem-enum` or
collect tag alone on its line takes the line with it; `<lem.…>`, `lem-if` and `lem-for` keep
theirs. A function, capture or collect body loses one newline at each edge and its shared
indentation (a body hugging its tag, `<lem-fn f()><svg>`, stays verbatim), and nothing
re-indents a substituted value. Whitespace matters only where a browser renders it — a
newline between inline elements is a space, `<pre>` is byte for byte — so keep those on one
line, and ignore indentation in the output.

**Escaping:** `<lem.…>` of a `string` escapes `& < > " '`, safe in text and in a quoted
attribute — always quote attributes. `html` (a block function's result, a capture, markdown,
`raw()`) is emitted verbatim, and `raw(s)` is the only cast from `string` to `html`. A `url`
is attribute-escaped; compose one as `url(site.siteUrl.text() + page.url.text())`.

## Pages & documents

A `.lem` that opens with `---` front matter is a page template: the front matter is its
record and its output is the body its layout receives. A `.lem` without front matter holds
definitions only, and output at its top level warns.

```lem index.lem
---
type: SitePage
title: Home
---
<ul class="recent">
  <lem-for post in site.recent>
    <li><a href="<lem.post.url>"><lem.post.title></a></li>
  <lem-else />
    <li>Nothing yet.</li>
  </lem-for>
</ul>
```

A `.md` document is markdown with lem blocks (D07, D39). Its front matter is its record — a
`Post` here, typed by the `feed/*/index.md` glob — and `page` and `site` are in scope.

```md feed/the-rebound/index.md
---
title: The Rebound
subhead: I’m rebuilding limulus.net.
date: 2024-11-24 11:35:00 -07:00
author: eric
hero: upper-falls
---

This used to be a proper website.

<lem.photo(site.photos["middle-falls"], .right)>

<aside class="note">

Prose inside an element: a blank line on each side lets the markdown run.

</aside>
```

- **A block starts a line** (`<lem.` or `<lem-` first) at the document's top level — not in
  a paragraph, list, blockquote, indented code or raw HTML block — and ends its line. Inline
  tags mid-paragraph are not supported (reserved for v2).
- **A block is a template** from its opening tag to its close: markdown stops at the tag, so
  `*x*` in a capture stays literal. The prose between blocks is one markdown document, so
  reference links and footnotes resolve across blocks.
- **An element around prose** is raw HTML with a blank line inside each edge, as the
  `<aside>` above; without them the prose stays unrendered and a block in it is an error.
- **A string is a string:** look a record up by id, `site.photos["middle-falls"]`; a miss
  fails the render.
- **Code fences become `code` marks** the host highlights: `highlighter()`, listed in
  `lemmon.config.ts` beside `bundler()`, claims every language highlight.js has, by name
  and alias, spelled as it spells them (`ts`, not `TS`). A language no producer claims
  warns. Write about Lemmon in a fence, a code span, or as `&lt;lem`.
- **A page no glob matched and no `type:` named** has `page: any` and renders its own
  output, with no layout.

## Checking your work

Run `lem check` (`references/checking.md` says how): it compiles, loads and renders the
whole site as `lem build` does, writing nothing, and prints each diagnostic as
`path:line:col` with the lines its span covers beneath, `^` under what it covers;
`lem build` prints the same report and writes nothing on an error. When the compiler
objects, its message says what to write instead, and `references/mistakes.md` lists the
diagnostics authors hit most, each with a source that earns it.

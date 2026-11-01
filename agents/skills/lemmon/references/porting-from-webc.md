# Porting WebC to Lemmon

How the idioms in limulus.net's `www/_includes/` (four layouts, twenty components) are
written in Lemmon. Read `SKILL.md` first; this page assumes it.

Four things change in kind, not just in spelling:

- **No JavaScript at build time** (D06). Every `<script webc:type="js">` and `webc:setup`
  becomes Lemmon: a typed `lem-fn`, a schema default, a computed binding in `site.lem` — or,
  where the work needs something Lemmon cannot compute, data a plugin hands the site. What
  `eleventy.config.ts` does to a post's markdown before it renders, such as appending link
  definitions, is a plugin's `transformDocument` in `lemmon.config.ts`, which rewrites the
  body alone; its front matter is the schema's to compute. Its markdown-it `highlight`
  function is the shell's `highlighter()`, which writes the same `<pre><code class="hljs">`.
- **Data is typed.** Front matter and data files load against declared records, and a record
  has exactly its declared fields. `$data`, `collections` and `this.getPhoto` become reads
  of `site`.
- **A component is called, not matched by tag name.** WebC turns `<chip-list>` into a
  component because `chip-list.webc` exists; in Lemmon `<chip-list>` is opaque HTML, and the
  component is `<lem.chipList(…)>`. Keep a custom element in the output where the 11ty
  output has one, written inside the function.
- **Layouts chain explicitly.** `layout: page` in a layout's front matter becomes the layout
  calling the next one with what it captured.

The examples compile against the `site.lem` sketch below, written for them; the port
(`∆foi`) designs the real one. Exact markup, class names and whitespace are the parity
harness's to judge (`∆Zo7`), not these examples'.

```lem site.lem
<lem-enum Placement = hero | left | right />

<lem-type Rendition>
  name: string
  format: string
  url: url
  width: int
  height: int
</lem-type>

<lem-type Photo>
  alias: string
  title: string
  description: string
  blurhash: string
  tags: [string]
  renditions: [Rendition]
</lem-type>

<lem-type Author>
  name: string
  photo: string
  url: url
  fediverse: string?
</lem-type>

<lem-type Vod>
  id: string
  alt: string
  duration: duration?
</lem-type>

<lem-type Revision>
  date: datetime
  commit: string
  message: string?
</lem-type>

<lem-type RevisionLog>
  revisions: [Revision]
</lem-type>

<lem-type Revisions>
  versions: {string: any}
  entries: {string: RevisionLog}
</lem-type>

<lem-type Article extends Page>
  title: string
  subhead: string?
  teaser: string?
  date: datetime
  author: string
  hero: Photo? by site.photos
  image: Photo? by site.photos
  vod: Vod?
  readingTime: duration = body.readingTime()
  layout: (Article, html): html = article
</lem-type>

<lem-type LinkPost extends Article>
  linkUrl: url
  linkTitle: string
  title: string = "Link: " + self.linkTitle
  linkDomain: string = self.linkUrl.text().split("/")[2].replace("www.", "")
</lem-type>

<lem-let photos: {string: Photo} from "_lemmon/photos.json" />
<lem-let authors: {string: Author} from "_data/authors.json" />
<lem-let revisions: Revisions from "_data/revisions.json" />
<lem-let posts: [Article] from "feed/*/index.md" />
<lem-let name: string = "limulus.net" />
<lem-let siteUrl: url = url("https://limulus.net") />
```

## The mapping

| WebC                                    | Lemmon                                      |
| --------------------------------------- | ------------------------------------------- |
| `<x webc:if="c">…</x>`                  | `<lem-if c><x>…</x></lem-if>`               |
| `<x webc:for="a of xs">`                | `<lem-for a in xs><x>…</x></lem-for>`       |
| `@text="e"`                             | `<lem.e>` inside the element (escaped)      |
| `@raw="content"`                        | `<lem.body>`, the layout's `html` parameter |
| `:href="e"`                             | `href="<lem.e>"`                            |
| `:@article="e"`, `@size="small"`        | arguments: `f(e, size: "small")`            |
| `@nolink="true"`                        | a typed argument: `nolink: true`            |
| `@attributes`                           | explicit parameters; there is no spread     |
| `<slot>`, named slots                   | `html` parameters, filled by capture        |
| `webc:root`, `<template webc:root>`     | the function writes its own root element    |
| `webc:nokeep`                           | leave the wrapper element out               |
| `<style webc:scoped>`, `:host`          | `<lem-style>` under `.fnName` (D20)         |
| `<style>`                               | `<lem-style>` with plain selectors          |
| `<script webc:keep>`                    | the script as text; it passes through       |
| `webc:type="js"`, `webc:setup`          | `lem-fn`s, defaults, computed bindings      |
| `$data.authors[id]`                     | `site.authors[id]`                          |
| `this.getPhoto(id)`                     | `by site.photos`, or `site.photos[id]`      |
| `collections.article`                   | a `from` glob binding in `site.lem`         |
| `---js pagination`, `size: 1`, `alias`  | `type:` and `for: alias: site.list`         |
| `pagination.href.previous`              | `page.prev.url`, declared `prev: T?`        |
| `pagination.page.previous.title`        | `page.prev.title` in `<lem-if page.prev>`   |
| `eleventyComputed.js`                   | schema defaults, reading `self`             |
| `layout: page` in a layout              | the layout calls the next layout itself     |
| `page.url`, `page.inputPath`            | the same fields of `Page`                   |
| `eleventy.generator`                    | `build.generator`                           |
| `site.url` (`_data/site.json`)          | a binding not named `url`: `site.siteUrl`   |
| `this.svg(name)`                        | `<lem.svgUse(name)>`; the sprite is a mark  |
| `<link>` to `getBundleFileUrl('css')`   | `<lem.mark("styles")>`, link and all        |
| `getBundle('html', 'head')`             | `<lem.mark("head")>`                        |
| `src="/lib/x/index.XXXXXXXX.js"`        | `src="<lem.asset("/lib/x/index.js")>"`      |
| `{% renderTemplate 'webc' %}` in a post | a lem block alone on its line               |
| JS appending to a post's markdown       | a plugin's `transformDocument`              |
| `eleventy-plugin-rss`, `post.content`   | `build.bodies[p.url.text()]` in a feed      |
| a feed's `base`, `HtmlBasePlugin`       | `origin` in `lemmon.config.ts`              |

## No direct equivalent

- **String building.** Template literals have no counterpart: interpolation is not
  implemented, `+` joins only strings, and nothing turns a number into a string. Build
  markup in the template instead — a `srcset` is a `lem-for` that prints each URL and width
  (below) — and keep string-returning functions to strings.
- **Regular expressions and URL parsing.** `replace(from, to)` replaces every literal
  occurrence, and `split`, `startsWith`, `endsWith` and `contains` are the rest. A domain is
  `u.text().split("/")[2]`; a numeric-segment test (`/^\d+$/`) is a `lem-fn` that `replace`s
  each of the ten digits with nothing and checks what is left is empty, as the port's
  `numeric()` does. Anything past that, give the page as data.
- **Time zones.** A datetime prints and formats in the offset it was written in (D38):
  `toLocaleDateString(…, {timeZone: 'America/Phoenix'})` is `format("%B %-d, %Y")` only
  because the posts are written in `-07:00`, since no zone can be named. Offset zero is
  `utc()`: `toISOString()` is `<lem.d.utc()>`, though a printed datetime drops trailing
  zeros from its fraction (`.42Z` where JavaScript writes `.420Z`, and `:00Z` where it
  writes `:00.000Z`), and `toUTCString()` is `d.utc().rfc822()`, which writes `+0000` and an
  unpadded day where JavaScript writes `GMT` and pads it.
- **Heterogeneous data.** `photos.json` maps aliases to ids as strings beside its records,
  and each photo has keys a `Photo` would have to declare; neither loads as
  `{string: Photo}`. A script that writes the declared shape beside it loads typed, as
  limulus.net's port derives `_lemmon/photos.json` with a spec that fails on drift; a
  plugin's data hook would do the same (`∆wOn`, parked). `{string: any}` loads anything and
  checks nothing.
- **Combining collections.** There is no list concatenation, so `collections.article` across
  `feed/`, `links/` and `tils/` needs one `from` glob that matches them all, with `type:`
  keys naming the subtypes. The shell matches a glob as passthrough's are matched:
  picomatch, from the input directory, dotfiles included, case-insensitive.
- **Throwing.** `throw new Error('Missing alt text…')` becomes a type: `alt: string` on
  `Vod` is required, so its absence is a load error at the front matter.

## Conditionals, text and attributes

`article-header.webc` reads `article.data.*` and tests with `webc:if`; the record's own
fields and `lem-if` replace both. An optional attribute is a `lem-if` inside the start tag,
which Lemmon can do because it never parses HTML.

```lem components.lem
<lem-fn articleHeader(a: Article)>
  <article-header class="articleHeader">
    <lem-if a.hero>
      <lem.articlePhoto(a.hero, .hero)>
    </lem-if>
    <hgroup>
      <h1><lem.a.title></h1>
      <lem-if a.subhead>
        <p><lem.a.subhead></p>
      </lem-if>
    </hgroup>
    <lem.chips(a, false)>
  </article-header>
</lem-fn>

<lem-fn articlePhoto(photo: Photo, placement: Placement)>
  <lem-let linked = photo.tags.contains("limulus.net/photos") />
  <article-photo class="articlePhoto" placement="<lem.placement>">
    <figure>
      <a<lem-if linked> href="/photos/<lem.photo.alias>/"</lem-if>>
        <lem.phoPicture(photo, ["lg", "md"])>
      </a>
      <figcaption><span><lem.photo.title></span></figcaption>
    </figure>
  </article-photo>
</lem-fn>
```

## Slots and scoped styles

`chip-list.webc` is a `<slot>` in a `<ul>` with `:host` styles. The slot is an `html`
parameter; the styles nest under a class named after the function (D20), and `:host[…]`
becomes `&[…]` (the whole rule is under Styles, below). The caller captures its items,
then calls.

```lem components.lem
<lem-fn chipList(size: string?, slot: html)>
  <chip-list class="chipList"<lem-if size> size="<lem.size>"</lem-if>>
    <ul>
      <lem.slot>
    </ul>
  </chip-list>
  <lem-style>
    .chipList {
      & ul { display: flex; flex-wrap: wrap; gap: 0.5rem; list-style: none; }
      &[size="small"] ul { font-size: 0.7rem; }
    }
  </lem-style>
</lem-fn>

<lem-fn chips(a: Article, nolink: bool)>
  <lem-let items>
    <li><lem.authorChip(a.author, nolink)></li>
    <li><lem.publishedChip(a.date)></li>
    <lem-match a>
      <lem-case LinkPost as link>
        <li><lem.linkChip(link, nolink)></li>
      </lem-case>
      <lem-default>
        <li><lem.timeChip(a)></li>
      </lem-default>
    </lem-match>
  </lem-let>
  <lem.chipList(nolink ? "small" : null, items)>
</lem-fn>
```

## JavaScript becomes typed functions

The chips' `webc:type="js"` blocks are small programs: `nolink ? chip : <a>chip</a>`, a
minutes rounding rule, a lookup in `$data.authors`. Each is a function or a capture. The
author's chip has a photo where the others have an icon, so `dataChip`'s icon is optional
and `authorChip` calls it rather than writing a `data-chip` of its own (Styles, below).

```lem components.lem
<lem-fn dataChip(icon: string?, slot: html)>
  <data-chip class="dataChip">
    <lem-if icon>
      <lem.svgUse(icon)>
    </lem-if>
    <lem.slot>
  </data-chip>
</lem-fn>

<lem-fn authorChip(id: string, nolink: bool)>
  <lem-let author = site.authors[id] />
  <lem-let face>
    <lem.phoPicture(site.photos[author.photo], ["xs"])>
    <lem.author.name>
  </lem-let>
  <lem-let chip = dataChip(null, face) />
  <lem-if nolink>
    <lem.chip>
  <lem-else />
    <a href="<lem.author.url>"><lem.chip></a>
  </lem-if>
</lem-fn>

<lem-fn publishedChip(date: datetime)>
  <lem-let when>
    <time datetime="<lem.date>"><lem.date.format("%B %-d, %Y")></time>
  </lem-let>
  <lem.dataChip("newspaper-solid", when)>
</lem-fn>

<lem-fn linkChip(post: LinkPost, nolink: bool)>
  <lem-let domain><lem.post.linkDomain></lem-let>
  <lem-if nolink>
    <lem.dataChip("link-solid-full", domain)>
  <lem-else />
    <a href="<lem.post.linkUrl>"><lem.dataChip("link-solid-full", domain)></a>
  </lem-if>
</lem-fn>

<lem-fn roundedMinutes(d: duration): int =
  d.seconds() % 60 gte 15 || d.minutes() == 0 ? d.minutes() + 1 : d.minutes() />

<lem-fn timeChip(a: Article)>
  <lem-let span = a.vod?.duration ?? a.readingTime />
  <lem-let label>
    <lem.roundedMinutes(span)> min <lem.(a.vod ? "watch" : "read")>
  </lem-let>
  <lem.dataChip(a.vod ? "film-solid" : "scroll-solid", label)>
</lem-fn>
```

`readingTime` counts words between tags at two hundred a minute, rounded up — not the
`reading-time` package's count over the raw markdown — so a chip can differ by a minute.

## Photos, renditions and `srcset`

`pho-picture.webc` filters `photo.renditions`, sorts them, and joins
`` `${url} ${width}w` `` strings. Without string building, the `srcset` is a function that
prints the list. Its body is one line, and a function body loses the newline at each edge
(D26), so the attribute gets exactly `url 1280w, url 640w`.

A photo without the wanted rendition is a render error at `jpeg[0]`, as the WebC original
threw; `.first()` behind a `lem-if` would render an empty picture and exit clean, which is a
guess (invariant 10).

```lem components.lem
<lem-fn srcset(renditions: [Rendition])>
  <lem-for r, i in renditions><lem-if i gt 0>, </lem-if><lem.r.url> <lem.r.width>w</lem-for>
</lem-fn>

<lem-fn phoPicture(photo: Photo, names: [string])>
  <lem-let wanted = photo.renditions
    .filter(|r| r.name in names)
    .sortBy(|r| r.width)
    .reverse() />
  <lem-let jpeg = wanted.filter(|r| r.format == "jpeg") />
  <lem-let largest = jpeg[0] />
  <pho-picture>
    <blur-hash blurhash="<lem.photo.blurhash>"
      width="<lem.largest.width>" height="<lem.largest.height>">
      <picture>
        <source type="image/avif"
          srcset="<lem.srcset(wanted.filter(|r| r.format == "avif"))>">
        <source type="image/jpeg" srcset="<lem.srcset(jpeg)>">
        <img loading="lazy" decoding="async" src="<lem.largest.url>"
          srcset="<lem.srcset(jpeg)>" alt="<lem.photo.description>"
          width="<lem.largest.width>" height="<lem.largest.height>">
      </picture>
    </blur-hash>
  </pho-picture>
</lem-fn>
```

## The sprite

`this.svg(name)` printed a `<svg>` that `use`s a sprite symbol, with a `desc` id counted per
page. `uniqueId(prefix)` is that counter: it counts per prefix and restarts every render,
body and layout in one run. The sprite itself is a mark a site plugin produces: limulus.net's
`lemmon/svg-sprite.ts` builds it from `www/assets/svg/common` with svg-sprite in its
`assets` hook, so a missing or unreadable icon stops the build, and its producer places it
where `page.lem` writes `<lem.mark("sprite")>` (`∆IvY`). `tests/fidelity/the-rebound/site/
sprite.lem` inlines one by hand.

```lem components.lem
<lem-fn svgUse(name: string)>
  <lem-let desc = uniqueId("symbol-" + name + "-desc") />
  <svg aria-labelledby="<lem.desc>" role="group">
    <desc id="<lem.desc>"><lem.name> icon</desc>
    <use href="#svg-<lem.name>"></use>
  </svg>
</lem-fn>
```

Two icons that sit side by side (`this.svg('creative-commons') + this.svg(…-by)`) stay on
one line, `<lem.svgUse("creative-commons")><lem.svgUse("creative-commons-by")>`: a newline
between them would render as a space.

## Lookups that may miss

`revision-history.webc` reads `$data.revisions?.entries?.[filePath]` with optional chaining.
A map index that misses is a render error in Lemmon, so test with `has()` first; the
annotation is what gives the empty `[]` its type. `slice(1)` is `drop(1)`, and
`[...xs].reverse()` is `reverse()`, since nothing is mutated.

```lem components.lem
<lem-fn revisionHistory(a: Article)>
  <lem-let revisions: [Revision] = site.revisions.has(a.inputPath)
    ? site.revisions[a.inputPath].drop(1).reverse()
    : [] />
  <lem-let label = revisions.length() == 1 ? "Revision" : "Revisions" />
  <lem-if revisions.length() gt 0>
    <details class="section-separator">
      <summary><lem.revisions.length()> <lem.label></summary>
      <ul>
        <lem-for r in revisions>
          <li>
            <a href="https://github.com/limulus/limulus-dot-net/commit/<lem.r.commit>">
              <time datetime="<lem.r.date>"><lem.r.date.format("%Y-%m-%d %H:%M")></time>
            </a>
            <pre><code><lem-if r.message><span><lem.r.message></span></lem-if></code></pre>
          </li>
        </lem-for>
      </ul>
    </details>
  </lem-if>
</lem-fn>
```

## The layout chain

`article.webc` declares `layout: page`, `page.webc` declares `layout: base.webc`, and each
receives `content`. In Lemmon the schema's `layout` default names `article`, which captures
its markup and calls `pageLayout`, which calls `base`. `base` takes the page as `Page` and
tests `p is Article` where `base.webc` tested `tags?.includes('article')`. The bundles
become marks, and the record arrives as a parameter — inside a layout, `page` is typed
`Page` and has no `title`. The site's config lists `plugins: [bundler()]`, the shell's
default plugin, which fills the marks: `mark("styles")` stands for the whole `<link>` to
the page's bundle, not its href.

```lem layouts.lem
<lem-fn article(post: Article, body: html)>
  <lem-let main>
    <article class="article">
      <header>
        <lem.articleHeader(post)>
      </header>
      <lem.body>
      <lem.revisionHistory(post)>
    </article>
  </lem-let>
  <lem-style>
    .article {
      & > header { margin-bottom: 1.5rem; }
    }
  </lem-style>
  <lem.pageLayout(post, post.title, post.subhead ?? post.teaser, main)>
</lem-fn>

<lem-fn pageLayout(p: Page, title: string, description: string?, body: html)>
  <lem-let main>
    <a href="#main" class="visually-hidden">Skip to main content</a>
    <main id="main">
      <lem.body>
    </main>
  </lem-let>
  <lem.base(p, title, description, main)>
</lem-fn>

<lem-fn docTitle(u: url): string = u.text() == "/" ? site.name
  : site.name + " / " + u.text().split("/").filter(|s| s != "").join(" / ") />

<lem-fn base(p: Page, title: string, description: string?, body: html)>
  <!doctype html>
  <html lang="en">
    <head>
      <meta charset="utf-8">
      <lem.mark("styles")>
      <lem.siteStyles()>
      <title><lem.docTitle(p.url)></title>
      <meta property="og:title" content="<lem.title>">
      <meta property="og:url" content="<lem.site.siteUrl><lem.p.url>">
      <lem-if description>
        <meta property="og:description" content="<lem.description>">
      </lem-if>
      <lem-if p is Article>
        <meta property="og:type" content="article">
        <meta property="article:published_time" content="<lem.p.date>">
      <lem-else />
        <meta property="og:type" content="website">
      </lem-if>
      <meta name="generator" content="<lem.build.generator>">
      <lem.mark("head")>
    </head>
    <body>
      <lem.body>
      <lem.mark("scripts")>
    </body>
  </html>
</lem-fn>
```

## Styles

A `<style webc:scoped>` becomes one `lem-style`, placed after the calls it makes (below)
and translated line for line so that it still diffs against the `.webc`: the body goes
inside `.fnName { … }`, each `:host` becomes `&` (a bare `:host { … }` is `& { … }`), and
an `@media` stays where it stood, nested. Every nested selector keeps its `&` — `& ul`,
never a bare `ul`, which Chrome before 120 and Safari before 17.2 drop. `@keyframes` cannot
nest in a style rule, so it goes after the wrapper's closing brace. A plain `<style>` keeps
its selectors and gets no wrapper. The specificity is WebC's: `&` under `.fnName` is one
class, as the hash was.

WebC bundles a page's styles children first: a component's after the components it calls,
the body's before its layout's, each layout's before the next one's. Lemmon collects
blocks in the order they run, so where a block stands decides its turn. A component's
block goes at the end of its body, after every call it makes. A layout's goes after its
`lem-let` capture and before its call to the next layout, as `article`'s above: before
the capture, it would collect ahead of the components the capture runs.

`base.webc`'s links to `../css/*.css` were bundled last, after every component's styles.
A `.lem` file cannot read a stylesheet, so the port generates a copy — a script writes
`siteStyles()`, one `lem-style` per stylesheet, each line indented so that the block's
trim and dedent (D26) give back the file trimmed — and `base` calls it beside the mark:
`base` runs last, so they collect after every other block. Edit the CSS and rerun the
script, never the generated file. A `lem-style` body is a template, which only `<lem.`,
`<lem-` and `</lem-` interrupt (invariant 2), so a stylesheet holding one of those — a
`</lem-` in a comment would close the block — cannot be copied so; the generator refuses
any `<` at all, as a guard.

```lem site-styles.lem
<lem-fn siteStyles()>
  <lem-style>
    html {
      font-family: 'Mona Sans', sans-serif;
    }
  </lem-style>
</lem-fn>
```

A block collects when its function runs — whether or not the value it stands in is
printed — and only then, so a component's styles reach the pages that call it. A function
that writes another component's class therefore calls that component instead: an
`authorChip` that printed `<data-chip class="dataChip">` itself left `.dataChip`'s styles to
whichever other chip on the page called `dataChip`.

## Client scripts

The client TypeScript is bundled by the shell's `esbuild()`, listed in the config (D45),
and keeps its `XXXXXXXX` placeholders — `import '/lib/x/index.XXXXXXXX.js'`,
`new URL('./worker.XXXXXXXX.js', import.meta.url)` — which the plugin resolves in the
bundles as Eleventy did. The markup changes: Eleventy rewrote every `….XXXXXXXX.js` string
in a page's HTML after rendering it, and a template names the script by its URL without
the hash, through `asset()`, which compiles to the hashed URL — a compile error with a
did-you-mean where no entry ships at that path:

```lem
<script type="module" src="<lem.asset("/lib/common/index.js")>" async></script>
```

A `<script webc:keep>` loading one becomes that tag where it stood — in the `head`, inside
a component — rather than a `lem-script`, whose `mark("scripts")` writes no `async` and
places every URL the page collected at every mark. An inline loader prints the URL into its
text, since a `<lem.` tag fires inside a script: `import('<lem.asset("/lib/rum/index.js")>')`.
A path in front matter is an `asset` field holding the path from `/`: the core does no path
arithmetic (D38), so a page-relative `./lib/index.js` cannot resolve, and penumbra's
`scripts:` names `/penumbra/journal/<slug>/lib/index.js`, typed `scripts: asset?`.

## Posts

A post keeps its markdown. Its front matter loses what the shell and the schema now own —
`tags: [article]` is collection membership, which the glob decides, and `layout: article` is
the schema's default — and an undeclared key left behind is a load error. Each Liquid
`{% renderTemplate 'webc' %}<article-photo …>{% endrenderTemplate %}` becomes one lem block
on its own line, the photo looked up by id.

```md feed/the-rebound/index.md
---
author: eric
title: The Rebound
subhead: >-
  I’m rebuilding limulus.net. It’s way past time to have a website again.
date: 2024-11-24 11:35:00 -07:00
hero: upper-falls-letchworth-state-park
---

This used to be a proper website.

<lem.articlePhoto(site.photos["middle-falls-letchworth-state-park"], .right)>

## The Last Glacial Period
```

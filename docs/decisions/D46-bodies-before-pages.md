# D46 — Bodies render before the pages that read them

- **Status:** Accepted 2026-10-19, planned 2026-10-18 and settled while building `∆zu0`;
  refines D42's clause on a rendered body reaching a template, and replaces the
  architecture's step 10, a second phase after every other page; its passages on the
  reload of a page no record typed hold until D48, which refuses such a page. Revised
  2026-11-06 (`∆H7Q`): a body's links are read as the link check reads them, by HTML's
  rules, and rewritten in place
- **Backlog item:** `∆zu0`; `∆H7Q` (the revision)
- **Normative text:** `docs/architecture.md` § WASM API, § Build pipeline;
  `docs/language.md` § Ambient types; `docs/shell.md` § `lem build`, and the
  config's `origin`

## Context

D42 settled that a rendered body reaching a template is a shell-supplied string of a
page's finished output — rendered, spliced, transformed — loaded back as a data source
for a second render phase, and the architecture's step 10 put that phase after every other
page was written. Building limulus.net's three feeds found what they carry. Eleventy's
item content is a post's template output without its layouts, run through every
transform with the feed's base and escaped once: its own page body with its links made
absolute is tree-identical to it on 23 of 25 items, and one string per post serves all
three feeds — 25 bodies, 125,311 characters. Two things happen to that body that the core
may not do: its code is highlighted by the shell's producers (D42), and its links — 44 in
the RSS feed, `../`, `./` and same-page `#` among them — are made absolute, which takes an
HTML parser (invariant 2). The sitemap reads records alone. No feed needs a finished
page, and `render` had no way to return a body: it always called the entry field.

## Decision

**A document's body renders alone, is finished in the shell, and is bound before any page
renders.** After the load, the shell renders each document the load made — each page
`plan()` lists whose file is a `.md` — with `render({path, body: true})`: its own output,
its entry field never called, which is what its layout would be given, with the marks
and fragments standing in it and its `uniqueId`s counted as its page counts them, since a
page renders its body first. It resolves the body's marks with the handlers a page's are,
so its code is highlighted as on its page, rebases its links (below), and then it loads
again with every body bound, by its page's url, and renders every page as before
(§ Build pipeline, step 6).

**The bodies are a field of `build`**, `bodies: {string: string}` on the shell's `Build`
in `ambient.lem`, since the shell produces them (D30), keyed by the url as `url.text()`
writes it: a template reads `build.bodies[post.url.text()]`. No name is reserved for them;
`output` already names the config's output directory. A body is a `string`, never `html`
(D37): printed, it is escaped once, which is a feed's form, and `raw()` is the only way to
splice it as markup (invariant 7).

**While the bodies render, `build.bodies` is empty**, since no body exists yet. Two
readers see that value. A document's own body, and any component it calls, read it in the
body pass, where an index into it is a render error at the read, labelled at the
document's front matter as rendering its body before any body exists; its page
renders its body again after the second load, so where a document reads `build.bodies`
the body on its page and the body in the map differ. A computed binding in `site.lem`
runs at both loads, so one that indexes a body fails the first load, and a tolerant read
— `has`, `keys()`, a `lem-for` — sees `{}` there and every body at the second. A layout
renders after the body pass and reads every body, its own page's among them. A document
no glob holds and no `type:` names is no page `plan()` lists, so it has no body; its page
renders after a reload that carries every body, as does each page with no record (until
D48, which refuses such a page).

**Every page renders after the body pass.** Nothing decides membership — no key, no
analysis of what a page reads: the pass renders documents' bodies only, and every page
renders once, after it. A page template has no body in the map: its output is a page of
its own, not prose another page carries.

**Handlers run for a document's body, then for its page, and are told which.** A
producer's context and a decorator's third argument carry `body`, true in the body pass.
A producer's file is checked and written as a page's is, named as the body's — "the body
of `posts/a.md`" — and the same bytes its page writes are written once; so a document
that places `mark("styles")` in its own text gets a bundle of its own fragments, which
only `build.bodies` links to. A body's resolution reports only errors. A handler that
fails on a body, and fragments the body collects and nothing in it places, are not warned
at: its page resolves the same marks over the same fragments and warns once — so a layout
that never prints its body hides a handler's failure on it. Content a handler returns as
bytes must be UTF-8, on a page or a body alike, or it is a failure like any other answer
that is no content (D42), and the mark keeps what it had: a body is decoded into a string,
and a page is served as UTF-8.

**The body pass makes a body's links absolute against the site's `origin`**, a top-level
field of `lemmon.config.ts` — `origin: 'https://limulus.net'`, an http or https URL with
nothing after its host and port, checked as written, since Lemmon has no path prefix — and
only there: a page keeps its links as written. The rule is the one Eleventy's HTML base
plugin applies to a feed's content, over the attributes `@11ty/posthtml-urls` walks for it:
a url the URL parser reads on its own is kept, trimmed; any other is resolved against the
page's url on the origin. A `srcset`'s candidates and a `ping`'s urls are taken one by one.
_(Revised by `∆H7Q`: the rule is applied over every url the link check reads in an element's
attributes (D47), by HTML's rules — a `ping` split at whitespace, any SVG element's `href`
and `xlink:href`, a refreshing `meta`'s url, an Open Graph url's `content`, a `style`
attribute's urls — each stripped of the ASCII whitespace HTML strips from a url. A url that
is only a fragment is kept where its element embeds what it names, an SVG
`<use href="#icon">` or a `style` attribute's `url(#clip)`, since it names part of the body
itself: `use` loads no other origin's document, and CSS reads a `url(#…)` in the document
that holds it. A `<style>`'s text is left as written, though the check reads it: what
`lem-style` collects reaches a body through `bundler()` as a `<link href>`, which is
rebased.)_ The shell parses the body with parse5 and rewrites only the attributes that
change, so every other byte stays as rendered (invariant 2 keeps the parsing out of the
core). _(Revised by `∆H7Q`: only an attribute holding a url that changes, and in its value
only that url, so a `srcset`'s descriptors, a `ping`'s spacing and the rest of a `style` say
what they said; the attribute is written anew, double-quoted, its character references
decoded. A refresh's url has its quotes percent-encoded, since HTML's refresh steps end a
quoted url at its quote, and a page's url holds `'` as it is.)_ Where the site names no
origin, a relative url is resolved against the page's url and becomes a path from `/`, so a
body read on another page of the site still names what it named — one naming a host of its
own, `\\cdn/x` as much as `//cdn/x`, is kept, and a path that would start `//` is written
`/.//` so that it names no host. A url the URL parser refuses even against the page's url is
kept as written and warned at, at the document's front matter: a browser would refuse it
too, and one mistyped link costs no build. A `srcset` candidate whose descriptors
`parse-srcset` refuses is warned at the same way where the rebase drops it; what that
library would print on standard output is said there instead. _(Revised by `∆H7Q`: a
`srcset` is read by `@prettier/parse-srcset`, which refuses one whole where any candidate's
descriptors do not parse; it is kept as written, its urls unrebased, and nothing is said of
it here: the link check refuses it, as an error, on the page that prints the body (D47), and
one message is enough.)_ `∆yOJ`'s link check reads the same `origin`.

**An error in the body pass stops the build before the second load**, so the error
reported is the author's and not a fault downstream of it — D37's rule for the loader: no
page reports a body's runtime error again, and no feed or computed binding reports the
body it then lacks. A second load that fails stops the build before any page renders.

## Alternatives considered

- **Finished pages rather than bodies**, as D42 and step 10 had it. Rejected: no feed
  reads a page, and getting a body back out of one means parsing the layout's markup by
  convention.
- **An ambient value of its own, `output.bodies`.** Rejected by Eric: it reserves
  `output` and `Output` for every site, where `output` already names the output
  directory; a field on `Build`, which the shell produces too, reserves nothing.
- **Documents' pages in the first phase, templates in the second.** Rejected: a document's
  layout would read the empty map and a template's the full one, two values of one name
  across pages.
- **An opt-in front-matter key.** Rejected: a third reserved key that saves nothing, since
  every page renders once either way, and a page not opted in still sees some value.
- **Membership by what a page reads**, from its unit's facts. Rejected for now: facts do
  not cross the boundary (`∆Qr2`), and it would only skip a pass of tens of milliseconds.
- **Bodies passed with each render.** Rejected: on limulus.net 59 renders of 126 KB each,
  about 7 MB a build, where a load sends them once.
- **A page field the loader fills**, as it fills a generated page's neighbours (D43).
  Rejected: a host-declared field, its refusals in front matter and in defaults, and a
  field on every record that is null until the second load, for the same bytes.
- **A `from` source in `site.lem`.** Rejected: a source kind through parser, compiler and
  D37 for a value the shell produces itself (D30).
- **`build.bodies` absent, not empty, while the bodies render**, so that every read in
  the body pass is an error naming the field. Rejected: the field's type would have to be
  optional, so every legitimate read — a layout's, a feed's — would narrow it first, for
  the sake of reads that the empty map already fails at the index.
- **The body returned beside every page render.** Rejected: a layout prints its body
  anywhere or not at all, so the body is no slice of the page, and every reply would
  carry it.
- **A `transformBody` plugin hook, with a shipped `absoluteUrls({origin})` plugin**, as
  planned. Rejected by Eric: every site with a feed needs the rewrite, Eleventy ships its
  equivalent, and the origin is the site's — one config field the body pass and the link
  check both read, with no hook, async refusal or plugin-naming error path to carry.
- **`∆fwn`'s `transformOutput` with a body flag.** Rejected: a page transform that has to
  skip pages by a flag; whether page transforms also run over bodies, as Eleventy's
  `renderTransforms` does, is `∆fwn`'s to decide.
- **Absolutizing in a template, through a mark around the body.** Rejected: the mark's
  content would have to be escaped by its producer — D42's rejected `escape()` by another
  door.
- **A url the URL parser refuses as an error**, as the first build had it. Rejected by
  Eric: a post linking `http://localhost:{port}/`, or a mistyped port, would stop every
  build of a site — one with no feed among them — for a link the body keeps as written
  and a browser refuses alike.
- **No rewrite without an origin.** Rejected: a body's `../` and `#fn1` would then name
  other pages wherever it is read, silently; a path from `/` names the same thing on every
  page of the site.

## Consequences

- `∆zu0` builds `body` on `render`'s `PageRef`, `Build.bodies`, the body pass and the
  second load, the config's `origin` and the rebase, and the port's feeds and sitemap;
  the architecture's step 10 is gone, and step 6 begins with the body pass.
- D42's row and record gain this clause, and D30's consequence on a body's form points
  here.
- `∆yOJ`'s link check runs over pages still in memory after every page has rendered, and
  reads the config's `origin` to tell the site's own absolute links from others.
- An untyped page's own reload re-sends every body, so each costs more by the bodies'
  size; a site types its feeds to render them without one (until D48, which retires the
  reload).
- `∆xBO`: a body edit re-renders its page and every page that reads `build.bodies`; `∆Qr2`'s
  facts could skip the pass when no unit reads it.
- `body` in a default (D37) is a document's prose as comrak rendered it at compile;
  `build.bodies` holds its finished body after render. The language doc says both.
- The rewrite differs from Eleventy's in what parity may see: a relative url's query is
  kept, where Eleventy resolves the path and fragment alone and drops it; a body's links are
  read as the link check reads them, so a refreshing `meta`'s url, any SVG element's `href`
  and `xlink:href`, an Open Graph url's `content` and a `style` attribute's urls are
  rebased, where Eleventy leaves them — but a url that is only a fragment where its element
  embeds what it names, and a `<style>`'s text, which are kept as written — and a `ping` is
  split at whitespace, where posthtml-urls splits it at commas; a url that changes is
  rewritten in place, so a `srcset`'s descriptors and commas and a `ping`'s spacing stay as
  written, where posthtml-urls joins every list it rewrites anew, and a url in CSS is
  written as a single-quoted string and a refresh's url with its quotes percent-encoded; a
  `srcset` in which any candidate's descriptors do not parse is kept as written, its urls
  unrebased, and its page's link check fails the build at it, where Eleventy drops that
  candidate from a list it joins anew; parse5 builds a browser's tree, so an element its
  tree builder drops — a `<form>` nested in another, an `<a>` in a `<select>` — keeps its
  url as written, where posthtml rewrites every tag it sees, an element it copies is
  rewritten once, where it was written, and an `<image src>`, which it reads as `<img>`, is
  rebased where Eleventy leaves it; tag and attribute names are matched as a browser does,
  so `<A HREF>` is rebased where Eleventy matches case and leaves it; the text inside a
  `<textarea>`, `<xmp>` or `<iframe>` is text, where posthtml rewrites the tags it reads
  there; a `<noscript>`'s content is parsed and rebased, as posthtml parses it; a value's
  entities are decoded before it is resolved, so `/caf&eacute;/` becomes `/caf%C3%A9/` where
  Eleventy writes `/café/`; a path's doubled slash is kept, `/a//b`, where Eleventy's path
  join collapses it; with an origin, `\\cdn.example/x` names that host as a browser reads
  it, `https://cdn.example/x`, where Eleventy gives a path on the origin; and a rewritten
  attribute is written double-quoted by its name in lower case, its character references
  decoded, every other byte as rendered, where posthtml serializes the whole body again.
  _(Revised by `∆H7Q`; until then a refreshing `meta`, an SVG element's `href` but an `a`'s,
  `xlink:href`, Open Graph urls and `style` attributes were left, as Eleventy leaves them, a
  `ping` was split at commas, and a `srcset` was read by `parse-srcset` 1.0.2 and joined
  anew where a candidate changed, one whose descriptors it refused dropped, as Eleventy
  drops it.)_
- The pass is cheap on limulus.net, measured on a copy of the port: its 25 bodies render
  in 0.3 ms and are resolved and rebased in about 9 ms, and the second load takes about
  6.5 ms, so a warm build — `lem dev`'s rebuild — goes from about 427 ms to about 442 ms,
  and `lem build` from 0.74 s to 0.75 s. The bodies grow the envelope from 603 KB to
  727 KB; the port has no untyped page, whose reload would carry them, and a load of
  either size takes the same 6.5 ms there.

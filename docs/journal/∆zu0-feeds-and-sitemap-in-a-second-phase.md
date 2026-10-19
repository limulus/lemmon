---
id: ∆zu0
completed: 2026-10-19 07:09:38 -07:00
---

## Backlog item

> - `∆zu0` Feeds and sitemap in a second phase
>   - The three feeds (rss, two atom, `/feed.xsl`) and `sitemap.xml` with video entries as
>     templates rendered after every other page, reading finished output as a string source

## Planning

The item's brief, folded here. From `∆ePP`: the three feeds (RSS and two Atom, with
`/feed.xsl`) and `sitemap.xml` with video entries, the last pages on 11ty plugins, a feed
"rendering in a second phase, reading other pages' finished output as a string source"
(D42). Mapping slice 6 on 2026-10-18 found that no feed needs a finished page: an item's
content is the post's body without its layout, after Eleventy's transforms (code
highlighted, every link made absolute against the post's url), escaped once, and one
string per post serves all three feeds. Highlighting comes from the shell's handlers and
making links absolute needs an HTML parser (invariant 2), so both belong to the shell.

Eric decided on 2026-10-18 that the sitemap fixes Eleventy's three quirks rather than
reproducing them: it lists `/tils/nodejs/`, which Eleventy's pagination left out of
`collections.all`; it leaves out the three feeds; and an undated page gets no `lastmod`,
where Eleventy gave it the template file's creation time. Planning kept D38's
written-offset dates, with parity comparing instants, and put parity's XML leg in this
item.

A planner wrote the second phase's shape as D46, and its review changed it twice by Eric's
choice: the bodies are `build.bodies`, a field on the `Build` value the shell already
produces, rather than a new `output` value reserving two names; and the site's own origin
is a top-level `origin` in the config, which the body pass rebases links against and the
link check (`∆yOJ`) will read too, rather than a `transformBody` hook and an
`absoluteUrls` plugin. The review also had the main checkout's `.wasm` restaged and the
port's 55 pages compared between the lemmon half landing and the templates going in, the
regression gate for a change to how every page renders.

## Refinement

The lemmon half landed as `2340413` (D46): `render` takes `body`, which renders a page
without its entry field; after the first load the shell renders each document's body
alone, resolves its marks with the page's handlers keeping only errors (the page warns
once), rebases its links by Eleventy's `transformUrl` rule over posthtml-urls' table
(`rebase.ts`, parse5 with scripting off so `<noscript>` is parsed, `parse-srcset` 1.0.2
pinned), loads again with every body bound in `build.bodies`, and renders every page. It
adds about 15 ms to the port's warm build and 1.5 KB to the `.wasm`. Two reviews found
real bugs, fixed before landing: an SVG `xlink:href` crashed the rebase or wrote its target
into the plain `href`; a tag named after an `Object.prototype` key crashed it; with no
origin a `\\host` link lost its host and `.//x` became protocol-relative; `parse-srcset`
wrote to stdout; `origin` accepted text the URL parser forgives; a link the parser refuses
stopped every build, even a site with no feed (now kept as written, with a warning); a
handler's bytes that are not UTF-8 crashed with no span (now a handler failure, D42); and
a page url the page pass refuses produced a misleading error from the body pass. A handler
now learns from its context whether it resolves a body or a page, since each runs twice.

The port half landed as limulus-dot-net `01f7eb9` (local): an `xmlFile` layout writing the
declaration and the stylesheet instruction, so `<?xml` is byte 0; `/feed.xml`,
`/penumbra/feed.xml` and `/tils/feed.xml` (through an `atomFeed` component) reading
`build.bodies[post.url.text()]`; `/sitemap.xml` with Eric's three fixes; the config's
`origin` read from `www/_data/site.json`, which also prints every `<link>`, `<guid>` and
`<loc>`. Parity's XML leg parses strictly (xmldom 0.9.5 with the checks it lacks), keys feed
entries by guid or id and the sitemap by `<loc>`, compares dates as instants after their
grammar (`date-spelling`), and runs each entry's content through the page classes after
mapping same-origin links back to relative. `npm run parity` compares 55 pages and 4 XML
files clean, and the scripts byte for byte. Its review caught every mutation of real output
it tried; it added two specs for predicates nothing pinned and an `updated` for an empty
Atom feed.

Found and recorded: a layout that ignores its record cannot be written without an unused
warning, so the port's XML layout reads its record (`∆gsV`); D46 made `body` a required field
of a producer's context, which the port's sprite spec had to add (nothing is released).

## Retrospective

The brief's premise, a feed reading other pages' finished output, came from the
architecture sketch and was wrong: measuring what the feeds hold showed a body and a link
rewrite. The two places Eric chose differently from the plan both removed machinery (a
reserved name, a hook with one consumer), which is worth asking about up front whenever a
plan adds a name or a hook.

---
id: ∆foi
completed: 2026-10-15 21:20:29 -07:00
---

## Backlog item

> - `∆foi` Port: `site.lem`, three layouts, the posts' components
>   - `Page` and `Post`, the base, page and article layouts and the components the seven feed
>     posts use, written as an author would, on the limulus-dot-net port branch

## Planning

The item's brief, folded here. From `∆rT5` Schemas, layouts, components: `site.lem` types
for articles, link posts, TILs, photos, vod, authors; the base/page/article/video layouts
and ~20 WebC components rewritten as `.lem` functions with `lem-style` scoping (D20); the
first real exercise of the language at scale, expected to feed fixes back. From `∆o24`
Content + markdown parity: typed front matter for every `.md`, computed defaults, the
link-post appender, markdown-it's extensions in comrak, and code by the highlight.js
producer. Scope here: only what the feed posts need — `Page` and `Post`, the base, page
and article layouts, the components those posts use, typed front matter for them —
written as an author would and not in the shape of the fidelity fixtures; an interim
`<link>` to passthrough-copied CSS until `∆D3R`; the two posts with Liquid
`renderTemplate` blocks getting lem calls on the branch. The brief deferred the video
layout to `∆tAw`; the slice's acceptance named every feed post, and one uses it, so it
came in here.

Built in the slice-1 run of 2026-10-15 by an Opus agent on the `lemmon-port` branch of
limulus-dot-net (a worktree under its `.claude/worktrees/`, with the lemmon checkout
symlinked beside it so `file:../lemmon/packages/shell` resolves from both). The agent
started before `lem build` existed, compiling and loading through the staged core from a
scratch script; the in-progress `lem build` reached the branch through the `file:` link
before the first phase ended, and built the site. The branch has seven committed feed
posts, not eight: the inventory had counted an uncommitted draft.

## Refinement

Decisions on the branch: `photos.json` mixes alias → id strings with id → photo records,
which no one type describes, so `bin/lemmon-photos.ts` derives `www/_lemmon/photos.json`
keyed by alias, outside Eleventy's data cascade, with a spec that fails `npm test` when
the two drift; the lem calls in two posts translate back to their WebC blocks through one
function (`11ty/lemmon-calls.ts`) used by an Eleventy preprocessor and by the revision
hash, so Eleventy's output is byte-identical to `main` and no spurious revision is
recorded; `lem build` writes `dist/lemmon` and Eleventy's `clean` no longer deletes it;
`VideoPost extends Post` with `vod` required types the one video post through a `type:`
key Eleventy ignores; the three stylesheets are linked from `_includes/css/` and the
hashed `/lib` scripts are left out (`∆d1N`).

Every rendered difference on the seven posts, against the Eleventy build and compared as
rendered HTML, falls into one of fourteen named classes, with nothing unexplained — the
classifier prototype and its per-post table are the hand-off to `∆Zo7`. A read-only Opus
review read each component beside its WebC original and found the logic equal for every
input the content presents; its fixes were the revision hash, `clean`, misses that
rendered nothing where the original throws (now hard reads, so a photo without the wanted
rendition is a render error with a span), an indented lem line the preprocessor missed, a
document title written to match bytes rather than as an author would, and the card's
hero-plus-vod case. It also found four hand-off classes loose enough to hide a regression
without changing a count; those requirements went to `∆Zo7`.

What the language could not say, fed back: a datetime prints only in its written offset,
so UTC text is a parity class until something converts (D38); a number has no string
form; a record literal inside a `map` lambda is not typed by the expected element; `url`
needs `.text()` to pass as a string; there is no comment form (`∆8Kj`); a layout only a
subtype can use cannot refuse the base record (`∆qLe`). `∆tAw`'s text was amended for
what landed here. The branch commit is `d40a90c`.

## Retrospective

Starting the port before its build tool existed cost nothing and bought a day: the agent
compiled through the core until `lem build` arrived through the link, which also proved
the `file:` mechanism on the real site before anyone planned to. The review's best finding
was not in the port but around it — a hash of the raw markdown that would have recorded a
fake revision on deploy — which is the kind of consequence a builder focused on output
does not look for.

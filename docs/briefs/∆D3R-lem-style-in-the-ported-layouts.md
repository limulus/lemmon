# `lem-style` in the ported layouts

## Background

### From `∆rT5` Schemas, layouts, components

Schemas, layouts, components — `site.lem` types for articles, link posts, TILs, photos, vod,
authors; the base/page/article/video layouts and ~20 WebC components rewritten as `.lem`
functions with `lem-style` scoping (D20). First real exercise of the language at scale;
expect it to feed fixes back into Phase 1 items

## Scope and constraints

- Slice 2's acceptance, which this item serves: Eric opens the seven posts and sees them
  identical to the live site, with their CSS as per-page hashed bundles under
  `/assets/immutable/`, and `npm run parity` reports zero unexpected differences for them.
- Branch work: the layouts and components from `∆foi` gain `lem-style` scoping (D20), and
  the interim `<link>` goes.

## Handoff from ∆F4y

The shell fills `styles`, `scripts` and `head` marks with its default plugin, `bundler()`,
which writes each page's CSS as a bundle. What the branch changes to use it:

- `lemmon.config.ts` lists `plugins: [bundler()]`, imported with
  `import { bundler, type Config } from '@limulus/lemmon-shell'` — its first value import,
  resolved under the `source` condition the `lem` bin and `npm run parity` run with; the
  branch's `npm run tscc` is among this item's checks. `'_includes/css/*.css'` leaves
  `passthrough`.
- `www/_includes/layouts/base.lem`: `<lem.mark("styles")>` replaces the three interim
  `<link>`s, a literal name (no `<lem.styles()>` wrapper until ∆c4j), with
  `<lem.siteStyles()>` on the next line. A mark inside a `lem-style` body is a render
  error.
- Global stylesheets: `npm run lemmon:css` (`bin/lemmon-css.ts`) writes
  `www/_lemmon/site-styles.lem` from `www/_includes/css/*.css` — `<lem-fn siteStyles()>`
  with three `lem-style` blocks, normalize, main and hljs-github, each line indented so
  D26's trim and dedent give back the file trimmed. It refuses a stylesheet holding any
  `<`, and a spec fails `npm test` when the committed file drifts from what it derives.
  `base` runs after `pageLayout`'s blocks, so the globals collect last, as Eleventy's do.
- Placement: Eleventy collects post-order. A component's `lem-style` goes at the end of
  its body, after every call it makes; a layout's goes after its `lem-let` capture and
  before the call to the next layout. `pageLayout` holds two plain blocks there, before
  `<lem.base(…)>`: the sprite's `@supports` rules (the bundle's line 16) first, then
  `page.webc`'s own `<style>` (skip link, footer).
- On disk: `dist/lemmon/assets/immutable/bundle/<name>.css`, the name the first ten
  base64url characters of the bundle's SHA-256, one per distinct page CSS, fragments
  joined by `\n` with no trailing newline; each page's mark becomes
  `<link rel="stylesheet" href="/assets/immutable/bundle/<name>.css">`.
- Parity: a factory `cssBundle(post)` in `bin/parity/classes.ts` replaces `interim-css`,
  once per post. Its `attr` hook fires only on `href`, where both elements are a `link`
  with exactly `rel="stylesheet"` and `href`, both children of `head`, ours matching
  `^/assets/immutable/bundle/[\w-]{10}\.css$` and theirs
  `^/assets/immutable/11ty/bundle/[\w-]{10}\.css$`. It then requires: (1) both files
  exist, each named by the hash of its bytes; (2) ours ends with the three global
  stylesheets, trimmed and joined by `\n`, byte for byte; (3)
  `normalize(ours) === normalize(rename(theirs))`, `rename` turning each `SCOPED` hash
  into its function name on css-tree `ClassSelector` nodes, `normalize` lightningcss's
  `transform` (minified, `include: Features.Nesting`, given `filename` and the code as a
  `Uint8Array`, inside a `try` whose parse error refuses) then css-tree's parse and
  generate. Hooks are synchronous, so `parity.ts` reads both bundle directories into
  `Post.css`, carried by `check.ts`'s `PostInput`; the class records why it refused, for
  `parity.ts` to print the first offset where the normalized texts differ.
  `scoped-class` stays, moved under a D20 heading, until ∆oAl.
- Warnings `lem build` prints: one per code block naming a language — 8 fences on the
  penumbra and TIL pages, none in the feed posts — until the highlighter (∆Nxd); and an
  unplaced-`styles` warning on any page that calls a styled component with no layout
  placing the mark.

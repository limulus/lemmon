# Cutover

## Background

Cutover — at zero unexpected diffs, replace Eleventy with `lem` in limulus-dot-net (`build`,
`dev`, `verify` scripts, CI, `11ty/` removed), single switch, no dual-serving, CloudFront
config unchanged

Decided by Eric on 2026-10-18: the switch is prepared on a local branch he reviews, and the
live site stays on Eleventy until he merges it ("do not actually cut the live site over.
changes to limulus-dot-net need to stay in a branch for my review"). Nothing in
limulus-dot-net is pushed, since its CD workflow deploys a push to any branch. How CI gets
lemmon is `∆FnH`, decided before the merge.

## Scope and constraints

- The inventory mapped on 2026-10-18. Deleted: `eleventy.config.*`, `types/eleventy.d.ts`,
  the svg-sprite patch (and then `patch-package`), `11ty/dprint.ts`, `11ty/esbuild.ts`,
  `dprint.json`, the 35 `.webc` files, `sitemap.njk`, `dynamic-content-fallback.liquid`,
  `_data/eleventyComputed.js`, `links/links.json`, Eleventy, its plugins, markdown-it and
  its plugins, highlight.js, reading-time and globby, and — after the last green run —
  `bin/parity*` with its devDependencies. Replaced: the revision updater (`∆shS`).
  Rewritten: `package.json`'s scripts (`build` is `lem build`, `dev` today's
  `lemmon:dev`, `wwwc` `lem check`; `lint`, `tsconfig` and vitest drop `11ty` and
  `types`), `lemmon.config.ts`'s output to `dist/www` so `clean` and `cd.yaml`'s upload
  path stay, `cd.yaml`'s build job alone, and the docs (CLAUDE.md, README,
  `.claude/commands/link.md`, `.vscode/extensions.json`). Kept: `site.json`,
  `authors.json`, `revisions.json`, `penumbra.json`, the `_lemmon/` style copies and their
  generator. Untouched: `infra/**`, `cdk.json` and the stack snapshot. CSS bundles move
  from `/assets/immutable/11ty/bundle/` to `/assets/immutable/bundle/`, the same CloudFront
  behaviour; every other file keeps its name and bytes.
- Accepted without deploying: parity's last green run over the whole output on the
  commit before the switch, with Eleventy built under `TZ=UTC`; on the switch commit, a
  fresh clone's `npm ci`, `npm run verify` and `TZ=UTC npm run build` with no `.env` but
  the token in the environment; the new output byte-compared with that last Lemmon output,
  every difference traced to a named change; two builds, from two paths, identical;
  `git diff public/main -- infra cdk.json` empty; and the `s3 sync` plan computed locally
  (new, changed, unchanged, left stale). `aws s3 sync --dryrun` and `cdk diff` need
  credentials, so they are Eric's.

- What readers will see change, from the parity classes that last until the cutover;
  Eric accepts the list then, and any class still firing that is not on it:
  - Photo spacing at phone width: markdown-it leaves an empty paragraph inside each
    in-body photo's figure, about 10 px a photo below 600 px wide (`md-photo-wrap`).
  - Revision-history times and date tooltips (`time[title]`) show the offset a date was
    written in, not UTC (D38; `utc-revision-display`, `datetime-text`).
  - Reading time reads a minute lower on a page near a rounding boundary, in its header
    and in its card on the lists (the-rebound's on `/feed/`): Lemmon counts what a reader
    sees, not the markdown source (D38, accepted by Eric 2026-10-17; `reading-time`).
  - Photo alt text WebC cut short at a quote comes out whole (`attr-escaping`), on
    `/photos/`, in `/feed/`'s cards, and on the two photos' own pages too.
  - The Source link in the header of a page template (the index pages, and the photo and
    TIL topic pages) names its `.lem` file where it named the `.webc` (`source-pill`).
  - Heading anchors with punctuation in their text are spelled as comrak spells them: seven
    anchors on four pages are renamed, so a link to the old one lands at the top of the
    page.
  - The sitemap lists `/tils/nodejs/`, which Eleventy left out, no longer lists the three
    feeds, and gives an undated page no `lastmod` where Eleventy gave its checkout time
    (Eric, 2026-10-18; `∆zu0`).
  - A feed's dates are spelled in the offset each was written in (D38), where Eleventy's
    RSS spelled the build machine's zone; every instant is the same.
  - Footnotes are comrak's, not markdown-it-footnote's (`footnote-markup`, which
    `parity:look` neutralizes): the reference reads `1` where it read `[1]`, the back
    link `↩` where it read `↩︎`, and the section loses the `section-separator` class, so
    the centred rule `main.css` draws above it goes unless the site's CSS gives
    `.footnotes` its own (decided in `∆ITg`, slice 4, rather than reproducing
    markdown-it's markup in the core). The footnote anchors are renamed on six pages
    (`#fn-out-of-scope` for `#fn2`), so a link to one from another page must follow.
- The sprite (`∆IvY`, the port's `lemmon/svg-sprite.ts`) strips the `<style>` inside the
  `svg-ldn` symbol because WebC hoists it into the CSS bundle, and `page.lem` carries a
  hand copy of those rules naming ids like `#elogo-p3`. svg-sprite prefixes the ids inside
  each file by the order files are added, so the letter is the number of icons sorting
  after `ldn.svg`: adding or removing an icon renames them, which the plugin's spec and
  parity's `css-bundle` catch and `lem build` does not. Once no Eleventy output has to be
  matched, keep the `<style>` in the sprite (it applies to the whole document) and delete
  `page.lem`'s copy.
- Two hazards the esbuild plugin (`∆d1N`) first kept because Eleventy had them are fixed
  on limulus-dot-net's main (`d1dc734`, 2026-10-18) and followed in the plugin (`∆EEa`),
  so the cutover inherits neither: `XXXXXXXX` was rewritten after hashing, so an edit to a
  client worker alone shipped its loader under the loader's old hashed name, which
  CloudFront serves as immutable for a year; and the wasm worker's source map named its
  module by absolute path, so its hash depended on where the site was checked out. Each
  bundle is now named by what it ships as, the names it holds included, and its map names
  that module relative to itself; the port compares the new names byte for byte.
- The revision hash translates a post's lem calls back to the WebC they replaced (`∆shS`),
  and no spec exercises the ten real calls in six posts, since every recorded commit
  predates the port: a regression in that translation after the switch shows first as six
  spurious revisions in a deploy's history commit. Check `npm run update-revisions`
  records nothing on the switch commit.
- Two photo-tooling tasks, from `∆pP5` (closed 2026-10-18): make `@sindresorhus/slugify`
  a direct dependency, since `bin/process-photos.js` imports it and it arrives today only
  through `@11ty/eleventy`; and have `process-photos.js` write the alias-keyed records the
  port reads, EXIF numbers included (`∆hvm`), once Eleventy, the one reader of the id keys
  in `www/_data/photos.json`, is gone, retiring `bin/lemmon-photos.ts` and its drift spec.
- limulus.net's blur-hash script (`www/lib/blur-hash/index.ts`) has two races, live today:
  an element the parser creates after the async `/lib/common` script has defined
  `blur-hash` is connected before its `<img>` exists and throws, and a placeholder whose
  worker answers after its photo loaded is painted back behind the photo. The port's
  `npm run parity:look` orders requests around both (its `Gate`, `∆d1N`); fixing the
  script lets that ordering go.

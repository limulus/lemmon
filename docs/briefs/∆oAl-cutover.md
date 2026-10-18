# Cutover

## Background

Cutover — at zero unexpected diffs, replace Eleventy with `lem` in limulus-dot-net (`build`,
`dev`, `verify` scripts, CI, `11ty/` removed), single switch, no dual-serving, CloudFront
config unchanged

## Scope and constraints

- What readers will see change, from the parity classes that last until the cutover;
  Eric accepts the list then, and any class still firing that is not on it:
  - Photo spacing at phone width: markdown-it leaves an empty paragraph inside each
    in-body photo's figure, about 10 px a photo below 600 px wide (`md-photo-wrap`).
  - Revision-history times and date tooltips (`time[title]`) show the offset a date was
    written in, not UTC (D38; `utc-revision-display`, `datetime-text`).
  - Reading time reads a minute lower on a page near a rounding boundary: Lemmon counts
    what a reader sees, not the markdown source (D38, accepted by Eric 2026-10-17;
    `reading-time`).
  - Photo alt text WebC cut short at a quote comes out whole (`attr-escaping`).
  - Footnotes are comrak's, not markdown-it-footnote's (`footnote-markup`, which
    `parity:look` neutralizes): the reference reads `1` where it read `[1]`, the back
    link `↩` where it read `↩︎`, and the section loses the `section-separator` class, so
    the centred rule `main.css` draws above it goes unless the site's CSS gives
    `.footnotes` its own (decided in `∆ITg`, slice 4, rather than reproducing
    markdown-it's markup in the core).
- The sprite (`∆IvY`, the port's `lemmon/svg-sprite.ts`) strips the `<style>` inside the
  `svg-ldn` symbol because WebC hoists it into the CSS bundle, and `page.lem` carries a
  hand copy of those rules naming ids like `#elogo-p3`. svg-sprite prefixes the ids inside
  each file by the order files are added, so the letter is the number of icons sorting
  after `ldn.svg`: adding or removing an icon renames them, which the plugin's spec and
  parity's `css-bundle` catch and `lem build` does not. Once no Eleventy output has to be
  matched, keep the `<style>` in the sprite (it applies to the whole document) and delete
  `page.lem`'s copy.
- Two hazards the esbuild plugin (`∆d1N`) keeps because Eleventy has them, for parity:
  `XXXXXXXX` is rewritten after hashing, so an edit to a client worker alone ships its
  loader under the loader's old hashed name, which CloudFront serves as immutable for a
  year — a browser that cached the loader keeps asking for the old worker; and the wasm
  worker's source map names its module by absolute path (`wasm-module:/…/node_modules/…`),
  so its hash depends on where the site is checked out (stable in CI, not across
  machines). Once parity no longer binds, rewrite before hashing and strip the path. On
  2026-10-18 Eric started fixing the first on limulus-dot-net's main instead, with lemmon's
  plugin following in `∆EEa`.
- limulus.net's blur-hash script (`www/lib/blur-hash/index.ts`) has two races, live today:
  an element the parser creates after the async `/lib/common` script has defined
  `blur-hash` is connected before its `<img>` exists and throws, and a placeholder whose
  worker answers after its photo loaded is painted back behind the photo. The port's
  `npm run parity:look` orders requests around both (its `Gate`, `∆d1N`); fixing the
  script lets that ordering go.

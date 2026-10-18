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
- The sprite (`∆IvY`, the port's `lemmon/svg-sprite.ts`) strips the `<style>` inside the
  `svg-ldn` symbol because WebC hoists it into the CSS bundle, and `page.lem` carries a
  hand copy of those rules naming ids like `#elogo-p3`. svg-sprite prefixes the ids inside
  each file by the order files are added, so the letter is the number of icons sorting
  after `ldn.svg`: adding or removing an icon renames them, which the plugin's spec and
  parity's `css-bundle` catch and `lem build` does not. Once no Eleventy output has to be
  matched, keep the `<style>` in the sprite (it applies to the whole document) and delete
  `page.lem`'s copy.

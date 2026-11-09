# Retire limulus.net's stylesheet copies

## Background

Found reviewing the cutover branch on 2026-11-08; Eric asked that it be marked for cleanup
after the cutover.

limulus-dot-net's `bin/lemmon-css.ts` (`npm run css`) writes `www/_lemmon/site-styles.lem`
and `www/_lemmon/mapbox-gl.lem`, each stylesheet's text as a `lem-style` block inside a
function, which `base.lem` and `map-box.lem` call where the bundle's order needs it. Under
Eleventy, `base.webc` linked `normalize.css`, `main.css` and `hljs-github.css`, which WebC
bundled after every component's styles, and `map-box.webc` linked mapbox-gl's stylesheet,
bundled before map-box's own. `bundler()` builds a page's CSS bundle only from what its
`lem-style` blocks collect, and a template cannot read a CSS file: a block holds what is
written inside it, and a data source is YAML or JSON. So the port carries the copies.

What they cost on 2026-11-08: about 7 KB of script and spec, and 55 KB of generated copies,
36 KB of them mapbox-gl's minified CSS. A spec fails `npm test` when a copy drifts from its
source, `npm run dev` reruns the script on every CSS change, and the script refuses a
stylesheet holding a `<`, since a `</lem-` inside a block would close it. Only parity
needed the copies.

## Implementation suggestions

- `bundler()` takes stylesheet files to append to the bundle, in an order it states. The
  site's output stays as it is; where a file sits among the collected blocks is the design
  question, since mapbox-gl's goes before map-box's own block and the global ones after
  every other.
- Or the site links the stylesheets as files and drops the script, the spec and the
  watcher, changing its output: three or four more stylesheet requests a page, or one if
  they are joined.

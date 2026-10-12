# Photos plugin

## Background

Photos plugin — `photos.json` as typed data with alias resolution (today's `getPhoto`
shortcode), and `lem photos add` wrapping `bin/process-photos.js` (resize, blurhash,
upload). A photo's URL is its stored `alias`, never `title.slugify()`: the aliases were
minted by `@sindresorhus/slugify`, whose German rule spells `Dürer` `duerer` where the
core's Latin fold spells it `durer`, so three live URLs move if a page recomputes one

---
id: ∆pP5
completed: 2026-10-18 12:48:52 -07:00
---

## Backlog item

> - `∆pP5` Photos plugin
>   - `photos.json` as typed data with alias resolution, and `lem photos add` wrapping
>     `bin/process-photos.js`. A photo's URL is its stored `alias`, never `title.slugify()`
>   - needs: `∆wOn`

## Planning

The item's brief, folded here. Photos plugin: `photos.json` as typed data with alias
resolution (today's `getPhoto` shortcode), and `lem photos add` wrapping
`bin/process-photos.js` (resize, blurhash, upload). A photo's URL is its stored `alias`,
never `title.slugify()`: the aliases were minted by `@sindresorhus/slugify`, whose German
rule spells `Dürer` `duerer` where the core's Latin fold spells it `durer`, so three live
URLs move if a page recomputes one.

## Refinement

Closed on 2026-10-18 with nothing built, as slice 5 started. Mapping the slice against the
port branch found both halves already served or without a consumer:

- Alias resolution landed with the port's first layouts (`∆foi`): `bin/lemmon-photos.ts`
  derives `www/_lemmon/photos.json`, keyed by alias, from Eleventy's mixed alias-and-id
  `_data/photos.json`; `site.lem` binds it as `photos: {string: Photo}`, front matter
  resolves an alias through `by site.photos`, and `npm run photos` reruns the derivation,
  with a spec failing the branch's tests when the two files drift. Slice 4 was accepted on
  it. A data hook would only replace that script.
- `npm run photos` runs `bin/process-photos.js`, which imports nothing from Eleventy, so a
  `lem photos add` adds a name rather than a capability, at the cost of a CLI that loads
  the config before it dispatches.

Eric chose on 2026-10-18 to park what has no consumer: `∆wOn` (the data and commands hooks
and plugin `.lem`) moved to Someday/Maybe, with D23's qualified names recorded as a
deferred question, and `lem photos add` became `∆x1B` there, needing `∆wOn`. The stored
alias as a photo page's URL is one line of the photo page type, in `∆hvm`, which no longer
needs this item. He also chose that the photo pages' EXIF numbers (focal length, f-number,
a composite exposure) come from the data, written by the derivation script with the same
`parseFloat` Eleventy uses, rather than from a string-to-number builtin. Two tasks went to
the cutover brief (`∆oAl`): `@sindresorhus/slugify` reaches `process-photos.js` only
through `@11ty/eleventy` today, and `process-photos.js` can write the alias-keyed shape
once Eleventy, the only reader of the id keys, is gone.

## Retrospective

The item was cut from the architecture's plugin sketch before the port existed, and the
port then solved its first half with a forty-line script nobody counted against it. An
item whose consumer is a site file should be checked against that file at planning, as the
brief for `∆wOn` asked.

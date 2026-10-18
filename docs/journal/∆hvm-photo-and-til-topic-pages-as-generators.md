---
id: ∆hvm
completed: 2026-10-18 15:13:52 -07:00
---

## Backlog item

> - `∆hvm` Photo and TIL topic pages as generators
>   - `photos/photo` and `tils/topics`, the two paginated pages, as generator templates whose
>     `for:` key names a list (D43), with `prev` and `next` filled by the loader
>   - needs: `∆99b`, `∆tAw`

## Planning

The item's brief, folded here. From `∆ePP` Page templates, generated and non-HTML pages:
the photo and TIL topic pages as generators (D43), the only two pages of the site that
paginate; `∆99b` built generators for them. Slice 5's acceptance, which it serves: Eric
opens `dist/` from `/` and reaches every page of the site, each comparing clean. The
photo pages' `site.lem` types were deferred here from `∆tAw`. A photo page's URL is its
stored `alias`, never `title.slugify()` (from `∆pP5`): `@sindresorhus/slugify` spells
`Dürer` `duerer` where the core's Latin fold spells it `durer`.

Decided by Eric on 2026-10-18: the three numbers `photo-meta.webc` parses out of EXIF
strings with `parseFloat` (focal length `"6.765 mm"`, f-number `"f/1.78"`, the second
time of a composite exposure `"0; 33298/9605; …"`) come from the data, written by the
port's `bin/lemmon-photos.ts` with the same `parseFloat`, rather than from a core builtin;
`process-photos.js` writes them itself at the cutover. Mapping the slice found the
hazards the build met: WebC scopes `photo.webc`'s style to all 34 elements the template
writes, the photos sprite's symbols among them; the sprite sits inside `main`;
`mapbox-gl.css` (55 KB) joins the page's style bundle; and the date is ICU text.

## Refinement

Built in two phases on the port branch (limulus-dot-net `856f498`, local), the first
while `∆99b` was still being built, since a page with `for:` broke the port's build until
it landed: the EXIF numbers, `photoMeta` and `mapBox`, a generated `mapbox-gl.lem` with a
drift spec beside `site-styles.lem`'s, the photos sprite as a second `svgSprite` entry,
and the page types; then `www/photos/photo.lem` (`for: photo: site.gallery`, `PhotoPage`
declaring `prev` and `next` as `PhotoPage?`) and `www/tils/topics.lem`
(`for: topic: site.tilTopics`), unchanged from the drafts phase one had checked against
21 stand-in pages. `npm run parity` compares all 55 pages clean and their 37 script files
byte for byte, `npm run parity:look` shows all 165 screenshots identical, and a `lem dev`
full rebuild of the 55 pages takes a median of 421 ms, up from 373 ms at 34.

Phase one found that the core read four of the 59 fractional numbers in the photo data a
unit in the last place off, three of them printed map coordinates: serde_json's default
reader is inexact. It was fixed in lemmon as its own commit (`2ff19a6`, 41 KB of the
`.wasm`), and the same fixer found that a float prints with Rust's notation at the ends
of the range (`inf`, `-0`, `1e21` in full), recorded as `∆qoe`. A render failing on a
generated page now names its item: removing a lens from photo-meta's table gave one
`no key` error per affected photo, each labelled "rendering `photos/photo.lem` for
`site.gallery[8]`".

Parity: `scoped-class` covers `photoMeta`, `mapBox` and the photo page, which Lemmon
scopes by a `photoPage` class on its `article`; a new `page-scoped-sprite` forgives WebC's
page hash on the 24 elements of the photos sprite, only beside that page's `article`;
`webc-attr-leak` covers the pill buttons' `title` and `rel`. `parity:look` answers the
hashed map bundle with an empty module on both sides, since mapbox-gl draws nothing
against the stand-in host; the bundle's bytes are still compared. With no page left
pending, the pending list `∆LsH` added was removed.

The review recorded, unchanged, what holds only on today's data, each of which parity
would report rather than hide: `.fixed()` rounds an exact binary tie to even where
`toFixed` rounds it away from zero (a documented choice of the core's,
`docs/language.md`; no exposure or f-number in the data is a tie); the stored alias
equals `slugify(title)` for all 19 photos; the chip's `title` reprints a date through
`rfc3339()`, which writes `+00:00` as `Z`; `sortBy(…).reverse()` would swap two photos
taken in the same second; and the date's " at " and ASCII space before "PM" match the
ICU of the Node that built Eleventy's output. One behaviour is new: a photo missing a
number drops that chip, where WebC printed `NaN` or `undefined`. A template still cannot
fail a render with its own message, so photo-meta fails on an unknown lens or camera only
through its table lookups (`∆qLe`).

## Retrospective

Splitting the item into a phase that needed nothing unbuilt and a phase that needed the
generators let the port work run beside the core work, and the stand-in pages phase one
built found the float misreading before generators existed, which kept it out of
`∆99b`'s diff. The `.fixed()` tie was found by a reviewer computing rounding by hand
rather than by any data, which is the only way a rule that holds on today's data shows
its edge.

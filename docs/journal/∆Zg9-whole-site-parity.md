---
id: ∆Zg9
completed: 2026-10-19 10:08:47 -07:00
---

## Backlog item

> - `∆Zg9` Whole-site parity
>   - `npm run parity` over the whole `dist/` against the 11ty build, every difference in a
>     forgiveness class with its count, at zero unexpected differences; the acceptance bar

## Planning

The item's brief, folded here. From `∆hyz` Parity harness: build both dists, normalize,
diff with an allowlist of expected differences, report per page, as a local `npm run
parity` in limulus-dot-net that dies with Eleventy; the acceptance bar for the whole
initiative, landing before the port is done. Slice 6's acceptance, which it serves: Eric
sees `npm run parity` report zero unexpected differences over the whole `dist/`, and a
local cutover branch builds limulus.net with `lem`, ready for his review.

Mapping slice 6 on 2026-10-18 found parity compared the pages, the CSS bundles they link
and esbuild's scripts, and nothing else: the 34 passthrough files had never been compared.
The XML leg came with `∆zu0`. This item was to put every file either output holds into a
category and fail on any it could not place, and to consider building the Eleventy
reference with `TZ=UTC`, as CD does.

## Refinement

Built on the port branch (limulus-dot-net `9be74d7`, local). `bin/parity/inventory.ts`
places every file each build wrote in exactly one category by the same predicate its
comparison reads by — page, XML, bundle, script, passthrough — and a file in no category,
or in two, fails by name; a CSS bundle no page links fails; passthrough files (a file at
the same path under `www/`) are compared byte for byte, and all 34 were identical; Eleventy's
unhashed stubs are set aside by content and pinned to one per entry bundle it wrote. The
PASS line names the counts: 140 files in Lemmon's output and 158 in Eleventy's. Building
Eleventy under `TZ=UTC` respelled the RSS `pubDate`s from `MST` to `GMT` and moved nothing
else, and Lemmon's output is time-zone independent (measured), so parity now builds the
reference under UTC.

The review found one real escape: a page or XML file read by path on APFS, which ignores
case, passed when the two builds named it in different case (`extra.xml` and `Extra.xml`
both "ok"), though S3 would 404 one of them. Presence now comes from each side's own
listing, Unicode-form variants included. It also pinned the "several categories" rule and
the stub count, which no spec had held.

## Retrospective

A comparison that opens a file by the other side's name inherits the file system's idea of
equality; on a case-insensitive Mac that is not S3's. Every presence check should read the
side's own listing.

# Feed and sitemap dates in UTC

## Value

limulus.net's feeds and sitemap keep spelling their dates in UTC after the cutover, as they
did under Eleventy, which Eric judges the better choice for feeds (2026-11-01).

## Background

D38 has a `datetime` print in the offset it was written in, and every `datetime` method —
`rfc3339()`, `rfc822()`, `format()` — reads it through that offset; with no datetime
arithmetic, a template cannot print a date in UTC at all. So the port's RSS `pubDate`, Atom
`<updated>` and sitemap `<lastmod>` spell each date in the offset its post was written in:
`Sat, 4 Jul 2026 09:40:00 -0700` where Eleventy wrote `Sat, 04 Jul 2026 16:40:00 GMT`, and
`2024-06-04T13:20:00-07:00` where it wrote `2024-06-04T20:20:00Z`.

Nothing a feed reader acts on changes either way. Compared on 2026-11-01 across the kept
parity builds (`lemmon-port/dist/parity-final/`): every RSS `<guid>` and Atom `<id>` is the
same in the same order, every link is the same, and all 34 entry dates and both feed-level
Atom `<updated>`s are the same instant, only spelled differently. Eric first worried that
readers would take the respelled entries as new; they key on the ids, so the reason for the
work is the preference above, not breakage.

## Scope and constraints

- Every date in the RSS feed, the two Atom feeds and the sitemap is printed in UTC.
- An RSS `pubDate` keeps `rfc822()`'s form: `Sat, 4 Jul 2026 16:40:00 +0000`, not
  Eleventy's padded day and `GMT` (Eric, 2026-11-01). `rfc822()` itself is unchanged, so
  the pages' date tooltips that use it keep their written offset.
- It is done before the cutover, as part of slice 6, and lands on the `lemmon-cutover`
  branch (Eric, 2026-11-01).

## Implementation suggestions

- A `utc()` method on `datetime` giving the same moment in offset zero: it needs no
  time-zone database, so the core stays pure (invariant 1). Then each date in
  `www/feed.xml.lem`, `www/_includes/components/atom-feed.lem` and `www/sitemap.xml.lem`
  gains `.utc()` — five places on the cutover branch.
- Parity's XML leg (`bin/parity/dates.ts` on `lemmon-port`) compares feed and sitemap dates
  as instants, so the change should leave a parity run's verdict as it is.

---
id: ∆QZz
completed: 2026-11-01 14:28:10 -07:00
---

## Backlog item

> - `∆QZz` Feed and sitemap dates in UTC
>   - The feeds and the sitemap print every date in UTC, as Eleventy's did, where they now
>     print the offset it was written in; Lemmon has no way to print a datetime in UTC yet

## Planning

The item's brief, folded here. D38 has a `datetime` print in the offset it was written in,
and every method read it through that offset; with no datetime arithmetic, a template could
not print a date in UTC at all. So the port's RSS `pubDate`, Atom `<updated>` and sitemap
dates spelled each date in its post's offset (`Sat, 4 Jul 2026 09:40:00 -0700` where
Eleventy wrote `Sat, 04 Jul 2026 16:40:00 GMT`). Eric first worried that readers would take
the respelled entries as new; compared across the kept parity builds, every RSS `<guid>` and
Atom `<id>` was the same in the same order, every link the same, and all 34 entry dates and
both feed-level `<updated>`s the same instant. Readers key on the ids, so the reason for the
work was Eric's judgement that UTC is the better choice for feeds, not breakage.

Eric decided on 2026-11-01: every date in the RSS feed, the two Atom feeds and the sitemap
prints in UTC; an RSS `pubDate` keeps `rfc822()`'s form, `Sat, 4 Jul 2026 16:40:00 +0000`,
rather than Eleventy's padded day and `GMT`, so `rfc822()` and the pages' tooltips that use
it are unchanged; and it is done before the cutover, in slice 6, on `lemmon-cutover`.

The plan was a `utc()` method on `datetime`, the same instant at offset zero, and `.utc()`
in the site's templates. Its review found a sixth date the plan's five missed — the
sitemap's `<video:publication_date>` — and that the sitemap's text could not all match
Eleventy's: Eleventy printed `revisions.json`'s strings verbatim, and several carry
`-07:00`. It also found the docs the plan missed: the porting guide's UTC bullet, the type's
definition in `language.md`, and `datetime.rs`'s own docs.

## Refinement

Built on 2026-11-01:

- lemmon `42dc7a1`: `Datetime::utc`, the `utc` builtin through `yields_receiver`, their
  tests, corpus lines printing a feed's dates through it (and a typo'd `toUTC()` offered
  `.utc()`), and D38's record, row and history, `language.md`, `architecture.md`, the
  authoring skill, the porting guide and the artifact ledger. No new decision number: it
  revises D38, whose reasoning holds — offset zero is a fixed offset, the instant does not
  move, so no time-zone database and no arithmetic.
- lemmon `dd004c9`, from the diff's review: `datetime.rs` still said a value prints in the
  offset it was written in in seven places, false once `utc()` has rewritten it; D38 had
  said a feed's reader "reads UTC", which the brief's own comparison contradicts; an inert
  assertion and a redundant test went, and the corpus gained `(revised ?? published).utc()`,
  the shape the site's templates use. Making `utc()` an identity fails all four tests.
- limulus-dot-net `ba9b40e` on `lemmon-cutover`: `.utc()` on all six dates, and the site's
  `CLAUDE.md` and README. Against the build before it, only the four XML files differ, and
  every instant is unchanged. Atom's dates now read as Eleventy wrote them; RSS writes
  `+0000` and an unpadded day for `GMT`; the sitemap's 30 dates are UTC, 22 of them as
  Eleventy wrote them, seven revision-dated `lastmod`s respelled from `-07:00`, and one
  `.770Z` printed `.77Z`. Parity's XML leg compares these dates as instants, so its verdict
  is unchanged.

lemmon `npm run verify` passed (857 tests, the Rust leg) and so did the site's.

## Retrospective

The question Eric asked — would readers see new entries? — was answered by comparing the
two builds' feeds before anything was planned, which turned a worry into a preference and
kept the scope to what he wanted. The plan's review earned its cost twice over: a date the
plan missed, and a claim of text equality with Eleventy that the data did not support.

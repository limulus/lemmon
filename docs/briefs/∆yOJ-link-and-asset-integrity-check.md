# Link and asset integrity check

## Background

### From `∆7Xs` Output writer

Output writer — HTML to disk with marks resolved, the `styles` producer writing per-page
hashed CSS bundles of what `lem-style` collects under `/assets/immutable/…` (today ten
characters of a sha256; `name.<8hex>.ext` is esbuild's and passthrough's convention,
D13/D20), passthrough copy by glob (`assets/**` plus png/svg/jpg/xsd/xsl/ txt/pdf), and a
link/asset integrity check, CloudFront untouched

## Scope and constraints

- Moved from slice 4 to slice 6 on 2026-10-17 (`∆c4j`'s journal entry): in slice 4 the
  port's output links to some twenty pages slices 5 and 6 write (`/`, `/feed/`, `/eric/`,
  `/photos/…`, `/feed.xml`, `/penumbra/feed.xml`, the TIL topic pages), so the check could
  pass only with an exception list. After the feeds every page exists.
- Architecture step 9 puts the check after the write, against the shell's rule that every
  check runs before anything is written, and `lem check` reporting what a build would.
- The integrity check from the original output writer item; HTML and passthrough are `∆muD`,
  the `styles` producer is `∆F4y`.
- A link must match a written file's path exactly, case and Unicode form included. S3
  and CloudFront match names exactly, but `lem dev` (`∆Vhi`) serves from a file system
  that may not: on macOS it answers `/Feed/` and an NFD `café` with the page at `/feed/`
  and the NFC `café`, so a mis-cased link works in the dev server and 404s in production.
  This check is where that is caught, not the dev server.

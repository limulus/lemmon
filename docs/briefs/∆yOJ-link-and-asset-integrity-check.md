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
- Decided on 2026-10-18: the check runs before anything is written, at the end of
  `renderSite()`, where every page's resolved bytes, the asset and produced files, the
  passthrough list and the manifest are in memory, so `lem build` writes nothing on a broken
  link and `lem check` and `lem dev` report it; architecture step 9 moves. It checks
  fragments too, since a renamed anchor is a broken link a reader meets.
- Mapped the same day over the port's output: of 4,025 references, 115 were broken, across
  four targets. The feeds (`∆zu0`) clear 112. `https://limulus.net/@eric` is a 302 in the
  CloudFront function, not a file, so the check needs the site's own origin and the paths
  its host answers without a file. `/links/2026/03/01/surviving-ai-revolution-software-development/`
  writes `[wonderful essay](link)` where it meant a reference, a live bug in both builds.
  And with fragments checked, make-ai-boring-again's `/feed/better-software-with-ai/#fn2`
  breaks on Lemmon only: comrak names a footnote by its label (`#fn-out-of-scope`).
- Not links: a `url(%23clip)` inside an SVG `data:` URI in a CSS bundle, and a commented
  `xml-stylesheet` in the passthrough `feed.xsl`; passthrough files are link targets, not
  scanned. `asset()` URLs that appear only inside an inline `import()` are covered by
  checking that each manifest value names a file the build claims.
- A diagnostic's span: output bytes have no source span, so the nearest is the page's url
  key, a generator's `for:` entry, or a producer's mark (invariant 8).
- Placeholders: `docs/architecture.md` names this item and `∆fwn` on one line the gate
  reads, so split it; `packages/shell/README.md` needs a hand sweep.

---
id: ∆yOJ
completed: 2026-10-19 09:20:33 -07:00
---

## Backlog item

> - `∆yOJ` Link and asset integrity check
>   - Every link and `asset()` URL a build writes resolves to a file it writes, fragments
>     included, checked before anything is written so `lem check` reports it too
>   - needs: `∆zu0`

## Planning

The item's brief, folded here. From `∆7Xs` Output writer: a link and asset integrity check,
CloudFront untouched. Moved from slice 4 to slice 6 on 2026-10-17, since in slice 4 the
port's pages linked some twenty pages later slices wrote. Architecture step 9 had put the
check after the write, against the shell's rule that every check runs first and `lem check`
reports what a build would; a link must match a written file exactly, case and Unicode form
included, since S3 and CloudFront do while macOS's file system under `lem dev` does not.

Mapping slice 6 on 2026-10-18 scanned the port's output: 4,025 references, 115 broken
across four targets, the feeds `∆zu0` was to write clearing most. It decided the check
runs before the write and checks fragments too. A planner then designed it and prototyped it
over the port's output with the feeds in place: 4,107 references, 1,013 fragments, five
broken. Two were the CloudFront function's work rather than files: `/@eric`, a redirect,
and the Atom feeds' alternate links `/penumbra` and `/tils`, which only its dot-less
rewrite serves. Two were content: `[wonderful essay](link)`, a live 404 on limulus.net, and
make-ai-boring-again's `#fn2`, which only Lemmon breaks, since comrak names a footnote by
its label. Eric asked for the essay link fixed on limulus-dot-net's main, live (done in this
session, `13d4599`, deployed and confirmed), and chose to edit the footnote link on the port
branch. The main session chose to fix the feeds' links in the port's templates rather than
encode CloudFront's rewrite, as Eric had chosen for the sitemap's quirks, so the config's new
`served` lists `/@eric` alone.

## Refinement

The lemmon half landed as `2d50e29` (D47): at the end of `renderSite`, before anything is
written, every page, stylesheet and XML file the build holds is read for its links — HTML
through the posthtml-urls table D46's rebase uses (moved to `links.ts`) plus a refreshing
`meta`, SVG links, Open Graph urls and inline CSS; CSS through `@csstools/css-tokenizer`;
XML through `@xmldom/xmldom` 0.9.12 — and each link to the origin (either scheme) must name
a file the build writes, its fragment an id on that page; each manifest value must name a
file. A broken link is an error at the writer of the page holding it, once per page per
resolved url, so `lem build` writes nothing; a `served` entry naming a written file is a
warning. It adds about 65 ms to the port's build. The review found a broken manifest value
reported again on every page that loaded it, and the HTML and XML walks (and D46's rebase)
overflowing the stack on deep nesting with no span; both were fixed, the walks made
iterative, and a few cheap gaps closed (SVG `image`, `feImage` and `xlink:href`, RSS image
urls, the origin over `http:`, `ping` split at whitespace, XML in another declared
encoding refused).

The port half landed as limulus-dot-net `9f0a630` (local): `served: ['/@eric']`, the Atom
feeds' alternate links ending in `/` (their `<id>`s unchanged, parity's
`feed-alternate-slash` counting one in each), the footnote link edited, and the port's
`@xmldom/xmldom` raised from 0.9.5, which carries advisories, to 0.9.12. `lem check` is
clean, parity compares 55 pages and 4 XML files clean, and a copy with `#fn2` put back
writes nothing. The port first merged limulus-dot-net's main, the essay fix and the revision
CD recorded for it, and parity's counts for that post moved by one each.

## Retrospective

Checking links before the write turned the link check from a report into a gate, which is
what found a 404 live on limulus.net since March; it also means a site with a stale link
cannot build until it is fixed, which `served` and a content edit had to answer for the
port. The prototype the planner ran over real output was worth more than the design text:
it found the two feed links the earlier survey's "no link relies on CloudFront adding
index.html" had missed.

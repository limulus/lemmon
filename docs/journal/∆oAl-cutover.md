---
id: ∆oAl
completed: 2026-10-19 10:54:26 -07:00
---

## Backlog item

> - `∆oAl` Cutover
>   - On a local branch Eric reviews, replace Eleventy with `lem` in limulus-dot-net, single
>     switch, no dual-serving, CloudFront config unchanged; nothing deploys

## Planning

The item's brief, folded here. Cutover: at zero unexpected differences, replace Eleventy
with `lem` in limulus-dot-net (`build`, `dev`, `verify` scripts, CI, `11ty/` removed),
single switch, no dual-serving, CloudFront config unchanged. Eric decided on 2026-10-18
that the switch is prepared on a local branch he reviews, and the live site stays on
Eleventy until he merges it ("do not actually cut the live site over. changes to
limulus-dot-net need to stay in a branch for my review"); nothing in limulus-dot-net is
pushed, since its CD workflow deploys a push to any branch or tag. How CI gets lemmon is
`∆FnH`, slice 7, decided before the merge.

The list of what readers will see change, which Eric accepts at the switch; every parity
class still firing at the last green run maps to an entry on it:

- What readers will see change, from the parity classes that last until the cutover;
  Eric accepts the list then, and any class still firing that is not on it:
  - Photo spacing at phone width: markdown-it leaves an empty paragraph inside each
    in-body photo's figure, about 10 px a photo below 600 px wide (`md-photo-wrap`).
  - Revision-history times and date tooltips (`time[title]`) show the offset a date was
    written in, not UTC (D38; `utc-revision-display`, `datetime-text`).
  - Reading time reads a minute lower on a page near a rounding boundary, in its header
    and in its card on the lists (the-rebound's on `/feed/`): Lemmon counts what a reader
    sees, not the markdown source (D38, accepted by Eric 2026-10-17; `reading-time`).
  - Photo alt text WebC cut short at a quote comes out whole (`attr-escaping`), on
    `/photos/`, in `/feed/`'s cards, and on the two photos' own pages too.
  - The Source link in the header of a page template (the index pages, and the photo and
    TIL topic pages) names its `.lem` file where it named the `.webc` (`source-pill`).
  - Heading anchors with punctuation in their text are spelled as comrak spells them: seven
    anchors on four pages are renamed, so a link to the old one lands at the top of the
    page; and headings no longer carry `tabindex="-1"`.
  - The six in-body photos on two posts lose the `id` WebC left on them, their component's
    `id` input leaking onto the host element; nothing links those anchors (Eric,
    2026-10-19), and each photo's own page stays its link.
  - The sitemap lists `/tils/nodejs/`, which Eleventy left out, no longer lists the three
    feeds, and gives an undated page no `lastmod` where Eleventy gave its checkout time
    (Eric, 2026-10-18; `∆zu0`).
  - A feed's dates are spelled in the offset each was written in (D38), where Eleventy's
    RSS spelled the build machine's zone and its Atom and sitemap wrote UTC (`Z`); every
    instant is the same. RSS's `pubDate` has an unpadded day and a numeric zone, and a
    revision time written `.770Z` prints `.77Z`.
  - Feed entries carry the same markup differences the pages do (comrak footnotes and
    heading ids, no WebC hashes or leaked attributes, no stray `<p>` in a photo, the alt
    whole), without dprint's whitespace, and the XML files have indented blank lines
    between elements; the sitemap lists its urls by group, not by date and file creation.
  - The Atom feeds' alternate links name `https://limulus.net/penumbra/` and `…/tils/`, the
    pages themselves, where Eleventy wrote them without the slash only CloudFront's rewrite
    serves (`feed-alternate-slash`; their `<id>`s are unchanged, `∆yOJ`).
  - make-ai-boring-again's "previously suggested" link names `#fn-out-of-scope` on
    better-software-with-ai, comrak's footnote id, where it named markdown-it's `#fn2`
    (Eric, 2026-10-19, `∆yOJ`); the switch records one revision for that post.
  - Footnotes are comrak's, not markdown-it-footnote's (`footnote-markup`, which
    `parity:look` neutralizes): the reference reads `1` where it read `[1]`, the back
    link `↩` where it read `↩︎`, and the section loses the `section-separator` class
    (decided in `∆ITg`, slice 4, rather than reproducing markdown-it's markup in the
    core); `main.css` draws the centred rule above `.footnotes` itself, so it stays (Eric,
    2026-10-19). The footnote anchors are renamed on six pages
    (`#fn-out-of-scope` for `#fn2`), so a link to one from another page must follow.

The brief also carried the work the switch inherits: the sprite's logo `<style>`, which the
port stripped because WebC hoisted it and `page.lem` copied by hand; the two photo-tooling
tasks from `∆pP5` (`@sindresorhus/slugify` as a direct dependency, and `process-photos.js`
writing the alias-keyed records with their EXIF numbers so `bin/lemmon-photos.ts` retires);
the revision hash's lem-call translation, which no spec exercises on the real posts; npm
12's install-script gate; and the inventory and no-deploy acceptance mapped on 2026-10-18.

A planner sequenced it on 2026-10-19 and its review found what would have failed as written:
an Eleventy rebuild cannot be byte-compared across checkouts (its sitemap prints file
creation times), npm 12's `deny --all` misses fsevents, a negative build run's `prebuild`
deletes the positive run's output, an empty token passes `lem check`, and a local git tag
would itself be a deploy path. Eric decided two points from the parity classes: the six
in-body photo `id`s (WebC's component input leaking onto its host element) are let go, and
the footnotes' centred rule is restored in `main.css`.

## Refinement

Built on 2026-10-19. On `lemmon-port`: `@sindresorhus/slugify` as a direct dependency
(`983c9f1`), the footnote rule restored with parity:look made to see it (`d33eed8`), and
the last green whole-site run at `d33eed8`: every file both builds write, 140 and 158, zero
unexpected differences, 165 screenshots identical, both outputs and their checksums kept in
`dist/parity-final/` with the `.wasm`'s sha and lemmon at `f9c4acc`. Then a new local
branch, `lemmon-cutover`, in its own worktree:

- `fe25fb5` retires the parity harness, its output still byte-identical to the reference;
- `3380edc` is the switch: 46 Eleventy files and 17 dependencies removed, `build` is
  `lem build` into `dist/www`, its output byte-identical to the reference;
- `2c564be` keeps the logo's `<style>` in the sprite and deletes `page.lem`'s copy; every
  page gains exactly the rules the bundles lose;
- `5ff3e06` has `process-photos.js` write the alias-keyed records with their EXIF numbers
  into `www/_data/photos.json`, a round-trip spec proving the committed file reproduces;
- `d7f3ed1` records npm 12's `allowScripts`, every install script denied and each package
  shown to work without its own;
- `4ffdb74` rewrites the site's CLAUDE.md and README for a site Lemmon builds.

Accepted without deploying, in fresh `--no-local` clones with no remote at the switch commit
and at the tip, beside a lemmon clone at `f9c4acc` whose `.wasm` matched the reference's:
`npm ci` (silent at the tip), `npm run verify` with the CDK snapshot test, a build that
refuses without the token, the output byte-identical to the reference at the switch and
differing only by the sprite style at the tip, identical builds from three paths and time
zones, `infra/`, `cdk.json`, `.github/` and `.husky/` untouched, `update-revisions`
recording only make-ai-boring-again, and the s3 sync plan: 10 new files (the bundles at
their new path), 59 changed (pages and XML), 71 unchanged (scripts and passthrough), 28
left stale and unreferenced (Eleventy's old bundles and stubs). The live site's script
hashes equal both builds', so the unchanged count holds against the bucket.

Left for Eric: `aws s3 sync --dryrun` and `cdk diff`, which need credentials; a look at the
logo on a P3 and an sRGB display; the merge commit's subject, which CD records as the
make-ai-boring-again revision's message; and `∆FnH` before the merge, since a push today
fails at `lem` (exit 127) and deploys nothing.

## Retrospective

The switch commit carried no risk to the output because everything risky had been moved
before it or after it, each with its own byte comparison; the hard parts were the
environment (npm 12's gates, a token that must never print or be empty, git refs that
deploy). Reviewing the plan against a scratch run found five steps that would have failed,
which is cheaper than finding them mid-build on a branch that must stay clean.

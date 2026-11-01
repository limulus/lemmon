---
id: ∆GUI
completed: 2026-11-01 14:30:12 -07:00
---

## Backlog item

> - `∆GUI` limulus.net uses `@limulus/blur-hash`
>   - The site's own `<blur-hash>` element, with two races live today, gives way to the
>     package made to fix them

## Planning

The item's brief, folded here. limulus.net's `<blur-hash>` element had two races, live on
the site: an element connected before its `<img>` was parsed threw, and a placeholder
painted after its photo had loaded stayed behind it. They were found building the esbuild
asset pipeline (`∆d1N`), when instrumenting the element showed why `npm run parity:look`
stalled, and `parity:look` steps around them. Eric chose to fix them by pulling the element
out of the site into its own public package, `@limulus/blur-hash`, scaffolded with
`@unallocate/create` on 2026-10-18: he did not want to introduce the testing infrastructure
into limulus-dot-net yet, and expects others may find the component useful. The package was
built by 2026-11-01, and 1.1.0 is on npm.

Asked where the item belonged, I recommended after the merge, so that the switch stays a
change of builder alone. Eric decided on 2026-11-01 that it is part of slice 6, so that the
live site switches to the package as part of the cutover, on `lemmon-cutover`.

The brief's suggestions held against the code: the package reads the markup
`pho-picture.lem` writes (`blurhash`, `width`, `height`, the `<img>` inside), and its
`BlurHash.workerUrl` lets the site bundle `@limulus/blur-hash/worker` itself, which esbuild
needs since it bundles no `new Worker(new URL(…))`, named by the `worker.XXXXXXXX.js`
placeholder the site's other workers use. No change to Lemmon was needed.

## Refinement

Built on 2026-11-01 on `lemmon-cutover`:

- `cb036ce`: the site depends on `@limulus/blur-hash@^1.1.0` (the lockfile gains it and
  `fast-blurhash`); `www/lib/blur-hash/index.ts` sets `BlurHash.workerUrl` to the site's
  bundled worker and defines the element, `worker.ts` is the package's worker, and the
  site's element and its message protocol go. Pages differ from the build before only in
  the common bundle's name, which names `/lib/blur-hash/worker.<hash>.js` with no
  placeholder left. In a browser on `lem dev`, `/photos/` defines the element, loads the
  worker, leaves no placeholder behind a loaded photo, paints a `blob:` placeholder for an
  image that never loads, and an element connected before its `<img>` no longer throws.
  The other race wants a decode slower than the photo's load, which the browser check
  could not arrange; the package's load-race spec is the evidence for it.
- `28dde17`, from the diff's review: the site's placeholder CSS (`pho-picture.lem`'s
  `<lem-style>`, `photo-list.lem`'s blurhash rule) never applied any more, since the
  package sets the background's size, position and repeat inline from the image's
  `object-fit` and `object-position`, and clears them on load; the bundles lose exactly
  those rules.

The review found two changes for the better: the worker starts on the first decode rather
than on every page that loads `/lib/common`, and the author chip's placeholder is `cover`,
matching its cropped photo, where the site's CSS gave it `contain`. The `blurhash`
dependency stays, since the photo scripts encode with it. The site's `npm run verify`
passed after each commit.

## Retrospective

The switch needed nothing from Lemmon because the package was given a `workerUrl` for
bundlers that cannot follow `new Worker(new URL(…))` — the escape hatch decided which
repo the work landed in. Reading the package's inline styles against the site's CSS is
what found the dead rules; a build diff alone would never have.

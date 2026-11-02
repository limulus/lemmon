# Output a build no longer writes is removed

## Background

Recorded on 2026-10-18 from the review of `∆99b`. `docs/shell.md` documents it:
neither `lem dev` nor `lem build` removes anything from the output, so a deleted or moved
page's file stays there and `lem dev` goes on serving it, and every edit to a client script
leaves its old hashed bundle beside the new one. Generators make it common in the daily
loop: removing a photo from `photos.json`, or changing its alias, leaves its page served,
and the review probed a removed item's neighbour still linking to it (`/photos/b/` showing
`next=Sea`). The port's `npm run parity -- --no-build` also fails on a page `lem dev` left
behind, since it compares every page either output holds.

Production is unaffected: the deploy builds into a fresh checkout's empty output.

## Scope and constraints

- Remove only what an earlier build of this site wrote: the output directory may hold
  files the site does not own, and the shell must never delete outside it. A manifest of
  what the last build wrote, or a clean output before a full build, are the obvious
  shapes; `lem dev` rebuilds the whole site, so either fits it.
- Passthrough files, asset-hook files and producers' files count as written.

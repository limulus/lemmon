# Builds that hold no site in memory

## Value

A large site builds without holding its whole output in memory, and a broken link still
leaves the real output untouched.

## Background

Since D47, `lem build` renders every page into memory, checks every link in what it would
write against the files it writes, and writes only when nothing is broken. For limulus.net,
55 pages, that is a few megabytes, and the check reads about 4,100 links in about 65 ms of
a build (45 ms in a `lem dev` rebuild). Eric raised on 2026-11-06 that both grow with the
site: 10,000 pages at 50 KB would hold about 500 MB, and the check's time grows with the
links.

The check needs only two things from each page: the links it holds and the ids its
fragments may name. Taking those as each page renders, writing the page to a temporary
directory, checking the summaries once every page has rendered, and then swapping the
directory in as the output — or deleting it on an error — keeps the guarantee while
holding only the summaries. Each page is still parsed once.

Eric weighed two alternatives the same day. A separate check over the written output after
the build, possibly optional, solves memory too, but a failing build has then already
replaced the output, `lem dev` would serve the broken pages, and an optional check is one
that gets skipped. Parsing HTML in the core would break invariant 2, buy little speed once
every page crosses the boundary, and the check needs the shell's list of output files
(D47). He chose the temporary directory.

## Scope and constraints

- A build with an error writes nothing to the output directory, as today (D47).
- Memory no longer grows with the size of the site's rendered output.

## Implementation suggestions

- Re-checking only the pages a change touches in `lem dev` builds on `∆xBO`'s
  render-reported dependencies, and a worker pool spreading render and parse across cores
  is sketched in `docs/architecture.md`; both are separate from this item.

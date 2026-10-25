# The `transformOutput` hook

## Background

Split from `∆c4j` Plugin host on 2026-10-17 and placed in Someday/Maybe beside `∆wOb`,
formatted HTML for limulus.net, its only consumer. Built once for `∆Oyi` and parked unmerged
on the local branch `parked/oyi-transform-output-hook` (`ba9ece6`):
`Plugin.transformOutput?: (page: PageOutput) => string | Uint8Array`, `PageOutput` being
`{file, url, path, bytes}`; transforms run after `resolvePage`, in plugin order, each over
the previous one's output, on pages only; bytes are copied in and out; a throw or a bad
return is a warning at the page's front-matter span and the page keeps its prior bytes;
async transforms are refused at load.

## Scope and constraints

- Start from the parked commit. Its production files merged cleanly with main as of
  2026-10-17 (`git merge-tree`); its doc hunks did not all conflict but are wrong now: they
  say `transformOutput` landed in `∆Oyi` and that dprint is the site's plugin, both false
  since Eric retired `∆Oyi`. Reuse the code and specs and rewrite every doc hunk.
- A page transform runs after Resolve and before the link check (D47, `∆yOJ`), which reads
  what is written: architecture step 8, at the end of `renderSite()`.

---
id: ∆Vhi
completed: 2026-10-17 18:11:50 -07:00
---

## Backlog item

> - `∆Vhi` `lem dev`
>   - Dev server with COOP/COEP headers and live reload; a change rebuilds everything, and the
>     full rebuild time is recorded for the Incrementality epic. Replaces `eleventy --serve`
>   - needs: `∆muD`

## Planning

The item's brief, folded here. `lem dev` — dev server with COOP/COEP headers and live
reload, incremental rebuilds from render-reported deps and `upsertRecord`, `--drafts`
(D19), replacing `eleventy --serve --incremental`. The slices reshaping of 2026-10-12 made
every change rebuild everything, the full rebuild time recorded here to decide whether the
Incrementality epic opens (above one second); drafts moved to `∆Han` on 2026-10-17. Slice
3's acceptance: Eric runs `lem dev` in the branch, edits a post or a layout and sees the
change in the browser, and sees `lem check` report a deliberate error with its span.

Eric decided on 2026-10-17 that the dev server's headers come from the site's config,
`dev.headers`, rather than from the shell: a dev server sending COOP/COEP itself would
break, in dev only, a site embedding a cross-origin image or video that does not opt in.
limulus.net lists the two its `eleventy.config.ts` middleware and CloudFront policies
send. Planned by the main session and reviewed by a read-only Opus agent that probed in a
`node:24` container as well as on macOS, which changed the plan in five places. Node's
recursive `fs.watch` on Linux missed every edit after an atomic save and every edit under
a renamed directory (its `#unwatchFiles` matches by prefix), so the watch is chokidar 5. A
server-sent event with no `data:` line is never dispatched, and an event stream holds one
of a browser's six connections per host for each open tab, so reloads go over a WebSocket
(`ws`). A `localhost` listen binds `::1` alone on macOS, so the server binds `127.0.0.1`.
`fetch` normalises `/../` away, so the traversal specs send raw paths. And a wasm trap
poisons the resident instance for every later build, so it is fatal. The config file is not
watched; a change to it needs a restart, which `lem dev` says as it starts.

## Refinement

Built in three reviewed commits by Opus builders, each reviewed by a read-only Opus agent
running real sessions, the main session committing after `npm run verify` passed.
`857e097`: `dev.headers`, validated at load through Node's own header validators, two
names equal but for case refused, and so is any name in `SERVER_HEADERS`, the set the
server writes itself, and the server in `serve.ts` — the output's files by `outputFile`,
the url rule `lem build` writes by, a directory without its `/` redirected, the reload
client injected before an HTML page's last `</body>` in the bytes sent, the page on disk
left as built. Its review found a `Trailer` header that passed validation and then made
`writeHead` throw an unhandled rejection on the first request; `trailer` and
`content-encoding` joined the refused set, and a throw while answering now ends that
response, not the server. It also added a `Host` check against DNS rebinding, kept
non-WebSocket upgrades from `ws` (`shouldUpgradeCallback`, Node 24.9), and made the client
reconnect and reload when a restarted `lem dev` comes back. A page read while a rebuild
rewrites it can be served partial; that is documented, not fixed. `262b305`: `createCore`
asks the instance its version before constructing a core, since a core constructed in a
trapped instance is freed later by the glue's finalizer, inside that instance, as a throw
nothing can catch. `bf34900`: the watcher (`watch.ts`, the walk's own `skipped` predicate,
symlinks not followed), a pull-based scheduler (`scheduler.ts`: a rebuild once 50 ms pass
with no change, chokidar's own throttle, and one more for changes during a build), the
loop (`dev.ts`), `lem dev [--port]`, and `Io.interrupted()`, armed only by `lem dev`. Its
review found that a stack overflow inside the core, the second shape of `∆ugN`, is a
`RangeError` rather than a `WebAssembly.RuntimeError`, so a trap is now known by the core
refusing to be freed and reported as `CoreTrapped`; that the specs could not see a lost
edit, because each retried its write until the effect showed (now a warm-up, then single
writes); that a 404 carried no reload client, so a tab opened on a failed first build never
reloaded; that a directory link into the output looped, 71 rebuilds in 3 s; and that an
interrupt during a rebuild still reloaded the pages into a closing server. On macOS a
watch loses a write made in its first milliseconds and replays writes made just before
it, so real-file-system specs assert only what happened, after a warm-up. Dependencies:
`ws` 196 KB, `mime-types` with `mime-db` 276 KB, `chokidar` with `readdirp` 109 KB.

On the port branch (`ef99c3a`), `npm run lemmon:dev` runs `lem dev` beside a `chokidar`
watch that reruns `npm run lemmon:css` when a global stylesheet changes. The time, from
`lem dev`'s own lines on 2026-10-17, building 25 documents, 2 produced files and 34
copied: the first build 295 ms, and 21 rebuilds after five edits to a post's markdown and
five to `article.lem`, each reverted, 233–257 ms, median 250 ms. That is under one second,
so the Incrementality epic stays closed. What skews it: the port builds 25 documents
where Eleventy's `dist/www` has 55 HTML pages, and 18 of the 25 are untyped pages loaded
one at a time, about 0.35 s of a `lem build`, which `∆tAw` will mostly type; the number is
worth taking again at the whole-site run (`∆Zg9`). A stylesheet edit costs two rebuilds,
the `.css` change and then the regenerated `site-styles.lem`; every rebuild prints the
eight code-block warnings until `∆Nxd`; and macOS's replay rebuilds once at startup.

## Retrospective

Every review round found defects the builders' own negative tests had not, because the
reviewers ran the software rather than its specs: the plan review's Linux container (the
lost edits), a real `Trailer` header, a real directory link, a real stack overflow. For
code that waits on the file system, a spec that retries until it sees the effect proves
only that the effect happens eventually; it cannot see the event that was lost.

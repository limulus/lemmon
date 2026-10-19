---
id: ∆EEa
completed: 2026-10-18 21:29:42 -07:00
---

## Backlog item

> - `∆EEa` `esbuild()` names follow the files they reference
>   - Adopt the naming limulus-dot-net's `11ty/esbuild.ts` moves to, a loader's hash following
>     the workers it names, so parity's byte comparison holds once the port merges main

## Planning

The item's brief, folded here. Value: a fix to one of limulus.net's client workers reaches
every reader on the next deploy, and the port keeps comparing its scripts with Eleventy's
byte for byte. limulus-dot-net's `11ty/esbuild.ts` took each bundle's hash from esbuild's
output names before rewriting the `….XXXXXXXX.js` placeholders inside them, so after a
change to a worker alone a loader such as `/lib/common` kept its file name while naming
the new worker; CloudFront serves `/*.????????.*` with a 365-day `immutable`
`Cache-Control`, and the deploy's `aws s3 sync` deletes nothing, so a cached loader kept
running the old worker. The port found it reproducing the step (`∆d1N`, D45), and on
2026-10-18 Eric started a session to fix the naming on limulus-dot-net's main, this item
to follow it: take the exact rule from that change, stay byte-identical with Eleventy once
the port merges main, and revise D45 and the cutover brief.

Eric chose on 2026-10-18 to hold slice 5 open until this item was built.

## Refinement

The limulus-dot-net session landed `d1dc734` ("fix: name script bundles after their final
content") and `b8afe6a` (`"root": true` in `.eslintrc.json`, which also lets plain
`npm run lint` run in the port's nested worktree) on public main the same evening, deployed
them, and sent the rule: bundle in memory with `….XXXXXXXX` entry names, resolve each
entry's placeholders with the entries it names named first (a cycle is an error), make the
checkout's path in each map relative to the map, and name each bundle by the first eight
base32 characters of the SHA-256 of its JS, its map comment still a placeholder, and its
map. The port branch merged public main (`f89f30a`), and lemmon's `esbuild()` was rebuilt
on the rule in one isolated worktree, landing as `49e4c60`: over the port's sources it
writes all 37 files byte for byte as Eleventy does, where the old naming matched 34, and a
loader is renamed when only its worker changes, the hazard's own spec. D45 was revised in
place; the cutover brief's hazard paragraph now says both hazards are fixed upstream and
followed here.

Departures from the upstream code, each recorded in D45: the cycle error names the cycle
alone where Eleventy's lists the whole naming stack; no check that a bundle ends in its map
comment, since esbuild writes one even for an empty entry; esbuild is given its own
`.[hash]` after the `XXXXXXXX` placeholder so that two `index.ts` or a CSS entry and an
entry's imported styles cannot collide before naming, and those nine characters are cut
before hashing; styles a script imports are named by content where Eleventy leaves
esbuild's name (limulus.net imports none); no stub files, as before.

The builder found a separate bug in the same file, landed first as its own commit
(`fc64b91`): a dependency's map naming a source without its text put a `null` in
`sourcesContent`, and placing a mistyped placeholder then threw a TypeError
(limulus.net's `performance-now` carries such a map). The review found no correctness bug
and four small things, fixed before landing: the hash spec passed with the eight-character
padding removed, the claim that no name depends on where the project is holds only for a
path esbuild writes into a map unescaped (Eleventy shares the limit), two passages kept
the retired rationale, and a map written into the root itself got an empty relative
prefix. It recorded, unchanged, that Lemmon replaces every `XXXXXXXX` in a placeholder's
path where Eleventy replaces the trailing one, and two older `new URL()` divergences
(absolute-URL placeholders, spaces), none with an input where both succeed differently.

The same evening limulus-dot-net's main also took `7d87ed5`, which fills `/photos/`'s
hidden headings (`∆LsH`'s `photo-list-heading`); the port merged it with this item, the
class retired, and its entry left the cutover list.

## Retrospective

Fixing the hazard upstream instead of carrying it to the cutover cost one evening and
left the port nothing to unlearn: parity's byte comparison is what proved the two
implementations agree. Holding the slice open for it was right, since merging main without
this item would have failed every script comparison.

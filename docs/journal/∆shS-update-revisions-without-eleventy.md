---
id: ∆shS
completed: 2026-10-18 23:09:42 -07:00
---

## Backlog item

> - `∆shS` `update-revisions` without Eleventy
>   - A script lists the articles and their titles as the port's `site.lem` does, so the
>     deploy records each revision as it does today, every recorded hash unchanged

## Planning

The item's brief, folded here. limulus.net's deploy (`.github/workflows/cd.yaml`) runs
`npm run update-revisions` before the build on every push to every branch and commits
"chore: update revision history [skip ci]". The updater ran a whole in-memory Eleventy
build only to list the articles and read each one's `title` and `subhead`; the body was
gray-matter's `content`, the markdown as written. The hash is shake128 to six bytes over
`[title, subhead?, eleventyFormsOfLemmonCalls(body)].join("\n")`.

The item began as a revisions plugin on a content API (`∆Gg3`). Mapping slice 6 on
2026-10-18 found a short script reproduces all 25 recorded hashes with neither Eleventy nor
the core, while a core call would need the whole program compiled and the asset hooks run,
so the deploy step would newly need the Mapbox token. Eric chose the script and parked
`∆Gg3`. The data half already worked without a plugin: the port binds `revisions.json` as a
typed data file and `base.lem` derives the modified time from it. The survey also found
what neither brief said: the port hashes a body through the lem-call translation, so the
six posts holding ten lem calls hash as the WebC and Liquid those calls replaced, and
hashing the Lemmon source would record six spurious revisions.

## Refinement

Built on the port branch (limulus-dot-net `f7294ea`, local): `bin/revisions/articles.ts`
lists the articles with the port's own glob and gray-matter, its title rule
`title ?? "Link: " + (linkTitle ?? "")` beside `site.lem`'s default, and a spec reads
`site.lem`'s text to pin both the glob and the rule, so neither copy changes unseen.
`revisions.ts` and the lem-call translation moved from `11ty/` to `bin/revisions/`; the
translation has two variants, one that passes an unknown lem line through to the hash
(the updater's, so a post using a new call cannot fail a deploy) and one that throws
(Eleventy's, until the cutover). The updater writes `revisions.json` only when it records
something. The set it lists matches Eleventy's `article` tag, the same 25 paths with the
same titles, subheads and hashes; all 52 recorded revisions reproduce. The two backfill
scripts, one-off migrations whose work was done, were deleted: rerunning one would have
rewritten the history. On the branch, `npm run update-revisions` prints
"Done. 0 revision(s) recorded." and leaves no diff, with an empty environment too.

The review found the spec that reproduces the recorded hashes read each one's commit with
`git show`, which fails in a shallow clone and, after a branch CD recorded is squashed and
its commit pruned, fails on every push, since `.husky/pre-push` runs the tests. A check
whose commit the repository lacks is now skipped with a title naming it, and the whole
describe skips in a shallow clone. It also recorded that no spec exercises the ten real
lem calls, since every recorded commit predates the port; the cutover brief says to check
the updater records nothing on the switch commit.

## Retrospective

The brief's reason, "reimplemented on the content API", was the architecture sketch's
assumption; counting what the updater reads showed it needed a list of files and one
line of the title rule. A test that reads git history is a test of the repository's
shape as much as of the code, and wants a skip with a reason where that shape varies.

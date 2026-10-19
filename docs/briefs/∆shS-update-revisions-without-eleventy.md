# `update-revisions` without Eleventy

## Background

limulus.net's deploy (`.github/workflows/cd.yaml`) runs `npm run update-revisions` before
the build, on every push to every branch, and commits "chore: update revision history
[skip ci]". The updater (`11ty/update-revisions.ts`, `11ty/revisions.ts`,
`11ty/get-articles.ts`) runs a whole in-memory Eleventy build only to list the articles
(the `article` tag) and read each one's `title` and `subhead`; the body is gray-matter's
`matter(rawInput).content`, the markdown as written. The hash is shake128 to six bytes over
`[title, subhead?, eleventyFormsOfLemmonCalls(body)].join("\n")`; a changed hash appends
`{v: 1, hash, date, commit, message}` under the article's `www/…md` path.

Planned as a plugin on the content API (`∆Gg3`), then reshaped on 2026-10-18: mapping slice
6 found a short script reproduces all 25 recorded hashes without Eleventy or the core, and
Eric chose it and parked `∆Gg3`. The data half already works without a plugin: the port
binds `revisions.json` as a typed data file and `base.lem` derives the modified time from
it, as Eleventy's computed field did.

## Scope and constraints

- On the port branch now, in limulus-dot-net: list the articles with the port's own glob
  (`www/{feed,links,penumbra,tils}/**/*.md`) and gray-matter, with the link title rule
  `title ?? "Link: " + linkTitle` written a second time beside `site.lem`'s default; a
  spec pins the four computed link titles, so the two copies cannot drift unseen.
- The body is hashed through the lem-call translation (`11ty/lemmon-calls.ts`) for good:
  the six articles holding ten lem calls hash as the WebC and Liquid those calls replaced,
  and hashing the Lemmon source would record six spurious revisions. Once Eleventy is gone,
  a lem line with no WebC form passes through to the hash instead of failing the deploy.
- `revisions.ts` and the translation move out of `11ty/`, which the cutover deletes;
  `bin/backfill-revisions.ts` and `bin/backfill-revision-messages.ts` are repointed or
  deleted. `cd.yaml` and the script name stay; the step needs no secret.
- Eric accepts it by running `npm run update-revisions` on the branch and seeing no new
  revision and no diff.

# Opt a change out of limulus.net's revision history

## Background

Eric on 2026-11-08, reviewing the cutover branch: "we probably need a way to opt-out of
revision creation". He considered keying it on conventional-commit types, so that only
`feat` and `fix` record a revision, but was unsure the branch's commits would meet that
rule and whether it suits a website; and "I would prefer to avoid the new revisions for
pages due to the cutover".

CD's `npm run update-revisions` hashes each article's title, subhead and content, and
appends a revision when the hash differs from the latest one recorded, with the pushed
head commit's subject as its message. A post's revision history, its
`article:modified_time` and the sitemap's `lastmod` show every revision after the first,
all three through `revisionsOf` in `revision-history.lem`. A spec checks each article's
latest recorded hash against the article as that revision's commit holds it.

Six posts hold an `articlePhoto` or `demoWithFallback` lem call where they held WebC or a
Liquid include: tuples, ray-sphere-interactions, rust and canvas-and-matrices in the
penumbra journal, better-software-with-ai and the-rebound in the feed. Each has recorded
revisions. `bin/revisions/lemmon-calls.ts` hashes each call as the markup it replaced, so
the switch records none for them; kept, it stays for good, since a revision recorded
through it would fail that spec without it. make-ai-boring-again records a revision at the
merge for a link the cutover's renderer moved, `#fn2` to `#fn-out-of-scope` (`9f0a630`).

## Scope and constraints

- Lands on the cutover branch, before the merge.

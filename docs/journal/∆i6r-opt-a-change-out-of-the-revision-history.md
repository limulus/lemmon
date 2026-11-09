---
id: ∆i6r
completed: 2026-11-08 20:17:05 -07:00
---

## Backlog item

> - `∆i6r` Opt a change out of limulus.net's revision history
>   - A change to an article's text that readers need not see, as the cutover's lem calls
>     are in six posts, still records a revision the page shows; the author needs a way out

## Planning

This section folds in the item's brief. Eric found the problem on 2026-11-08, reviewing
the cutover branch: "we probably need a way to opt-out of revision creation", and "I would
prefer to avoid the new revisions for pages due to the cutover".

On every push, CD's `npm run update-revisions` hashes each article's title, subhead and
content. Where the hash differs from the article's latest recorded one, it appends a
revision. The revision names the pushed commit and takes that commit's subject as its
message, and the bot commits `revisions.json` back to the branch. A post's revision
history, its `article:modified_time` and the sitemap's `lastmod` show every revision after
the first. A spec checks each article's latest hash against the article as that revision's
commit holds it.

The cutover changed seven articles. Six hold a lem call where they held WebC or a Liquid
include: tuples, ray-sphere-interactions, rust and canvas-and-matrices in the penumbra
journal, and better-software-with-ai and the-rebound in the feed. The port had kept those
six from recording a revision with `bin/revisions/lemmon-calls.ts`, which hashed each call
as the markup it replaced. Kept, that translator would have stayed for good, since a
revision recorded through it would fail the spec without it. The seventh,
make-ai-boring-again, would have recorded a revision at the merge for a footnote link the
cutover moved, `#fn2` to `#fn-out-of-scope` (`9f0a630`).

Eric's first idea was to key it on conventional-commit types, so that only `feat` and
`fix` record a revision. That was set aside for three reasons:

- CD reads only the pushed commit. At the merge, that is the merge commit, not the
  branch's commits.
- A commit's type describes the code change, not whether readers see a different
  article.
- Every listed revision so far happened to be `feat` (19) or `fix` (9), so the rule would
  have hidden none of them. But a content edit under `chore:` would be hidden.

The next proposal was a local flag, `npm run update-revisions -- --quiet`. Eric asked how
it would work from his seat. Walked through, it was a command and a second commit after
every quiet edit, in a fixed order, since run before the commit, the revision names the
previous commit and the spec fails. Forgetting it lists the revision. He chose a commit
trailer instead, `Revision: quiet`, read from the commit CD already takes the message
from. He also chose that all seven go quiet, make-ai-boring-again included, since its
readers see the same text and the link still reaches its footnote.

## Refinement

Built on 2026-11-08 on the cutover branch as limulus-dot-net `1b0da5e`, on top of
`∆FnH`'s `3b64e12`:

- A revision may carry `"quiet": true`, and `site.lem` declares `quiet: bool = false`.
- One template function, `listedRevisionsOf`, replaces `revisionsOf`. It drops the first
  revision and filters the quiet ones, and the revision history, `base.lem`'s
  `article:modified_time` and the sitemap read it. Filtering after `drop(1)` means a new
  post pushed with the trailer still lists its first edit.
- `update-revisions` reads the pushed commit's `Revision` trailers with `git
  interpret-trailers --parse`. A value other than `quiet` throws, which stops the deploy
  before the build.
- The translator, its spec and its three `computeHash` tests are gone.
- The site's `CLAUDE.md` describes the trailer, and the epic's text here says the merge
  is a merge commit (`--no-ff`) whose message ends in the trailer. A fast-forward would
  push the branch's own tip, which has no trailer.

The trailer's tests run on message text, not on a throwaway repository. A pre-push hook
started from a worktree exports `GIT_DIR`, which a quick probe confirmed, so git commands
run in a temporary repository would read the worktree's instead.

The evidence:

- The site's `npm run verify` passed, with 130 tests. The 25 recorded hashes reproduce
  without the translator, since their commits hold the old markup.
- The branch built byte-identical to its build before the change, all 140 files.
- A scratch clone rehearsed the merge: a two-parent `--no-ff` merge into `public/main`
  whose message ended in the trailer, with revisions recorded as CD records them. The
  seven articles were recorded quiet, the hash spec passed against the merge commit, and
  the build was byte-identical to the reference.
- The same merge without the trailer recorded the same seven as listed. Each page gained a
  revision and a new modified time, and the sitemap moved.
- Removing `quiet` by hand from make-ai-boring-again's entry changed only its page and the
  sitemap.
- `Revision: quite` exited 1 and wrote nothing, and a wrong hash planted on a quiet
  revision failed the spec.

The first rehearsal clone symlinked another clone's `node_modules`. esbuild then recorded
those paths in its source maps, whose content the client scripts' hashed names cover, so
every page's script names differed. A copied `node_modules` gave the identical build.

The code review found that deleting the translator's tests had left no unit test pinning
`computeHash`'s formula, and the per-article checks are skipped in a shallow clone. A test
was restored. Two findings were kept as built: a mistyped trailer stops even a deploy that
records nothing, and `getCommitBody` lets git's failure throw rather than fall back to a
listed revision. Eric accepted the item on 2026-11-08.

## Retrospective

The local flag was offered as the smallest change, measured in code, and it was the most
work for the author: walking through the steps from his seat found the order trap before
anything was built. A mechanism a person runs is judged by the steps it asks of them, not
by its line count.

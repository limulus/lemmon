---
id: ∆F4y
completed: 2026-10-17 12:18:59 -07:00
---

## Backlog item

> - `∆F4y` Mark resolution and the default plugin
>   - Producers and decorators resolve marks in the shell (D42), and the default plugin's
>     `styles` producer writes per-page hashed CSS bundles under `/assets/immutable/`

## Planning

The item's brief, folded here. From `∆7Xs` Output writer: HTML to disk with marks
resolved, the `styles` producer writing per-page hashed CSS bundles of what `lem-style`
collects under `/assets/immutable/…`, passthrough copy by glob, and a link/asset integrity
check, CloudFront untouched. From `∆PgF` Plugin host: the `Plugin` hooks, among them
`marks` — producers with keys and a payload record, decorators, two producers on one key a
load error, and the shell's `styles`/`scripts`/`head` bundler a default plugin the site
lists (D42). Scope here: the marks part of the plugin host only (claims sent as
`HostDecl.marks`, two producers on one name and key a load error naming both, the default
plugin the site config lists); the other hooks are `∆c4j`. Slice 2's acceptance, which this
item serves: Eric opens the seven posts and sees them identical to the live site, with
their CSS as per-page hashed bundles under `/assets/immutable/`, and `npm run parity`
reports zero unexpected differences for them.

Planned in the slice-2 run of 2026-10-17 by a read-only Opus agent, reviewed by a second
before anything was built. The plan checked the brief's claims: Eleventy names a bundle by
the first ten characters of the **base64url** SHA-256 of its bytes (eleventy-plugin-bundle
1.0.5), not hex, and recomputing all ten of the branch's bundles confirmed it;
`name.<8hex>.ext` is the passthrough convention only, esbuild writing eight base32
capitals; and CloudFront already caches all of `/assets/immutable/*`, so any directory
under it keeps D13. It settled four questions Eric left to the run: the bundle path
`/assets/immutable/bundle/<10 base64url>.css`, in Eleventy's hash form but Lemmon's own
directory, since the href differs from Eleventy's whenever the CSS bytes do, which is
always; the default plugin listed explicitly as `plugins: [bundler()]`, with an omitted
list declaring no claims; the `scripts` producer built now; and code-block payloads handed
to producers now. Three checkpoints: plugins, claims and per-page resolution; files a
producer writes; the default plugin and the docs.

The plan review found CI on main red from slice 1: the fidelity script imported the
shell, whose bare `@limulus/lemmon-core` import plain `node` resolved to a `dist/` CI never
builds, while a stale local `dist/` hid it from the pre-push gate. It was fixed first, as
its own commit (`37b1047`), and every checkpoint since ran `npm run clean:js` before
`verify`. The review also found that the core reads a claim listing no keys as covering
every key, so the plan's name-only claims for decorators would have silenced the warning at
every code block's language; that a computed mark key no producer lists is reported
nowhere, against `docs/language.md`'s promise (minted as `∆CxX`); and that `resolveMarks`
crashed `lem` with a stack trace on a producer returning `[undefined]`.

## Refinement

Built by Opus agents, each checkpoint reviewed by a read-only Opus agent before it was
committed. Checkpoint 1 (`83786be`): `plugins` in `lemmon.config.ts`, each plugin's `marks`
handlers validated at load — two producers on one name and key, two handlers naming
different payloads or key fields, keys for a name no handler keys by a payload field,
async handlers and an uncalled plugin factory are refused, naming the plugins — with
`producerOverlap` the one predicate the core's `resolveMarks` and the shell's load check
share; every claim sent with a key list; per-page resolution through `resolveMarks`, code
payloads beside their ids, a failing or malformed handler a warning at the mark that keeps
its content, and fragments a page collects but never places a warning at the block.
Checkpoint 2 (`16a3667`): `ProducerContext.write`, files claimed like pages and written
after them, identical bytes from two pages shared, everything checked before the first
write. Checkpoint 3 (`6f7f6fa`): `bundler()` with `styles`, `scripts` and `head`, and the
docs.

The reviews found, among others: an array of promises from a producer still crashing Node
with an unhandled rejection (every item, and any thenable, is now settled before it is
refused); handler keys on a name with no key field, silent at every stage (now refused at
load); `Buffer.slice()` sharing memory, so a producer that reused its buffer changed a file
it had already written; specs that compared lengths where they meant bytes, accepted
either of two messages, or pinned only hashes with no `-`, `_` or non-ASCII byte in them;
and the next item's handoff restating a superseded draft of its parity class. On the
branch, unchanged, `lem build` adds exactly the eight expected warnings at code blocks no
highlighter claims (`∆Nxd`), and `npm run parity` still passes.

## Retrospective

Every review in this item found something its builder's own perturbations had not, and
most of it was in the seams between stages: a claim shape the core read differently from
the shell, a rejection one level inside a returned array, a byte copy that was a view. The
pre-push gate itself had been passing on a stale build; the habit worth keeping is
cleaning generated output before trusting a green gate.

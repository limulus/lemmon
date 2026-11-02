# Every page has a type

## Value

An author whose page no glob collects is told so when the site builds, with the fix,
instead of finding the page published without its layout. The shell loses the machinery
that renders such pages one load at a time.

## Background

D30 decided that a document "no glob matches and no `type:` names is unknown-typed and
renders", and D37 that the loader binds the envelope's document value as sent for those
renders, one value per load. So a page with no record renders its own output with no
layout (the layout is the record type's `layout` default), its front matter unchecked by
any schema, and the shell (`renderSite` in `packages/shell/src/lib/build.ts`) calls
`loadData` once per such page with its front matter as `page`, each call re-sending and
re-validating the whole envelope, every rendered body included since D46.

It is a silent fallback. Through slice 3 the limulus.net port's `site.lem` collected only
`feed/*/index.md`, and its 18 other documents rendered as bare markdown with no head or
header while `lem check` stayed clean; nothing said a glob had missed them. The shell
already refuses a missing `site.lem` for this reason (`SiteError` in `program.ts`: every
document "would render as it is, without its layout, far from the cause"); an untyped page
is that failure one page at a time. Generators already require a type (D43).

Eric questioned on 2026-10-24 whether untyped pages should exist, and agreed to remove them
from the standard shell: a page with neither a collecting glob nor a `type:` becomes an
error, and the core may keep its ability to render an unknown-typed document for hosts with
no schemas (a playground, an editor).

Measured on 2026-10-24 against lemmon `b2d4350`:

- Production: the per-page reload loop (`build.ts` ~252-258) and the non-string url path
  only an untyped page reaches (~5 lines) go, with ~10 lines of JSDoc; `program.ts`'s
  `CompiledPage.record` becomes required, and `load.ts`'s pages filter is rewritten. The
  new error is ~15-25 lines after `program.ts`'s generator check, and can absorb its
  `untyped()` refusal. Nothing in `packages/core` exists only for this; the core's path
  (`host.rs` ~536-538, ~71 lines of tests) stays for other hosts.
- Tests: 3 specs (34 lines) are deleted; 18 specs carry untyped pages of their own, and the
  shared fixture site's `notes/loose.md` (`packages/shell/src/mocks/sites.ts`) reaches 131
  specs, so typing it or dropping it rewrites 32 to 36; ~3-4 new specs. `scripts/` needs
  nothing: the fidelity and bench harnesses use a schema-less host.
- Corpus: `host/unmatched/` (4 files) and 18 more `host/` cases are untyped pages by the
  shell's rule; whether the corpus keeps the core's view or mirrors the shell decides
  whether they change.
- Docs: ~65 lines across D30, D36, D37, D43, D46, the decision index, language.md,
  architecture.md, `docs/shell.md`, the skill and checking.md, artifact-changes.md, and
  the `∆Han` brief, plus a new decision record.
- limulus.net has no untyped page.

## Scope and constraints

- A page — a document, or a page template with front matter — that no collecting glob
  matches and no `type:` names is an error at its front matter, or at the file's start
  where it has none, naming the fix: `type:`, or a glob in `site.lem` that collects it.
- The per-page reload goes; every page is loaded once, with the site.
- A decision record revising D30 and D37 (D48 was the next free number on 2026-10-24),
  the docs above swept, and an `artifact-changes.md` entry.
- Decide in planning whether the corpus keeps the core's unknown-typed behaviour (the
  `host/unmatched/` cases) or mirrors the shell's refusal, as its generator check does.
- Related, with no hard dependency: a page that wants no layout now needs a type whose
  layout passes its body through, which warns until `∆gsV`; `lem init` (`∆V7i`) should
  scaffold every page typed; `∆xBO` and `∆Qr2` get simpler, with no per-page reload to model.

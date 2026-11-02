# D48 — Every page has a type

- **Status:** Accepted 2026-11-01, agreed with Eric 2026-10-24 and settled while building
  `∆Lj6`; revises D30 — the standard shell refuses a page no glob collects and no `type:`
  names, which the core still renders unknown-typed for other hosts — and D37, whose
  as-sent document value the shell no longer sends
- **Backlog item:** `∆Lj6`
- **Normative text:** `docs/shell.md` § `lem build` (step 3); `docs/language.md` § Types ›
  Ambient types

## Context

D30 let a document that no glob matches and no `type:` names compile unknown-typed and
render, and D37 had the loader bind the envelope's document value as sent for those
renders, one value per load. So a page with no record rendered its own output with no
layout — the layout is a field of the record type — its front matter checked by no
schema, and the shell loaded each such page on its own before it rendered, every load
re-sending and re-validating the whole envelope.

It was a silent fallback. Through slice 3 the limulus-dot-net port's `site.lem` collected
only `feed/*/index.md`, and 18 of its 25 pages rendered as bare markdown, with no head or
header, while `lem check` stayed clean: nothing said a glob had missed them. The shell
already refused the same failure for a whole site — a missing `site.lem` is a `SiteError`,
since without it every page would render without its layout, far from the cause — and
D43 already refused a generator with no type. The reloads cost those 18 pages about
0.35 s in all, and since D46 each one carries every body, so the cost grew with the
untyped pages times the bodies. On 2026-11-01 the port had none: of its 40 pages, on both
`lemmon-port` and `lemmon-cutover`, 31 were typed by a glob alone, 7 by `type:` alone, 2
by both, and 0 by neither.

## Decision

**In the standard shell, every page has a type.** A page — a document, or a template with
front matter (D41) — that no collecting glob matches and no `type:` names is an error at
its front matter, or at the file's start where it has none, written from `site.lem`'s name
and naming both fixes: "no glob in `site.lem` collects this page, and it names no
`type:`, so nothing checks its front matter or gives it a layout: give it a `type:`, or add
a glob to `site.lem` that collects it".

**It is refused between the two compile calls.** That is the first place the globs exist:
the first call reports them, and the second compiles each page as its record. A refused page
is left out of the second call — a page template has been compiled once already, in the
first, with the templates — and the build stops before it, as it does for a page two globs
hold. A document whose front matter never closes reads as having none, so it is sent to the
second call without a record rather than refused: the core's error about the unclosed front
matter is the one its author needs, and it stops the build there. Each page is refused once:
a generator with no type keeps D43's refusal at its `for:` key in place of this one.

**Every page is loaded once, with the site.** The per-page reload is gone, and with it the
shell's one use of the envelope's document value: the shell sends none, and every page it
renders is one `plan()` lists.

**The core keeps its unknown-typed path.** A host with no schemas — a playground, an
editor, the fidelity and fixture harnesses — still compiles a document with no record
unknown-typed and binds the document value as sent (D30, D37). The corpus keeps the core's
view: `host/unmatched/` and the other `host/` cases with no record are unchanged.

## Alternatives considered

- **A warning, rendering on.** Rejected: the same silent failure, since a check with only
  warnings passes, and the reload stays.
- **An implicit pass-through type** for a page with neither. Rejected: a page a glob
  missed would still render without the layout its author meant, as silently, and the
  shell would name a type of its own beyond `Page`, `Build` and `Code` (D30).
- **Refusing in the core, or mirroring the shell in the corpus.** Rejected: a host with no
  schemas needs the path, and the core cannot tell a missing glob from a host that has
  none.
- **Refusing at discovery, or at load.** Rejected: discovery has no globs yet, and by load
  the page has compiled unknown-typed.

## Consequences

- The shell loses the reload loop and the path for a url that is not a string, which only
  an untyped page's front matter could carry; a compiled page's record is required.
- A page that wants no layout needs a type whose layout passes its body through, and a
  layout that ignores its record warns that the parameter is never used until `∆gsV`.
- `∆V7i`'s scaffold types every page it writes.
- `∆xBO` and `∆Qr2` have no per-page reload to model.
- Every page that compiles has its `draft` validated by its type (`∆Han`); whether a
  draft the shell drops at discovery, before any glob is known, is refused when nothing
  types it is `∆Han`'s to settle.
- A refused page hides every other page's compile errors until it is typed, as a page two
  globs hold does.
- A document with `for:` and no type is told in D43's words that a document takes no
  `for:` key, and once the key is gone, that nothing types it.

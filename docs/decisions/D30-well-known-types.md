# D30 — Well-known types

- **Status:** Accepted 2026-08-28
- **Backlog item:** ∆1vS (the `from` clause, the alias form, retiring
  `lem-collection`/`lem-data`)
- **Normative text:** `docs/language.md` § Types › Well-known types

## Context

Writing limulus.net's `site.lem` ahead of the checker found `layout` used as a type that
nothing declares — the only lowercase type name in the language — and `Site` to be a type
the checker would have had to assemble from `lem-collection`/`lem-data` tags, since no
declaration of it existed anywhere.

## Decision

The core provides no complex types of its own. `Site`, `Page` and `Build` are
*well-known*: the names are reserved, a site declares each as an ordinary `lem-type`, and
the checker verifies the declaration against a core contract — `Page` needs `url: url`,
`inputPath: string`, `body: html`, `prev: Page?`, `next: Page?`; `Build` needs
`time: datetime`; `Site` needs nothing.

Everything past the contract is the site's: `Page.data` is whatever record the site
names, `Build.env` lists the environment values the site needs (a missing one is a load
error at the declaration, not an `undefined` at render), and `Site`'s fields carry the
collections and data bindings directly — `from "glob"` / `from plugin("…")` is a field
clause beside `by`, so field order is `name: Type [by …] [from …] [= default]` and
`lem-collection`/`lem-data` are retired.

A program that declares none of the three keeps the checker's unknown type for that name:
declaring is how a site buys the checking, not how it earns permission to render — which
is what lets the Phase 0 spike and a first-day site render unannotated.

There is no `layout` builtin. A string in data resolves by its expected type — record →
its `by` index, enum → a variant, `asset` → the manifest, `datetime` → parse, function
type → a project-namespace function of that type — so `layout: (Page, html): html =
article` needs nothing special, and the void `<lem-type Layout = (Page, html): html />`
alias (D21's `= expr` void marker applied to `lem-type`) is only a shorter way to write
it.

## Alternatives considered

- **Keep `Site`, `Page`, `Build` and `layout` as builtins.** Rejected: a builtin has no
  span, so "expected because of this declaration" has nothing to point at; and `Site` was
  assembled from tags rather than defined anywhere.
- **Generics on `Page`** for the site's own root record. Rejected: a user-facing generic
  bought for one field.

## Consequences

- `site`, `page` and `build` stay ambient values that cannot be redefined (D23); their
  *types* are the site's to declare.
- Until ∆iCW gives them schemas, the checker keeps `site`/`page`/`build` on an internal
  unknown type, so the spike keeps rendering while the checker lands (∆up2).
- The parser still knows `lem-collection`/`lem-data` as unsupported builtins and has
  neither the `from` clause nor the alias form until ∆1vS.

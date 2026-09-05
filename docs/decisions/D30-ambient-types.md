# D30 — Ambient types

- **Status:** Accepted 2026-08-28 as *well-known types* — `Site`, `Page`, `Build` reserved
  names a site declares and the checker holds to a core contract, `Page.data` for the
  site's own record, `from` as a field clause — and revised the same day, before any of it
  was implemented, to host-declared ambient types; the `from` clause parsed and
  `lem-collection`/`lem-data` retired 2026-09-04 (∆1vS), which chose `env("…")` for
  environment values
- **Backlog item:** ∆1vS (the `from` binding clause, retiring `lem-collection`/`lem-data`)
- **Normative text:** `docs/language.md` § Types › Ambient types

## Context

Writing limulus.net's `site.lem` ahead of the checker found `layout` used as a type that
nothing declares, and `Site` a type the checker would have had to assemble from
`lem-collection`/`lem-data` tags. The first answer reserved the three names, had the site
declare each as a `lem-type`, and had the checker audit the declaration against a
contract. Reopening it asked a simpler question of every field: who produces its value?
The shell computes `url`, `prev`, `next` and `time`; the author writes `title`;
`site.lem` binds the collections. Nothing is produced by the language itself, so nothing
needs a name the core knows.

## Decision

The core knows no type or value by name. A host declares its ambient environment when it
compiles a program — the names of the ambient values and their types, written in Lemmon
and passed like any other file, so every field has a span and the checker that validates
the site's records validates what the host sends. The standard shell ships `ambient.lem`,
declaring `Page` (`url`, `inputPath`, `prev`, `next`, `draft`) and `Build` (`time`,
`generator`), and binds `page`, `build`, and `site` — the last to the record of
`site.lem`'s top-level bindings. § Ambient types has the rule; what follows from it:

- A content type is page-backed by `extends Page`, and its front matter is its fields,
  flat: `page.title` beside `page.url`. A `from` glob's element type must extend `Page`.
- A document's type is the element type of the collection whose glob matched it; a
  `type:` front-matter key may name a subtype of it instead. The shell passes the type per
  file, and the document's unit depends on it as a compile-time fact (D25). A document
  no glob matches is unknown-typed and renders.
- `layout` is a field every page schema declares, `(T, html): html` for its own `T`,
  resolved from a string by its expected type at load like any such string. The shell
  names it as the entry field and renders a page by calling it with the page and its
  rendered body. It is neither a type nor a core concept.
- `body` is not a `Page` field: nothing has produced it before render. It is the layout's
  second parameter and the input computed defaults see.
- `url` is the one `Page` field a source may provide (`<lem-page url = … />`, or `url:`
  in front matter); there is no `permalink`. `draft` (D19) is a `Page` field, not a key.
- `from` binds on a `lem-let` head at the top of `site.lem`, void like `= expr`; the
  binding's type decides what the shell loads. `lem-collection`/`lem-data` stay retired,
  and the `lem-type` alias form the first reading added is withdrawn: nothing needs it.

## Alternatives considered

- **Builtin `Site`, `Page`, `Build` and `layout`** (the pre-D30 docs). Rejected: a builtin
  has no span, and `Site` was never defined anywhere.
- **Reserved names the site declares, audited against a contract** (this decision's first
  form). Rejected: the site retypes fields it cannot change so the checker can confirm
  the transcription; `Page.data` adds a field access to every read in a layout, for a
  need `extends` already meets; and `Site` became a type whose fields were really
  bindings.
- **Core-defined `Page` and `Build`, site-declared `Site`.** Rejected for host-declared:
  the values come from the shell, so the shell owning the declaration keeps the core
  generic (the LSP and a playground declare their own environments), gives the fields
  spans, and lets `loadData` validate the shell's own envelope with the code that
  validates records.
- **`permalink` and `draft` as reserved front-matter keys.** Rejected: both are fields of
  the type the shell owns. A source providing `url` is one clause; `draft` needs no key.
- **`layout` on the shell's `Page`.** Rejected: `(Page, html): html` cannot accept a layout
  that takes `Article` (parameters are contravariant), and a per-schema declaration needs
  no self type.

## Consequences

- `site`, `page`, `build` are reserved because the host names them (D23), not because the
  language does; a host that names none compiles a program with no ambient values.
- The shell reads two names of its own: `type`, a raw key read before a document has a
  type, and `layout`, a field required on every page schema. Everything else it knows
  about a page is a field with a span, in `ambient.lem` or `site.lem`.
- Documents are typed per file, so the shell compiles templates first, reads the `from`
  bindings `site.lem` reports, matches documents to those globs, and compiles each with
  its record (∆4tG); `plan()` fills each page's entry from the host-named field (∆RGQ).
- Environment values — the first form's `Build.env` — become a `from` source on `site`:
  `<lem-let siteUrl: url from env("SITE_URL") />` (∆1vS). The same shape as
  `plugin("…")`, a call naming a host source with a literal argument, so the core
  still names nothing: `env.SITE_URL` would have made `env` a value the core knows,
  and a `lem-env` tag a builtin for a value the host produces. The environment reaches
  the core as data like every ambient value, and the shell resolves what it holds by
  the binding's type.
- `lem init`'s scaffold supplies a blog's conventions, since the core has none (∆V7i).
- A subtype that redeclares `layout` with its own parameter type — `Article`'s
  `(Article, html): html` over a site base record's `(SitePage, html): html` — narrows a
  parameter, which ordinary subtyping forbids. The field is only ever called with its own
  record, so ∆C8d decides whether `layout` gets that rule or each leaf schema declares it.
- A feed reads other pages' rendered bodies (`a.body` for each article), and `body` is
  no longer a `Page` field. A rendered document body as a value needs a form of its own
  (∆ePP, ∆RGQ).
- The spike is untouched: its ambient fields are ad hoc and unknown-typed, which is the
  fallback this decision keeps.
- D19 and D23 gain a clause; D29's field order loses the `from` clause the first form
  added; ∆C8d and ∆iCW are reworded.

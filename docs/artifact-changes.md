# Artifact changes

The three Claude Artifacts Eric owns — "Shell & Core", "The Lemmon Language", and "From
Eleventy to Lemmon" — are the architecture record, and `docs/` is its working
distillation. When implementation moves a design point, the repo docs change in the same
pass and the artifact change is recorded here instead of made, so that the artifacts are
reconciled once (∆MlD, at the end of Phase 1) rather than edited item by item.

One entry per change: which artifact, what to change, and the decision or backlog item that
moved it. Newest last. An entry is removed when its change has been applied.

## The Lemmon Language

- **`lem-let` row, `from` sources** — add `env("…")` beside `plugin("…")` as a source form:
  an environment value the shell passes as data, resolved by the binding's type like any
  string in data. Example line for the `site.lem` block: `<lem-let siteUrl: url from
  env("SITE_URL") />`. Any mention of `Build.env` goes; environment values are a `from`
  source on `site` (D30 revision, ∆1vS).
- **Head-shape rule** — wherever the text says `= expr` is the void marker of `lem-let`,
  add that `from` past the name is its void source form; a capture named `from` stays a
  block (D21 as revised by D30, ∆1vS).
- **`lem-collection` / `lem-data`** — remove as tags; they are retired, and an author who
  types one gets "unknown tag" with no did-you-mean (D30, ∆1vS).

## Shell & Core

- **WASM API, `Diagnostic`** — the boundary diagnostic is `{path, span, severity, message,
  labels}`: `severity` is `error` or `warning`, a program is still produced when the only
  diagnostics are warnings, and `labels` are path-qualified secondary spans. Wherever the
  text says `severity` and `code` arrive with the checker as optional fields, `severity`
  and `labels` have arrived and `code` alone is deferred. `compile`'s diagnostics describe
  the files passed to that call, not the whole program (∆VNk).

## From Eleventy to Lemmon

_No pending changes._

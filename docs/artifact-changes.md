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
- **Names, stdlib shadowing** — where the text says a `lem-fn` or `lem-let` taking a
  stdlib name warns, say the warning fires when the name it displaces is the stdlib's: a
  parameter or loop binding never warns, and neither does a definition displacing a
  project function, a parameter, or an outer binding (D23, ∆e4d).
- **Types and Checking** — add `any`, the spelled unknown, to the type list, with its
  rule (assignable both ways; every operation on it is `any`, checked by the VM at
  render time). Replace the Checking paragraph with `docs/language.md` §Checking as it
  now reads: assignability (no `int → float`, provisional), the literal join rule (first
  element sets the type, `null` makes it optional, a mismatch is an error), the
  condition rule with narrowing of a plain optional local in `lem-if`/`lem-else-if`/`?:`,
  the print rule, the operator rules, the call rules (a lambda with no expected type has
  `any` parameters, provisional), and the unused rule (`lem-let` and parameters warn,
  loop bindings do not). `{K: V}` keys are always `string`. An expression-bodied `lem-fn`
  must declare its return type (invariant 11). `.` reads records, `[]` indexes maps, and
  a map miss is a render error like a list's (∆C8d).
- **Records and enums** — where the type list says records are nominal with single
  `extends`, add D32's readings: a subtype may redeclare a field only with the identical
  type (to change its default), so `layout` is declared once where its self type is and
  inherited; a bare variant name resolves only where its enum is expected (arguments,
  annotated bindings, returns, a UFCS receiver, the right operand of `==`/`!=`/`in`
  beside an enum) and wins over a local of the same name there; an enum prints as its
  variant's name and is that name as a string at run time; a record has no printed form;
  builtin type names and the `map`/`list`/`function` spellings cannot be declared; the
  type namespace is project-flat with the value namespace's duplicate rule, and a type
  may share a name with a function. `.` on a record is typed by its declaration and its
  ancestors'; `x.f(…)` calls a record field `f` as its value and is UFCS otherwise. Field
  defaults and `by` indexes are not checked until the loader (∆iCW) (D32, ∆cea).

## Shell & Core

- **WASM API, `Diagnostic`** — the boundary diagnostic is `{path, span, severity, message,
  labels}`: `severity` is `error` or `warning`, a program is still produced when the only
  diagnostics are warnings, and `labels` are path-qualified secondary spans. Wherever the
  text says `severity` and `code` arrive with the checker as optional fields, `severity`
  and `labels` have arrived and `code` alone is deferred. `compile`'s diagnostics describe
  the files passed to that call, not the whole program (∆VNk).
- **WASM API, resident compile** — where the text describes the resident `Core`, say it
  keeps one export index of every project-flat name alongside the units, offered to as
  each file compiles: a name a resident file owns stays that file's, so a colliding
  definition is refused, blamed on the file just handed in whatever its path sorts as,
  and yields no unit — reported once, by the call that introduced it, with the last good
  program still renderable (∆e5i).

- **Core pipeline, "checker"** — wherever the pipeline lists scanner → parser → checker →
  compiler → VM as passes, say the checker is not a pass of its own: the compiler's one
  walk types each expression as it lowers it (D31). `Unit::facts` carry typed signatures
  (parameter names and types, return type), and a callee whose type changed under a
  caller is a link diagnostic naming the caller (∆C8d).
- **Separate compilation, facts** — where the text lists the compile-time facts, say the
  export index has a type namespace beside the functions, that a unit records a second
  kind of fact — the declaration of every foreign type its checking reached, span-free,
  at the use that reached it — and that a declaration whose shape changed under a
  consumer is a link diagnostic naming the consumer, while a default edit leaves it
  byte-identical. The VM section's "maps in place of records" stands: records are
  checked by declaration but a record value is a map and an enum value a string until
  ∆PCK's `Value::Record` (D32, ∆cea). The boundary is unchanged.
- **WASM API, `Diagnostic`** — where the text gives a diagnostic's span as `{start, end}`,
  say it is `{start, end, range}`: byte offsets *and* the same two points as LSP
  `Position`s, zero-based with `character` in UTF-16 code units. The core computes them
  because it holds the bytes; a consumer in JavaScript would have to re-encode the source
  to derive a UTF-16 column from a byte offset. Spans in the core stay bytes (invariant 8),
  and a span is placed against the text that produced its unit — not the latest text at
  that path, which differs whenever a recompile failed (D33, ∆cE6).

## From Eleventy to Lemmon

_No pending changes._

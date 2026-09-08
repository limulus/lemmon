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
  condition rule with narrowing in `lem-if`/`lem-else-if`/`?:` (of an optional path — a
  local, or fields read off one — and of `x is T`; the Narrowing entry below has it),
  the print rule, the operator rules, the call rules (a lambda with no expected type has
  `any` parameters, provisional), and the unused rule (`lem-let` and parameters warn,
  loop bindings do not). `{K: V}` keys are always `string`. An expression-bodied `lem-fn`
  must declare its return type (invariant 11). `.` reads records, `[]` indexes maps, and
  a map miss is a render error like a list's (∆C8d).
- **Records and enums** — where the type list says records are nominal with single
  `extends`, add D32's readings: a subtype may redeclare a field only with the identical
  type (to change its default), so `layout` is declared once where its self type is and
  inherited; a variant always carries a dot — `.hero` resolves against
  the enum expected where it stands (arguments, annotated bindings, returns, the right
  operand of `==`/`!=`/`in` beside an enum) and `Placement.hero`
  needs no expectation, so a bare name is only ever a name and `Name.x` is the one
  place the type namespace is read before the value one (∆EN6 revised ∆cea's bare
  reading; never describe a bare variant or the shadowing it needed); an enum prints as
  its variant's name and is that name as a string at run time; a record has no printed form;
  builtin type names and the `map`/`list`/`function` spellings cannot be declared; the
  type namespace is project-flat with the value namespace's duplicate rule, and a type
  may share a name with a function. `.` on a record is typed by its declaration and its
  ancestors'; `x.f(…)` calls a record field `f` as the value it holds. Field
  defaults and `by` indexes are not checked until the loader (∆iCW) (D32, ∆cea).
- **Calls, and everywhere UFCS is described** — remove it. `x.f(y)` no longer resolves to
  a project function `f(x, y)`: `.` asks the receiver's type and nothing else, so
  `photo.rendition(md, jpeg)` is written `rendition(photo, md, jpeg)`. In its place, each
  builtin type carries its own methods — `heading.slugify()`, `words.first()`,
  `m.keys()`, and the collection functions the stdlib chains — reached by the receiver's
  type and named in no namespace at all, so a `lem-fn` may take a method's name freely.
  A method is called and never read; through `?.` the call is skipped when the receiver
  is null; on `any` the VM resolves the name at render time. Wherever the text argues
  that a pipe operator is redundant given UFCS, say instead that it is retired: methods
  answer the chaining, and `|>` cannot be written in a head at all (invariant 3)
  (D34, ∆495).
- **Checking, builtins and lambdas** — where the text says a builtin has a typing rule,
  add that the rule may shape each argument from the ones before it: `xs.filter(|a| …)`
  expects `(T): bool` and yields `[T]`; `xs.map(|a| …)` expects a function of `(T)` and
  no return in particular, so a lambda there takes `T` as its parameter, its body is
  inferred, and the result is a list of what it returns. A lambda's parameter is thus
  typed from the receiver's element type without annotation, and an expected `any`
  keeps its opt-out meaning everywhere. `map` and `filter` are the first two list
  methods; the rest arrive with the stdlib (D34, ∆987).
- **Narrowing, type tests, matching, record literals** — wherever the text describes
  narrowing, `is`, or `lem-match`, replace it with `docs/language.md` §Checking's
  Narrowing, Type tests, and Matching bullets and the record-literal clause of Literals:
  a condition narrows a *path* (a local, or fields read off one with plain `.`s) — an
  optional to its inner type, `path is T` to `T` — for the arm it guards, a lambda inside
  included, ending at `lem-else`, an operator, a `?.` step, or a shadowing binding; `x is
  T` names a declared record and must be able to go either way (the subject's own type on
  a non-optional, an ancestor, or an unrelated record is an error; its own type on an
  optional is a null test); a `lem-match` tries its cases in order, a type case narrowing
  the subject path and binding its `as` name, and without a default must cover an enum
  (every variant, plus `null` for `T?`) — a record or anything else needs a default, on
  the provisional reading; a `{…}` where a record is expected is that record, every key a
  declared field, omitted optionals null, an omitted required field an error whether or
  not it has a default (D35, ∆PCK). The `lem-if`, `lem-match`, and Type tests rows of the
  tag and expression tables say the same in brief.
- **Ambient types, `site`** — say the top-level bindings of the file an ambient value
  reads declare their types, `= expr` bindings included, and give the rule its name:
  every surface one file offers another is read from heads alone, never inferred from a
  body — a `lem-fn`'s signature, a `lem-type`'s fields, and these. That is what invariant
  11 rests on, and it is why such a binding never warns as unused. Where the text says a
  path narrows from a local, add that it may be rooted at an ambient value too, by its
  name (D36, D35 as revised, ∆Iau).
- **Ambient types, the entry field** — say the checker asks for `layout` where a record's
  base *is* the type the host declared its document value to be, since a field may be
  redeclared only with the same type (D32): the entry is declared once, where its self
  type is, and inherited below. Its own base may stand in for `T` — `(Page, html): html`
  on a schema extending `Page` is a layout a family of schemas shares (D36, ∆Iau).

## Shell & Core

- **WASM API, `compile`** — give the declaration its shape:
  `{ambient: {[name]: TypeName | {bindingsOf: path} | null}, document?, entry?}`, where
  `null` names a value without typing it (the Phase 0 spike's host), `document` says
  which ambient value a file's `record` retypes, and `entry` names the field a page
  renders through. Say it is resident like the units — a call that changes nothing about
  the environment omits it — and that what each unit was compiled against is one of its
  facts, so a host that rebinds a name leaves those units stale and named for recompiling
  (D25). A core given no declaration has no ambient values at all, and `page` in a
  template is an undefined name. Files carry `record` (D30, D36, ∆Iau).
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
  byte-identical. Records are checked by declaration, and a record value is tagged with
  its declared name (D35, ∆PCK — the VM entry below); an enum value is a string (D32,
  ∆cea). The boundary is unchanged.
- **WASM API, `compile`** — give the declaration its shape:
  `{ambient: {[name]: TypeName | {bindingsOf: path} | null}, document?, entry?}`, where
  `null` names a value without typing it (the Phase 0 spike's host), `document` says
  which ambient value a file's `record` retypes, and `entry` names the field a page
  renders through. Say it is resident like the units — a call that changes nothing about
  the environment omits it — and that what each unit was compiled against is one of its
  facts, so a host that rebinds a name leaves those units stale and named for recompiling
  (D25). A core given no declaration has no ambient values at all, and `page` in a
  template is an undefined name. Files carry `record` (D30, D36, ∆Iau).
- **WASM API, `Diagnostic`** — where the text gives a diagnostic's span as `{start, end}`,
  say it is `{start, end, range}`: byte offsets *and* the same two points as LSP
  `Position`s, zero-based with `character` in UTF-16 code units. The core computes them
  because it holds the bytes; a consumer in JavaScript would have to re-encode the source
  to derive a UTF-16 column from a byte offset. Spans in the core stay bytes (invariant 8),
  and a span is placed against the text that produced its unit — not the latest text at
  that path, which differs whenever a recompile failed (D33, ∆cE6).
- **VM, builtins** — where the text describes `CallBuiltin` as a pure call over the
  argument values, say a builtin runs against the VM as a host: one that takes a function
  (`map`, `filter`) invokes it re-entrantly — the VM pushes the frame and runs its loop
  again until that frame returns, inside the one instruction — and a failure inside the
  invoked function is placed at its own instruction, not the builtin's. The frame cap
  bounds the native recursion this adds (∆987).
- **VM, records and type tests** — where the text lists the value set, say a `Record`
  carries its declared type's name and its fields, read by name (`GetField` by index is
  an optimisation not taken); where it lists the ops, add `MakeRecord` (a map literal
  under an expected record) beside `MakeMap`, and say `TypeTest` reads the tag — by name
  and through the module's name → base table for a subtype — is `false` on `null`, and
  is a render error on any other value, since only a record carries a type. Say `Module`
  carries that table (`types`), built at link from the index. Where the text describes
  the `any` receiver of `x.f(…)`, say a record reads its field and everything else takes
  its runtime type's method — the agreement with the checker. Loaded data stays untagged
  `any` until the loader tags it at the boundary (∆iCW) — ambient values included, since
  which record a page carries is the record store's to know (D35 as revised, D36, ∆Iau).

## From Eleventy to Lemmon

_No pending changes._

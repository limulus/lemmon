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

- **Schemas, defaults and references** — replace the two bullets on reference fields and
  computed defaults with `docs/language.md` §Schemas as it now reads: a `by` index is an
  expression of type `{string: R}` checked where the field is declared; a default is
  checked against its field and runs at load in dependency order, seeing `self` as the
  declaring type, `path` and `body` on a page-backed record, and the ambient values but
  never the host's document value; its reads are recorded and a cycle is a link error;
  `layout`'s default names a function. Drop the bullet that checks a plain string key
  against a like-named site map by convention, and the `extra: {string: any}` opt-out
  from the extra-key rule — both were names the core would have to know (D37, ∆iCW).
- **Ambient types, `from` and computed bindings** — say a `from` binding stands only at
  the top level of the bindings file and its source is a string literal, `plugin("…")`,
  or `env("…")` with one string-literal argument; that a computed binding compiles to a
  function the loader runs in dependency order; and that at that file's top level a
  sibling is read by its bare name, the same read as `site.x` (D37, ∆iCW).
- **Schemas, the loader** — where the text says how loaded data is read, say the loader
  validates by declared type and fills an omitted field with `null` exactly when its type
  admits one — a `T?`, or `any` — the rule a record literal follows; that a whole number
  in data is a `float` where one is declared (a shell serialising JSON cannot tell `2`
  from `2.0`; the checker's `int`-is-not-`float` rule stands; one a float cannot hold
  exactly is an error); that a string is never `html`, since `raw()` is the only string
  that becomes one; and that a page-backed record is loaded from a document of its own
  only — never from a data file or nested in another document's front matter (D37,
  ∆iCW).
- **Schemas, defaults through calls** — where the entry above says a default's reads are
  recorded and a cycle is a link error, add that the reads are closed through the
  `lem-fn`s the default calls by name, however deep — a function handed `self` whole
  reads its fields on the default's behalf — so the order is exact for those too and a
  cycle through a call is refused with the call named; that a call through a value is
  not followed and a field reached through one before the loader computed it stays a
  load error at the read; and that a default or computed binding reaching the host's
  document value through a call is a link error at the field (D37 revised, ∆HF0).
- **Schemas, reference fields** — where the text says a `by` index is an expression of
  type `{string: R}`, say it names one field of one ambient value, `by site.photos`,
  declared that type: a pair the head declares, checked against the value's
  declaration, never an expression the loader runs. The loader looks a front-matter
  string up in the map as loaded, before the record's defaults run, and a `from`
  binding loads after the siblings its records index; a map in another bindings file
  or a later host value is a load error at the field (D37 revised, ∆eag).
- **Schemas, default scope** — where the entries above say a default sees the ambient
  values except the host's document value, say it sees none: `self`, and `path` and
  `body` on a page-backed record, and nothing else, directly or through a call, which
  is a link error naming the call; what a page needs of the site its layout reads. A
  `from` binding is then ordered by its records' `by` indexes alone, and a default on
  the host's own record reads nothing of the site (D37 revised, ∆eag).
- **Ambient types, load order** — say the host fulfils a `from` binding by its name
  alone, so no two bindings files may load one of the same name (a link error); that the
  host's own values load first, in the host's declaration order, then each bindings
  file, so a default on a host-typed record may read the host's other values and nothing
  a bindings file loads (a load error at the step, before it runs); that once a value has
  failed no later plan runs, so the one error is the author's; and that the host's
  document value is bound as sent, untyped, since the envelope carries it only for a
  document no collection matched (D37, ∆iCW).

- **Types, the stdlib's values** — where the text lists the builtin types, say what each
  one now *is*. A `datetime` is an instant plus the offset it was written in, printing
  RFC 3339 in that offset and comparing by the instant alone, with no time-zone database
  and a named zone refused rather than read for its offset; a `duration` is whole seconds
  with no printed form; a `url` is text percent-encoded once, idempotently, with `text()`
  to compose another from; an `asset` is a path resolved against the manifest the host
  declares at `compile`. Add the per-receiver method inventory D34 deferred — string,
  list, map, int, float, datetime, duration, url, html — and say that `slugify` folds
  Latin marks rather than transliterating, carrying a script it cannot fold through
  unchanged, and that a character is a grapheme cluster throughout (D38, ∆Q6L).

- **Names, the ambient inventory** — where the text says the stdlib's ambient functions
  are `raw`, `datetime` and `url` with others to come, say the namespace is closed at
  five: `raw`, `datetime`, `url`, `asset`, `uniqueId`. Everything else the stdlib offers
  is a method on the type it works on and occupies no namespace (D34, D38).

- **Checking, printing and calls** — where printing is described, add `datetime`, `url`
  and `asset` to what prints and `duration` to what does not. Where a builtin's typing
  rule is described, say that `datetime("…")` and `.format("…")` are read a second time
  at compile when their argument is a literal, and that `asset("…")` takes only a literal
  and is answered entirely at compile — it emits no call (D10, D38).

- **Schemas, the computed example** — the `readingTime` line is
  `readingTime: duration = body.readingTime()`, a method on the document's `html`, not an
  ambient function over it (D34, ∆Q6L).

- **Fields, the did-you-mean** — a misspelled method *read* now names the call it meant
  (`s.slugfy` → "did you mean `.slugify()`?"), as a misspelled call already did; a call
  lists the receiver's whole inventory only when no name is near (∆eHR).

## Shell & Core

- **WASM API, `compile` and `loadData`** — a file may carry `recordSpan`, the byte
  offsets of its `type:` key's value, which a diagnostic about the record then points
  at. Say what `loadData` grows into: `ambient` as bare JSON by any host-bound name,
  and `sources` fulfilling each `from` binding by name (`{path, value, spans}` per
  document of a glob, one for a data file, `{value}` for a plugin or env source), with
  `spans` a field-path → boundary-span table the host computes from the YAML it
  parsed, and the `type:` key stripped by the shell (D37, ∆iCW).
- **VM, the module** — where the text gives `Module`'s shape, add the loader's plans:
  `records` (each record's default and `by` functions in dependency order), `bindings`
  (one plan per bindings file, its bindings in load order), and `exports` (function
  name → index). A default, `by` index, or computed binding is an ordinary function of
  its unit that the loader invokes; `Op` gained nothing (D37, ∆iCW).

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
  its runtime type's method — the agreement with the checker. The loader tags what it
  validates at `loadData`; what it leaves as sent — an untyped host's values, the host's
  document value where no loaded document stands in for it — stays untagged `any`, and
  which record a page carries is the store's to know (D35 as revised, D36, D37, ∆Iau,
  ∆iCW).
- **VM, the module's tables** — where the text (and the entry above) says `Module`
  carries a name → base table (`types`), say instead it carries the project's
  declaration table (`declarations`, each with its file), its project-flat functions by
  name with their types (`exports`), and per document its record and rendered prose
  (`documents`); `TypeTest` walks `extends` links in the declaration table, the same
  table the loader validates against. Say `Diagnostic` and `Label` carry an optional
  `range` — the host's positions, set by the loader for a file the host holds — and
  the boundary places such a span by it rather than by an index (D37, D35 revised, ∆iCW).
- **WASM API, `loadData` and `render`** — where the class sketch and the boundary rules
  describe `loadData`, say its input is `{ambient, sources?}`: `ambient` bare JSON by
  the name the host bound each value, `sources` fulfilling each `from` binding by name
  with `{value, path?, spans?}` entries (a list for a glob) whose `spans` map field
  paths to complete spans the host computed; that the core validates, defaults, tags,
  and keeps the result resident as the store every render binds; that a load reporting
  an error leaves the previous data bound, as a failed compile leaves the program; and
  that an envelope the program cannot be loaded from throws. Say the store lives with
  the linked program — a compile that installs a new program drops it, and `loadData`
  runs against the program that linked, under the declaration it linked with — and
  that `render` binds the host's document value to a loaded document's own record from
  the store, and to the envelope's value, unvalidated, where no loaded document stands:
  a template, or a document no collection matched — one value for every such render
  until `plan()` (∆RGQ) gives each page its own (D37, ∆iCW).

- **VM, the module's plans** — where the text gives `RecordPlan` as a record's ordered
  steps, add `indexes` beside them: each a field with the ambient value and field its
  strings resolve through; a step is a default only, with no kind (∆eag).

- **WASM API, `compile` and the manifest** — where `HostDecl` is given as
  `{ambient, document?, entry?}`, add `assets?: {[logicalPath]: url}`: the manifest the
  host's build produced, its values the URLs assets shipped at rather than content
  hashes, since the core does no path arithmetic. Say it rides on the declaration and not
  on `loadData`'s envelope because `asset("…")` is answered while compiling, and that a
  rebuild re-hashing an asset is therefore a change to the declaration (D10, D38).

- **Build pipeline, steps 2 and 4** — step 2's esbuild manifest is handed to `compile` on
  the host declaration rather than to `loadData`; step 4 resolves `[asset]` fields against
  that same manifest, reporting a miss at the YAML position the shell sent (D38).

- **Separate compilation, the facts a unit records** — add the manifest entries a file
  resolved to the signatures, type references and ambient bindings it already lists. A
  file that named an asset is recompiled when that asset's URL changes, and a file that
  named none is not (D25, D38).

- **VM, the stdlib's values and the id counter** — `Value` carries `Datetime`, `Duration`,
  `Url` and `Asset` beside the rest, and stays twenty-four bytes: a datetime is packed
  into sixteen rather than held as a timestamp beside an offset. Ordering lives in
  `Value::order` so one rule serves `lt` and `sortBy`. Native re-entry has a cap of its
  own, separate from the frame count, because a builtin that calls back into the VM costs
  the machine's stack rather than the heap. The VM carries a per-prefix counter behind
  `uniqueId`, cleared at the start of every render (D38, ∆Q6L).

- **Crates** — `jiff` (`alloc` only: no clock, no system zone, no tzdb),
  `percent-encoding`, `unicode-normalization` and `unicode-segmentation`. Say that
  transliteration was measured and rejected — `deunicode` cost 479 KB of the artifact and
  `any_ascii` 582 KB — and that the `url` crate was rejected for refusing relative
  references (D38).

## From Eleventy to Lemmon

_No pending changes._

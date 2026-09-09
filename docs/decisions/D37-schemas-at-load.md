# D37 — Schemas at load

- **Status:** Accepted 2026-09-08 (∆iCW)
- **Backlog item:** ∆iCW
- **Normative text:** `docs/language.md` § Schemas, § Ambient types;
  `docs/architecture.md` § WASM API (`loadData`, as it grows), § VM

## Context

The content model was typed before anything fulfilled it. `lem-type` declared fields
with defaults and `by` indexes that were parsed and kept, `site.lem` declared `from`
bindings whose types reached `site` while the file compiled to a placeholder error, and
`loadData` stored whatever JSON it was handed and validated nothing. D35 and D36 both
deferred tagging loaded data to this item, because which record a page carries is per
page and only a record store knows it. Three questions had no answer in the docs: when
and by whom a default runs, since it reads `body` and `site` and a template has neither;
where a loader diagnostic points, since the YAML it is about is text the core does not
hold (D33); and what `loadData` receives, which the spike's `{ambient: {site?, page?,
build?}}` had left open by design.

## Decision

**Defaults, `by` indexes, and computed site bindings compile to functions.** Each is an
expression the loader evaluates rather than a render, so each is lowered into a function
of its declaring unit — typed where it stands, against the field or binding's declared
type — and the compiler records what it reads: sibling fields through `self` or by bare
name, and fields of ambient values. A default's scope is `self`, the record being built
as the declaring type; on a record that extends the host's document type, `path` and
`body` (the document's rendered prose) as well; and the ambient values, except the host's
document value, which a default may not read since it is bound to the record only once
the record is built. A `by` index is an expression of type `{string: R}` for the field's
record `R`, checked where it is declared. No fact carries any of this: a default is not
part of a type's shape (D32), so a consumer's unit is byte-identical whatever the
default says (invariant 11). The reads recorded are the ones written in the function
itself — what a called `lem-fn` reads is its own body, which invariant 11 keeps out of
reach — so the order is exact for lexical reads and silent about reads through a call; a
step that reaches a field the loader has not computed is a load error at the read
(invariant 10). A field's steps come from the nearest declaration in its chain that
gives it any: a redeclaration changes the default (D32), and one that gives none leaves
the ancestor's in force.

**The link step orders the work and refuses a cycle.** From the recorded reads it builds,
per declared record, the steps of its nearest declarations in an order that computes
every field before the fields that read it, and for the bindings file the bindings in an
order that has each computed one after the siblings it reads and each `from` binding
after every ambient field the records its value may hold — through `extends` and through
field types — read while they load. A cycle, a self-read included, is a link error at the
field or binding, and every cycle is reported, not the first. The loader then runs each
plan front to back and reasons about nothing.

**The bindings file is where a `from` binding stands.** Its source is a string literal (a
glob or a data file, told apart by the binding's type), `plugin("…")`, or `env("…")` with
one string-literal argument; the kind and literal are recorded for the host. A `from`
binding anywhere else is an error naming the file the host reads bindings from. The host
fulfils a `from` binding by its name alone, so the link step refuses one name loaded for
two bindings files. At the
top level of that file a sibling binding is read by its bare name, which is the same read
as `site.x`; a `lem-fn` or a default in the file reads `site.x` like every other file.

**The loader validates by declared type and tags what it builds.** A JSON object under a
record type becomes a `Value::Record` of that type with every declared field — an
omitted field `null` exactly when its type admits one (a `T?`, or `any`), which is the
rule a record literal follows too; an omitted field of any other type without a default
an error; an unknown key an error with no opt-out. A string resolves by its expected
type: an enum's variant, a project function of an assignable type where a function type
is expected (`layout: article`), the value a `by` index maps it to where a reference
field is — and never `html`, since `raw()` is the only string that becomes one
(invariant 7). A whole number is a `float` where one is declared, because a shell
serialising JSON cannot tell `2` from `2.0`; the checker's rule that an `int` is not a
`float` stands, and a whole number a float cannot hold exactly is an error. A document in
a glob binding is validated as the record it was compiled as, which must extend the
binding's element type, so `is` and `lem-match` reach loaded data; a page-backed record
is loaded from a document of its own and nowhere else — not from a data file, and not
nested in another document's front matter. The host's own ambient values are validated
against what the host bound them to, except its document value: the envelope carries
that only for a render no loaded document stands in for — a document no collection
matched, whose `page` is unknown-typed (D30) — so the loader binds it as sent, and
`render` binds the document value from the store for a loaded document, else from the
envelope. Loading proceeds in one order — the host's own values in its declaration
order, then each bindings file — and no function runs over a value that is not loaded
yet or that failed: a default on a host-typed record that reads a bindings value, or a
computed binding that reads a later file's, is a load error at the step, and once a
value has failed the plans after it are not run, so the one error reported is the
author's and not a fault downstream of it.

**Positions come from the host, as complete boundary spans.** `loadData` receives, per
loaded value, a table from field path to `{start, end, range}`. The host owns the text of
every file data comes from — YAML front matter, JSON data files — and its parser hands it
offsets and positions; the core stores the table with the record and reports against it
verbatim, falling back from a nested path to its nearest ancestor and then to the value's
own span. A source with no file behind it — a plugin's data, an environment value —
reports at the `from` binding in the bindings file; the host's own values at the field's
declaration.

**The envelope grows additively.** `ambient` stays bare JSON by the name the host bound,
which an untyped host passes through as before; `sources` fulfils each `from` binding by
name — a list of `{path, value, spans}` for a glob, one such for a data file, `{value}`
for a plugin or env source. The shell strips the `type:` key it consumed to choose a
document's record, so the core never sees it.

## Alternatives considered

- **Running defaults at compile time, or in the VM at render.** Rejected: a default reads
  `body` and `site`, which a template cannot supply and a render should not compute;
  D35 had already settled that a literal never fills a default.
- **Inferring order at load from the values as they arrive.** Rejected: the reads are
  lexical and known at compile time, so a cycle can be refused before any data is loaded,
  and the loader stays a walk over plain tables.
- **Ordering at compile time, per file.** Rejected: a subtype's inherited default is
  another file's body, which a unit may not depend on (invariant 11); the link step is
  the first place every unit is in hand.
- **The core parsing YAML.** Rejected: D30 has the shell read `type:` before a document
  has a type, the shell already parses front matter to match globs, and the same JSON
  path serves data files, plugin hooks, and env values.
- **Ranges only from the host, byte offsets computed by the core.** Rejected: it works
  for a document the core holds an index for and not for a data file it never sees; one
  shape for every source is worth a byte conversion in the shell.
- **Naming the `type:` key in the host declaration** so the loader can exempt it.
  Rejected as more machinery than stripping a key the shell already consumed.
- **The naming-convention check** in §Schemas, where `author: string` was to be
  looked up in `site.authors` by name. Dropped: `author: Author by site.authors` says it
  explicitly and typed, and the convention was a name the core would have to know (D30).
- **An `extra: {string: any}` opt-out from the extra-key rule.** Dropped: a field name
  the core knows, for a need the port's schemas do not have.
- **Tolerating a declared-but-absent field in the VM** (D35's open question). Rejected:
  the loader fills omitted optionals as the literal does, so both producers agree and
  `field`/`equals` keep assuming completeness.

## Consequences

- `Unit` carries `defaults` and `bindings`; `Module` carries `records`, `bindings`, and
  `exports`, all plain data. `Op` gained nothing: a loader-run function is an ordinary
  function the loader invokes — with the arguments its own declaration takes, so a
  default inherited from a record above the document type runs without the document.
- `Module` carries the project's declaration table and, per document, its record and
  rendered prose; the closed ancestry table D35 described is gone, and `TypeTest` walks
  the same table the loader validates against (D35 revised).
- A load diagnostic about a file the host holds carries the host's positions with it
  (`Diagnostic::range`), and the boundary places such a span by them rather than by an
  index it does not have.
- A host that binds a name to a type no file declares, carries a name twice, or fulfils
  a source the program has no `from` binding for is a caller bug — an `Err` with no data
  to point at — not a diagnostic.
- The bindings file's top-level `lem-let`s are no longer locals of its entry: reading one
  by bare name compiles to a read of the ambient value's field, so rendering the bindings
  file shows whatever `site` was bound to.
- A `from` binding is admitted to the language; `∆1vS`'s placeholder is gone. The two
  statements dropped from §Schemas leave `artifact-changes.md` entries.
- `datetime`, `duration`, `url`, and `asset` fields accept a string and keep it as one
  until ∆Q6L gives those types values; `language.md` says so where the loader is
  described.
- D33's open question — the index a loader diagnostic needs — is closed by the host
  supplying the span; D35's tagging clause and completeness question are closed here.
- The `host/` corpus loads its documents' front matter through the loader, parsed with
  positions by a YAML crate in the harness, each case the sole member of the glob it
  matches.

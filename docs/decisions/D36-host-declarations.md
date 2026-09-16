# D36 — The host declaration and the surfaces a file offers

- **Status:** Accepted 2026-09-07 (∆Iau); revised 2026-09-14 (D43) — the declaration
  also names the field a page's url is in and the two neighbour fields
- **Backlog item:** ∆Iau
- **Normative text:** `docs/language.md` § Ambient types (including **Declared, not
  inferred**), § Tags (`lem-let` row); `docs/architecture.md` § WASM API

## Context

D30 settled that the core knows no type or value by name: a host declares its ambient
environment in Lemmon, compiled with the program. It left the shape of that declaration,
and one question the implementation could not avoid — where the fields of `site` come
from. `site` is the record of the bindings file's top-level `lem-let`s, and a `from`
binding already declares its type (∆1vS), but a computed `= expr` binding did not.

Nothing about typing such an expression is hard; the difficulty is *when* its type
exists. Compilation runs in two phases: `prepare` reads every file's signatures and
declarations from heads alone and offers them to the export index, and then each file is
lowered, typed as it is lowered (D31). An inferred binding type would only exist in the
second phase, for the file it is in — after every other file needed it.

## Decision

**The declaration is `{ambient, document?, entry?}`.** Each ambient name is bound to a
declared type, to the top-level bindings of a file (`{bindingsOf: path}`), or to
`null` — named without a type, which gives the value the checker's unknown type and is
how the Phase 0 spike renders. `document` names the ambient value a file's `record`
retypes; `entry` names the field a page renders through. Nothing else is needed: the
type a page schema extends is the one the host declared its document value to be.

**The declaration is resident, and a fact of every unit that read it.** `compile` takes
it optionally, replacing what the core holds; a unit records what the host bound each
ambient name to where it first read it, and the link step reports a unit the host has
rebound the name under, as it reports a changed signature (D25). A host that names
nothing compiles a program with no ambient values, so `page` is then an ordinary
undefined name — the reservation rule (D23) follows the host, not the core.

**`site`'s fields are a synthesized declaration named by the bindings file's path.**
Collected in `prepare` like any other declaration, offered to the index, and consumed by
readers as an ordinary type fact — so `site.lem`'s bindings changing leaves its readers
stale through the machinery that already exists, and a name no `lem-type` can spell
cannot collide with one.

**Declared, not inferred: every surface one file offers another is read from heads.**
The bindings file's top-level `lem-let`s declare their types, exactly as an
expression-bodied `lem-fn` declares its return. This is not a new rule but the existing
one reaching a third kind of export, and it is what invariant 11 rests on. A binding at
the top level of that file is therefore never "unused" — every reader of the ambient
value reads it — while a `lem-let` anywhere else is a local and infers as before.

**A record is checked as far as the core can see it.** The named type must be declared,
be a record, and extend the type the document value has; the diagnostic points at the
document's front matter, since that is where a `type:` key naming it stands. A document
the host gave no record reads the document value unknown-typed (D30), while a template
with no front matter keeps the declared type (D41, D43).

**Tagging loaded data stays with the loader (∆iCW), revising D35.** Ambient values are
typed for checking here and untagged at run time, so a type test over one is still the
render error D35 specifies.

## Alternatives considered

- **Inferring a computed binding's type** by lowering the bindings file before its
  readers and carrying the result on its unit. Rejected: it makes one file's exported
  surface depend on its own body, so a type error there changes how every other file is
  checked (or silently widens them to `any`); it needs cycle detection for `site.x` read
  inside `site.lem`; and it adds a compile-ordering phase to both entry points of a
  compiler that is deliberately per-file. Rust requires explicit signatures for the same
  reason, and TypeScript retrofitted `isolatedDeclarations` to get back to it.
- **An annotation only where inference cannot reach.** Rejected: whether a binding needs
  one would depend on what its expression turns out to be, which phase 1 cannot know —
  so the ordering comes back anyway, for a smaller saving.
- **A structural record type for `site`.** Rejected: types are nominal (D32), and a
  synthesized declaration reuses the index, the staleness check, and every field rule.
- **Naming the page schemas' base in the declaration.** Rejected as redundant: it is the
  document value's declared type.
- **Rewriting the declaration per file** so that a file's view carries its record.
  Rejected after it broke the base a page schema extends: the file's view answers what a
  name means *here*, and the declaration keeps answering what the host declared.
- **Tagging ambient values from JSON in this item** (D35's original pointer). Rejected:
  which record `page` carries is per page, and only the record store ∆iCW builds knows
  it; tagging without validation would be half a loader in the wrong item.

## Consequences

- `Host` and `Ambient` are part of the VM's public API; `compile`, `prepare`,
  `compile_unit`, and `link` all take the declaration, and `SourceFile` carries a
  `record`.
- A file compiled as a record binds the document value to it, so `link` compares a
  unit's ambient facts against the host *as that file saw it* — otherwise every document
  would look stale.
- The redefinition diagnostic is now "`page` is bound by the host and cannot be
  redefined": it is the host's name, not the shell's.
- `tests/corpus/host/` is a fourth corpus group, compiled against the environment files
  in `host/environment/`; `render/` and `document/` keep an untyped host, which stays a
  real configuration.
- ∆iCW inherits the tagging clause of D35 along with validation, defaults, and
  references.
- `docs/artifact-changes.md` carries the WASM API's declaration, the ambient-type rules,
  and the moved tagging pointer for ∆MlD.

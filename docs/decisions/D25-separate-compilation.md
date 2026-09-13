# D25 — Separate compilation

- **Status:** Accepted 2026-08-23; revised 2026-09-12 (∆Hbw) — reference-index ids are
  not a fact: a string literal in a template is a string, and a record named by id is
  read from its map at render; extended 2026-09-13 (∆BfR) — the grammars a document's
  code was highlighted against are a fact (D40)
- **Backlog item:** ∆TNT (made it real); ∆RGQ (reported facts across the boundary);
  ∆Hbw (the revision)
- **Normative text:** `docs/architecture.md` § Separate compilation

## Context

`lem dev` must not recompile the layouts a document depends on when that document's
content is edited. The spike's compiler produced one `Module` with one constant pool for
the whole site, so any edit meant recompiling everything — and ∆TNT was about to freeze
the WASM boundary's contract around whatever shape `compile` took. Retrofitting per-file
units under a shipped boundary would have changed that contract.

## Decision

The compile unit is the file. Each `.lem`/`.md` compiles to a **unit** — file-local
constant pool (document prose included), cross-file references symbolic — and a cheap
link step over plain data joins units into the `Module` the VM runs and `exportModule`
serializes.

A unit is a function of the file's text plus the **compile-time facts** it consumed:
imported signatures and types, the host's ambient bindings, the document's record,
asset-manifest entries, and the grammars a document's code was highlighted against
(D40). The compiler reports those facts, so the shell's cache key
`hash(text) + hash(facts)` is built from evidence, not guessed. _(Revised by ∆Hbw: the
first reading listed the ids of any reference index a string literal resolved through;
the alternative below records why none is.)_

No file's bytecode may depend on another file's body (invariant 11). The checker stays
whole-program — the program is small — but runs over units and is not a compile-unit
input, so re-checking never requires recompiling.

## Alternatives considered

- **Keep the spike's one-pool `Module`.** Rejected: every edit recompiles every file, and
  a boundary shipped on that shape would have to change its contract to fix it later.
- **Resolving a string literal to a record at compile time**, the ids as facts, so a
  typo'd photo id is a compile error. Rejected at ∆Hbw: `compile` runs before
  `loadData`, and which maps are indexes is itself a product of compiling `site.lem`,
  so the core has no map to check a literal against without a pre-compile protocol or
  a second pass; no syntax names the index a parameter would resolve through, as `by`
  does for a field; and the fact bookkeeping exists only to recompile documents that a
  render-time lookup never needs recompiled. Its one benefit — an editor underlining a
  typo'd id — can be an analysis over loaded data later, with no fact.

## Consequences

- Invariant 11 is a test, not a claim: editing a callee's body leaves the caller's unit
  byte-identical.
- A content edit re-runs comrak and codegen for that one document; its layouts and
  components are cache hits. A new photo id changes no existing unit: a record named by
  id is read from its map at render, and no unit's facts name an id. Which pages then
  re-render — a render-time read of the map, or a load-time `by` resolution — is a
  dependency question that is not yet answered (∆RGQ).
- The VM never changed: `link` concatenates pools with offsets into the same `Module`
  shape it always ran.
- ∆TNT's retrospective: `Unit::facts` was written and tested for content before anything
  consumed it — a field nothing reads is not a fact, it is a comment. ∆RGQ makes the
  reported facts cross the boundary and the serialized form first-class.

# D25 — Separate compilation

- **Status:** Accepted 2026-08-23
- **Backlog item:** ∆TNT (made it real); ∆RGQ (reported facts across the boundary)
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
imported signatures and types, reference-index ids, asset-manifest entries. The compiler
reports those facts, so the shell's cache key `hash(text) + hash(facts)` is built from
evidence, not guessed.

No file's bytecode may depend on another file's body (invariant 11). The checker stays
whole-program — the program is small — but runs over units and is not a compile-unit
input, so re-checking never requires recompiling.

## Alternatives considered

- **Keep the spike's one-pool `Module`.** Rejected: every edit recompiles every file, and
  a boundary shipped on that shape would have to change its contract to fix it later.

## Consequences

- Invariant 11 is a test, not a claim: editing a callee's body leaves the caller's unit
  byte-identical.
- A content edit re-runs comrak and codegen for that one document; its layouts and
  components are cache hits. A new photo id invalidates only the documents whose facts
  named the photo index, never the layouts.
- The VM never changed: `link` concatenates pools with offsets into the same `Module`
  shape it always ran.
- ∆TNT's retrospective: `Unit::facts` was written and tested for content before anything
  consumed it — a field nothing reads is not a fact, it is a comment. ∆RGQ makes the
  reported facts cross the boundary and the serialized form first-class.

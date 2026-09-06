---
id: ∆e4d
completed: 2026-09-06 10:18:51 -07:00
---

## Backlog item

> - ∆e4d Single name resolver — a `Scope` with one `bind(name, kind, span)` (definition,
>   parameter, loop binding, capture) and one `lookup`, applying ambient rejection,
>   duplicate detection, and stdlib shadowing in a fixed order, shared by codegen and the
>   checker. The five sites in `crates/lemmon-vm/src/compile/mod.rs` each re-apply those
>   rules and each has skipped one (∆c8h, ∆TNT, ∆VNk); ∆C8d adds a type namespace there, so
>   this goes first

## Planning

The item came out of a question at the end of the `[[∆VNk]]` session — what one thing about
the codebase would you change — and was evaluated before it was written down: the code had
the shape described, and the journal held the three bugs the shape had produced. It opened a
standing Refactors epic above the initiatives, with the rule that an item there is absorbed
by the work below it if that work is claimed first.

Planning confirmed the design against every caller and corrected it twice. Captures are
recorded in every frame a lookup crosses, not only the innermost, so they could not go
through the general `bind`; a frame-level routine makes them. And the resolver takes a name
and a span rather than a syntax token, so its rules can be tested without parsing a template
— which is the property the item was for. One rule was refined on the way: the
stdlib-shadowing warning fires when the meaning a definition _displaces_ is the stdlib's,
generalising the `[[∆VNk]]` fix (a project function already shadowed it) to parameters and
outer locals. `docs/language.md` §Names says so, and the ledger carries it for the Language
artifact.

## Refinement

The build landed in one pass with the corpus byte-identical and 26 resolver tests. The
review (`/code-review medium`, Opus finders and verifiers) confirmed seven findings and
refuted two; all seven were fixed, and one of the fixes reshaped the module.

- **One `bind` was two return shapes in one type.** `collect_exports` read only the admitted
  half and codegen only the slot; folding them into a `Bound` enum had also narrowed a
  file's function count to sixteen bits, a fresh spanless panic. The split into `bind_local`
  (a slot, refused or not) and `bind_function` (admitted or not) removed the enum, the
  narrowing, the kind-to-scope match written three times, and a coverage-only assertion
  together.
- **A function's shadow warning was raised before anything could be displaced.** Exports are
  collected before the table exists, so a `lem-fn raw` that lost to another file's got the
  duplicate error _and_ the warning. The warning now fires when the unit compiles, and only
  when the table kept this file's definition; a two-file compile test pins it.
- **A span stored on every local was read by nothing**, and `resolve` had grown a span
  parameter only to fill it. Removed — then put back with a reader, when Eric took the
  follow-up of giving repeated locals the "first bound here" label function duplicates
  already carried. Captures moved out of scope zero into their own list so every scoped
  binding carries its binder's span and a capture never needs one.
- A debug assertion that two stacks moved together could not fire, since one private
  push/pop pair moves both; the shadow check walked every frame and the table before asking
  the one-entry stdlib; a doc comment said "ledger … caught against".

Two things were refuted and are worth keeping. A rejected local's store into a slot no
reference can reach never renders: an error compile yields no unit anywhere, the wasm
resident set included. And a test whose second half asserts a warning is not misnamed when
the warning is the outer binding's and the assertion is that no second one appears.

Deferred to `∆e5i`: the project's export index is still derived by the signature table and
again by `link`, with the wasm boundary ordering resident units before incoming ones so
duplicate blame lands on the file just compiled — the same two-views shape one layer up.

## Retrospective

**A field with no reader is a bug waiting for its first reader.** The local span was stored
so a label could use it later, and the one use it anticipated would have pointed at the
wrong bytes for a captured name. The repo's rule about recorded facts — the test that
matters is where consuming it changes an outcome — caught it in review; writing it would
have caught it earlier. Add the reader with the field or leave the field out.

**A rule stated in words should be tested across files, not only within one.** "One
diagnostic per binding, first rule wins" held for every single-file case and every test,
because the one place a second diagnostic could come from was outside the module that
promised it. The corpus harness compiles one file at a time; a rule about the project needs
a compile test with two.

**Refactor plans should name their scope's edges.** The plan said the table and the linker
stay merges of already-admitted names, and that was right for this item — but saying it made
the leftover duplication visible enough to become the next item the same day, instead of a
surprise in the checker.

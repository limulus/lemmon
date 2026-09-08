---
id: ∆Iau
completed: 2026-09-07 21:20:43 -07:00
---

## Backlog item

> - ∆Iau Host-declared ambient types — `compile(files, host)` taking the host's `ambient.lem`
>   and a per-document `record` (D30, docs/architecture.md §WASM API): `page`/`build` typed
>   from the declaration, `site` from `site.lem`'s top-level bindings, the record as a
>   compile-time fact, the entry field checked on page schemas. Until then every ambient
>   value is `any`; needs: ∆cea

## Planning

Four questions went to Eric. Three he took as recommended: tagging loaded data stays with
the loader ([[∆iCW]]) rather than landing here, since which record a page carries is per
page and only the record store that item builds knows it — a revision to D35, whose text
had pointed at this item; a new `host/` corpus group rather than typing the whole `render/`
group, so the untyped host stays covered as the real configuration it is (the Phase 0
spike's, and any document no glob matched); and one item rather than a split.

The fourth he reopened twice, and that is where the item's shape came from. `site` is the
record of the bindings file's top-level `lem-let`s, and a `from` binding declares its type
already ([[∆1vS]]) — but a computed `= expr` binding did not, and I proposed requiring one.
He asked first whether that was a real constraint, "because I am not understanding why an
expression assignment type can't be inferred". It is not a hard constraint: inference works
if the bindings file is lowered before its readers and carries the result on its unit. What
it costs is error isolation (a type error in one body then changes how every other file is
checked), a cycle to detect, and a compile-ordering phase in a compiler that is per-file on
purpose. I had called it a blocker; it is a cost, and saying so plainly was the right
correction.

He then asked the better question: "is there a bigger systemic issue or incorrectly
accepted rule at play?" There is not — but the rule that answers it had never been written
down. D31 makes typing part of lowering, and D25 makes a unit a function of its own text
plus the facts it consumed, so every surface one file offers another has to be readable
from heads alone. That already governs `lem-fn` signatures and `lem-type` fields; the
bindings file is simply the third kind of export, not a new rule. Naming the principle —
**Declared, not inferred** in `docs/language.md` §Ambient types, cited from both the
`lem-fn` return rule and the new one — became part of the item, and D36 records it. Rust
requires explicit signatures for the same reason; TypeScript retrofitted
`isolatedDeclarations` to get back to it.

## Refinement

**The file's view of the host broke the declaration.** A file compiled as a record reads
the document value as that record, and the first shape of that was a cloned `Host` with the
`page` entry rewritten. That also rewrote what `document_type()` answers, so the base a
page schema extends became whatever one file was compiled as. A test written straight from
the doc comment's own promise — "the declaration itself is never rewritten" — failed on the
first run. The view now carries the file's binding in a private field that only `binding()`
reads, and `document_type()` answers from the declaration.

Three smaller things surfaced while building. A top-level binding in the bindings file is a
field every reader of the ambient value reads, so the unused-binding warning is wrong there
(`BindingKind::Exported`, tested on both halves). A `from` binding still compiles to
[[∆iCW]]'s placeholder, so the corpus fixture's `site.lem` uses computed bindings and a
Rust test covers what matters — the binding's type reaches `site` from the head even while
the file it stands in yields no unit. And `rules::field` said "a `Article` has no field";
the article went, matching the wording the `by`-key rule already used.

Reading blessed output caught two double diagnostics, both the [[∆3oX]] shape: an undeclared
record was reported by the record check and again by the first read of `page`, and an enum
record likewise. Recording the ambient binding where the record is — with `any` once the
name has been refused — makes one mistake cost one diagnostic.

`/code-review high --fix` confirmed five findings, two of them real compile bugs, and both
were about a host that is not the standard shell. A host whose own `Page` declares the
entry field — the natural place, and what D30's "declared once where its self type is"
invites — made every direct subtype an error, because the check read only a record's own
fields. And a host that *names* a document value it never *bound* had the record check
record a fact for an unbound name, which `link` then read as a dropped binding: "the host
no longer binds `page`" on a first compile, and no module at all. The other three were the
declaration installed at the boundary before the call's own caller-bug check could refuse
it, a blank line that had turned D36's row into a paragraph of pipes, and a corpus harness
reading `type:` past an unterminated front matter.

## Retrospective

**Every test used a host shaped like the shell, and both real bugs lived outside that
shape.** The item's whole premise is that the core knows nothing by name, and the fixtures
still all declared `Page`, `Build`, `site`, `layout` in the arrangement
`@limulus/lemmon-shell` will ship. The failure modes were a host that puts the entry field
one level up and a host with a name mismatch between `document` and `ambient` — neither
exotic, both invisible to a corpus written from one host. When a feature exists to serve
callers the repo does not contain, the tests owe it a caller that differs.

**A doc comment written ahead of the code is a test waiting to be written.** "The
declaration itself is never rewritten" was aspiration when I typed it; turning that
sentence into an assertion failed immediately and produced a better design than the one it
described. Worth doing deliberately: when a comment promises an invariant, write the
assertion before believing it.

**Double diagnostics show up in blessed output and nowhere else.** Both were invisible in
unit tests, which assert one message at a time, and obvious in a `.html` trailer. That is
the second time the corpus has earned its keep this way, and an argument for writing the
corpus case before the unit test when a rule can misfire twice.

**The premise question beat the options.** I offered a choice between two designs; the
answer that mattered came from asking whether an accepted rule already covered the case.
The durable part of this item is a paragraph of prose naming a rule that was already true.

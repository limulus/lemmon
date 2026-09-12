---
id: ∆Hbw
completed: 2026-09-12 15:45:08 -07:00
---

## Backlog item

> - ∆Hbw Document mode, settled rules — a block in a `.md` is a template through its close,
>   body included (D39, revising D07), and reference-index ids are not a compile-time
>   fact: a string literal is a string, and a record named by id is read from its map at
>   render (D25 revised). Docs, the register rows, and corpus cases pinning both; the
>   sub-document and compile-time-resolution readings are recorded as rejected

## Planning

Eric asked for the plan to justify the item for a reader without much knowledge of the
codebase, and the evidence answered it. The item as filed promised two things: a capture
body inside a `.md` file rendered as markdown (§Document mode item 2), and a string
literal where a record is expected resolved through a reference index at compile time,
the ids reported as compile-time facts (item 6, D25). limulus.net's markdown — the first
production site — has six component calls, all void tags naming a photo by id; ten
front-matter photo references already resolved through `by site.photos` at load (∆iCW);
and no component call with a body — its four wrapper elements hold a Liquid include, not
prose.

Compile-time resolution was rejected on mechanism. `compile` runs before `loadData`, and
which maps are indexes is itself a product of compiling `site.lem`, so the core has no
map to check a literal against without a pre-compile protocol or a second pass; no
syntax names the index a parameter would resolve through; and the fact bookkeeping
existed only to recompile documents that a render-time lookup never needs recompiled.
Its one benefit, an editor underlining a typo'd id, can be an analysis over loaded data
later. `site.photos["id"]` already works, a miss is a render error with a span, and a
component may take the id and look it up, as the WebC component does today.

Markdown capture bodies were first planned as the half worth building — D22 makes
capture the only way to hand content to a component — with a design checked against the
code: compiler-side, no parser change, one comrak run with sentinels at every boundary, a
per-line source map for dedented prose, and a fence-in-body limitation. Eric asked
whether the rule "Lemmon in a `.md` file emits HTML" obviates it, and it does: one rule
instead of an exception, no second rule for which bodies are markdown, simpler
tree-sitter injection, and the raw-HTML idiom — a wrapper element with a blank line on
each side of the markdown — covers prose inside an element, which was verified through
the document stage before it was claimed. Direction settled it: the narrow rule can be
widened by an additive form later, while the wide one could not have been narrowed
without breaking documents. His follow-up — is there a path to markdown values by
syntax? — became D39's alternatives: a head-marked capture, or a `markdown()` function
over a string, which would be a second string → `html` cast beside `raw()`.

So the item became a docs-and-decisions sweep: D39 new, D07 and D25 revised, §Document
mode items 1, 2 and 6 rewritten, one row in each deferred-questions register, three
corpus cases pinning the settled behaviour, and no crate change beyond one stale comment
on the unit's cache key. The backlog text above is the reworded item; the original is
quoted in the first paragraph.

## Refinement

The medium review (Opus) found eight things, all fixed. Two were survivors of the deleted
promise outside the section rewritten: the `lem-let` row of the tag table still said a
capture body is a document, and the escape table told authors a code fence handles a
literal tag in documents, which is false inside a block. Item 6's "a string literal is a
string" sat in the front-matter item and read as revoking `by`; it now scopes the rule
to templates and prose and names the loader's resolution. Two sentences described
planned mechanisms in the present tense — `compile` "tells the shell which sources to
fulfil", which today's boundary does not (∆RGQ), and pages "re-render from
render-reported deps", which do not exist yet — so the first was reworded to the
ordering argument alone and the second became a placeholder naming ∆RGQ, which the gate
now tracks. The register row as first written mixed the settled rule with design detail
and cited the wrong clause of invariant 7; the detail moved into a record of its own,
D39, and the register is one line. The artifact ledger had two entries under one heading
and an instruction contradicting the normative sentence it came from. And the raw-HTML
wrapper corpus case pinned only comrak until a block was placed inside the wrapper,
making it the clean twin of `errors/raw-html-wrapper`; the capture case now shows the
same link resolving in prose beside the literal one in the body.

## Retrospective

**A decision not to build is still a sweep.** The falsified sentences were not in the
section being rewritten but in a table row, an escape-table cell, and the item's own
backlog text; the review found all three. Grepping for the promise's nouns — "document",
"fence", "reference" — before the rewrite is cheaper than a review finding them after.

**A planned mechanism is written as a placeholder or not in the present tense.** Two
sentences credited `compile` and render-reported deps with work that belongs to ∆RGQ,
and neither would have tripped the gate because neither said "not yet". Say what is true
now and mark the rest with the gated phrase, so the docs stay honest if the item lands
differently.

**Design detail belongs in a record's alternatives, not in the register.** A deferred
question is one line; the forms considered and not taken are an Alternatives section.
That is where a future reader looks, and it keeps the settled half of a decision out of
the list headed "do not decide these ad hoc".

**Evidence from the site answered "is it needed" faster than argument.** Six void calls
and zero bodied ones settled one half; the phase order settled the other. Reading
limulus.net's content took minutes and made the recommendation concrete.

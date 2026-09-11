---
id: ∆eag
completed: 2026-09-11 12:18:19 -07:00
---

## Backlog item

> - ∆eag Restrict defaults to the record and `by` to a declared field — a default reads
>   `self`, `path`, and `body`, never an ambient value, and `by` names one field of an
>   ambient value (`by site.photos`), a pair the head declares rather than a function the
>   loader runs. A record's plan then depends on its declarations alone, never on a body: a
>   default reading `site` couples every record to the site's load order (D37 revised); needs: ∆HF0

## Planning

The item was filed as "drop `by`": replace `hero: Photo? by site.photos` with a string
field and a map index in the layout. The plan for that was written, and Eric
interrogated its first consequence — a misspelled photo id becoming a render error at the
layout's lookup instead of a load error at the front matter — from two angles. What if
the photos moved to a database? Both designs materialise the whole map, since the core
cannot query, so that case does not separate them. What does load time buy over render
time? Not timing, since the dev server does both on one save, but the position: a load
error points into the file the author just edited, and a render error points at the
layout. The scale question came last, and there `by` came out ahead: a dependency
declared in a type's head is one a shell can see before rendering, which a lookup inside
a body is not, so `by` is the construct better placed for a future on-demand mode.

That settled the middle option, and the item was reworded before work started: keep
`by`, narrow it to naming one field of one ambient value, and restrict defaults to the
record. The reworded item kept the goal — a record's plan depends on its declarations
alone — in a weaker form: plus the ambient fields its heads name, never anything a body
says.

## Refinement

**Narrowing `by` grew the code; the item's goal was never line count.** Eric expected a
net decrease and got about a hundred and sixty lines of production Rust more. The
measured reason: a `by` index used to ride on the default machinery for about fifteen
lines, told apart by a two-variant enum; as a declared pair it has its own path from
the unit through the plan to the loader, and defaults still need everything it left
behind. The stricter rules also refuse more: four distinct mistakes after `by` where
there was one type check, and every ambient value in two places where there was one
value in one. What the item bought is that load order is no longer inferred from
function bodies and one sentence governs what a default may read.

**Both reviews earned their place.** The first caught a `by` on a field declared `any`
compiling clean and doing nothing, a misspelled value name getting the generic shape
message, a cross-file index error blaming the wrong load rule, and an absent map
reported as a missing entry. The second caught that a doc-editing script had aborted
before writing D37's own scope sentence, so the decision record contradicted its index
row; that the link refusal reported only the first ambient value a default reached and
would silently drop a direct read; and that an inherited refused default was reported
once per subtype. One review candidate was refuted by a four-line probe rather than
argued: a bare sibling name inside a default declared in the bindings file resolves to
`undefined name`, because the file's top-level names are exposed only to its entry.

**One rule, said in two places.** The compiler refuses an ambient read where it stands
and the link step refuses one reached through a call. The first cut had each side
deciding "which values are refused" with its own closure. `Runs` and `refused_at_load`
now hold the predicate and the sentence together, and both sides call it — the same
shape the ∆HF0 journal recorded for the reason clause alone.

**A simplification claimed at the end was half wrong.** Asked whether two leftovers
could be folded in, I looked before answering: the `Planned` wrapper on record steps is
what lets a record cycle name the call a read came through, so it stays; `Through`
duplicated three fields an `Index` already carried and went, for fifty lines.

Deferred: the identical cycle diagnostic is still reported once per subtype that walks
the same ancestor, which predates the item; limulus.net's `photos.json` maps some ids
to a string alias of another id and will not validate as `{string: Photo}` as-is, which
is ∆o24/∆rT5's port work.

## Retrospective

**A script that asserts every edit before writing any is the wrong shape for a doc
pass.** The D37 edit list aborted on its third pair, so its first two — the status line
and the scope sentence, the ones that mattered — were never written, and only the
second review noticed. Write-per-edit, or check the file after, would have caught it in
the same minute.

**A claimed simplification is measured before it is offered.** "Both are modest" went
out before either had been looked at, and one of the two was load-bearing. The cost was
small because Eric asked; the habit to keep is the one that answered him: read the code
the sentence is about, then say what it would delete.

**Interrogating the first consequence was worth a checkpoint.** The plan as written
would have shipped a real regression in author experience — the front-matter position
lost from a class of errors — for a line count it would not have delivered. Two
questions about mechanism (what happens with a database, what does load time buy)
changed the item; the plan file had listed the consequence and moved on.

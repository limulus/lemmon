---
id: ∆cea
completed: 2026-09-07 09:00:00 -07:00
---

## Backlog item

> - ∆cea Records and enums — `lem-type`/`lem-enum` resolved project-flat through a type
>   namespace in the export index, `extends` with no field redeclaration in subtypes (D30's
>   `layout` question), field access typed by declaration, bare variants where an enum is
>   expected, and a consumed type's shape as a compile-time fact (D25). Split from ∆C8d
>   because nothing produces a record value before ∆iCW and ∆PCK

## Planning

The plan settled the four questions the item left open and put them in a decision record,
D32. Declared types are nominal, by name: one `Type::Named`, with the kind and shape in a
type namespace the export index holds beside the functions, and the rules that need a shape
— field access, assignability along the chain, printing, variants — take the declaration
table as an argument, as D31 had said they would. A subtype may redeclare a field only with
the identical type, which changes its default and nothing else; that answered D30's
`layout` question with no special rule, against the item's own "no field redeclaration"
wording, because the corpus and the limulus.net port both re-default `title` from
`linkTitle`. A bare variant resolves only where an enum is expected, and an enum is its
variant's name as a string at run time. A consumed declaration is a second kind of
compile-time fact, span-free, recorded transitively through bases and field types at the
use that reached it.

A Plan agent reviewed the draft before Eric saw it and moved two things. The draft had
separate `Record` and `Enum` type variants decided at annotation time, which forced
signature resolution out of `prepare` and a four-phase driver in both `compile` and the
resident core; one `Named` resolved late kept the three phases. And the draft recorded
facts through a recorder inside the declaration table, which could not give a stale
diagnostic a span (invariant 8); recording the closure at the sites that have a span
over-records and costs a needless recompile, never a wrong one. It also caught that the
canonical case's `article.vod.poster` needs member-path narrowing, which is [[∆PCK]]'s, so
the case gained a `lem-let` of the field and the acceptance became "output unchanged"
rather than "source unchanged". Eric approved the plan with one instruction: review with
`/code-review medium` on Opus and fix what it confirmed.

## Refinement

The implementation went to plan; the review is where the work moved. Eight finders on Opus
produced twenty-nine candidates, and eight correctness findings were confirmed and fixed:

- **A declaration moved between files in one batch panicked the resident core.** Offering
  incoming files one by one refused the name for its new home and then withdrew it from
  the old, so the new home's compile hit an `expect`. The same hazard existed for
  functions since [[∆e5i]]. `ExportIndex::withdraw` now runs for every incoming file
  before any is offered.
- **A UFCS receiver lost the expectation it used to be checked under.** Lowering the
  receiver before deciding field-versus-function dropped what `call_function` had flowed
  into it, so a list literal or lambda receiver behaved differently from the same
  expression as an argument, and a bare variant could not be a receiver. A receiver that
  can never have a field — a literal, a lambda, a variant of the first parameter's enum —
  is now lowered expecting that parameter.
- **`==` against an optional enum reported a false mismatch**, because flowing the left
  type into a right-hand name through `expr_in` applied the one-directional expected rule.
  The walk gained `lower`, which shapes an operand under an expectation without holding it
  to one, and the syntactic gate on the right operand became a pure rule.
- **An undeclared map-key type was reported twice**, a guard the diff had deleted with its
  test. A refused key's names are no longer recorded as uses.
- **A subtype whose base's file lost the declaration gave a false "has no field" in a
  third file.** The consumer now says the base is not declared, at its own use.
- **A duplicated field's type was never resolved**, so an unknown name inside it went
  unreported until the duplicate was removed.
- **Link diagnostics fell out of unit order**, since the type-fact check was a second loop
  over all units.
- **`map`, `list`, and `function` were called builtin types** in the refusals, which the
  annotation resolver itself denies three lines earlier.

The cleanup findings were applied too: one `TypeRef` list instead of parallel `type_refs`
and `type_facts` vectors that only `link` re-associated by name; `duplicate_named` for
field and variant repeats; the `Type` walkers beside `Display` rather than two hand-written
recursions in the compiler; `variant` taking whether the name resolves, so the rule holds
the whole verdict; the stale-type message showing both shapes like the signature one; an
unread accessor and a `too_many_arguments` allowance removed. Three were left as they were:
a local shadowed by a variant warns as unused, which is true and now has its corpus case;
the `expect` on a declaration being in the index matches the function convention; a
duplicate type's uses are checked against the other file's shape, as a duplicate
function's are. The conventions finder found two "other half" cases the corpus lacked —
a non-variant local under an enum expectation, an `extends` of an undeclared name — and a
D23 row the change had falsified.

Two items came out of the aside afterwards. [[∆cE6]] gives the corpus trailers a line and
column beside the byte span, because Eric could not match diagnostics to source by eye.
[[∆EN6]] gives variants a dotted spelling — `.hero` where the enum is expected,
`Placement.hero` anywhere — which retires the variant-over-local shadowing rule; the bare
variant rule was the one part of this item that kept needing another special case.

## Retrospective

**A design review before implementation is cheap and this one paid twice.** Both of the
Plan agent's structural objections — the four-phase driver, the span-less recorder — would
have been expensive to unwind after the code existed. The habit to keep is the one
[[∆C8d]] named from the other side: when a plan argues for a shape, ask what the shape
costs at the one place it is weakest before writing it.

**"Lower the receiver first" was right and still lost something.** The plan reasoned about
stack order and about which value decides the call, and missed that the old path had also
been a bidirectional check. When a change reorders when something is lowered, the question
to write down is what the old order was giving that expression, not only what the new
order needs from it.

**Corpus "other halves" were found by a reviewer, twice.** The rule that a suppression is
tested on both halves is in CLAUDE.md, and I wrote the silenced half for two rules without
their neighbours. Writing the case that must stay clean is part of writing the rule, not a
review finding, and the cheapest time is while the refusal is being written.

**A provisional reading that needs three special cases is a design signal, not a debt to
carry.** The bare-variant rule was accepted from the item text and each gap it opened was
patched in the walk. The better move was to say so at planning time and ask whether the
syntax was right; that conversation happened after the fact and became ∆EN6.

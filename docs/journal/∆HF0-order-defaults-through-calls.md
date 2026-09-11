---
id: ∆HF0
completed: 2026-09-10 20:24:50 -07:00
---

## Backlog item

> - ∆HF0 Order defaults through calls — each unit records the reads of every function it
>   defines, and the link step closes reads through calls before ordering a record's defaults
>   and the bindings file's computed bindings, so a default that calls a `lem-fn` reading a
>   not-yet-computed sibling is ordered instead of faulting at load. Removes the "only reads
>   written in the default itself count" clause from D37 and docs/language.md

## Planning

Two calls went to Eric before the plan started, and both took the recommendation: refuse
at link a default that reaches the host's document value through a call, and name the
call in a cycle message. Neither is in the item's text; both fall out of the same closure,
and the alternative to the first is a default silently reading the envelope's `page`.

The mechanism the plan rested on is that invariant 11 keeps a callee's body out of the
compiler's reach, so the link step — the first place every unit is in hand — is where
reads through calls can be closed. Each unit records, per function the loader may run,
what it reads off `self`, its parameters, and ambient values, and which project functions
it calls with which of those roots handed over whole; the link step closes that to a
fixpoint and translates a callee's parameter reads through what the caller handed. Nothing
enters a consumer's fact, so a callee's body edit reorders the module's plans while every
unit stays byte-identical, and a test now asserts exactly that. Two checkpoints: the
recording alone, behaviour unchanged, then the closure with its diagnostics, docs, and
corpus.

Stated up front and kept: a local bound to a root is not a root, so a field read through
an alias is outside the order, as `self` already was.

## Refinement

**The first review moved the unit-only read variant off the module type.** I had put a
`Param` variant on the plan's `Read` with a doc comment saying only a unit holds one;
both plan consumers grew a `=> None` arm for it. The verifier refuted it as a defect —
nothing in that tree could reach it — and recommended the shape that landed: a unit-side
`Access` with a total conversion at link, so a leaked parameter read is unrepresentable
rather than unreached. The same round made the uses table a map, folded two duplicated
walks into `bare_root`, and recorded a whole-value read of an ambient value so a
`lem-fn` returning `page` is findable through a call.

**The second review caught a regression the corpus could not.** In the first checkpoint,
answering a finding that `self.words.map(loud)` recorded nothing, I had recorded a
function *named as a value* as a call handed nothing. Once the closure consumed that,
`layout: (Post, html): html = post` inherited everything `post` reads — a link error for
any layout reading `page`, a `posts needs posts` cycle for any layout listing
`site.posts`. Three angles reproduced it against HEAD. The corpus passed because no
layout in it reads either. The fix is the simple rule the docs now state: only a call
written as `f(…)` is followed; a function stored in a field or handed to a builtin is a
value. The cost, stated honestly: `map(loud)` no longer orders `loud`'s ambient reads,
which stays an error at the read. A corpus schema whose layout reads `page` now pins the
rule.

**Cycles through a `from` binding now say where the read is.** A host-supplied binding
has no reads of its own; its edges come from record steps, so the message named neither
the call nor the field. The dependency closure became an edge table carrying the record
field and the call, and the diagnostic labels the field: "`posts` needs `posts`, which
`Post.siblings` reads through `all` while `posts` loads". The pre-existing lexical case
gained the same attribution.

**The refusal's reason clause is now the compiler's.** Within one checkpoint the link
step's copy of "bound per page, and a binding here is computed once" had already drifted
by a word. One function owns the sentence; the link step prefixes it with what reached it.

Deferred: the loader's "`banner` reads `site`, which is not loaded yet" points at a field
that may not mention `site`, since plan reads carry no call provenance — it wants spans on
`Read` and `Call`, decided together when a consumer exists. The closure closes every
project function rather than the loader-reachable subset; sub-millisecond at limulus.net's
scale. And the question Eric asked at the end — what one language change would simplify
this — became [[∆eag]]: restrict defaults to the record and drop `by`, which deletes the
ambient half of this item's closure and the `from`-binding ordering with it.

## Retrospective

**Widening a rule in response to a finding, without enumerating the shape.** The
value-naming bug came from fixing one instance (`map(loud)`) of "a function name in
value position" without asking what else has that shape. `layout = article` is the most
common line in every schema, and it was the counterexample. When a review finding
proposes following more, list the other instances of the syntax first; the fix that
survives is usually narrower, not wider.

**A gate that never exercises the common case is not a gate.** Every corpus layout read
only its parameters, so the corpus was green through a change that broke every real
layout. The case that would have failed — a layout reading `page` — took four lines.
Before trusting a green corpus for a semantics change, add the case an author would
actually write, not the one the implementation suggests.

**Exhaustive-sounding prose gets falsified.** I wrote "a call through a value is the one
call not followed" and it was wrong twice by the next review: aliases, and namings. The
docs now say what *is* followed and leave the rest to the negative.

**Review agents in one working tree collide.** In the first round, verifiers probing and
reverting in the same checkout stepped on each other and left probe files; two reported
foreign edits appearing mid-run. The second round gave each a scratch copy and went
clean. A verifier that must mutate gets its own tree.

---
id: ∆1Hp
completed: 2026-09-15 20:12:49 -07:00
---

## Backlog item

> - ∆1Hp Side channels and marks — `lem-style`/`lem-head`/`lem-script` collected per
>   render as deduplicated fragments under a name; `mark(name, default?)` placing a named
>   range the host fills, its literal name checked against the host's claims and recorded
>   as a fact; `render` returning the page as bytes beside its marks and fragments,
>   resolved by name in the wrapper (D42). Planned in `docs/plans/∆1Hp-side-channels.md`

## Planning

**The item arrived planned twice over, and the plan was wrong in one place that
mattered.** [[∆RGQ]] split it out and wrote the first plan; the session that settled D42
revised it, generalizing an emit into a named byte range the host fills. This session
began by checking every claim in that plan against the code — four read-only passes over
the parser, the compiler and VM, the boundary, and an adversarial one against D42, the
invariants, and limulus.net's source. That pass changed four things:

- **An unclaimed mark name is a warning, not an error.** D42 says so twice, and the plan
  had made it an error. That broke the plan's own staleness fact: a file with any error
  yields no unit, so no unit could record that it had marked a name nothing claimed, and
  "a host that starts claiming leaves it stale" could never fire. It would also have
  failed every `mark()` compiled by a host with no handlers — the playground, the editor,
  and `@limulus/lemmon-core` before the boundary carried claims.
- **Names travel as strings, not constant indexes.** Each file interns its own strings and
  link concatenates the pools with an offset, so a fragment map keyed by index would keep
  equal CSS from two files apart. No single-file corpus case can see it.
- **`mark()` lowers to its own op.** A builtin cannot reach the file and span of its own
  call, which is exactly what a handler's diagnostic points at, and builtins refuse named
  arguments before their own code runs. The compiler already intercepts `asset()` by name;
  `mark()` follows it.
- **Marks and fragments carry a file beside their span**, since a mark made in a layout
  points there and not at the page being rendered.

**The plan's own test inputs were checked by running them.** Two replacement parser tests
it proposed still passed with the parser's error handling deliberately broken; the reviewer
found inputs that fail. That set the habit for the rest of the item: every test that claims
to pin a rule was run against a broken version of that rule before being trusted.

**The `default` argument is named, and that is what keeps ∆7wK additive.** It is the
language's first optional parameter and the only named argument any builtin takes. A
positional default would have taken the slot ∆7wK's payload needs, and two optional
positionals would have made the checker guess a record from an `html` by type.

## Refinement

**Building the collect tags exposed a bug D26 and D27 have had since [[∆2vO]].** A body
ending in a statement line kept the newline above it, because the trailing edge was found
at the last piece rather than past the statements whose lines the line rule had taken.
Nothing hit it before: nothing ends a body with a statement, and an unused `lem-let` there
warns. This item's canonical placement — a `lem-style` closing a component — makes it the
common case, so every component would have emitted a blank line. Fixed with the item,
since it is D26's rule as written; the review then found the first fix still missed bodies
whose closing tag is indented, which is what a capture nested one level in looks like.

**The boundary change paid for itself as D42 predicted.** Taking the page out of the JSON
reply took the spike's page render from 176 µs to 96 µs — under a tenth of the 1 ms budget
rather than a fifth — with zero unexpected diffs against the 11ty build. The `.wasm`'s
imports are unchanged, so `wasm:check` needed nothing: a `Vec<u8>` return copies out
through glue that was already there.

**Three reviews found what the tests did not.** A trailing-edge fix that held only at the
top level; a mark that allocated its name per call rather than sharing the constant's;
a cross-file test whose first version passed even with link's rebasing removed, because the
marking file was linked first and so needed no offset; and a fragment lookup in the wrapper
that read through `Object.prototype`, so a name like `constructor` would hand a producer a
function instead of an empty list.

**Left for the items that own them:** the `<script>` markup around a collected URL and its
attributes are the producer's (∆7Xs, ∆PgF); file-backed stylesheets, which WebC inlines
today, have no form yet and would be an additive `<lem-style src = asset(…) />`; a
top-level `lem-style` in a file that is not a page never runs, which ∆c9S can report once
"is a page" is knowable; and a wrapper's mark points at the plugin's `lem-fn` rather than
the author's call, which matters first for ∆7wK.

## Retrospective

**The plan was worth more than its instructions.** Its *why* held up everywhere; its *how*
was wrong in one place that would have been discovered only after building the boundary,
and stale in a dozen line numbers. Verifying a handed-on plan against the code before
building it cost one pass and changed four decisions.

**I wrote three corpus cases that tested the wrong thing, and read the output to find
out.** One named a function `page`, colliding with the host's ambient value; one left a
binding unused and blessed the warning; one put a self-closed `<lem-style />` where it
swallowed every later line, hiding the mistakes below it. Blessed output is a proposal, and
reading it as an author is what caught all three.

**My own test for the cross-file rule passed under perturbation.** I wrote it to pin link's
constant rebasing, broke the rebasing to prove it, and it stayed green: the file that marked
was linked first, where the offset is zero. The second version runs both orders. A test
written for a rule is worth nothing until the rule is broken under it — including when I am
the one who just wrote both.

**Two checkpoints landed in one commit.** The workflow is one commit per checkpoint, and
the collect tags and marks were separable when the first finished. I started the second
before that commit existed, so the tree mixed them; splitting them afterwards would have
meant re-blessing the corpus in between. Commit the checkpoint before opening the next one.

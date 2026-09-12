---
id: ∆eHR
completed: 2026-09-12 12:28:36 -07:00
---

## Backlog item

> - ∆eHR A misspelled method read gets no did-you-mean — `s.slugfy` falls to the catch-all
>   "a `string` has no field `slugfy`" in `crates/lemmon-vm/src/compile/rules.rs`, while the
>   same mistake called gets `no_method`'s list of what the type has. `nearest` and
>   `methods_of` are already there; undecided is how a field's refusal names a method. Both
>   siblings sit in `render/errors/method-errors`; this case is the one it never asked for

## Planning

No plan of its own. The item was picked up inside [[∆Q6L]]'s second checkpoint because
`rules::field` was open for the stdlib's methods anyway, which is what the working
agreement asks: fix a confirmed finding while the crate is already in hand.

The question the item recorded as undecided — how a field's refusal names a method —
answered itself once the builtin types had methods. A builtin type has no fields under
any spelling, so the only reading of `s.slugfy` worth offering is a method, and the only
way to offer one is in the form it would be called: `did you mean `.slugify()`?`. There
was no second candidate to weigh.

## Refinement

**Fixing the read form showed the call form had quietly got worse.** The item's premise
was that a misspelled *call* was already well served, because `no_method` listed what the
type carried. That was true when a `string` carried one method. [[∆Q6L]] took it to ten,
and the list stopped being a fix and became a wall to read — `a `string` has no method
`slugfy`; it has `.contains()`, `.endsWith()`, `.length()`, `.lower()`, …`. So the change
went both ways: a call now gets the one nearest name when a name is near, and the
inventory only when none is. The item asked for one of those and the other was the more
useful half.

Both halves are tested against their negatives, as a suppressing rule must be: a name near
nothing the type carries (`s.byline`) gets no guess, and a type with no methods at all
(`bool`) gets none either, so a suggestion offered for anything would be worth nothing.

## Retrospective

**It shipped inside another item's commit, and that is a real cost.** The fix is in
`8471b79`, a `feat` for [[∆Q6L]], because its tests are entangled with the string type
growing ten methods in the same change — splitting it afterwards would have meant
reconstructing an intermediate that never compiled. A reader looking for why a
did-you-mean appeared on a field read will find it by searching the message, not by
searching the history. Picking the item up at the point `rules::field` was open was still
the right call; the thing to have done differently is to have committed the stdlib's
methods first and this on top, which was available at the time and was not taken.

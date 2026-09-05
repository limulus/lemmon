---
id: ∆DA2
completed: 2026-09-05 10:36:22 -07:00
---

## Backlog item

> - ∆DA2 Placeholder gate — every "not yet" refusal in `crates/` (the parser's "not
>   supported yet", the VM's "not here yet") and every "not yet" note in `docs/` names the
>   deltoid it waits on, and a test fails the build once that deltoid has a journal entry, so
>   a stage landing cannot leave a stale refusal or note behind. Replaces CLAUDE.md's
>   grep-on-landing agreement; ∆1vS's review caught a wrong message in one such arm

## Planning

This item came out of a question Eric asked at the end of [[∆1vS]]: what one thing about
the project would I change. The answer was the placeholders. The compiler carried
hand-written "needs the checker, which is not here yet" refusals and the docs carried
"_Not yet (∆C8d)_" notes, and nothing checked either against reality; a working agreement
in CLAUDE.md asked whoever landed a stage to grep for them. Eric placed the item after
∆C8d, which was in flight. The ∆C8d session — which had split that item into ∆VNk, ∆C8d,
∆PCK and ∆Iau at claim time — asked for the gate to be built first, since it only pays off
if it exists when the checker lands, and Eric agreed. So it was built in the ∆1vS session,
inside file boundaries the peer set: its uncommitted ∆VNk work was under review in the same
working tree.

The gate is one integration test in `crates/lemmon-corpus`, beside the corpus's orphan
check, because the rule spans every crate and the docs. Code placeholders are the phrases
"not here yet", "not supported yet" and "not yet supported", so that ordinary prose ("not
yet consumed") is left alone; a docs placeholder is any "not yet". A line carrying one must
name a deltoid that is an item in `BACKLOG.md` and has no journal entry. Each placeholder
was pointed at the split's items: `lem-type`/`lem-enum` at ∆C8d, `lem-match` and `is` at
∆PCK, `from` at ∆iCW, the side-channel tags and `lem-page` at ∆RGQ, `lem-import` at ∆PgF,
the host declaration at ∆Iau, syntect at ∆BfR.

## Refinement

The first live run failed on eleven lines, none of them the ones it was built for. The
gate file itself spells the phrases out, in its doc comment, its constants and its tests,
so it now excludes its own path. And two sites picked their item from a table — the
parser's opaque builtins, the compiler's `lem-type`/`lem-enum`/`lem-match` arm — so the
deltoid arrived through a variable and the literal was never on the line. The gate reads
text, not values; the fix was to put the phrase on the table row beside its item
(`Builtin::unsupported` returns the diagnostic's whole tail, and the compiler arm became
three literal arms), and the gate's doc says why.

Two of the eleven sites were in files the peer held: a parser test pinning "`lem-style` is
not supported yet", and the architecture doc's "syntect is not yet". The peer made both
one-line edits before the gate first ran, so neither session touched the other's files.
Blessing the corpus rewrote `compile.html`, which the peer also held; bless writes what the
tree produces, so its label line and my suffix both survived.

One stale comment surfaced on the way: `FnVoid`'s doc said "recognised by the parser, not
yet supported", and it has compiled since [[∆c8h]]. The gate would have flagged it for
naming no item, which is the right outcome for a sentence that stopped being true.

## Retrospective

**Building a gate finds what the gate is for.** The one comment the gate flagged that
nobody had noticed was exactly the kind of sentence the item exists to catch: a "not yet"
that outlived its stage with no test noticing. A rule worth writing down as an agreement is
usually worth writing as a test instead; the agreement in CLAUDE.md lasted three items.

**A text gate needs the text on the line.** Twice the deltoid was there in the program
and invisible to the gate. Choosing between "make the gate understand values" and "make the
code keep phrase and item together" was easy — the second is a convention one sentence
long — but it had to be chosen, and the gate's doc now says it so the next table-driven
site does not rediscover it.

**Two sessions in one working tree works with a file list and a message.** The peer named
the files it held; I stayed out of them and asked for the two lines I needed; both replies
crossed harmlessly because each said what it had done rather than what it wanted done
next. The cost was one message each way.

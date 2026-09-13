---
id: ∆RGQ
completed: 2026-09-13 14:21:15 -07:00
---

## Backlog item

> - ∆RGQ Side channels + generators — `lem-style`/`lem-script`/`lem-head`/`lem-emit`
>   collection and dedup, `lem-page` generators (prev/next), `plan()` filling each page's
>   entry from the host-named field (D30), serializable modules, render-reported deps, and
>   compile-reported facts (signatures, ambient bindings, manifest entries, a document's
>   record) keying the unit cache so a content edit never recompiles its layouts (D25)

## Planning

No code for this item's features was written. Planning split it into four items, settled
two designs, and wrote a plan for the first; this entry records where each part went.

Eric opened with a question: why `prev` and `next` belong in the core rather than coming
from the shell. Two reasons, both about where values live. After `loadData`, every page is
a validated record held only in the core, and a neighbour is a reference to one of those
records, which the shell cannot express in the JSON it sends. And a generated page does
not exist until the core evaluates the list it iterates over.

**The split.** The five pieces had different consumers and different risks, so each got
a new deltoid:

- [[∆1Hp]] side channels, planned in full (`docs/plans/∆1Hp-side-channels.md`);
- [[∆c9S]] page templates and `plan()`, with its design settled in D41;
- [[∆Qr2]] compile-time facts and serialized modules across the boundary, moved to the
  end of Phase 1 so it serializes whatever the other items add;
- [[∆xBO]] render-reported deps.

The first pass kept ∆RGQ for the page-template part, following ∆C8d's precedent. Eric
reversed that: the items changed enough that the split deserves its own entry, so every
part got a new deltoid and this item closes here.

**Side channels resolve in the host.** The first plan had the core splice `head` and
`scripts` and leave `styles` for the shell. Eric asked whether the shell could fix up the
output instead, and it can: the CSS bundle's URL is the shell's to decide (D38), and a
core that resolved two of three channels would treat one name specially, which D30 removed.

How an emit marks its place took four rounds:

- a fixed comment sentinel, with a compile-time check that authors cannot type it — Eric
  called the check over-engineering;
- the same sentinel with no check — Eric objected that a hand-typable sentinel is an
  undocumented API;
- a sentinel made unguessable by a per-render nonce;
- **bytes and marks**, proposed by a separate session that had measured the boundary: an
  emit writes nothing and records a byte offset, the offset travels on `html` values
  through captures, and `render` returns the page as UTF-8 bytes beside its marks.

Marks won. Nothing in the page can be forged, splicing at known offsets costs a median
limulus.net page about 2 µs against 11 µs for a scan, and taking the HTML out of the JSON
reply removes more than half of a `render` call (108 µs to 49 µs on the spike page).
Captures were the real cost and turned out bounded, because `html` is opaque at run time.
An emit printed twice is allowed. The plan takes D42.

**Pages come from front matter (D41).** Eric asked whether Lemmon templates should be
pages at all, and why not give them front matter like markdown. limulus.net answered:
eight static WebC pages and the sitemap carry plain YAML, and only its two generators
need expressions. Generated pages as computed `site.lem` bindings were considered and
dropped. Eric pointed out that Eleventy's pagination keeps a file where the pages come
from, and that property is worth keeping. So a generator is a page template whose front
matter names a `site` list; filtering is a computed binding, and each page's title and url
are defaults on its type. `lem-page` is retired. Page templates compile with the
documents, and the shell parses their YAML.

## Refinement

- Checkpoint 0 of the original plan landed here: the backlog split, the `needs:` edges
  re-pointed, and every placeholder that named ∆RGQ moved to its new item, so this entry
  does not trip the placeholder gate.
- Three edges came out narrower than planned. ∆rT5 needs side channels but not page
  templates; ∆xBO needs neither; ∆4tG gained no edge on ∆Qr2, whose text names it instead.
- A plan review against the code found about twenty `render` callers inside the VM's own
  tests, five test strings spelling the old refusal, and a replacement test input that
  would have produced one diagnostic where the test needs two. All were fixed in the plan.
- D40 was already taken by [[∆BfR]]'s uncommitted work in the main checkout, found by
  listing worktrees; pages took D41 and side channels D42.
- `<lem-page>` now reports "did you mean `lem-case`?". Eric left it: until the first
  stable public release there are no users of retired names to guide.
- Plans for items a later session builds now live in `docs/plans/`, named from the
  backlog item, so an ordinary `/delto plan ∆xxx` finds them; CLAUDE.md says so.

## Retrospective

**Check that each reason for rejecting a design is specific to that design.** Ranges were
rejected with four reasons. Two were wrong: returning bytes removed the UTF-16 problem,
and "a transform before the splice invalidates them" applies to sentinels just as much.
The other session's measurements overturned a rejection that had looked settled.

**When a rule exists only to protect a representation, question the representation.** The
reservation check, the nonce, and the documented format all guarded one thing: text in the
page. Removing the text removed all three questions.

**Count the real site before arguing about shape.** "Why does a template need a tag?"
was answered by listing limulus.net's page sources, as ∆Hbw's retrospective also found.

**Take a decision number against every checkout, not just the branch.** Another session
held D40 in uncommitted files; `git worktree list` found it before a collision did.

**A session that reshapes the backlog should hand off, not implement.** This one grew too
large to build ∆1Hp well, so the plan went into the repo for a fresh session.

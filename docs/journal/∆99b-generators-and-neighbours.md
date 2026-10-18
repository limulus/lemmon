---
id: ∆99b
completed: 2026-10-18 14:43:30 -07:00
---

## Backlog item

> - `∆99b` Generators and neighbours
>   - A page template whose `for:` key names a field and an `<ambient value>.<field>` list
>     becomes one page per item at load, its url from its type's default (D43)
>   - needs: `∆Bhz`

## Planning

The item's brief, folded here: the loader fills the `prev`/`next` fields a generated
page's type declares, in list order, as references into the page table (D43);
limulus.net's photo and TIL topic pages are generators (now `∆hvm`).

Planned on 2026-10-18 as slice 5 started, by a read-only planner against `091822a`, then
reviewed before building. D43 was accepted and not re-litigated; the plan settled what it
left to the builder, each answer recorded in D43's record as an "As built (∆99b)" Status
clause and _(Refined by ∆99b)_ notes, so no new decision was taken (D46 stays free):

- The shell sends `for:` as it parsed it, `{value, spans, valueSpans}`, the entry's span
  and the value's own, so an error about the list underlines the list; the core rejects
  and words every malformed pair at load.
- A generator with no `type:` is refused by the shell, which alone sees it; a `.md`
  document with `for:` is refused whether typed or not; a glob that holds a generator is a
  load error at its `for:`, labelled at the binding.
- The neighbour rule applies to page records only: each of `prev` and `next` is declared
  on its own, as an optional of the record or an ancestor, with no default; nothing
  reserves the names on a data record.
- A default's read of a neighbour is refused at compile where it stands and at link
  through a call, re-checked on every link rather than recorded as a unit fact (revising
  D43's Consequences sentence, as `∆Bhz` did for `url`); neighbours are absent while
  defaults run, so a read the compiler cannot see faults where it happens instead of
  reading null.
- The reference is `Value::Neighbour(position)` into the store's page table, resolved on
  a field read; a mark payload refuses it.
- A generated page is named by its list position in the language's own index syntax,
  "`photos/photo.lem` for `site.gallery[3]`", in the core's messages, the shell's, and the
  label a render diagnostic on a generated page now carries.
- A list the loader cannot check is refused (untyped, `any` elements, or the document
  value); `for:` over a map is refused with a hint to bind `.values()`; a page type with no
  url default is one error per generator.

The plan review added nine amendments, among them that the main checkout's `.wasm` must be
restaged the moment this lands, since `HostInput` refuses the `neighbours` field the shell
now sends, and dropped one rule as over-built (all-or-nothing loading, which the store
already gets by being dropped on any error).

## Refinement

Built in one isolated worktree in four checkpoints, each with `npm run verify` green and
the ABI suite after the two that moved the boundary: generators in lemmon-vm
(`load/generators.rs`, page identity as `(path, index)` through the store, `plan()` and
the url pass), the boundary (`for` on a loaded page, `index` on a page reference and a
planned page), neighbours (the host's names, the declaration rule, `Value::Neighbour`,
the read refusal), and the shell (expansion replacing its refusal, every generated page
rendered and written at its own url, one `pageName` helper for every message). Landed as
`89e0abf`. The `.wasm` grew by 60 KB, from 2,337,495 to 2,397,564 bytes before the float
fix landed beside it.

Departures the builder reported: the read refusal is worded "out of reach of a default",
since the read is lexically in scope; the shell's naming takes the list as well as the
index; the neighbour check at link follows each page record's chain, so a neighbour
declared or inherited is judged where it is declared; and the builder moved to a Cargo
target of its own after the shared one linked another checkout's stale `lemmon-vm` as
fresh.

Two reviews of the diff (the core and corpus; the boundary, shell and docs) found no trap
and no wrong page, and eleven smaller things, all fixed before landing: a url default that
forgot its item reported once per sibling (19 photos, 18 diagnostics) and now reports once
per generator; four item-naming sites in the shell and two collision messages named only
the path; two halves of the neighbour rules lacked a test; and six doc sentences were
false as built, among them D43's "with nothing narrowed": `page.prev.title` still needs
`lem-if page.prev`, since the field is optional, and D43's refinement note now says so.

Probed and recorded, unchanged: `Store::page` is a linear find, so a build is quadratic in
pages (2,000 generated pages loaded in 28 ms and rendered in 47 ms in a debug build), and
under `lem dev` a page whose item leaves its list stays served from the output with stale
neighbour links, the documented "nothing removes output" behaviour that generators make
common (`∆0uV`).

## Retrospective

Planning first paid for itself: the planner found seven places D43 could not be built as
written, and the plan review caught the stale-artifact break the port would have hit on
landing. The shared Cargo target did not: two worktrees and the main checkout built into
one `target/`, and cargo took one checkout's rlibs as fresh for another, which cost a
clean of main's crates to be sure the float fix's artifact was its own. Give every
worktree its own target.

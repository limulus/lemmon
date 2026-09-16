---
id: ∆c9S
completed: 2026-09-15 18:05:16 -07:00
---

## Backlog item

> - ∆c9S Page templates and `plan()` — a `.lem` file becomes a page through front matter as a
>   document does, a generator's front matter names a `site` list with per-item values as
>   defaults on its page type (D41), `plan()` lists every output with its url, entry (D30),
>   and prev/next, and `render` binds each page's own record, closing D37's shared-envelope
>   gap. The port's index, photo, and TIL topic pages (∆ePP) are written this way

## Planning

No code was written for this item. Planning surveyed the code and limulus.net, settled
D43 with Eric, split the item into [[∆Bhz]] and [[∆99b]], and planned ∆Bhz in full
(`docs/plans/∆Bhz-pages-render-through-their-layout.md`).

**The survey is why it split.** Four facts, none of them in the item's text:

- `render` runs a file's own entry function and stops. No page, `.md` included, has ever
  gone through its layout; `Host::entry` was only ever a compile-time check that the
  field is declared.
- A `.lem` file's leading `---` block is text, and a `<lem.` inside it is a tag.
- A page template cannot be loaded: `Module.documents` is `Some` only for `.md`, and a
  file typed by a `type:` key that no glob matched has nowhere in the envelope to go.
- Nothing computes `url`, `prev`, or `next`. The corpus harness invents `url` itself.

So "page templates and `plan()`" was two items: making a page render through its layout at
all, and the generator machinery on top. ∆Bhz took the first, ∆99b the second, and the
`needs:` edges came out ∆1Hp → ∆Bhz → ∆99b, the first at Eric's request because ∆1Hp
reshapes the same `render` path and carries a line-numbered plan.

**Eric settled three questions** (D43):

- **Neighbours belong to generated pages, declared by their own type.** The docs had
  `prev: Page?` and `next: Page?` on the shell's `Page`, which two mechanisms refuse:
  values are immutable and reference-counted, so two pages cannot hold each other, and
  `Page?` is too wide for `page.prev.title` while D32 forbids a subtype from narrowing the
  field. limulus.net decided the scope — its photo pages are the only reader anywhere, and
  nothing reads a collection's neighbours, whose order would have been the order the shell
  listed files in. A neighbour is now a reference into the page table that a field read
  resolves.
- **The generator key is `for:`, read and stripped by the shell, checked at load**, its
  list written `<ambient value>.<field>` as a `by` index is.
- **The split itself**, with `needs:` trailers where landing ∆1Hp first helps.

Four more points were put to him as what the plan would do unless he objected, and stand:
the declaration names the url field so `plan()` can report it, with a repeat a load error;
a page typed by `type:` alone loads from a new `pages` envelope section; a record whose
defaults read `body` cannot type a page template; and a reference is `{path, index?}`.

## Refinement

- **Three reviews ran against the plan and the docs, and both found real work.** The docs
  sweep found that the `body` refusal was written into the docs and the plan but recorded
  in no decision — it is now a clause of D43 — that one placeholder named ∆Bhz for work
  ∆99b does, that `url` was required in the sketch and optional in the plan, and that D30
  still said the shell reads two names of its own.
- **The plan review found three claims that would have failed when built.** The url check
  cannot run where a compile-time check would go: the document type has no base, so
  `check_declarations` skips it, and neither field is a fact of any unit, so a
  declaration-only compile relinks without recompiling. Both it and the entry check move
  to link. A corpus function named `page` cannot exist, since the host binds that name.
  And `body` reads are not tracked by the resolver's `used` flag, which `Implicit`
  bindings are exempt from; the span has to be recorded where the name resolves to the
  binder `compile_step` made.
- **One rule was dropped outright.** "Every compiled page must be carried by the envelope"
  would have thrown on a deleted or renamed page and on a draft, since the resident `Core`
  cannot remove a unit, and it broke an existing loader test. An uncarried page is now
  simply not loaded: `plan()` omits it, and rendering it is a caller bug.
- **The backlog cross-check** found ∆ePP calling the links index a generator, which it is
  not, and no item owning limulus.net's seven index pages; ∆ePP now names both.
- **The placeholder gate reads `docs/plans/` too.** Quoting the phrase it looks for, inside
  the new plan, failed the build until the sentence was reworded.
- **Left open for Eric:** whether ∆4tG gains a `needs:` edge on ∆Bhz or is reworded (its
  text calls `plan()` and handles page templates, and it had no edge on ∆c9S either);
  whether a missing closing `---` should be an error in both modes; and whether a file
  that is no page should warn about top-level output, which [[∆1Hp]]'s plan raised.
- **∆1Hp is being built in the main checkout**, and its plan hands two follow-ons to
  "∆c9S decides what a page is". Both are carried into ∆Bhz's plan; that line wants
  re-pointing when the branches meet.

## Retrospective

**Count the real site before designing a feature for it.** The neighbour design changed
shape once limulus.net was counted: one reader, of two fields, on one kind of page. The
same lesson [[∆RGQ]] and ∆Hbw recorded.

**A mechanism claimed in a plan is not a mechanism checked.** Three of this plan's steps
were wrong in ways only a read against the code could find — where a check can run, what
a name collides with, what the resolver actually records. The review that found them cost
a fraction of the checkpoint it saved.

**Every rule the pass introduces belongs in the decision record.** The `body` refusal was
written into `language.md`, the plan, and the artifact ledger while D43 said nothing about
it. A rule that lives only in prose is one a later session will treat as an accident.

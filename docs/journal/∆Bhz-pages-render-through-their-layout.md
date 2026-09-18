---
id: ∆Bhz
completed: 2026-09-18 06:56:21 -07:00
---

## Backlog item

> - ∆Bhz Pages render through their layout — a `.lem` file's leading `---` block is front
>   matter; page templates load as documents do, and a page typed by `type:` alone loads
>   from a new `pages` envelope section; `render` calls a page's layout with its record and
>   body; `plan()` lists pages with their urls (D43). Today no page renders through its
>   layout. Planned in `docs/plans/∆Bhz-pages-render-through-their-layout.md`; needs: ∆1Hp

## Planning

**The item came out of a split.** The session that planned ∆c9S split it into this item
and ∆99b (generators and neighbours) and settled D43 with Eric on 2026-09-14; see
[[∆c9S]]. It wrote the plan this entry absorbs, with the docs already describing the
target under placeholders naming the two items. A second session re-verified the plan
against `main` on 2026-09-17, weighed waiting for [[∆7wK]], split checkpoint 1 into a
syntax half and a compiler half so the first could go while ∆7wK was in flight, and held
the rest until it landed. This session built all four checkpoints, each opened with a
plain-language preview and closed with a review pass and Eric's commit.

**Why the item was larger than its old text.** Before it, no page rendered through its
layout: `render` ran a file's entry function and returned what it printed, and the
host's entry field had one consumer, a shape check at the schema. A `.lem` file's front
matter was text, so a `<lem.` inside it was a tag. A page template could not be loaded,
since only a `.md` had a page source in the module. A page typed by its own `type:` key
with no glob had nowhere in the envelope to send its front matter. Nothing reported a
url.

**What the design settled, and with whom:**

- **Front matter is one textual rule and a token** (D41). `lemmon_syntax::front_matter`
  finds a first line of exactly `---` through the next such line, for both modes, and
  the block is a `FrontMatter` token first under `Root`, so the tree stays lossless and
  nothing in the YAML is scanned. Eric decided an unclosed block is an error in both
  modes, at its opening line (invariant 10), where a `.md` used to render a rule and
  prose.
- **A page is a page-shaped file compiled with a record**: any `.md`, or a `.lem` that
  opens with front matter. A `.lem` with no front matter takes no record, a compile
  error at the start of the file. Eric decided that such a file's top-level output
  warns once under a host that declares a document type, since a shell never renders
  it and that is where every one of limulus.net's nineteen WebC `<style>` blocks would
  land.
- **A page template has no `body` before it renders.** The compiler records where a
  default first reads `body`, matched by binder so a lambda parameter of that name is
  not mistaken for it, and link refuses a template whose record's plan runs such a
  default, inherited ones included, labelled at each. The loader hands a template's
  defaults null for `body`, so that refusal is the only thing keeping a null out of a
  record; its test is a perturbation that disables it and watches the null arrive.
- **The envelope gains `pages`**, loaded after every binding so a `by` index may read a
  map the site loaded. A page carried twice or never compiled is a caller bug. A page
  not carried is simply not loaded, because the resident `Core` cannot remove a unit
  and a deleted file or a draft would otherwise make every later load throw.
- **The host names the url field**, checked at link with the entry field, because
  neither name is a fact of any unit and a compile that changes only the declaration
  relinks without recompiling. Two loaded pages at one url is a load error at the
  second, labelled at the first.
- **`render_page` renders a page through its entry in one VM**, so `uniqueId`, the
  fragments, and the marks are continuous across body and layout. `vm::invoke` could
  not do it, since its machine has nowhere to collect.
- **`plan()` lists loaded pages in load order**, which follows the bindings' link order
  and can differ from declaration order. A test holds the order where the two differ.
- ∆4tG gained `; needs: ∆Bhz`, since its text calls `plan()` and compiles page templates
  with their record.

## Refinement

**Departures from the plan, by checkpoint.**

- **1b.** The store kept `{path, record}` per page and found one by a scan; the spans
  and `record_span` the plan listed landed in checkpoint 2 with the url pass that reads
  them. The corpus's `.lem` cases that render output were given front matter rather
  than a warning line each, which Eric chose. Twelve Rust tests and two wrapper specs
  compiled a `.lem` with top-level `<lem.page.x>` under a typed host and asserted no
  diagnostics; they became `lem-fn` bodies, because front matter alone would have made
  `page` unknown-typed and defeated the typing check they exist for.
- **2.** The entry-field rule became one predicate, `entry_fault`, that the compiler
  and link both ask, and link also refuses a misshaped entry field and a url field of
  a type other than `url`, each where it is declared. `render_page` takes the entry's
  name as an option and renders the page alone when the host names none. The url pass
  is a pure function over the store's pages, so its unreachable arm, a loaded page
  with no url, is tested directly rather than trusted.
- **3.** The boundary refuses a render of a page with a record in two sentences: one
  after a load that left the page out, one after a compile dropped the store, which
  used to fail at render as "`page` is not bound". `plan()` before any successful load
  throws.

**What the corpus showed.** Fifteen `host/` expectations gained their layout's heading
and nothing else. `host/standalone/ids-across-the-layout.lem` shows the body's id given
out before the layout's. A repeated url needs two pages in one load, which no corpus
case can hold, so it stays in unit tests and the boundary's.

**A checkpoint 1a fallout surfaced in checkpoint 3.** 1a made an unclosed block an
error, which turned `document/front-matter/unclosed.md` into a deliberate-error case
outside `errors/`. `npm run wasm:bench` compiles every document case outside `errors/`
expecting no diagnostics, and it runs in no gate, so it threw only when checkpoint 3
ran it by hand. The case moved to `document/errors/front-matter-unclosed.md` in its own
commit.

**The staged `.wasm`** went from 2,292,523 bytes after 1b to 2,322,239 with the
boundary's `plan`, `pages`, and the render swap.

**Left for the follow-ons:**

- ∆99b: `for:`, expansion at load, neighbours and the page table, and `index` on
  `PageRef`. "Carried twice" is keyed by path, the url pass must run after generators
  expand, and `render_page` gains the page table behind the same signature.
- ∆4tG: stripping `type:`, sending `pages`, deriving a `.lem` page's url, walking
  `plan()`, and deciding what to do with drafts and deleted files, which stay compiled
  in the resident `Core`. Its YAML reader decides which files have front matter before
  compiling, so it must apply exactly `lemmon_syntax::front_matter`'s rule, unclosed
  blocks included, or the shell and the core disagree about which files are pages.
- ∆5MM: tree-sitter-lemmon's front-matter rule, held to `.tags` like any boundary.
- ∆xBO: a page render reads its layout's unit, which its deps must name.
- An entry of `pages` with an unknown field is refused by serde without naming
  `pages`, where a `sources` entry names its binding. Small, and the shell is the only
  writer of that envelope.

## Retrospective

**A corpus case that becomes an error has to move.** I changed what
`document/front-matter/unclosed.md` means in 1a and left it where it was. The corpus
gate was happy, since it blesses whatever a case produces, and the one consumer that
reads the directory convention runs in no gate. When a change turns a clean case into
an error, move it under `errors/` in the same pass, and run the benchmark and the spike
after any corpus reshaping rather than only at the boundary checkpoint.

**I swept placeholders by checkpoint number rather than by what each checkpoint made
false.** `architecture.md` said a page template loads no record until checkpoint 2's
sweep, a checkpoint after 1b had made it untrue. The sweep belongs to the pass that
falsifies the sentence.

**A bulk edit script damaged two sites it should not have matched**, a destructuring
pattern and a test helper, while adding a field to every `Data` literal. Both failed to
compile, so nothing wrong landed, but a pattern narrow enough to match only literals
would have cost less than the repair.

**A fixture's types reached further than I read them.** A store-order fixture with a
layout typed `(Page, html)` made every page schema reachable from a collection, and the
link step reported a cycle I first took for a bug in the new code. It was the fixture:
reachability through a field's type is the rule, and I had not applied it to my own
test data.

**What worked:** landing each deferred piece with its consumer. The store's spans came
with the url pass and the uncarried-page refusal with the boundary, and each arrived
with a test where consuming it changed an outcome.

---
id: ∆7wK
completed: 2026-09-17 19:03:37 -07:00
---

## Backlog item

> - ∆7wK Code marks — every code block a `code` mark keyed by its language word, the whole
>   `<pre>` its default content, its payload (language, info, code) reported at compile by
>   content hash and referenced by id; `mark()` gains a payload typed by the record a claim
>   names, claims gain keys, the shell's `ambient.lem` declares `Code`; syntect and the
>   grammars leave the core (D42). Planned in `docs/plans/∆7wK-code-marks.md`; needs: ∆1Hp

## Planning

**The plan was written against the code, then three of its questions went to Eric.** The
session read D42, D40, ∆1Hp's journal, every file the item touches at `959b860`, comrak
0.54's renderer, and limulus.net's fences (16: eight unlabelled, four `typescript`, two
`rust`, one `txt`, one `bash`). It settled the shape in three checkpoints — claims with
keys and payloads in the VM, fences as marks with the highlighter removed, then the wire
and the wrapper's dispatch — and left three choices open, which Eric answered the same
day:

- **A template mark's key is the payload field the claim names** (`keyedBy`), read at
  compile when the call writes it as a literal and at render when it computes it. A
  convention (the first field, a field named `key`) would have been magic the plugin did
  not state; no keys on template marks would have left `mark("code", …)` unable to reach
  a producer.
- **Payloads are pushed from `compile`** as `{diagnostics, payloads}`, the shape ∆Qr2
  extends with `facts`, rather than pulled by a `payload(id)` method. `Unit.payloads`
  keeps the text, so a pull stays possible later.
- **Staleness is recorded per name and key**, not as D42's one digest per name: the fact
  is the answer a compile consumed, so a highlighter gaining `rust` stales the documents
  that named `rust` and says so, and leaves the rest alone.

**A `math` fence is not a mark, and that changed the mechanism.** The first plan replaced
each code block's node with a raw HTML block the core wrote itself, because comrak
renders a `math` fence before consulting any seam, and D42 said a `math` fence is a
`code` mark keyed `math`. Checked on Eric's prompt: no comrak option turns that special
case off, in 0.54 or 0.55 (the `math_code` option only gates the inline syntax). Eric
decided nothing about `math` needs a producer, so the plan went back to comrak's
highlighter seam: an adapter that writes comrak's own tags and escaped body, with a
numbered sentinel before each `<pre>`, and a parity test holding the page to comrak
rendering with no adapter. D42's "as built" clause and `docs/language.md` § Code blocks
say so.

**The rest of the design followed D42 and ∆1Hp's mark.** `mark(name, payload?, default:)`
took the positional slot ∆1Hp's named `default:` had left free. A payload is checked
against the record the claim names; a claim that names none takes none. `Op::Mark` gained
a key operand (none, a constant, or a payload field read at render) and a payload operand
(none, popped, or a reported id), and link rebases all three. A code block's payload is
`{language, info, code}` named by SipHash-1-3 over the info string and the code, a fixed
algorithm because a host's cache outlives a build of the core; `siphasher` was already
in the tree through comrak's `phf`.

## Refinement

**syntect's cost was what D42 measured.** Removing it took the staged `.wasm` from
4,207,690 to 2,258,225 bytes; checkpoint 3's JSON conversion and reply fields put 30 KB
back, to 2,292,476. The payload id was checked against a SipHash-1-3 written separately
from the paper before its value was pinned in a test.

**Several rules the plan stated turned out wrong or incomplete once tested:**

- A payload nothing claims had to be "a record", but a map literal with nothing to type
  it is a map, and the VM already accepted maps. The compile rule now takes a record, a
  map, or `any`, the same three the VM takes.
- A claim naming an enum as its payload reached `check` and told the author their
  literal was the wrong shape. It is now refused as the host's mistake.
- A computed key accepted markup at render, which no written key could be. Only a
  string keys a mark now, on both paths.
- A payload crosses as JSON, which has no form for a function or a non-finite float, so
  the VM's check at the call refuses those as well as a mark, at any depth.
- A file that compiles beside a broken one goes resident without a relink and is never
  compiled again, so `compile` reports its payloads in that call.
- A claim naming a key field but listing no keys covered every key in the core while
  the wrapper gave its keyless producer none of them. Naming a key field now keys the
  name, so an uncovered key warns where the author wrote it.

**The pairing of rendered blocks with the tree walk rests on comrak.** Each element is
matched to its block by position, which holds because comrak removes an unreferenced
footnote from the tree and reorders the rest to reference order before rendering. Both
are pinned by tests, which is the first thing to rerun after a comrak upgrade.

**Reviews found what the tests did not**: a doc comment split by an inserted function; a
link-time payload dedup that was quadratic on every relink; one variable meaning two
things in the VM's mark op; and a producer listing a key twice refused as two producers.

**Left as they are, and why:**

- A fence records the whole claim answer, payload record included, though fences never
  read it, so a highlighter renaming its payload record recompiles fenced documents for
  nothing. Rare, and narrowing the fact would give it a second shape.
- A document's `body` now holds plain code blocks with no mark, so an excerpt built from
  `body` and shown elsewhere is not highlighted; syntect's colours used to ride along.
  That is D42's second render phase (∆ePP), not something the core can fix.
- A host that registers a producer for `code` without listing languages gets no warnings
  and an `unclaimed` report at build, since the core cannot tell that claim from a
  decorator's. ∆PgF's plugin loading is where to refuse a keyless producer for a keyed
  name.
- A site that declares handlers but no highlighter warns at every labelled fence. That
  is `docs/language.md` as written; ∆Nxd is what a new site lists.

## Retrospective

**Twice I inserted code between a doc comment and the item it documented**, once in
`unit.rs` and once in `value.rs`, by anchoring an insertion on a line that was the start
of the item's docs. The first reached review; the second I caught on reading. Anchor an
insertion on the item's first line of doc comment, or on the item itself with its docs.

**I wrote a test to match a message instead of the message to match its purpose.** The
enum-payload refusal read "names a payload `Placement`, is an enum" and the test I wrote
asserted exactly that. Reading the assertion as an author would caught it.

**Three tests or probes claimed more than they checked.** A corpus case meant to reach
the render-time payload check stopped at a type error first; a footnote test's comment
described a reordering its input never caused; and the `math` unit test only passed
because its `math` fence came last. Each was found by reading the blessed output or by a
probe, which is the argument for running the probe before trusting the test.

**The two biggest design changes came from asking whether a claim was true.** D42 said a
`math` fence is a mark, and one digest per name would do; both were checked against the
code and against what staleness is for, and both changed before any code depended on
them.

**I skipped the plain-language preview before checkpoint 2**, having been told to
continue. It cost nothing here, but the preview is where Eric redirects a checkpoint
before it is built.

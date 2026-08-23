---
id: ∆c8h
completed: 2026-08-23 09:46:33 -07:00
---

## Backlog item

> - ∆c8h Codegen + VM — stack machine per docs/architecture.md §VM (output buffer,
>   Capture/EndCapture for `lem-let` blocks and slots, calls with named-arg reordering,
>   spanned runtime errors), enough to run the spike subset

## Planning

The item named the machine but not its edges, so planning put four scope questions to Eric
before any code existed. Two were answered by taking the piece now: **lambdas compile to
closures**, because the expensive half is capture analysis in the scope resolver and
retrofitting that after the resolver exists is worse than writing it in; and **a
`CallBuiltin` op with an ambient table holding only `raw()`**, because this item owns the
escaping rule and `raw` is the escape hatch that rule is defined against — without it the
`html` half of invariant 7 has no test that is not a `lem-let` block. One was deferred: **no
serde_json**, since `∆TNT` is the item that gives JSON a real consumer and the workspace
manifest says not to pin a dependency before something tests it. Data crosses as Rust
`Value`s, and corpus cases write their own with `lem-let` literals.

The fourth was a language question the checker will eventually own: what an untyped `lem-if`
does with a value that is neither `bool` nor `null`. Settled as **non-null counts as true**,
matching the optional-narrowing reading in `docs/language.md`; `∆C8d` tightens it to a
compile error later.

A survey of limulus.net's own templates shaped the lambda answer. Its ~20 callback sites are
all list operations — `find` by key, `filter` then render, `sortBy`, `map`+`join` for
`srcset` — every one of them a stdlib generic belonging to `∆Q6L`. So lambdas ship here with
no consumer, deliberately: what ships is the closure machinery, and pattern 2 (`filter`
capturing an enclosing `author`/`topic`) is why it had to be closures rather than anonymous
functions.

One rule turned out to be nobody's: "a void tag alone on a line consumes that line's
trailing newline" is in `docs/language.md`, but the scanner keeps text runs verbatim and the
parser has no opinion, so it fell to codegen. The plan chose to apply it to keyword voids
(`lem-let =`, `lem-fn =`, `lem-else`, `lem-else-if`) and **not** to `<lem.…>`, so that
`<h1>\n<lem.title>\n</h1>` renders the newlines it shows.

## Refinement

The parser had been reporting `<lem-fn f(a: int): int = a />` as "not supported yet" while
parsing it perfectly — a diagnostic that meant "no stage below me runs this". A stage below
it now does, so the diagnostic went, its test inverted, and
`syntax/let-fn/expression-bodied-fn.cst` lost exactly those two trailer lines. That is the
whole syntax-side corpus diff.

`Op::Pop` came from the architecture sketch and nothing ever emitted it. The corpus's op
gate turned that into a build failure on the first run, and the honest fix was to delete the
op rather than to write a case reaching it.

A `/code-review high` pass over the working tree confirmed seven findings; all seven were
small, inside the crate, and fixed:

- **Two adjacent void tags defeated the void-line rule.** Each tag's neighbour was only read
  as a line boundary if it was a _text run_, so a pair of them blocked each other:
  `<lem-let a = 1 /> <lem-let b = 2 />` trimmed, and the same line without the space leaked
  a blank line. A neighbouring void tag now counts as a boundary — a line of nothing but
  void tags is still an empty line — while any other tag still does not, because it renders
  onto that line.
- **Duplicate `lem-fn` parameter names were accepted silently** while duplicate _lambda_
  parameters errored, because the signature path allocated slots directly instead of going
  through `declare`. Worse, the caller then got a `missing argument` error for a call that
  supplied it. Parameters now declare like every other binding.
- **`page`/`site`/`build` could be shadowed**, contradicting `docs/language.md`'s "cannot be
  redefined" — a template that bound `page` by accident rendered wrong pages with no signal.
  Now a spanned error. Shadowing _stdlib_ names stays silent, which D23 says should warn;
  there is no warning channel yet and the code says so.
- **The UFCS probe mutated the resolver.** `resolve` records a capture at every function
  level it crosses, and the probe discarded its answer without discarding the capture, so
  `|o| o.name()` beside a local `name` grew an unused capture slot. Replaced with a
  read-only `peek_callable`. The reviewer's framing is the durable part: a resolver whose
  "just checking" path allocates is a trap laid for `∆C8d`.
- Duplicate map-literal keys collapsed last-wins with no diagnostic (now an error — `Map`'s
  replace-in-place is right for merging front matter, wrong for a literal); `lem-for` over a
  map was O(n²) via `iter().nth(i)` (now indexed).

Adding corpus cases for those fixes exposed an eighth problem the review had not: compile
errors came out in **pass** order, not source order — the list jumped 50 → 425 → 465 → back
to 85. The parser sorts its diagnostics; the compiler now sorts by file then span.

Left as noted decisions rather than fixed: `?.` does not short-circuit the rest of its chain
(`a.b?.c.d` errors at `.c`), `&&`/`||` demand a real `bool` while `lem-if` and `?:` accept
any non-null, and `4.0 / 2.0` prints `2` indistinguishably from an int. The first two
tighten under `∆C8d`; the third is float formatting, which `∆Q6L` will settle.

## Retrospective

**A coverage gate finds dead design, not just missing tests.** The op gate's first failure
was `Pop` — an instruction the architecture doc had sketched and nothing needed. The reflex
is to write a case that reaches the uncovered thing; the right move was to delete it. A gate
that asks "does anything reach this?" is asking a design question, and "nothing should" is a
legitimate answer that a new corpus case would have papered over.

**My unit test mirrored my assumption instead of challenging it.** I wrote the void-line
rule, wrote `trim_void_line`'s tests, and they passed — because every case I invented had
_text on both sides_, which is the only shape I had in mind while writing it. The corpus
case inherited the same blind spot. The reviewer found the bug in ten minutes by asking what
happens when the neighbour is not text. When a helper takes "the thing on either side", the
tests owe it every kind of thing that can be there, including nothing. This is [[∆TEJ]]'s
lesson from the other end: there, prose next to code was not a test; here, a test written by
the same mind as the code was not an independent check.

**When several tests in a new subsystem fail at once, suspect the harness.** Five VM tests
failed together with plausible-looking wrong output; the cause was my `run` helper shifting
constant indices _and_ rewriting the ops that referenced them, double-shifting `Const`. Ten
minutes went into reading the VM before the helper. A test helper that transforms its input
is code, and it is the least-tested code in the file.

**"Not in the diff" was the wrong reason to defer, again.** [[∆hw4]] recorded that lesson
about a scanner bug; this item hit the same shape with the parser's "expression-bodied
functions are not supported yet". That diagnostic was correct when written and became false
the moment this crate existed, and nothing would have flagged it — no test asserts that a
"not yet" message still describes reality. Worth a habit: when a stage lands, grep the
stages above it for the "not yet" they wrote about you.

---
id: ∆7Ib
completed: 2026-08-22 11:02:20 -07:00
---

## Backlog item

> - ∆7Ib Parser bake-off — one-day lelwel spike against a hand-written recursive-descent
>   start (docs/architecture.md §Crates names the candidates); keep whichever gives better
>   error recovery for less code, and record the loser and why in the journal

## Planning

Plan-mode produced the comparison's ground rules before either candidate existed: both parse
the same ∆hw4 subset, consume the scanner's tokens unchanged (no re-lexing), and emit the
same s-expression dump over the same fixture set — six well-formed files and ten broken ones
chosen to break _early_ in a realistic file, the lesson from [[∆FDQ]]. Scoring followed the
item's own weighting: recovery quality first, owned code volume second, then build
complexity and wasm cleanliness, fit to the CLAUDE.md invariants, and readiness for a typed
AST layer. The fixtures (`crates/lemmon-syntax/tests/fixtures/`) were written inline before
any agent started so that neither candidate could shape the yardstick.

Eric asked whether the item suited a dynamic workflow. Two candidates with disjoint paths
and identical briefs are a clean two-way fan-out, so the build ran as a four-agent workflow:
two builders in parallel (hand-written RD + Pratt on `rowan` in `crates/lemmon-syntax`;
`lelwel` 0.10.4 in a throwaway `spikes/lelwel-parser/` crate outside the Cargo workspace),
then two judges, each assigned a side to argue and required to score both candidates on
every criterion from the dumps, the code, `wc -l`, and their own probes. Wall-clock for the
build-and-judge round was eighteen minutes.

**Verdict: hand-written.** Both judges concluded so, including the one arguing for lelwel.
Scores (hand-written / lelwel, the two judges within a point of each other): recovery 8/6,
code volume 5/7, build + wasm 7–8/6, invariant fit 8/6, AST readiness 8/5.

Why lelwel lost, since the item asks for the loser on record:

- LL(1) cannot see the name inside the scanner's `OpenTag`/`CloseTag` tokens, so the adapter
  had to split them by builtin (`OpenIf`, `CloseFor`, …). The grammar's vocabulary diverged
  from the scanner's, and every unsupported block close collapsed into one token, so
  `<lem-match x>a</lem-type>` was accepted silently.
- A junk rule inside heads conflicts with the Pratt FIRST sets (lelwel's E012), so
  operator-ish leftovers escape the `Head` node: `<lem.a = b>` cost two diagnostics, one an
  eighteen-token generator list that slipped past the message rewriter.
- Lambda had to be hoisted above the binary operators for LL(1); `x ?? |a| a` was rejected.
- The `&` cut-head recovery demoted the tag to a bare `Error` and added a fallout diagnostic
  pointing at innocent HTML (fixture `broken/05`); `broken/04` cost four diagnostics for
  three mistakes because the misspelt close was classified before nesting was known;
  `<lem-if x />` was guessed to be a block and then blamed as unclosed.
- The 3,857-line generated file must pass `clippy -D warnings` (one grammar change already
  produced a rustc warning fixable only by rewriting the grammar), good messages meant
  string-matching generator text, and nothing documents the generated API — the builder
  learned it by reading the generated file.

What lelwel did well: about 700 fewer owned lines (1,232 vs 1,972), zero runtime crates (the
generator is build-only, so the wasm graph was untouched), a 144-line grammar that reads as
a specification and would seed the tree-sitter grammar, an LL(1) check that turns
head-decidability into a compile error, and a match span-for-span on seven of the ten broken
fixtures. Its `broken/02` shape — the wrong close left in a root `Error` node rather than
adopted as the block's close — is arguably the more honest tree.

The rowan pin, the `docs/architecture.md` §Crates rows, and the crate doc in `lib.rs` were
updated in the same pass. The spike directory was moved out of the repo; the grammar and
both sets of dumps are preserved only in this entry's description, by design.

## Refinement

`/code-review xhigh` over the working tree confirmed nineteen findings and surfaced one more
as plausible. Eight were fixed here, two small ones on top, and the rest became policy
decisions:

- **Unbounded recursion** in expressions and nested blocks aborted under the wasm32 1 MiB
  stack at around three thousand nested `lem-if`s while `parse` promised never to panic. A
  `MAX_DEPTH` of 200 with `enter`/`leave` now stops the recursion with one diagnostic; tests
  run at 100k nested parens and 50k nested blocks, verified on a 1 MiB thread stack in debug
  and release.
- **The same-start-byte diagnostic suppression** — the rule that had done most of the
  one-mistake-one-diagnostic work — was dropping real errors: an "unclosed" report anchored
  at an opener where an unknown-tag error already sat vanished, and `<lem-- note -->`
  followed by a whole file swallowed it into one `UnknownTag` block with a single message.
  Replaced by an explicit error state (suppress only a diagnostic about the current token
  while nothing has been consumed since the last one; diagnostics anchored elsewhere are
  always kept). Fixture `broken/01` caught the first draft of this rule collapsing two
  genuinely unclosed blocks at EOF into one.
- `</lem-if />` parsed silently; `lem-else-if` after `lem-else` inside a `lem-for` got the
  wrong message through match-arm shadowing; an empty argument slot after a named argument
  was reported as a positional one; a reserved word after `.` or as a named-argument name
  was re-read as the `in`/`is` operator; composite types (`[T]`, `{K: V}`, `(A): T`) were
  rejected with a misleading message instead of "not supported yet"; the scanner treated
  only `\n` as a line end, so CRLF sources carried the `\r` inside an unterminated string's
  span.
- The did-you-mean used an absolute edit distance of 2 with table-order ties, so every name
  of two characters or fewer suggested `lem-if`. Now optimal-string-alignment distance,
  case-insensitive, with a cutoff relative to length (`elif` → `else-if`, `-` and `widget` →
  nothing). The void-marker scan no longer counts bracket depth: nothing in a head can
  legitimately be an `=` except the marker, and the depth logic only changed the answer on
  malformed input, in the wrong direction.
- The one `unsafe` block — a `u16` → `SyntaxKind` transmute in the `rowan::Language` impl —
  was replaced by a `syntax_kinds!` macro that emits the enum and an indexed table from one
  variant list, which also removed the hardcoded last-variant bound.

The findings that are recovery _policy_ rather than bugs were split out of ∆hw4 into a new
item, **∆jHE**, because the typed-AST item could not name them within the backlog's
five-line limit and because they must settle before the AST accessors are written against
the tree's shape: a block tag's name wins over a contradicting `/>`; a scanner-cut head adds
no second diagnostic (as worded first, "must not open a block", it would have regressed the
matched-close case that works today); a wrong close never adopts and the author's real close
is never "stray"; unknown tags get one consistent shape rather than inheriting their
did-you-mean neighbour's; scanner `Error` tokens enter the error state. ∆hw4 now carries
only the AST views and the trailing-comma decision, and needs ∆jHE.

Known tail, verified by the review but not taken: `f(,,,)` costs N diagnostics through a
dead stall guard in `comma_list`; the dump's `quoted()` leaves `\t` and `\r` raw; `|a||b| x`
lexes as `PipePipe`; `f(a b: B)` costs three diagnostics; `finish()` allocates a `Vec` per
node (~13% of parse time); `head()` formats its error string eagerly; `fn_signature`
re-implements `comma_list`; D21's "keyword heads with `= expr` are void" over-generalises
what holds only for `lem-let`/`lem-fn`.

## Retrospective

**Two builders from one brief is a fair experiment only if the brief is the yardstick.**
Writing the fixtures and dump format before either agent started was the decision that made
the result trustworthy — the judges could diff trees line for line rather than weigh prose.
The judges' own probes beyond the fixtures (`</lem-type>` closing a `lem-match`,
`<lem-if x />`, a cut `lem-for` head) were what separated the candidates most, which says
the fixture set should have had those cases from the start; they go into ∆TEJ's corpus.

**Assigning each judge a side worked better than asking for neutrality.** The judge arguing
for lelwel produced the most precise account of why it lost, because it had to concede each
point specifically. It also corrected a builder's own numbers: the reported 11,379 generated
lines were 3,857 (three target directories summed).

**The recovery heuristic that works best is the one that is explicit.** "Never two
diagnostics at the same byte" looked like a clean rule and carried most of the fixture wins,
but it was keyed on a coincidence, and the review found three ways it hid real errors. The
error-state replacement is the same size and reads as intent. The same applies to the
bracket-depth counting in the void-marker scan: logic that can only matter on malformed
input deserves a test showing it helps, or removal.

**Policies hidden in a five-line backlog item are policies nobody decided.** ∆hw4 was
carrying "settle the recovery policies the bake-off flagged" as one clause; it took the
review's concrete inputs to see there were five distinct decisions in it, and a split to
make them visible on the critical path rather than absorbed into "typed AST" work.

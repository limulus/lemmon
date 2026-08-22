---
id: ∆hw4
completed: 2026-08-22 12:32:10 -07:00
---

## Backlog item

> - ∆hw4 Typed AST over the ∆7Ib rowan CST for the spike subset — `AstNode` views with typed
>   accessors (`IfTag::condition()`, `ForTag::binding()`, `Expr`…) for the checker and
>   codegen, plus trailing commas decided in decisions.md

## Planning

The item carried one open language question — trailing commas — and the plan put it to Eric
before anything else, since it changes the parser and the tree-sitter grammar alike. The
choice was **allow one trailing comma everywhere** (lists, maps, call arguments, `lem-fn`
parameters, lambda parameters), recorded as D24; empty slots stay errors. That let
`comma_list` drop its after-comma bookkeeping and absorb `fn_signature`'s hand-rolled copy,
a duplication [[∆7Ib]] had listed as known tail.

For the views themselves the plan was rust-analyzer's shape: one newtype per node kind with
a `cast` that checks the kind, accessors that read the tree on demand and return `Option` or
an iterator, and two enums — `Item` for what stands in a body, `Expr` for heads. Two
structural calls were made in the plan and held: the separators `lem-else-if` and `lem-else`
are not items but the boundaries of `IfTag::arms()` and `ForTag::else_items()`, and recovery
`Error` nodes are skipped outright, because a parse with diagnostics never reaches the
checker. The acceptance test was designed before the code: walk every `ok/` fixture through
the views and assert the set of visited byte ranges equals the set of every non-structural
CST node, so that a kind without a view, or an accessor that drops a child, fails the build.

## Refinement

`/code-review high` over the working tree confirmed eleven findings and surfaced three as
plausible. All confirmed findings were applied:

- **The crate already had the wheel.** `rowan::ast` ships `AstNode` (with `type Language`)
  and `support::{child, children, token}`; the first draft had re-implemented both. The
  views now implement rowan's trait — which also makes them usable with `AstPtr` later — and
  the only local addition is a blanket `Spanned` extension giving every view a `Span`
  (invariant 8). `impl From<TextRange> for Span` replaced four hand conversions.
- **Two operator tables.** `BinaryOp::from_kind` re-listed `infix_bp`'s token set, kept in
  sync by a test that enumerated raw kinds. `infix_bp` is now keyed off `BinaryOp` with `is`
  as its one extra case, and the test is gone.
- **Zero-width nodes on empty slots.** `{a: 1,,}` and `f(,)` produced a `MapEntry@11..11`
  and a `Param@10..10` while `[1,,]` produced nothing — a tree boundary tree-sitter could
  never agree with (invariant 9). Both item parsers now report the slot and open no node.
  The `comma_list` stall guard turned out to be dead code (a comma is always consumed before
  it runs) and was removed; one diagnostic per empty slot is the pinned behaviour.
- `ast_enum!::cast` dispatched by cloning the node into every variant's `cast` in turn; it
  now checks `can_cast` on the kind first. `ForTag::body()`/`else_body()` each walked the
  block and built both vectors; they are single-pass iterators `items()`/`else_items()`. The
  unused `CloseTag` view went; `body()` means "the block" on every view that has one;
  `Unsupported/UnknownTag::name()` returned an allocated `String` where every other `name()`
  returns a token — now `open_tag()`.
- A misplaced `lem-else-if` inside a `lem-for` split the body in `IfTag::arms()` but not in
  `ForTag`; both now use one `Block::is_separator`.

The review also confirmed one finding outside the diff: a scanner-cut close tag naming an
_outer_ block (`</lem-for` + newline inside an open `lem-if`) still took the `OuterClose`
path, silently closing the inner block and making the author's real closes "stray" —
contradicting the rule [[∆jHE]] wrote into `docs/language.md`. Eric asked for it in this
item rather than a follow-up: a cut close now never reaches past the innermost block, and
the doc sentence says so.

Not taken: two plausibles that only show on malformed trees (`TernaryExpr::then()` shifts
when the middle operand is missing; `IfArm.condition == None` cannot tell a cut
`lem-else-if` head from a `lem-else`), five recovery candidates that were settled ∆jHE
policy, and S4 (unit tests that mirror the fixture — the same belt-and-braces call as ∆jHE).

The review's orchestrator ended its turn "waiting on 9 verifiers" while the verifiers kept
running and finished on disk; nudging it resumed it, but it re-spawned them and trickled one
verdict per turn. Reading the verifier transcripts directly, as ∆jHE's retrospective
suggested, was faster and complete.

## Retrospective

**Check what the dependency already provides before writing the trait.** The R1 finding was
the most expensive in the review and the cheapest to have avoided: rowan's `ast` module is
one `lib.rs` line away, and the CLAUDE.md rule against reinventing wheels applies to crates
as much as to npm. Ten minutes reading `rowan/src/ast.rs` would have saved a rewrite.

**The completeness walk earned its keep twice.** It caught nothing on the first run — which
is the point — but it made the review fixes safe: renaming accessors, changing `ast_enum!`'s
dispatch, and reworking the `for` body iterators all had a test that would fail if a node
went unreached. Write the structural test before the structure.

**A half-applied patch script looks exactly like a clean one.** A multi-file Python patch
that asserted mid-way left three files untouched while the tests it _had_ changed passed;
only a grep for each intended edit exposed it. When a script edits more than one file,
verify each edit landed — the test suite only proves the files it reached.

**"Not in the diff" is a weak reason to defer a confirmed bug.** The cut-close finding was
∆jHE's territory, but the fix was one condition and two tests, and leaving it would have
minted a backlog item to carry twenty lines. Eric's call to take it now was right; the
default should be to fix confirmed findings that are small and in scope of the crate being
touched, and defer only what needs its own design.

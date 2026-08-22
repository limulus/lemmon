---
id: ∆TEJ
completed: 2026-08-22 15:30:52 -07:00
---

## Backlog item

> - ∆TEJ Golden corpus harness — `tests/corpus/` inputs with expected trees/diagnostics (later
>   expected HTML), cargo-test driven, so language changes are locked by tests from the first
>   week and the future tree-sitter grammar has an agreement target

## Planning

The snapshot tests from [[∆FDQ]] already existed — `crates/lemmon-syntax/tests/fixtures.rs`
walking `tests/fixtures/{ok,broken}/NN-name.lem` — so this item was less "build a harness"
than "move one stage's fixtures to a place every stage can share, and give the tree-sitter
grammar something to agree with". Four decisions shaped it, each written into the code or
`tests/corpus/README.md` rather than left as folklore:

- **Discovery and blessing live in their own crate.** `crates/lemmon-corpus` knows where
  cases live and how an expectation is compared and rewritten, and nothing about scanning,
  parsing, or rendering — so every stage crate takes it as a dev-dependency without a
  cycle, and adding a stage is an extension in `EXPECTATIONS` plus one `check` call rather
  than a second walker with its own blessing protocol.
- **No `ok/` versus `broken/` split.** Whether a case parses clean is *data* — an empty
  diagnostics trailer in its `.cst` — not a directory. A case stays in the directory that
  says what it tests when a language change turns it from clean to broken, or back, and
  the numeric filename prefixes went with the split.
- **The agreement target is a projection, not the tree.** `.tags` keeps tags, close tags,
  text runs, and item-position errors, eliding heads, expressions, and `Block` wrappers.
  A context-free grammar can say where a tag begins and ends; it cannot be held to the
  shape a Pratt parser gave a head. That line is what makes invariant 9 checkable by
  something other than a person.
- **The gates are mechanical.** Every `SyntaxKind` must be reached by some case (escape
  hatch: an `UNREACHABLE` list that today holds only `OpenInHead`, with its reason, and a
  counter-test that fails if an entry becomes reachable); no expectation may outlive the
  input it belongs to.

The item also unblocked a carve-out `CLAUDE.md` had explicitly deferred to it — "the Rust
crates have no coverage gate yet — the tool and threshold get chosen against real code
when the corpus harness lands". Settled as `cargo-llvm-cov` with `--fail-under-lines 97`,
a ratchet set from the measured baseline, in its own CI job because instrumentation needs
different `RUSTFLAGS` and so a second target tree.

The 21 carried-over cases moved byte-identically: every `.txt` became a `.cst` with no
content change, so nothing about the parser's output shifted under the reorganisation.
Eleven new cases followed, and the kind gate is why — `As`, `FnVoid`, `Gt`, `Gte`, and
`UnsupportedTag` were reached by no fixture at all. The `gt`/`gte` gap is the invariant-3
tax made visible: word operators exist precisely because `>` cannot appear in a head, and
nothing had tested them.

## Refinement

A review of the finished change turned up one real bug, one doc contradiction, and four
simplifications; all six were fixed before the item closed.

- **`.tags` leaked head interiors.** `is_boundary` admitted `Error` unconditionally for
  nodes while gating only `Error` *tokens* on item position — contradicting the doc
  comment three lines above it, which said an error inside a head is tag interior and
  elided with the rest. Nine such lines were checked in as expected output across four
  cases, the deepest being `Error@30..39` for `[Article]` inside a `TypeAnnotation` inside
  `Params`. A grammar that does not model heads cannot reproduce those spans, so the
  agreement target as shipped was unmeetable. `is_boundary` now takes an `in_items` flag
  and applies the same test to nodes and tokens, which also collapsed the two token match
  arms into one.
- **The README claimed the opposite of its own test.** It said the projection is
  "deliberately a *partition* of the file — the corpus test asserts its top level tiles
  every byte", while `tag_dump`'s doc and
  `projects_tag_boundaries_that_nest_and_never_overlap` both say tiling is deliberately
  *not* asserted (trivia between items belongs to no boundary; a void tag whose head the
  scanner cut ends at its last head token). Rewritten to the property that actually holds.
- **One orphan gate, not two.** `check_no_orphans` was exported and called from
  `lemmon-syntax`'s integration test while `lemmon-corpus` also asserted it on the real
  corpus. The export went; the gate now runs once, in the crate that owns the layout,
  and no future stage crate has to remember to call it.
- `read_or_empty` returned `unwrap_or_default()`, so a permissions or encoding failure
  read as an empty expectation — a diff a blessing run would quietly absorb. Now empty
  only for `NotFound`, panicking otherwise, with both arms tested.
- The nesting test's `open[depth - 1]` would underflow on a depth-zero line after the
  root, reporting an arithmetic overflow instead of its own message; the guard is now a
  range check. `SyntaxKind::all()` became `SyntaxKind::ALL`.

Deferred by design, not oversight: `.html` joins the table with [[∆c8h]], a `document/`
group with [[∆Ldn]], and the agreement test itself with ∆5MM — this item builds the
target, nothing diffs a grammar against it yet.

## Retrospective

**A blessing harness will bless a bug.** The nine wrong `.tags` lines were not a
regression that slipped past review; they were checked in *as the expectation*, which is
exactly what blessing invites — the output looks intentional because a human ran the
command that wrote it. The README already says "blessing is a proposal, not a fix", and
that was still not enough. What catches this class is a property the expectations must
satisfy independently of what the code produced.

**The property that existed was orthogonal to the bug.**
`projects_tag_boundaries_that_nest_and_never_overlap` passed on the wrong output, because
head-interior errors nest perfectly well inside their tag. The doc comment stated the
missing property in prose — errors are kept "only where items stand" — and prose next to
code is not a test. When a doc comment states a rule the code is supposed to obey, that
sentence is a test waiting to be written.

**Two documents, written at different times, disagreed about the same design.** Nobody
would write "is a partition" and "is deliberately not a partition" in one sitting. The
README paragraph plausibly predates the decision the code and its test record. Worth a
pass over the prose whenever a design point moves, since neither `cargo test` nor
`npm run verify` will ever read it.

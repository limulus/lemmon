---
id: ∆1vS
completed: 2026-09-05 09:04:03 -07:00
---

## Backlog item

> - ∆1vS Ambient bindings — a `from` clause on the `lem-let` head (`name: T from "glob"` /
>   `from plugin("p")`, void like `= expr`; top of `site.lem` only), with AST views and
>   corpus `.cst`/`.tags`; `lem-collection`/`lem-data` retired from the builtin table and
>   the unsupported corpus; a syntax for environment values. No semantics: it is the
>   grammar D30 needs so `site` is typed from `site.lem`'s own bindings, nothing built in

## Planning

Seven choices were made in planning and Eric took all of them as recommended. The `from`
form got its own node kind, `LetFrom`, rather than a source child inside `LetVoid`, because
every consumer — the VM now, the checker and the shell reading the reported globs later —
dispatches on the form. The head-shape rule stayed head-decidable: a `lem-let` is void when
its head holds `=` or a `from` past its first token, the first token being the name, so a
capture named `from` is still a block (the `by` field precedent from [[∆up2]]). The type
annotation is required on a `from` binding, reported at the `from`, because nothing can
infer a type from a glob and the type is what tells the host what to load. The source is one
expression, stored like a field's `by` index; which source forms mean anything is the host's
question. Environment values are `from env("NAME")`, the same shape as `plugin("…")`, a call
naming a host source with a literal argument, so the core still names nothing; `env.NAME`
would have made `env` a value the core knows, and a `lem-env` tag a builtin for a value the
host produces. Placement is the VM's to report, where `lem-fn`'s file-level check already
lives. `lem-collection` and `lem-data` left the builtin table outright and became unknown
tags; neither gets a did-you-mean, since the distance rule finds nothing within half their
length.

Left out on purpose: `lem-import`'s `from` head stays opaque until ∆PgF gives it meaning.

## Refinement

Building it as planned surfaced nothing; the review did. `/code-review medium` with Opus
agents confirmed eight findings, all fixed here, and refuted or set aside four more.

- **A scanner error in the name slot flipped the shape.** `bump` skips error tokens, so
  by the time the head was scanned, the slice began at the `from` keyword — and the
  scan's "skip the name" discarded it. `<lem-let @ from "x" />` became a block that
  swallowed the rest of the file. The name slot is now skipped only when nothing was
  displaced, and `let_from_head` reads the current `from` as the keyword.
- **A document's top level took the permissive arm.** The compiler carried one boolean,
  true for a template's top level and a document's alike; `lem-fn` never needed more. A
  `from` binding in a `.md` was told it merely awaited the loader. A three-state `Level`
  (file, document, nested) sends documents and nested bindings to the placement arm,
  whose message now says what the core can enforce — "top level of a template" — instead
  of naming `site.lem`, which the core cannot identify.
- **`name: from "src"` took `from` as a type name** and blamed the source for standing
  before a `from` no longer ahead. A `from` directly after the colon now reports the
  missing type. This is the mirror of [[∆up2]]'s `by` bug: the keyword that chose the
  shape was re-read as something else one step later.
- **The missing-type guard duplicated `error_expected`'s suppression** and, by mutation
  testing, was covered by no test: deleting it passed the suite, and the cut-head half
  was unreachable. `Parser::error_at_current` owns the rule now, `error_expected`
  delegates to it, and the displaced half has a test.
- **`LetFrom::name()` returned the keyword when the name was missing**, as did
  `Field::name()` with `by`. Both now take the first significant element only when it is
  an identifier, with negative-half tests.
- **Prose the change falsified**: CLAUDE.md invariant 4 and the D21 row still called `=`
  the only void marker.
- **Two duplications**: `skip_until_by` copied `Parser::skip_while` and is now a one-line
  delegation; `LetFrom::source()` re-derived the keyword position and is now
  `head_child`, since the parser reads exactly one expression in that head.

Two things about the environment, for the next person on this machine: wasm-bindgen-cli
was not installed and had to be (`cargo install wasm-bindgen-cli --version 0.2.127
--locked`), and a local `npm run rust:cov` reported 85% with execution counts on comment
lines after its first run overlapped `npm run verify`. A fresh worktree gave 98%, and
deleting `target/llvm-cov-target` and rerunning gave the same, so that directory is the
thing to suspect when the ratchet fails for no visible reason.

## Retrospective

**The recovery paths were the whole review.** Every clean shape parsed right the first
time; all three correctness findings were about what happens after a mistake — a scanner
error, a missing type, a missing name. The corpus mistakes case was written from the
author's seat, but an author's mistakes are still well-formed tokens. The shapes that
broke were a scanner error in the one slot the head rule counts, and a keyword landing in
the one slot that accepts any identifier. Next time a rule counts tokens or positions,
the tests owe it the case where a scanner error sits in the counted slot.

**A boolean that was sufficient for one rule was reused for a stricter one.** `file_level`
answered `lem-fn`'s question and silently answered the `from` binding's question wrong.
When a new construct is placed by an existing flag, ask whether the flag's states are the
construct's states.

**Mutation testing found what the ratchet cannot.** The suppression guard was on an
executed line, so line coverage counted it covered while every conjunct was inert. That
is the [[∆up2]] lesson again — a suppression rule is tested on both halves — with a
sharper tool: delete the guard and see what fails. Nothing did.

**Dedupe before verifying.** Eight finder angles produced twenty-nine candidates that
collapsed to twelve; the overlap was the reuse, simplification, and altitude angles
naming the same helper from three directions. Verifying each once was the right cost.

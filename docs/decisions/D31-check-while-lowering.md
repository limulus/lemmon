# D31 — Check while lowering

- **Status:** Accepted 2026-09-06 (∆C8d)
- **Backlog item:** ∆C8d
- **Normative text:** `docs/language.md` § Types › Checking; `docs/architecture.md`
  § Build pipeline (step 3)

## Context

The architecture set lists the core's stages as scanner → parser → checker → compiler →
VM, and the checker was sequenced as its own backlog item after codegen. When it came to
be written, the compiler already walked every item and head in evaluation order with one
resolver (∆e4d) and one export index (∆e5i), and the only consumer a separate checker's
output would have had was codegen — which, for this item, needed nothing from it: no
instruction changes when a type is known, and typed lowering (record fields by index, the
`TypeTest` op) is ∆PCK's. A separate pass would have re-walked the tree with a second
scoping that had to agree with codegen's, and produced a per-node type table nothing read.

## Decision

The checker is the compiler's walk. `expr(&Expr, expected) -> Type` emits an expression's
code and returns its type; bidirectional checking flows an expectation into literals,
lambdas, and conditionals and compares everything else after inferring it. Bindings carry
their types in the one resolver (`Names`), which also records whether a binding was read,
so the unused rule fires when a scope closes. Narrowing is an entry on a stack the arm
pushes and pops, keyed by the binding's identity. A type error is a compile diagnostic
like any other: the file yields no unit, and every other rule in the file is still
reported, because an expression that failed types as `any` and lowering goes on.

## Alternatives considered

- **A separate checker pass over the AST, before codegen.** Rejected: a second walk with
  its own scoping — the two-views shape ∆e4d and ∆e5i had just removed — and a
  `SyntaxNodePtr → Type` side table with no reader until ∆PCK. The repo's rule that a
  recorded fact needs a consumer applied to the table as a whole.
- **A checker over compiled units.** Rejected: bytecode has lost the shape the rules are
  about (an expression's expectation, a literal's elements), and the program is small
  enough that "re-checking without recompiling" buys nothing measurable.

## Consequences

- Typed lowering falls out: when ∆PCK needs a record's field index or a `TypeTest`
  operand, the type is in hand at the emit site.
- The LSP's hover (∆5MM) will want a per-node type table; it is added as a recording
  flag on the same walk, with its reader, not before.
- The rules are applied where the lowering they guard happens, but they are not
  written there: the walk (`compile/expr.rs`, `compile/items.rs`) emits, asks, and
  reports, and the rules it asks — operator, member, index, condition, print,
  iteration, arity — are pure functions from types to a type or a message in
  `compile/rules.rs`, testable without compiling a template (∆2c6). The relations they
  are built on — `Type`, `assignable`, `join` — are a module of their own
  (`crates/lemmon-vm/src/types.rs`), and annotations resolve in another. A rule that
  needs a declaration table (∆cea's field and variant rules) takes it as an argument.
- Signatures are read from heads alone, so an expression-bodied `lem-fn` must declare its
  return type: inferring it from the body would make a caller's unit depend on that body
  (invariant 11).

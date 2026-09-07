# D32 — Records and enums

- **Status:** Accepted 2026-09-06 (∆cea)
- **Backlog item:** ∆cea
- **Normative text:** `docs/language.md` § Types (the records-and-enums paragraph),
  § Schemas (`extends`), § Checking (Fields, Variants, Declarations)

## Context

∆C8d landed the checker with every non-builtin type name an error, because nothing could
produce a record value before the loader (∆iCW) and runtime type tags (∆PCK). ∆cea had to
give `lem-type` and `lem-enum` a meaning the checker could use with `any` as the only
producer, and settle four things the docs had left open: how a declared type is
represented, what a subtype may do to an inherited field (D30's `layout` question), how a
bare variant name resolves, and what an enum is at run time.

## Decision

**Declared types are nominal, by name.** A type annotation naming a declaration resolves
to one `Type::Named` — a name alone. The export index holds a type namespace beside the
functions, under the same first-offer-wins duplicate rule, and the rules that need to know
what a name is — field access, assignability along the `extends` chain, printing,
variants — take the declaration table as an argument, as D31 said they would. Whether a
name is declared is checked once every file's declarations are in, at the name's span.

**A subtype redeclares a field only with the identical type.** That changes the field's
default and nothing else, so a subtype is always usable for its base. `layout` therefore
needs no special rule: `Article` declares `layout: (Article, html): html`, `LinkPost
extends Article` inherits it, and a layout only a link post needs takes an `Article` and
narrows inside (∆PCK). A base must be a record — not an enum, not a builtin type, not the
type itself through any chain.

**A bare variant resolves only where its enum is expected** — an argument (a UFCS
receiver included), an annotated binding, a declared return, a `?:` branch under an
expectation — and on the right of `==`/`!=`/`in` when the left is an enum: the right
operand is lowered expecting the enum (a list of it under `in`) and the operator rule
then judges the two types, so an optional on either side compares as it always did.
Under an enum expectation the variant wins over a local of the same name, which is then
unused unless something else reads it; anywhere else a bare variant is an undefined name.

**An enum is its variant's name at run time**, a string: it compares structurally, prints
as the name, and a front-matter string needs no conversion at load. A record value stays
a map until ∆PCK's `Value::Record`.

**A consumed declaration is a compile-time fact** (D25): a unit records, span-free, the
declaration of every foreign type its checking reached — transitively through bases and
field types, each at the use that reached it — and a declaration whose shape changed
under a consumer is a link diagnostic naming the consumer. A default is not part of the
shape.

## Alternatives considered

- **Separate `Record` and `Enum` type variants, decided at annotation time.** Rejected:
  resolving an annotation would then need the namespace, so signature resolution moves
  out of `prepare` and the driver grows from three phases to four in both `compile` and
  the resident core, for a distinction every rule can read from the table anyway.
- **Ancestry embedded in the type**, so `assignable` needs no table. Rejected: a chain
  can only be built once every file's declarations are in, which puts a whole-program
  topological pass before any file compiles; the table answers the same question at
  check time.
- **No field redeclaration at all** (the backlog item's wording). Rejected: the corpus
  and the limulus.net port both re-default a field in a subtype (`title` from
  `linkTitle`), and forbidding a same-type redeclaration buys nothing the type rule
  does not already guarantee.
- **A special rule for `layout`** letting a subtype narrow its parameter. Rejected: the
  same-type rule makes the question moot, and a rule for one field name is a name the
  core knows (D30).
- **Precise fact recording** through a recorder inside the declaration table. Rejected:
  a type consulted only through a chain has no span in the consumer, and a stale
  diagnostic needs one (invariant 8). Recording the closure at the sites that have a
  span over-records and costs a needless recompile, never a wrong one.

## Consequences

- The canonical corpus case is typed end to end: `Article`, `Photo`, and `Vod` are
  declared, every `: any` parameter is a record, and the output is byte-identical.
- `member`, `printable`, `expected`, `operator`, and the literal rules take a `&dyn
  Decls`; `()` is the empty table the rules are tested against.
- `x.f(…)` lowers the receiver before deciding whether `f` is a field or a function, so
  its type is in hand — the receiver is the first value pushed on either path. A
  receiver that could never have a field (a list, map, or lambda literal, or a bare
  variant) is lowered expecting `f`'s first parameter, so it is checked as an argument
  would be.
- Member-path narrowing (`<lem-if article.hero>` narrowing `article.hero`) is ∆PCK's;
  until then a plain `lem-let` of the field narrows.
- Defaults and `by` indexes are parsed and kept, not checked: their scope is the loader's
  (∆iCW).
- The enum and `lem-match` questions stay deferred; the readings here are provisional.

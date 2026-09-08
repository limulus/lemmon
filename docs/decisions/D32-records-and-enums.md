# D32 — Records and enums

- **Status:** Accepted 2026-09-06 (∆cea); revised 2026-09-07 (∆EN6) to spell a
  variant with a dot, retiring the bare-variant rule and the shadowing it needed. D34
  (∆495) then retired UFCS, so a receiver is no longer one of the places an argument's
  expectation reaches; nothing else here moved with it
- **Backlog item:** ∆cea, ∆EN6
- **Normative text:** `docs/language.md` § Types (the records-and-enums paragraph),
  § Expressions (Variants), § Schemas (`extends`), § Names & namespaces (`Name.x`),
  § Checking (Fields, Variants, Where an expectation reaches, Declarations)

## Context

∆C8d landed the checker with every non-builtin type name an error, because nothing could
produce a record value before the loader (∆iCW) and runtime type tags (∆PCK, landed as
D35). ∆cea had to
give `lem-type` and `lem-enum` a meaning the checker could use with `any` as the only
producer, and settle four things the docs had left open: how a declared type is
represented, what a subtype may do to an inherited field (D30's `layout` question), how a
bare variant name resolves, and what an enum is at run time.

## Decision

∆EN6 reopened one of those four. The bare-variant rule had needed a special case at
every turn — shadowing, a flag threaded through the rule to withhold a refusal, an arm
in the UFCS receiver check — and the ∆cea retrospective named that as a design signal
rather than a debt to carry. The question it reopens is only how a variant is spelled;
everything else below stands as accepted.

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

**A variant is written with a dot** (∆EN6). `.hero` is a variant of the enum expected
where it stands — an argument, an annotated binding, a declared return, a `?:` branch
under an expectation, and the right of `==`/`!=`/`in` when the left is an enum, which is
lowered expecting the enum (a list of it under `in`) before the operator rule judges the
two types. Where no enum is expected, `.hero` is an
error naming the qualified spelling instead. **`Placement.hero` needs no expectation**:
`Name.x` is read in the type namespace before the value namespace, so a qualified
variant means the same thing wherever it stands — a local of that spelling is still read
as a value by its bare name, and warns as unused if nothing does. A bare name is
therefore never a variant, which is what retires the original reading below.

**An enum is its variant's name at run time**, a string: it compares structurally, prints
as the name, and a front-matter string needs no conversion at load. A record value stays
a map until ∆PCK's `Value::Record` (landed: D35).

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
- **A bare variant, resolved by expectation alone** — the original ∆cea reading:
  under an enum expectation a bare name is that enum's variant before it is anything
  else, shadowing a local of the same spelling. Rejected by ∆EN6: it made a name's
  meaning depend on what was expected around it, and every gap it opened needed its own
  special case — a shadowing rule, a `is_name` flag threaded through the rule so a
  refusal could be withheld, and a UFCS receiver arm that asked the variant rule what a
  bare name was. The dot asks the question the author already knows the answer to.
- **A dotted declaration head** (`Placement = .hero | .left`), for one spelling
  everywhere. Rejected: nothing after `=` in an enum head could be anything but a
  variant, so the dot disambiguates nothing there and only makes the head noisier;
  the head is where a variant's name is defined, and the dot is what refers to it.
- **The value namespace answering `Name.x` first**, with the enum as the fall-through
  when nothing is in scope. Rejected: it makes a fixed spelling mean different things
  in different scopes — the same shape as the bare-variant rule this revision retires.
  Letting the enum claim `Name.x` costs a local named for a type its field access, and
  D23's capitalization convention makes such a local pathological.
- **Refusing a binding that collides with a declared type name.** Rejected: it makes
  the type namespace police the value namespace's binding rules, a change to D23
  reaching well past enums, to close a collision the narrow rule already answers.
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
  receiver that could never have a field (a list, map, or lambda literal, or a dotted
  variant) is lowered expecting `f`'s first parameter, so it is checked as an argument
  would be. A qualified variant is not a receiver: `Placement.hero(…)` calls the
  variant, which is an error (∆EN6).
- Member-path narrowing (`<lem-if article.hero>` narrowing `article.hero`) is ∆PCK's;
  until then a plain `lem-let` of the field narrows. (Landed: D35.)
- The dotted form needs an expectation, so it is refused where none reaches: printing,
  a `lem-if` or `lem-match` subject, the left of `==`/`in`/`??`, an unannotated
  `lem-let`, an argument of a builtin or of an `any`, and every element of a literal
  that is not itself expected, the first included.
  Comparing against an ambient value its host named without typing — `any` — is
  the common one.
  The qualified form covers every such place, and the refusal names it (∆EN6).
- A `lem-case` pattern takes the dotted form only: the bare slot stays `Type as name`,
  whose binding is optional, and the parser resolves no name that could tell them
  apart. `<lem-case hero>` is therefore a type pattern, and ∆PCK reports it as the
  unknown type it is (landed: D35).
- A declared type used as a value reports what it is (`` `Placement` is an enum, not a
  value ``) rather than an undefined name — the author who wrote a bare variant is the
  one who most often lands there (∆EN6).
- Defaults and `by` indexes are parsed and kept, not checked: their scope is the loader's
  (∆iCW).
- Closed unions and `lem-match` exhaustiveness stay deferred, and what an enum is at
  run time stays provisional; how a variant is spelled no longer is (∆EN6).

# D35 — Runtime type tags and narrowing

- **Status:** Accepted 2026-09-07 (∆PCK); revised 2026-09-07 (∆Iau) in two clauses —
  a narrowed path may be rooted at an ambient value, now that a host types one and no
  binding may take its name; and tagging loaded data is the loader's alone (∆iCW),
  since only the record store it builds knows which page's record a document's ambient
  value carries; revised 2026-09-08 (∆iCW, D37) — the closed ancestry table is replaced
  by the declaration table the module carries, walked per test, so the VM's type tests
  and the loader's validation read one table; the completeness question is closed:
  the loader fills an omitted field as a literal does, by whether its type admits `null`;
  and the tagging clause is delivered, with the untyped values it leaves as `any` named
- **Backlog item:** ∆PCK
- **Normative text:** `docs/language.md` § Tags (`lem-if`, `lem-match` rows),
  § Expressions (Literals, Type tests), § Checking (Literals, Narrowing, Type tests,
  Matching); `docs/architecture.md` § VM

## Context

∆cea made the checker read records by declaration while a record value stayed a map:
nothing produced a record before the loader (∆iCW) and runtime type tags. So `is` and
`lem-match` were refused, narrowing covered a plain optional local only, and the canonical
corpus case carried a `lem-let` for a field it could not narrow. This item had to say what
a record is at run time, who makes one, what a type test does with everything else, how
far narrowing reaches, and what a `lem-match` must cover — the last on the provisional
reading, since exhaustiveness stays a deferred question.

## Decision

**A record value carries its declared name.** `Value::Record` is a type name and a map
of fields, and the name is the most specific type the value was made as. Fields are read
by name: reading by declaration index is an optimisation with no consumer, and D31's rule
— nothing recorded until something reads it — applies. The linked module carries the
project's declaration table, and a type test walks a record's `extends` links in it
(revised by D37: the closed ancestry table went when the loader needed the declarations
themselves, and one table for both is worth a walk of a few hops). The walk ends at a
base the table lacks or a cycle, and a tag the table has no row for is no type at all
rather than a type that is itself — a module whose constant pool went wrong must not pass
a test against its own bad tag.

**A map literal where a record is expected is that record** — the expected type, never a
subtype. Every key must be a declared field, each value is checked against it, an omitted
optional field is filled with null, and an omitted non-optional field is refused whether
or not it has a default: defaults are evaluated at load, where `body` and `site` exist,
and a declaration's facts (D25) do not carry them. The loader tags what it validates at
`loadData` (D37): every record in a typed ambient value or a `from` binding. What it
leaves as sent stays untagged `any` — an untyped host's values, and the host's document
value where no loaded document stands in for it — and a type test over one of those is
a render error.

**A type test reads the tag, and only a tag.** `x is T` over a record answers by name and
ancestry; over `null` it is false; over anything else — an untagged map that came through
`any` — it is a render error. Guessing `false` would let data that predates the loader
pass a test silently (invariant 10). The checker refuses a test that could not go either
way: the subject's own type on a non-optional, an ancestor, or an unrelated record. Its
own type on an optional is a null test and stands.

**Narrowing is by path.** What a condition narrows is a local or a field path off one,
keyed by the binding's identity and the field names, so an inner shadow ends it. Values
are immutable, which is what makes a narrowed path hold for the arm, a lambda inside
included. Only a plain `.` extends a path; `?.` yields an optional by construction, and
an operator (`&&`, `!`) narrows nothing. A path is rooted at a local, by the span of its
name token, or — since ∆Iau — at an ambient value, by its name: no binding may take one
(D23), so nothing can shadow it and no span is needed to tell two apart.

**`lem-match` on the provisional reading.** The subject is evaluated once into a slot no
name reaches; each case is a test and a jump. A type case narrows the subject path and
binds its `as` name; a literal compares as `==`; `null` needs an optional; `.v` needs an
enum. A pattern spelled twice is refused at the repeat. Without a default the cases must
cover the subject, and only an enum can be covered — every variant, plus `null` for a
`T?`; a record needs a default because its set of subtypes is open, and so does anything
else. `bool` completeness and any closed-union form stay with the deferred question.

**The `any` receiver agrees with the checker.** `x.f(…)` on an `any` reads a record's
field and takes a map's method — what the typed path does for each — so a map whose key
spells a method's name resolves the same way typed and untyped. Equality between a record
and a map is `false`, and a record has no methods: both mirror what the checker would
have refused, which is the only sense in which they guess by type.

## Alternatives considered

- **`false` for an untagged value under a type test.** Rejected: it hides the one data
  problem the tag exists to expose, and loaded data is untagged until ∆iCW.
- **Filling omitted defaulted fields in a literal.** Rejected: a default expression sees
  `body`, `path`, `self`, and `site`, which a template cannot supply; and carrying
  defaults in the declaration fact changes the cache key for something nothing in a
  template can evaluate.
- **Embedding a record's ancestry chain in the value at construction.** Rejected: the
  loader and the ambient values will have a name at the JSON boundary and nothing else,
  so the module's table serves every producer alike.
- **Reading fields by declaration index.** Deferred, not rejected: the checker has the
  index in hand at the emit site, and nothing measures a cost yet.
- **A name-to-base table walked per test.** Rejected during review, then adopted by
  D37 once the module carried the declarations for the loader: the walk is a few hops
  over a table of tens, it collects nothing, and its guards end it at a missing base or
  a cycle. The objection that stands — "itself" for an unknown tag — is answered by the
  walk finding no row and saying `false`.
- **Exhaustiveness over `bool` and `null`-complete optionals.** Left with the deferred
  question rather than decided here.

## Consequences

- `Value::Record`, `Op::MakeRecord`, `Op::TypeTest`, and `Module.declarations` exist;
  the `Op::NAMES` gate covers both ops, and `link`'s relocation of their constant
  operands is negative-tested at a non-zero offset, which every single-file corpus case
  misses.
- **A record's completeness is the producer's.** `Value::record` accepts any map, while
  `field` and `equals` assume every declared field is present. Both producers keep it
  by one rule (D37): an omitted field is `null` exactly when its type admits one, and
  the loader fills it so before any default runs, so a loaded `Article` without `hero`
  compares equal to a literal one and a default reading `self.hero` sees `null`.
- `render/calls/methods-on-any` pins the agreement of the two receiver paths, and
  `render/errors/is-on-untyped-value` the render error.
- The canonical corpus case reads `article.vod.poster` again, with its output byte
  for byte as before.
- D31's "record fields by index" and the architecture doc's "GetField is an index"
  describe an optimisation not taken; both are marked.
- `docs/artifact-changes.md` carries the VM value set, the two ops, the module table,
  and the Narrowing/Matching rules for ∆MlD.

# D34 — Builtin methods

- **Status:** Accepted 2026-09-07 (∆495); dispatch and UFCS's removal implemented
  2026-09-07 (∆495), the argument-shaping rule and higher-order methods (`map`, `filter`,
  over a re-entrant invoke) 2026-09-07 (∆987)
- **Backlog item:** ∆495, ∆987
- **Normative text:** `docs/language.md` § Expressions (Access, Calls), § Names &
  namespaces (Ambient), § Checking (Calls, Fields)

## Context

UFCS was how the stdlib was going to read. § Checking puts generics on builtins only —
`filter`, `map`, `sortBy`, `groupBy` — and those are chained or they are nothing:
`articles.filter(…).sortBy(…).take(5)` against `take(sortBy(filter(articles, …), …), 5)`,
on every feed and index page of the port (∆rT5). It also let a plugin's function read as a
method, `photo.rendition(md, jpeg)`.

The price was that `x.f(…)` asked the receiver's type *and* the enclosing scope, and
everything that met a call had to answer to both. D32's bare variant needed an arm in the
receiver check; ∆EN6 retired the bare variant and the dotted one inherited the arm, which
made `<lem..left.place()>` legal source. The probe that asks the scope for `f` before the
receiver is lowered has produced two bugs on its own — a capture recorded for a name that
compiled to nothing (∆c8h), a receiver lowered without the expectation that checked it
(∆cea). And a call's meaning depends on a declaration in another file: a record that gains
a field `f` turns `x.f(y)` from a call of the function into a call of the field, legal
both ways and so silent, though D25's fact recording does make the unit stale.

The fallback the deferred list named — a pipe operator, "likely redundant given UFCS" —
is not available at any price: invariant 3 ends a tag at the first `>` outside a string
literal, so `|>`, `->`, and `=>` cannot be written in a head.

## Decision

**A builtin type carries its own functions.** `[T]`, `{K: V}`, `string`, `int`, `float`,
`bool`, `datetime`, `duration`, `url`, `html`, and `asset` each name methods, written
`xs.filter(|a| …)` and resolved against the receiver's type. The set is the core's and
closed: nothing an author or a plugin writes adds to it.

**`.` asks the receiver's type and nothing else.** A record's field, or a builtin type's
method — one question with two answers. UFCS is retired: `x.f(…)` never consults the
value namespace, so no project function can capture a call on a type it knows nothing
about, and no field added elsewhere can capture one back. `photo.rendition(md, jpeg)`
becomes `rendition(photo, md, jpeg)`: builtin types are the core's and carry behaviour,
records are the author's and take functions.

**A method is called, never read.** `xs.filter` standing alone is an error naming the call
form, as `<lem.f>` on a `lem-fn` is — one call form (D22), no bound-method values.

**A method belongs to the type it operates on; a function makes a value or vouches for
one.** A stdlib entry whose first parameter *is* the value it works on is a method —
`xs.filter`, `s.trim`, `s.slugify`, `d.rfc3339`. One that constructs from something else,
or casts, is an ambient function — `asset()`, `url()`, `datetime()`, `uniqueId()`, and
`raw()`, which stays a function so the one `string` → `html` cast stays conspicuous
(invariant 7). ∆Q6L writes the inventory against this line.

**Method names occupy no namespace**, as builtin `lem-*` tags do not (revising D23). A
`lem-fn map(…)` displaces nothing and warns about nothing; the ambient value namespace
holds only the functions above.

**A builtin rule may shape the arguments after it.** `Builtin::ty` is already a rule
rather than a signature, for exactly this reason; it gains a companion that says what the
*n*th argument is expected to be given the types already in hand, and arguments lower left
to right. That is what makes `xs.map(|a| a.date)` take the lambda's parameter from the
receiver's element type. It needed one more thing nothing had when this was decided: a
builtin could not call a `Value::Fn`, so a re-entrant invoke arrived with it (∆987).

**Through `?.` a method call propagates null**, as a field read does: `xs?.filter(…)` is
optional in whatever `filter` returns.

**A method on `any` is the VM's.** With no compile-time type the VM tries the field, then
the method table for the runtime value's type, and errors with the span if neither
answers — the `any` path, not a second checker.

## Alternatives considered

- **Keep UFCS.** Rejected: the chaining it was kept for is what methods deliver, and
  everything else it brought — a dot that reads the enclosing scope, the probe, the
  foreign-field capture, an arm every new receiver form has to be taught about — is cost
  with nothing left on the other side.
- **Keep UFCS and refuse only the receivers typed by expectation** (a list, map, or lambda
  literal, or a dotted variant). Rejected: it does fix `<lem..left.place()>` and delete
  the receiver-expectation branch, but the collection stdlib would still read a call's
  meaning out of the enclosing scope, which is the part worth removing.
- **Drop UFCS and add a pipe operator.** Rejected: invariant 3 forbids the spelling, and
  the `>`-free candidates are all worse than the problem — bare `|` collides with lambda
  parameters and `||`, and a word operator is a new keyword in a language that has kept
  them to tag heads. Methods answer the chaining without an operator at all.
- **Free functions and inside-out nesting**, no methods and no pipe. Rejected:
  `take(sortBy(filter(articles, …), …), 5)` is the shape of every feed, sitemap, and index
  page in the port; the language would be paying for UFCS's removal on every page.
- **Author-declared methods on records.** Rejected: a record is a declaration of fields
  and nothing else (D30, D32), and an open method set reopens the capture question the
  closed one closes. A function over a record is a `lem-fn`.
- **Methods as first-class bound values.** Rejected: a new `Value` variant and capture
  machinery for a form nothing in the corpus or the port reaches for.
- **A method table plugins may extend.** Rejected: a plugin method would be a compile-time
  fact (D25) attached to a type the plugin does not own — UFCS's capture hazard with an
  import in front of it. A plugin ships functions and types (D23).

## Consequences

- The call path loses its hardest branch. `receiver_expectation`, the `peek_callable`
  probe, the `Given` enum, `field_of`, and `call_function`'s receiver parameter all go:
  `x.f(…)` lowers the receiver, asks its type, and calls a field or a method.
- **A method on a known, non-optional receiver needs no new bytecode**: it pushes the
  receiver, then its arguments, under the existing `CallBuiltin`, one id space, entries
  carrying how they are reached. The three paths off that road each cost something.
  Through `?.` the call must be jumped around when the receiver is null, which
  `CallBuiltin` cannot do. On `any` there is no compile-time id at all, so the call is a
  dynamic op that resolves the name against the runtime value. And a higher-order method
  needs the VM to run a closure and hand back its result, which `Builtin::call` — then a
  pure `fn(&[Value])` — and `Vm::call` — which only pushes a frame — could not do between
  them; ∆987 gave a builtin the VM as a host to invoke through.
- **No grammar change.** `xs.filter(…)` already parses as a member expression called, so
  tree-sitter (∆5MM) and the `.cst`/`.tags` projections are untouched. The corpus's UFCS
  call sites become plain calls, and `<lem-fn slug(s: string): string = s.slugify() />`
  stays exactly as written — a method now.
- The pipe operator is **retired, not deferred**: methods answer the chaining it existed
  for, and invariant 3 forbids its spelling. Its entry leaves the deferred list in
  `docs/decisions/README.md` and `docs/language.md` § Deferred questions.
- The work lands in two items. ∆495 is dispatch: the table, `.` resolving against the
  receiver, UFCS's removal, `?.`, `any`, and argument-free methods. ∆987 is the
  re-entrant invoke, the argument-shaping rule, and the higher-order methods that prove
  them — one unit, because the rule has no consumer without such a method and no such
  method runs without the invoke.
- ∆Q6L is reshaped and gains `needs: ∆987` — the inventory is written against the table,
  not moved onto it afterwards. ∆pXO gains `needs: ∆495`, so the authoring skill is not
  distilled from the retired call model.
- The LSP's completion after `.` (∆5MM) is answerable from the receiver's type alone,
  where UFCS needed the first parameter of every project function.
- `docs/artifact-changes.md`'s UFCS entries are superseded rather than added to (∆MlD).

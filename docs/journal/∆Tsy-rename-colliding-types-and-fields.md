---
id: ∆Tsy
completed: 2026-09-12 15:07:53 -07:00
---

## Backlog item

> - ∆Tsy Rename `lemmon-vm`'s colliding types and fields — `builtins::Kind` and `decls::Kind` are
>   two enums under one name, and the root `pub use` hands `lemmon_vm::Kind` to the second, so the
>   first squats on a name it cannot reach; `builtins::Host` shadows the exported `host::Host`;
>   `Builtin.ty` is a typing rule, `Record.ty` a type's name. `lemmon-syntax` settled this with
>   `SyntaxKind`/`LiteralKind`. `Type` and `Value` spell the type names twice, unchecked — test that

## Planning

**The item listed four collisions; the crate had seven.** Counting every `pub` type name
across `lemmon-vm` found three more of the same kind, and Eric took all of them in:
`compile::whitespace::Kind`, which made `compile/mod.rs` write `whitespace::Kind::Value`
beside `decls`' `Kind`; `compile::narrowing::Root`, which shared one `match` in
`compile/mod.rs` with the exported `unit::Root`; and `compile::surfaces::Source`, which
shadowed the exported `load::Source` inside `compile/` the way `builtins::Host` shadowed
`host::Host`.

The names:

- The three enums follow `SyntaxKind`: `ReceiverKind`, `DeclKind` (exported as
  `lemmon_vm::DeclKind`), `BodyKind`.
- `builtins::Host` became `Machine`, and its parameters `vm`. Everywhere else in the crate
  "host" means the shell's declaration (D36); `Runtime` was the alternative.
- `narrowing::Root` became `PathRoot`, and `surfaces::Source` became `ParsedFile`, so the
  pipeline reads `SourceFile` → `ParsedFile` → `PreparedFile`.
- `Builtin.ty` became `rule`, which its doc comment and D34 already called it.
- `Record.ty` became `name`, as `Declaration.name` and `TypeFact.name` are, and as D35
  words it ("carries its declared name"). `type_name` was rejected: `Value::type_name()`
  answers "`Photo` record" for the same value, so it would have been a new collision.

**The type names were spelled three times, not twice.** `annotations::BUILTIN_TYPES`, the
names an author writes, is the third, and each of the three was tested only against its
own literal list. The test is driven from that table: every name must be what `Type`
prints and what a value of the type reports, with the sample values built by a `match` on
`Type` that has no catch-all, so a new variant has to be answered before the test
compiles. The same loop holds `ReceiverKind::of` to `ReceiverKind::of_value` — the other
pair of parallel matches, whose doc said they must agree and which no test tied together.

A one-line naming rule for `CLAUDE.md` was offered and declined.

## Refinement

The test was negative-tested before it was trusted: misspelling `"duration"` in
`Value::type_name`, and dropping `Asset` from `ReceiverKind::of_value`, each failed it.

Code review found no bugs and four cleanups, all applied. Three were qualified paths —
`builtins::ReceiverKind` in `rules.rs` and `vm.rs`, `whitespace::BodyKind` in
`compile/mod.rs` — that existed only to get around the collision and became imports once
the names differed. The fourth was a line in D34 left past the wrap width.

The change is +558 −375. About 130 of the net lines are rustfmt splitting calls whose
arguments passed its 60-column width once `Kind` became `ReceiverKind`: eight one-line
entries in the `BUILTINS` table, and test calls across the builtin modules. About 50 are
the new test. No logic changed.

`cargo doc` under `-D warnings` reports six errors in lines this item did not touch
(`compile` and `load` are both a function and a module; redundant link targets). Rustdoc
is not in the gate, and they were left.

## Retrospective

The item's list of collisions was written from memory, and the scan that found the other
three was one `grep` over `pub` type names. For an item that names examples of a class,
run that scan before planning, not during it.

Renaming a type inside a dense table has a formatting cost that is easy to miss: the
table grew by more lines than the test did. It was predicted in the plan this time, from
rustfmt's width rule, rather than discovered in the diff.

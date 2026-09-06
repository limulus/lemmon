---
id: ∆e5i
completed: 2026-09-06 11:01:35 -07:00
---

## Backlog item

> - ∆e5i One resident export index — `SignatureTable::build` and `unit::link` each derive
>   the project's function index, and the wasm boundary orders resident units before
>   incoming files so duplicate blame lands on the file just compiled, a rule held in a
>   comment (∆TNT found the two views disagreeing). Keep one index in the `Core`, derived
>   from the units as each compiles, that `link` takes and `core.analyze()` (∆5MM) reads

## Planning

The item was [[∆e4d]]'s deferred half, written down the day that one landed. Reading the
three derivations found the shape the fix wanted: one `ExportIndex` whose single mutation
is `offer(path, exports)` — withdraw everything the path owns, then insert the names that
are free. Withdraw-then-insert *is* the duplicate rule, so the boundary's ordering comment
became an API property: a resident file keeps its names and the file just handed in is the
loser, with no ordering trick to remember.

Two behaviour changes fell out and were put to Eric before building. A definition the
index refuses now yields no unit, so the index and the resident units agree by
construction — previously a refused file's unit landed anyway, which would have let a
later clean relink silently pick one of two definitions. And a collision is reported once,
by the call that introduced it, rather than re-derived and re-reported by every later
call; Eric took that reading. It is what the boundary contract already claimed for
warnings, and today's re-report was an accident of rebuilding the table each call.

Planning also caught that `Resolved::Function(usize)` was a position into a `Vec`, sound
only while the index is immutable — a resident, mutable index makes it a hazard — so the
payload went and both call sites look up by the name they already hold. Following
[[∆e4d]]'s own lesson, the plan named its edges: functions only (a type namespace is
∆C8d's), and no `analyze()` accessor, because ∆5MM has no reader for one yet.

## Refinement

The build landed in one pass with the corpus byte-identical.

- **The restore is the load-bearing part, and the negative test showed why.** A file that
  is offered and then fails to compile leaves the index describing function indices no
  resident unit has. Removing the restore and running the test turned that argument into
  an observable: a later clean relink resolved `f` to the caller's own entry and blew the
  call-depth cap. Any call with an error now rebuilds the index from the resident units,
  and a `debug_assert_eq!` states the invariant across every boundary test.
- **Review found a real panic** (`/code-review medium`, Sonnet finder, verified by
  running it): a path repeated within one call. `offer` withdraws by path, so the second
  entry takes the first's names away, and the first file's export loop then looks up a
  name the index no longer holds. At the boundary that is a trap taking the resident
  `Core` with it — worse than the `JsError` the contract reserves for caller bugs. Fixed
  with one `repeated_path` rule both entry points call before anything is offered. The
  old table never withdrew by path, so it built a confused module instead; the panic was
  new, the silent two-units-one-path variant was not.
- `link` lost its duplicate check entirely, and its test with it: the cross-file rule is
  the offer's now, pinned by a two-file compile test and the boundary's resident tests.
  `Unit::exports` kept a reader — `ExportIndex::of`, which is what the restore derives.

## Retrospective

**Making state mutable means re-auditing every index cached into it.**
`Resolved::Function(usize)` was caught in planning as exactly this; `offer`'s
withdraw-by-path was the same class one layer out and was not. The question is not
"is this position valid now" but "what can change between the lookup and its use" — and
once one instance of a class shows up in a plan, the plan should go looking for the rest.

**Reproduce the failure a guard prevents before writing the guard.** The restore was
argued for on paper and would have been believed either way; deleting it and watching the
call-depth cap fire is what made the debug assertion worth adding rather than decorative.
The repo's rule about negative-testing a check paid twice here.

**Every entry point that takes a list should be tested with a repeated element.** Both
`compile` and `compile_json` accept a file list, neither guarded it, and the review found
it in one pass — a cheap case to have written first.

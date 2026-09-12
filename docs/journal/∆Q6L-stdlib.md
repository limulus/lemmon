---
id: ∆Q6L
completed: 2026-09-12 12:23:52 -07:00
---

## Backlog item

> - ∆Q6L Stdlib — strings/lists/maps, `datetime("…")` (compile-time checked on a literal
>   argument; there is no datetime literal) + rfc3339/rfc822 formatting, `slugify`,
>   `readingTime`, `url()`, `asset()`, `raw()`, `uniqueId()`, with typed signatures the
>   checker understands, since feeds/sitemap/layout ports all block on these

## Planning

The item read like four items, and the first question was whether to split it. It stayed
one, with four checkpoints that each end verify-green — because the four halves are not
independent: the value types have to exist before the methods on them, the methods have
to exist before the docs can list an inventory, and the manifest's arrival point could
not be settled without deciding what `asset()` does at compile. Splitting would have put
three of those decisions in items that could not be reviewed on their own.

Planning surfaced one correction before any code was written. `docs/architecture.md` and
∆d1N both said the asset manifest arrives with `loadData`. D10 says `asset()` is
compile-checked, and a manifest that arrives after compilation cannot check anything. The
plan moved it onto the host declaration at `compile`, which is what [[D38]] records.

Four questions went to Eric and all four changed the design: datetime keeps the offset it
was written in rather than normalising to UTC; the manifest rides on the declaration; the
inventory is port-driven rather than exhaustive; and `slugify` folds rather than
transliterates — though that last one was settled twice, and the second time is the
interesting one.

## Refinement

**The port's own dependency was the wrong model, and reading it was what showed that.**
`slugify` was specified as a `deunicode` transliteration, on the premise that limulus.net
reads stored photo aliases so parity was unaffected. Eric asked whether a Rust crate
already did `readingTime`'s work, which sent me to read the npm package the site actually
uses — and `reading-time` turned out to do *less* than the hand-rolled version, with a
hand-written CJK approximation carrying a `// Help wanted!` comment. But it also sent me
to `unicode-segmentation`, and from there to measuring what the transliteration table
cost: 479 KB of a 2.4 MB artifact, a fifth of it, for three photo titles. Cutting it for
canonical decomposition gave back 426 KB — more than half of everything the item had
added — and produced the *better* anglicization, since `@sindresorhus/slugify` applies
German's `ü` → `ue` to every language's `ü` and would turn a Turkish name into nonsense.

**Three of the four dependencies were decided by measurement rather than by argument.**
jiff, `percent-encoding` and `unicode-segmentation` all stayed; `deunicode` and
`any_ascii` were both measured and both cut. The numbers are in [[D38]] so the next person
weighing a table-backed crate has a baseline.

**A `usize` multiply in `readingTime` would have wrapped in the browser.** `usize` is
thirty-two bits on wasm32, which the ∆Q6L review caught for the second time — checkpoint
2's review found the same shape in `take(n)`. Worth treating as a standing hazard rather
than a bug found twice: any arithmetic on a length in this crate wants widening before the
multiply.

**The ABI tests are not optional.** `npm run rust:wasm:test` was outside the checkpoint-2
verification, and checkpoint 2 shipped with `reaches_the_depth_cap_through_nested_invokes`
red: the separate native-depth cap replaced the message it asserted, and every host-target
suite passed anyway. The test exists precisely because the host has a stack the `.wasm`
does not, so skipping it skips the only run that knows.

**A guard was built against a mistake nobody makes, and Eric interrogated it away.** A
probe showed `uniqueId` in a schema default restarting its counter per record, which was
reported as a uniqueness bug and fixed with a compile-time refusal. Two questions undid
it: `id` uniqueness is per-document, so two pages carrying `til-0` is the contract
working, not failing; and nobody would write a counter-derived id in data, because a
record's anchor comes from its slug. The refusal came out and [[D38]] carries the sentence
it was standing in for. The reasoning is worth keeping: a rule against a mistake with no
evidence behind it is a rule nobody can restate later.

Deferred to new items: [[∆vnI]] (pin the `.wasm` stack the depth caps were measured
against) and [[∆Tsy]] (the names in `lemmon-vm` that collide with names the crate root
already exports). ∆pP5 gained the constraint that a photo's URL is its stored alias, since
recomputing one now moves three live URLs; ∆d1N was reworded twice — the manifest's
destination, and the claim that it ports `11ty/esbuild.ts` 1:1, which it must not, because
the manifest's values are finished URLs rather than the bare hashes that plugin stores.

## Retrospective

**Checkpoint 2's "verify green" was not true, and I said it was.** The ABI suite was not
in my verification, and a test that had been failing since that checkpoint was committed
under a message claiming otherwise. The fix was one line; the reporting error was not.
Run the whole gate before saying the gate is green, and name which parts ran.

**A probe left a field in a corpus fixture across two green reports.** `anchor: string =
uniqueId("til")` sat in `site.lem` for ten minutes of investigation, and no gate noticed —
a defaulted field nothing reads costs nothing and shows up nowhere. Eric found it by
reading the file. `git diff` would have caught it on either report; I trusted my own
cleanup instead of looking.

**The size question was worth asking early and I did not ask it.** Three dependencies
landed before anything was measured, and the measurement — when Eric asked about binary
size for his own reasons — is what unwound the largest of them. A table-backed crate has a
cost that is knowable in one build; knowing it before writing the code that depends on it
would have saved writing `slugify` twice.

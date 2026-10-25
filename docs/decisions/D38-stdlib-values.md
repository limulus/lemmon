# D38 — Stdlib values and the manifest at compile

- **Status:** Accepted 2026-09-11 (`∆Q6L`); implemented 2026-09-12 (`∆Q6L`). On 2026-10-17
  Eric accepted for limulus.net that `readingTime()` counts what a reader sees where
  Eleventy's `reading-time` counted the markdown source, so a page near a rounding
  boundary reads a minute lower after the cutover
- **Backlog item:** `∆Q6L`
- **Normative text:** `docs/language.md` § Types, § Names & namespaces (Ambient),
  § Checking (Calls, Printing), § Schemas; `docs/architecture.md` § WASM API,
  § Build pipeline

## Context

`datetime`, `duration`, `url`, and `asset` were types the checker knew and the VM had no
values for: the loader passed their strings through untouched, and the builtin table held
six entries that existed to prove the two ways in (`∆495`, `∆987`). D34 said the inventory
would be written here.

The port needs all of it. Feeds want RFC 3339 and RFC 822; layouts want a date worded the
way a site words dates; schemas want `readingTime`; and `asset()` is compile-checked by
D10, which the docs recorded as arriving with `loadData` — too late to check anything.

## Decision

**A `datetime` is an instant and the offset it was written in.** It prints RFC 3339 in
that offset, `Z` where that is UTC, and compares by the instant alone, so one moment
written in two offsets is one value. The offset is kept because a site shows the time its
author meant: the civil accessors answer in it, and a post filed at 23:30 on the west
coast is the fourth there while the instant is already the fifth.

There is no time-zone database. A string naming a zone is refused rather than read for its
offset with its DST rules ignored. `jiff` supplies the calendar with
`default-features = false, features = ["alloc"]`, which is what makes invariant 1
structural here rather than reviewed: with no `std`, no `tz-system` and no tzdb feature,
the crate has no way to read a clock or an environment. Every refusal is worded by
`datetime.rs` and never by jiff, so a corpus expectation survives a patch release.

**A `duration` is whole seconds and has no printed form.** How long something takes is
shown as a site words it — through `seconds()` or `minutes()` — never as a bare count.

**A `url` is text percent-encoded once**, by `url("…")` or by standing where the type is
declared. The WHATWG `url` crate was the obvious choice and is the wrong shape: it refuses
a relative reference like `/feed/` without a base, which is most of what a site composes,
and pulls `idna`'s tables into the `.wasm`. `percent-encoding` alone encodes controls,
space, `" < > \ ^ ` { | }` and non-ASCII, leaves `%` so that encoding is idempotent, and
cannot fail. `text()` gives the encoded text back, which is how two URLs are composed:
`+` joins strings and a `url` is deliberately not one.

**`slugify` folds Latin marks off; it does not transliterate.** Canonical decomposition
reduces a letter carrying a mark to the letter, twelve letters whose mark is part of the
letterform are a table, and a script the fold has nothing to say about is carried through
rather than guessed at — so a Japanese title gives a Japanese slug rather than the empty
string. `ü` folds to `u`. `@sindresorhus/slugify` writes `ue`, which is German's own rule
for spelling an umlaut without one, applied there to every language's `ü`; it turns the
Turkish `Ümit` into `Uemit`, and dropping the mark is what English does with a borrowed
name.

**A character is a grapheme cluster.** `length()` counts what a cursor steps over and
`split("")` gives pieces that are each still text, both by UAX #29. `readingTime()` counts
words by UAX #29 too, at two hundred words a minute rounded up, over the text between
tags — a tag separating words as whitespace does, which is what never building a DOM
costs (invariant 2).

**The asset manifest arrives with the host declaration at `compile`.** `HostDecl.assets`
maps each logical path to the URL it shipped at — the finished URL and not a content hash,
because the core does no path arithmetic: where an asset lands is the shell's question,
and a core assembling a URL from parts would be deciding it.

`asset("…")` takes only a string literal, resolves during compilation, and folds to the
URL as a constant; the entry it consulted is recorded as a compile-time fact that the link
step compares (D25). A rebuild that re-hashes an asset therefore leaves exactly the units
that named it stale, and leaves every other unit byte-identical — which is invariant 11
doing the work it exists for. `[asset]` fields resolve at load against the same map, and
the refusal is worded once, in `host.rs`, so a path is one mistake whether a template
named it or front matter did.

**`uniqueId(prefix)` gives `prefix-0`, `prefix-1`, counted per prefix per render.** It is
the only builtin that remembers anything between calls, and it forgets between renders, so
two renders of one page are byte-identical.

The counter belongs to one render, which makes `uniqueId` a render-time function: its
purpose is a component instance that needs an `id` to point at — `aria-labelledby` on an
SVG `<use>`, `for` on a label — and cannot see the rest of the page to pick a name that
will not clash. A schema default is not a render. The loader builds a VM per default, so
`uniqueId` called there restarts at zero for every record and yields a per-record
constant. That is not refused: nothing wants a counter-derived id in data, and a rule
against a mistake nobody makes is a rule nobody can restate later. **A per-record anchor
comes from the record** — `title.slugify()` — which is stable under reordering and
meaningful in a URL fragment, as a counter is not.

**No datetime arithmetic.** `d + duration` would settle the number model, which is a
deferred question.

## Alternatives considered

- **`chrono` or `time` instead of `jiff`.** Both would work; jiff's `alloc`-only feature
  set is the one that makes invariant 1 structural rather than a thing to review, and its
  `Pieces` API parses without committing to a zone.
- **Bundling a tz database.** Four hundred kilobytes of the `.wasm` and a yearly update in
  the release process, to support named zones no corpus case asks for.
- **The WHATWG `url` crate.** Refuses the relative references a site actually composes.
- **`deunicode` or `any_ascii` for `slugify`.** Measured: 479 KB and 582 KB of the shipped
  artifact — a fifth of it — to answer a transliteration question no site in the corpus
  asks, and to answer it with a German rule. Cut for canonical decomposition at 26 KB.
- **Dropping what the fold cannot handle.** Would give every non-Latin title the empty
  string, which is a core that has assumed English.
- **The manifest via `loadData`** (`∆d1N` as written). `asset()` is answered while
  compiling, so the manifest has to be there by then.
- **Resolving `asset()` at render.** Would make a typo a broken link on one page rather
  than a compile error with a span (D10), and would make every unit depend on the whole
  manifest rather than on the entries it named.

## Consequences

- Three dependencies, each `alloc`-only: `jiff`, `percent-encoding`,
  `unicode-normalization`, plus `unicode-segmentation`. The `.wasm` grew by about 480 KB
  across `∆Q6L`, after 479 KB was given back by cutting transliteration.
- `Value` gained four variants and stayed twenty-four bytes: `Datetime` is packed into
  sixteen rather than held as a timestamp beside an offset, because every value in the VM
  pays for the widest one.
- Ordering moved out of `vm::binary` into `Value::order`, so one rule serves `lt` and
  `sortBy`.
- Native recursion got a cap of its own, separate from the frame count: `sortBy` and
  `groupBy` call back into the VM, and sixty-four such levels is what a megabyte of stack
  holds. `crates/lemmon-wasm/build.rs` pins that megabyte as the stack the `.wasm` is
  linked with (`∆vnI`).
- `Unit` gained `asset_refs`, a fourth kind of compile-time fact beside signatures, type
  references, and ambient bindings.
- `Host` (the builtins' trait) gained `next_id`, the only state a builtin may ask for.
- D37 is revised: a string in data now resolves by its declared type for `datetime`,
  `url`, and `asset` as well as for enums, functions, and `by` indexes.
- `∆d1N` is reworded: the manifest goes to the host declaration, its values are finished
  URLs, and it is not a 1:1 port of `11ty/esbuild.ts`.
- A photo's URL in the port must be its stored `alias` and never `title.slugify()`: those
  aliases were minted by `@sindresorhus/slugify`, so `Dürer` is `duerer` there and `durer`
  here (`∆pP5`).

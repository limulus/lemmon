# ∆W1D Gate the spike and the benchmark's fixtures — plan

Written on 2026-09-19 by a planning agent for the managing session, against `main` at
`679ae39`. Nothing was built and no file in the repository was touched. Every claim below
marked _verified_ was confirmed by reading the code or the corpus; claims marked _inferred_
were not. `docs/plans/∆W1D-gate-spike-and-bench.md` holds the survey notes this plan
supersedes; fold both into ∆W1D's journal entry and delete both when the item completes.

## Decisions by the managing session (2026-09-19) — these override the sections below

Eric delegated this item with the instruction to plan it, review the plan, and build it,
stopping for him only if the plan changes `verify:js` beyond what the backlog item says.
It does not: the item asks for one-pass modes run from `verify:js`, and that is § 2.

- **§ 4: `scripts/lib/timings.ts` is dropped.** `mean`, `percentile` and the table printer
  stay where they are, two copies each. Sharing them adds about 35 production and 90 spec
  lines to remove about 25 duplicated lines of timing code that no gate runs, and the two
  `percentile`s differ in contract. `scripts/lib/diagnostics.ts` (`expectClean`) and
  `scripts/lib/corpus-fixtures.ts` stay in the plan: both hold a rule the gate applies.
- **§ 9, question 1:** the script is `test:fixtures`.
- **§ 9, question 2:** assert the forgiveness counts.
- **A fact § 2 mentions in passing:** CI's `verify` job already runs `npm run wasm:bench`
  after `verify:js` (`.github/workflows/ci.yaml`, the step commented "The
  boundary-marshalling record ∆TNT owes"). So the bench runs in CI but not at pre-push,
  and CLAUDE.md's "run in no gate" was already imprecise. That step stays: it is the
  timing record. After checkpoint 3 CI runs the corpus scenarios twice per Node, once in
  one pass inside `verify:js` and once timed. Checkpoint 3's CLAUDE.md wording says what
  runs where.
- Commits are made by the managing session after an Opus or Sonnet review of each
  checkpoint; a builder does not commit.

## Start here

1. Read `CLAUDE.md` § Commands (the `wasm:bench` and `spike:the-rebound` entries and the
   paragraph after the CI note), § Testing, and § Working agreements.
2. Read the ∆W1D item in `BACKLOG.md` and the survey notes; § "Corrections to the notes"
   below lists what they get wrong.
3. Read `docs/journal/∆Bhz-pages-render-through-their-layout.md` (the Retrospective's
   first paragraph) and `docs/journal/∆VNk-diagnostics-with-severity-and-labels.md`
   (Refinement, "The spike scripts kept the old contract") — the two breaks this item
   exists to stop.
4. `npx @limulus/delto@1 claim ∆W1D`.
5. ∆omv is in flight in the same checkout and moves `scripts/wasm-artifact.ts` to
   `scripts/lib/wasm-artifact.ts`. Nothing here touches that file, but do not start until
   it has landed, because both items edit `CLAUDE.md` § Commands.

## Read this first: what is and is not broken

`npm run wasm:bench` and `npm run spike:the-rebound` both work today. Their fixture filter
is also correct today: of the 146 `.lem`/`.md` inputs under `tests/corpus/render/` and
`tests/corpus/document/`, every one whose blessed `.html` trailer holds an error line is
under an `errors/` directory, and every one outside such a directory is clean or
warning-only (_verified_ by sweeping all 208 `.html` expectations under `render/`,
`document/` and `host/`). So the path filter and a trailer filter select the same 77 files
right now, and the benchmark's numbers will not move.

What is broken is that nothing runs either script. Each has broken unseen once — a retired
diagnostics contract (∆VNk) and a corpus case turned into a deliberate error (∆Bhz) — and
both breaks were found by a human running a script by hand, long after the commit that
caused them.

One fact the notes miss and that settles the filter question: the directory convention is
already false elsewhere in the corpus. `tests/corpus/host/` holds 17 `.html` expectations
with error trailers outside any `errors/` directory — `host/pages/required-field-omitted`,
`host/tils/field-of-the-wrong-type` and so on (_verified_). The benchmark does not walk
`host/`, so it has not been bitten; but the convention it depends on is one the corpus
demonstrably does not keep, which is exactly what `tests/corpus/README.md` already says.

## 1. What "one pass" means for each script

### `scripts/render-the-rebound.ts` (378 lines)

Correctness — must run in the gate (all _verified_ by reading the file):

- `ambientData()` (lines 75–102) refuses a `fixtures/data.json` that disagrees with the
  post's front matter on `page.publishedIso` or `page.publishedUtc`, and refuses a `hero`
  that is not a key of `site.photos`. These are `throw`s, not comparisons, and they guard
  the hand-transcribed fixture against silent drift.
- `expectClean()` on the one `compile` call, both `loadData` calls and both `render` calls
  (lines 256–296).
- The rendered-HTML comparison: `compareRendered(renderedPage(pageHtml), renderedPage(
  reference), FORGIVENESS)` at lines 308–312, and the `unexpected.length > 0` failure at
  lines 368–373. This is D28's comparison — parse5 trees, attributes by value, text after
  white-space processing — and it is the whole point of the harness.

Timing — must not run in the gate:

- `RENDER_RUNS = 200` (line 37). The two renders and two loads are inside that loop purely
  to collect samples; the comparison uses only the last iteration's output.
- `record()`/`mean()`/`percentile()` and the table at lines 333–347.
- `BUDGET_MICROS = 1000` and the `timeOk` verdict at lines 349–377. This is the check that
  must not fail on a slow machine, and it is also the number ∆xWs's own retrospective
  flags as measuring something other than the criterion it is held against ("The number
  recorded against the criterion is not the number the criterion names"). Removing it from
  the gate costs no coverage: the budget runs in no gate today either.

One addition beyond the item's instruction, which the builder should take unless Eric says
otherwise: **assert the forgiveness counts**. `tests/fidelity/the-rebound/README.md` states
them in prose — "the expected counts are 6, 6, and 1" (_verified_, README lines 69–72) —
and the run prints them but checks nothing. A forgiveness class that begins absorbing more
than it explains is silent today; a class that absorbs less already fails, because the
difference becomes unexpected. ∆xWs's retrospective is about precisely the first half ("my
own allowlist was the suppression this repo keeps warning about… nothing made me look at
whether it silenced *more* than it claimed"), and CLAUDE.md's testing rules extend it to
diff gates by name. Eight lines: a `const EXPECTED_FORGIVEN = { 'heading-id': 6,
'md-photo-wrap': 6, 'attr-escaping': 1 }` compared against `comparison.forgiven`, failing
with the same report the unexpected differences use. It runs in both modes.

Keep in the gate: writing `lemmon.html` into the OS temp directory (lines 303–305). It
costs nothing, writes nothing into the tree, and is what someone reaches for when the gate
fails.

### `scripts/bench-wasm.ts` (346 lines)

Assertions it makes about the corpus fixtures (_verified_):

- `fixtures()` (lines 106–118) selects every `.lem`/`.md` under `render/` and `document/`
  whose path does not contain `/errors/`.
- `corpusScenarios()` (lines 208–228) compiles each one **in a fresh core** and calls
  `expectClean` on the compile, then `loadData` with the corpus's untyped ambient record
  and `render`, with `expectClean` again. A fresh core per fixture is required, not a
  choice: units accumulate in a resident `Core` and two fixtures defining the same
  top-level name would collide at link (_inferred_ from D25 and invariant 11; the code's
  own comment says "fresh core").
- `siteScenarios()` compiles a synthetic 101-file site in one call and again file by file,
  recompiles one page, renders, and calls `loadData` — every one guarded by `expectClean`.
- `typedLoadScenarios()` compiles a typed host, runs `loadData` with 100 documents' front
  matter and positions, and renders one post — the only place in the repo that sends the
  shell's real `sources`/`spans` payload shape across the boundary outside a unit test.
- `report()` (lines 306–315) throws when any scenario collected no samples, naming the
  corpus directory. That is the guard against the fixture walk silently finding nothing,
  and it must keep running in one-pass mode even though the table does not.

What would have caught ∆Bhz: running `corpusScenarios()` at all. The case that broke it
(`document/front-matter/unclosed.md`, made a deliberate error and left in place) was
selected by the path filter, compiled with an error, and `expectClean` threw. The break
existed from the commit that blessed it; nothing ran until someone ran the bench by hand.
The trailer filter in § 3 makes that situation a non-event, and the gate makes any other
such situation visible at push time.

Timing — must not run in the gate: `INCREMENTAL_RUNS`, `RENDER_RUNS`, `LOAD_DATA_RUNS`,
`TYPED_LOAD_RUNS` (lines 36–39) drop to 1 each; `record()` keeps measuring (it is how the
scenarios are structured) but the table is not printed. Nothing in this script has a
pass/fail threshold on time, so there is nothing to disable.

## 2. Where the gated check lives

**Recommendation: (a) — one-pass is the default mode of each script, and the two scripts
become two more legs of `verify:js`'s `concurrently` list.** The flag is `--time`, and it
adds the loops, the table and (for the fidelity harness) the budget verdict.

```
"test:fidelity": "node scripts/render-the-rebound.ts",
"test:fixtures": "node scripts/bench-wasm.ts",
"wasm:bench":    "node scripts/bench-wasm.ts --time",
"verify:js": "(is-ci || npm run rust:wasm) && concurrently -c cyan,magenta,blue,green,yellow npm:lint npm:test npm:tscc npm:test:fidelity npm:test:fixtures",
```

`npm run test:fidelity -- --time` is the timing run of the fidelity harness; it needs no
npm script of its own because nothing automated runs it.

Why the flagless invocation is the gate and not the timing run: the gate runs on every
push, the timing runs occasionally, and a bare `node scripts/bench-wasm.ts` should do the
cheap correct thing. `npm run wasm:bench` keeps its current meaning exactly, which is what
CI's `verify` job (`.github/workflows/ci.yaml:114`) and ∆TNT's record depend on.

### How the new legs get the artifact in CI

No workflow edit is needed (_verified_ against `.github/workflows/ci.yaml`):

- The `verify` job (lines 86–104) declares `needs: wasm` and a matrix of Node 24 and 26.
- Before running anything it does `actions/download-artifact@v7` with `name: wasm` into
  `artifacts/wasm/` (lines 100–103), so the exact bytes the `wasm` job staged are on disk
  for every Node in the matrix.
- Its only verify step is `npm run verify:js` (line 104). `verify:js` begins
  `(is-ci || npm run rust:wasm)`, so under CI the restage is skipped and the downloaded
  bytes stand.
- Both new legs run inside that command's `concurrently`, as children of the same step, so
  they inherit exactly that artifact. `loadCore()` finds it by walking up from the module
  to the nearest `artifacts/wasm/` holding a staged glue
  (`packages/core/src/lib/load.ts:26–35`), which from a script run at the repo root is the
  repo root's (_verified_).
- Pre-push (`.husky/pre-push` runs `npm run verify`) takes the other branch of
  `(is-ci || …)` and restages first, as it does today.

`verify` stays defined as exactly `verify:js` + `rust`; only `verify:js` grows.

### Why not (b), vitest specs

Considered and rejected for a specific, measurable reason. Both entry points are
imperative top-level scripts; turning them into specs means restructuring them into
exported functions, and anything that lands in `scripts/lib/` is held to 100% branch
coverage by the global thresholds in `vitest.config.ts:20–25`. The fidelity harness's
three forgiveness predicates (`render-the-rebound.ts:187–242`) are dense with `&&`
short-circuits whose false branches a passing run does not reach — `attr-escaping`'s
`if (!cut) return false` is reached only when an attribute differs for some *other*
reason, which never happens in a green run (_inferred_). Moving them into `scripts/lib/`
would therefore force roughly 100 lines of synthetic spec for predicates that are not what
broke, which is scope the item's reason does not ask for. It would also put the gate's own
report text behind a vitest assertion message, which is worse than the report the script
already prints.

(b) buys nothing on the CI side either: both options run under `npm run verify:js`, on both
Nodes, against the downloaded artifact.

What (b) does buy — and what (a) must therefore do deliberately — is that the *decision*
logic gets a spec. § 3 and § 4 move exactly that much into `scripts/lib/`: which fixtures
are clean, and what counts as an unacceptable diagnostic. The entry points stay uncovered
scripts, which is the carve-out CLAUDE.md names and the shape `scripts/check-wasm.ts` takes
after ∆omv.

### Wall clock

Cannot be measured here; the builder must measure and report both numbers.

Estimates from the scripts' structure (_inferred_): the fidelity check is one compile, two
loads, two renders and two parse5 parses — microseconds of work, so its cost is Node
startup plus one 2.3 MB wasm instantiation, about a second. The fixtures check is 77
fixtures × (`loadCore` + compile + load + render), plus the synthetic site's ~205 compile
calls, plus one typed load. `loadCore()` is cheap after the first call — instantiation is
idempotent per glue module and later calls only construct another resident core
(`packages/core/src/lib/load.ts:37–41`, _verified_) — so the repeated cost is 77 reads of
the 2.3 MB `.wasm` (page cache) and the compiles themselves. Estimate one to three seconds.

Both run in parallel with `npm:test`, which runs vitest with coverage and is the long pole
today, so the expected effect on `verify:js` wall clock is near zero.

**What would make me reconsider:** either script taking more than about 10 seconds on its
own. The lever then is to read the `.wasm` once and call `createCore(glue, wasm)` per
fixture instead of `loadCore()` per fixture — same freshness, one read.

## 3. The fixture filter

### Verified claims

- The filter is path-based today: `scripts/bench-wasm.ts:110`,
  `if (!/\.(lem|md)$/.test(path) || path.includes('/errors/')) continue`.
- `tests/corpus/README.md:58–60` says there is no clean/broken split and that cleanliness
  is data.
- The trailer marker is `--- diagnostics` and the warning prefix is `warning: `
  (`crates/lemmon-syntax/src/diagnostic.rs:20` and `:24`).
- The trailer begins after the **last** line equal to the marker, because a rendered page
  may contain the marker as ordinary output (`crates/lemmon-corpus/src/lib.rs:237–247`,
  and `tests/corpus/README.md:96`). No `.html` in the corpus has two markers today, so
  this is a rule the TypeScript reader must mirror rather than one it can observe.
- A trailer line is `path:span: [warning: ]message`, and each secondary label follows on
  its own line indented two spaces (`crates/lemmon-vm/src/diagnostics.rs:146–155`). The
  span is `line:col..line:col` when the corpus harness could place it against the case's
  own source, and stays `start..end` bytes when the line names another file — real example
  at `tests/corpus/host/errors/template-typed-by-a-record-that-reads-body.html:4`.
- Warning-only fixtures exist outside any `warnings/` directory and must survive the
  filter: `render/text/statement-lines`, `document/prose/article`,
  `document/fences/fences-in-containers`, `document/fences/unknown-language`, plus the
  eight under `render/warnings/`.
- Every input under `render/`, `document/` and `host/` has an `.html` sibling today, and
  every `.html` has a trailer.

### The module

`scripts/lib/corpus-fixtures.ts`, one responsibility: _which corpus inputs a harness may
compile clean, decided from each case's blessed diagnostics rather than from its directory
name._ One exported function, so there is no inert layer to test around it:

```ts
export interface CorpusInput {
  /** Corpus-relative path, as the boundary knows the file. */
  path: string
  /** The input's text. */
  text: string
}

export interface CorpusFixture extends CorpusInput {
  /** The blessed `.html` expectation beside the input. */
  expectation: string
}

/**
 * The fixtures whose blessed expectation shows no error — the ones a harness may compile
 * and render without a diagnostic. Warnings do not disqualify a case (∆VNk).
 *
 * Throws, naming the path, for an expectation with no diagnostics trailer: every `.html`
 * the corpus blesses has one, so a file without one is not an expectation.
 */
export function cleanFixtures(fixtures: CorpusFixture[]): CorpusInput[]
```

It returns freshly built `{ path, text }` objects rather than the inputs it was given, so
the expectation text cannot ride along into a `compile` payload — the boundary's serde
refuses unknown fields (_inferred_ from ∆Bhz's journal, "an entry of `pages` with an
unknown field is refused by serde").

Reading the trailer, private to the module:

```ts
// `lemmon_syntax::DIAGNOSTICS_TRAILER` and `lemmon_syntax::WARNING_PREFIX`
// (crates/lemmon-syntax/src/diagnostic.rs). A TypeScript reader cannot import them, so
// these are copies; change them together.
const DIAGNOSTICS_TRAILER = '--- diagnostics'
const WARNING_PREFIX = 'warning: '
```

The rule, line by line after the last marker line:

- a blank line is skipped;
- a line beginning with two spaces is a secondary label of the diagnostic above it and is
  skipped — a warning's label must not read as an error;
- otherwise the message is what follows the first `': '` at or after the first `'..'`,
  which accepts both span forms without parsing either; the line is an error unless that
  message starts with `warning: `;
- a line with no `'..'` at all carries no span, which invariant 8 says cannot happen; count
  it as an error so it surfaces rather than being skipped (errors over guesses).

The I/O stays in `scripts/bench-wasm.ts`: it keeps `walk()` and `fixtures()`, now reading
each input's `.html` sibling and handing the three fields to `cleanFixtures`. This differs
from the notes, which had the module do the walking; the reason is the precedent ∆omv sets
and `rendered-html.ts` already follows — the decision goes to `scripts/lib/`, the file
system stays in the script — and it keeps the spec free of temporary directories.

**What the guard prevents:** a corpus case that becomes a deliberate error while staying in
the feature directory that says what it tests no longer breaks the boundary harness, and
the harness stops depending on a convention the corpus says it does not have and that
`host/` already contradicts in 17 places. It does not change which fixtures are selected
today.

**What it deliberately does not do:** it does not check that the `errors/` cases still
produce errors. That is the corpus gate's job — `cargo test` compares the whole blessed
`.html` — and duplicating it here would be a second walker over the same expectations.

### Spec cases (`scripts/lib/corpus-fixtures.spec.ts`)

Written as literal expectation strings in the corpus's real shape.

- should keep a fixture whose trailer is empty
- should drop a fixture whose trailer holds one error line
- should keep a fixture whose trailer holds only warnings — the `render/warnings/unused`
  shape, several warning lines
- should keep a fixture whose warning carries an indented label (the negative half: a
  label is not an error line)
- should drop a fixture whose error carries an indented label, counting the diagnostic once
- should drop a fixture whose error line is qualified by another file's path and spans
  bytes rather than positions
- should read only the trailer after the **last** marker, when the rendered page itself
  contains a `--- diagnostics` line
- should ignore blank lines in the trailer
- should treat a trailer line with no span as an error
- should throw, naming the path, for an expectation with no trailer
- should return the inputs in the order given, without the expectation field
- and the two ∆Bhz negatives stated as behaviour: a case under a feature directory whose
  expectation shows an error is dropped although its path says nothing; a case under an
  `errors/` directory whose expectation is clean is kept although its path says otherwise.

## 4. The shared helpers

Line numbers re-found and confirmed. `expectClean` is byte-identical including its comment
at `bench-wasm.ts:87–93` and `render-the-rebound.ts:133–139` (_verified_). The table
printer is identical at `bench-wasm.ts:332–340` and `render-the-rebound.ts:342–347`
(_verified_). `percentile` is **not** identical: `bench-wasm.ts:301–304` takes an
already-sorted array, `render-the-rebound.ts:127–131` sorts a copy first (_verified_) —
the notes list it as a plain duplicate. `record` differs in shape: the bench's
(`bench-wasm.ts:61–74`) takes `() => { in, out }` and weighs both payloads, the fidelity
harness's (`render-the-rebound.ts:113–121`) takes `() => void`.

Two modules, each stating one responsibility:

**`scripts/lib/diagnostics.ts`** — _the rule both wasm harnesses apply to a boundary call's
diagnostics._

```ts
/**
 * Throws when any diagnostic is an error, naming what the call was doing. A warning does
 * not fail a compile and is not a harness's concern (∆VNk).
 */
export function expectClean(diagnostics: Diagnostic[], doing: string): void
```

Spec: throws on an error and names `doing`; throws when errors and warnings are mixed;
does not throw on warnings alone; does not throw on an empty list; the thrown message
carries the offending diagnostics.

Be honest about what this buys. A spec of `expectClean` would **not** have caught ∆VNk:
the rule itself was the thing that was wrong, and a spec written at the time would have
encoded the same wrong rule. The gate is what catches that class of break. What the single
definition prevents is the *second* fix — ∆VNk had to change the same seven lines twice
because there were two copies.

**`scripts/lib/timings.ts`** — _summarizing and printing a harness's timing samples._

```ts
export function mean(samples: number[]): number
/** The p-th percentile of `samples`, which need not be sorted. */
export function percentile(samples: number[], p: number): number
/** A right-aligned table as text: header, a dashed rule, then the rows. */
export function formatTable(header: string[], rows: string[][]): string
```

`formatTable` returns a string rather than writing to the console, so the spec asserts on
a value and neither stubs nor mocks anything. Callers keep their own `console.log`.
`percentile` takes unsorted input, which is the fidelity harness's contract; the bench
sorts once for its mean and will sort again inside each call, which is two extra sorts of
at most 200 numbers per scenario.

Spec: `mean` of one sample, of several; `percentile` at p50 and p99 for an even and an odd
count, for a single sample, and with unsorted input (the case that pins the contract
difference); its clamping at p=0 and p=100; `formatTable` widening a column to its widest
cell, to its header when that is wider, and the dashed rule matching.

**Not shared: `record`.** One recorder serving both shapes needs a payload-weighing
callback threaded through the caller that does not weigh payloads, which is more machinery
than the nine lines it saves, and all of it is timing code that no gate runs. Leave both.

**Measured cost of the sharing.** About 42 lines leave the two scripts; about 70 production
lines and about 165 spec lines arrive in `scripts/lib/`. This does not shrink the repo — it
grows by roughly 190 lines. What it buys is one definition of each rule instead of two, and
the first tests any of these lines have had. The part with the least safety value is
`timings.ts`, whose callers run in no gate either way; if Eric wants the smallest diff,
that is the piece to drop, and `percentile`'s two contracts stay two.

## 5. The rename, as a checkpoint of its own

`spike/the-rebound/` → `tests/fidelity/the-rebound/` (9 files, 1,595 lines, moved with
`git mv`, contents unchanged except the README) and `spike:the-rebound` → `test:fidelity`.

The rule for the sweep, which decides every mention: **the milestone keeps its name; the
directory and the npm script change.** Phase 0 was a spike and saying so stays true. A
mention that names `spike/the-rebound` the path or `spike:the-rebound` the script is a live
instruction and is swept.

Swept (all _verified_ present):

- `package.json:26` — the script entry.
- `scripts/render-the-rebound.ts`: `const SPIKE = 'spike/the-rebound'` (line 35) becomes
  `const FIXTURES = 'tests/fidelity/the-rebound'` with its three `join(SPIKE, …)` uses; the
  header's `npm run spike:the-rebound` (line 2) and `spike/the-rebound/fixtures/` (line 5);
  and the four places the prose calls this harness "the spike" (lines 10, 158, 250, 257,
  265), which become "the fidelity harness" or "the harness". The `∆xWs` provenance marker
  in the header stays. The file itself is not renamed: `render-the-rebound.ts` still says
  what it does.
- `spike/the-rebound/README.md` → `tests/fidelity/the-rebound/README.md`: the title (line
  1), the run command (line 9), and — in checkpoint 3 — a sentence saying the comparison
  now runs in `npm run verify`.
- `CLAUDE.md`, six mentions (the notes say five): line 93's `scripts/` row ("the spike" →
  "the fidelity harness"), line 163's "the spike, or the bench", line 169's command entry,
  line 182's "run in no gate" paragraph (checkpoint 3 — it becomes false), line 362's
  "`verify` covers neither the ABI suite nor the spike" (checkpoint 3 — also false). Line
  11's "Phase 0 (the spike …) is done" is the milestone and stays. Add a `tests/fidelity/`
  row to the Layout block beside `tests/corpus/`, since `tests/` now holds two things.
- `BACKLOG.md:23–27` and `docs/plans/∆W1D-gate-spike-and-bench.md` are this item's own
  text; both go when it completes.

Checked and left alone, with the reason:

- `docs/journal/**` — history, and exempt by Eric's instruction.
- `docs/decisions/D25, D26, D28, D30, D36, D37` — records of what was true when each was
  decided. Two of them name the path: `D26-body-whitespace.md:46` ("every template in
  `spike/the-rebound/site/`") and `D28-whitespace-is-a-source-concern.md:38` ("all six
  spike files"). A record is a snapshot of its own moment and Eric's instruction exempts
  records that are history, so both stay; this is the one place in the sweep where the
  call is arguable, and the cost of leaving them is one stale path in a record nobody
  navigates by.
- `docs/architecture.md:53, 75, 91`, `docs/artifact-changes.md:281, 316`,
  `docs/language.md:357`, `tests/corpus/README.md:51`, `packages/shell/README.md:5`,
  `crates/lemmon-vm/src/{compile/mod.rs,host.rs,value.rs}` and
  `crates/lemmon-vm/tests/corpus.rs:44` — every one of these names the Phase 0 milestone
  or the language subset it exercised, not the directory. They stay.

No lint, typecheck, vitest or CI configuration references `spike/` (_verified_ by grepping
the whole tree outside `node_modules`, `.git` and `target`), and nothing under
`tests/corpus/` is special-cased by `eslint.config.js`, so moving fixtures under `tests/`
changes no tooling behaviour.

## 6. The `tests/corpus/README.md` paragraph

Current text, lines 58–60 (_verified_ verbatim):

> There is no `ok/` versus `broken/` split. Whether a case parses clean is _data_: its
> diagnostics section is empty. That keeps a case in the directory that says what it tests
> even after a language change turns it from clean to broken, or back.

Two things are wrong with it now. `errors/` and `warnings/` directories do exist —
`render/errors`, `render/warnings`, `host/errors`, `host/warnings`, `document/errors`, and
this README names `host/warnings/` itself at line 48 — so a reader can reasonably conclude
the top level is a split after all. And "its diagnostics section is empty" is too strong:
a warning-only case is clean for every consumer, and there are twelve of them under
`render/` and `document/`. Replacement:

> There is no `ok/` versus `broken/` split. Whether a case is clean is _data_: it is what
> its `--- diagnostics` trailer says and nothing else, and a case that only warns is
> clean. Some directories are named `errors/` or `warnings/` because the diagnostic is
> what those cases are about, but the name decides nothing — `host/pages/` holds error
> cases too. That keeps a case in the directory that says what it tests even after a
> language change turns it from clean to broken, or back, and it is why the tool that
> needs only the clean cases reads the trailer: `scripts/lib/corpus-fixtures.ts`, for the
> boundary harness.

## 7. Checkpoints

Three, in this order. Each leaves `npm run verify` green and is separately committable;
Eric commits. Give a plain-language preview before each, and run `/code-review medium
--fix` on Opus over each diff before handing it back.

### Checkpoint 1 — the rename

Files: `git mv spike/the-rebound tests/fidelity/the-rebound` (9 files);
`tests/fidelity/the-rebound/README.md` (title, run command); `scripts/render-the-rebound.ts`
(the `SPIKE` constant and its three uses, the header, four prose mentions);
`package.json` (one line); `CLAUDE.md` (lines 93, 163, 169 and a new Layout row).

Added/removed: no production logic. About 20 edited lines, 9 moved files, 1 new doc row.

Acceptance: `npm run verify`; `npm run rust:wasm` then `npm run test:fidelity` by hand (it
is not gated yet), which must print the same forgiveness counts 6, 6, 1 and the same
"PASS"; `git status` clean of anything unintended.

Negative test: before fixing them, run `npm run test:fidelity` with the move done and the
`SPIKE` constant untouched and watch it fail on a missing `tests/fidelity/.../index.md` —
which proves the script really reads those files and that the path constant is load-bearing.
Then, with everything fixed, rename `page.publishedIso` to `page.publishediso` in
`tests/fidelity/the-rebound/fixtures/data.json`, run it, and watch `ambientData()` refuse
the fixture. Reverse both probes by reversing the edit — never with `git checkout --`.

### Checkpoint 2 — the shared decisions

Files: new `scripts/lib/diagnostics.ts`, `scripts/lib/timings.ts`,
`scripts/lib/corpus-fixtures.ts` and their three specs; `scripts/bench-wasm.ts` and
`scripts/render-the-rebound.ts` lose their copies and gain imports; `bench-wasm.ts`'s
`fixtures()` reads each `.html` sibling and calls `cleanFixtures`; `tests/corpus/README.md`
gets the paragraph from § 6.

Added/removed: about +70 production lines in `scripts/lib/`, about −42 from the two
scripts, about +165 spec lines, about +5 doc lines.

Acceptance: `npm run verify` — which now includes the three specs and holds
`scripts/lib/**` at 100% branches, functions, lines and statements. Then by hand, with a
staged artifact: `npm run wasm:bench` (still ungated) must print `corpus: 77 fixtures` and
complete, and `npm run test:fidelity` must still pass. The fixture count is the number to
watch: it was 77 under the path filter and must be 77 under the trailer filter.

Negative test: copy `tests/corpus/render/errors/misspelled-field.{lem,html}` to
`tests/corpus/render/text/` under a new name, run `npm run wasm:bench`, and watch it report
77 fixtures and pass — the case the path filter would have compiled and `expectClean` would
have thrown on is now skipped because its trailer says so. Then blank the copy's trailer
lines (leaving the marker), run again, and watch the bench fail on that fixture's compile:
that is ∆Bhz reproduced exactly, an expectation and a behaviour that disagree, and it
proves the filter reads content rather than assuming it. Delete both copies afterwards.

### Checkpoint 3 — one pass, and the gate

Files: `scripts/render-the-rebound.ts` (argument, run count, guarded timing report,
forgiveness-count assertion); `scripts/bench-wasm.ts` (argument, run counts, `report()`
split so the empty-scenario throw always runs and only the table is conditional);
`package.json` (`test:fidelity`, `test:fixtures`, `wasm:bench`, `verify:js`); `CLAUDE.md`
(the two command entries, the "run in no gate" paragraph at line 182, the Committing
sentence at line 362); `tests/fidelity/the-rebound/README.md` (one sentence: the comparison
runs in `npm run verify`, the timings do not).

Added/removed: about +25 production lines, about +10 doc lines, no spec lines. No workflow
change.

CLAUDE.md line 182's paragraph becomes something like: the one-pass modes of
`test:fidelity` and `test:fixtures` run in `verify:js`, so a boundary shape change that
breaks a consumer is now seen at push time; the timing modes (`wasm:bench`, `test:fidelity
-- --time`) still run in no gate, so grep `scripts/` for the consumers of a contract that
changes meaning. Line 362's sentence drops the spike and keeps the ABI suite.

Acceptance: `npm run verify` — which now runs five legs and must stay green; note its wall
clock before and after and report both numbers as the consequence ("`verify:js` went from
N to M seconds"). `npm run wasm:bench` still prints the full table. `npm run test:fidelity
-- --time` still prints the table and the budget verdict. `npm run rust:wasm:test` and
`npm run wasm:check` by hand, since `verify` covers neither.

Negative test, and this is the one that shows the gate exists: edit
`tests/corpus/render/text/statement-lines.lem` (a clean, warning-only fixture) to reference
an undefined name **without** re-blessing, run `npm run verify`, and watch `test:fixtures`
fail inside it — the expectation still says the case is clean, so the filter keeps it, and
`expectClean` refuses the error. Second: change `page.publishedUtc` in
`tests/fidelity/the-rebound/fixtures/data.json`, run `npm run verify`, and watch
`test:fidelity` fail — the spike-reads-a-field break, now caught by the gate. Third: raise
`EXPECTED_FORGIVEN['heading-id']` to 7 and watch the run fail on the count. Reverse each
probe by reversing the edit; copy a file aside first if the edit is more than one line.

## 8. What this plan does not do

- It does not touch the three front-matter models. `crates/lemmon-vm/tests/corpus.rs`
  parses front matter with saphyr and computes per-field positions (`front_matter` and
  `Parsed`, roughly lines 547–700 — the notes' 476–692 is off); `render-the-
  rebound.ts:81–83` does the same job with the `yaml` package; `bench-wasm.ts:150–204`
  computes front-matter spans by arithmetic for its synthetic documents (_verified_).
  Fixtures in two languages are fine. The rule this item leaves behind for **∆4tG**: when
  `@limulus/lemmon-shell` has a real front-matter reader, both scripts use it, and no
  fourth model appears in the meantime — in particular `corpus-fixtures.ts` must not grow
  one.
- It does not add a VM-only timer for the 1 ms criterion, which ∆xWs's retrospective says
  is the honest move if a page ever lands near 60% of budget. The budget stays where it
  is, in a mode nothing automated runs. No coverage is lost: it is ungated today too.
- It does not gate the `errors/` or `host/` corpus cases. `host/` needs the declared host
  and loaded data that `crates/lemmon-vm/tests/corpus.rs` builds, and the blessed `.html`
  comparison is the corpus gate's job.
- It does not move the forgiveness classes or `SITE_DISPLAY` into `scripts/lib/`.
- It mints nothing and changes no other backlog item.

One consequence worth naming rather than deferring: after checkpoint 3, both scripts'
imports of `packages/core/src/index.ts` are gated, so **∆Qr2's** change to
`@limulus/lemmon-core`'s public API will break `npm run verify` instead of breaking a
script nobody runs. That is the intent, and it is the notes' argument for landing ∆W1D
first.

## 9. Open questions for Eric

Both are small; the builder should proceed on the recommendation if no answer comes.

1. **The npm script name for the benchmark's one-pass mode.** Recommendation:
   `test:fixtures` — what it gates is that the checked-in fixtures still compile and render
   clean through the boundary. `test:boundary` reads better but collides with
   `rust:wasm:test`, which CLAUDE.md already calls the boundary's ABI suite.
2. **Whether the fidelity gate should assert the forgiveness counts (6, 6, 1).**
   Recommendation: yes. The numbers are already checked into the README as prose, a class
   that absorbs more than it explains is invisible without them, and ∆xWs's retrospective
   is about exactly that failure. It is eight lines and it is the only part of this plan
   beyond the item's instruction; drop it if the diff should stay minimal.

## Corrections to the notes

- `percentile` is listed as a duplicate. The two copies differ in contract: the bench's
  requires a sorted array, the fidelity harness's sorts internally. Sharing them means
  choosing the sorting one.
- "five mentions in `CLAUDE.md`" — there are six (lines 11, 93, 163, 169, 182, 362), and
  two of them (182 and 362) are statements the item makes false, not just names to change.
- The notes propose `cleanFixtures(groups)` doing its own walking, with `bench-wasm.ts`
  losing `walk` and `fixtures`. This plan keeps the walk in the script and gives the
  library the decision only, following ∆omv and `rendered-html.ts`.
- `crates/lemmon-vm/tests/corpus.rs:476–692` is the wrong range for the front-matter
  reader; `front_matter` is at line 550 and the code around it runs to roughly line 700.
- The notes say the path filter is "correct today" but do not say that `tests/corpus/host/`
  already holds 17 error expectations outside any `errors/` directory. The convention is
  not repo-wide; the benchmark has been spared only because it does not walk `host/`.

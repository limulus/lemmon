---
id: ∆TNT
completed: 2026-08-23 18:24:50 -07:00
---

## Backlog item

> - ∆TNT WASM boundary — wasm-bindgen build of the core, `@limulus/lemmon-core` typed wrapper
>   (compile/loadData/render), and a small Node harness that renders fixtures, measuring
>   boundary marshalling cost so the few-large-calls design is validated early. `compile`
>   takes changed files only: the spike's one-pool `Module` becomes per-file units plus a
>   link step (D25), since retrofitting that under a shipped boundary would change its contract

## Planning

The item bundles two things that only look separate: the boundary's contract, and the D25
split it is defined against. Exploration found the docs prescribed the class shape
(`compile`/`loadData`/`render`) but left `DataInput`, `PageRef`, the diagnostic JSON, and
the render result's fields unspecified — all shapes a shipped boundary would freeze. Each
was settled as an envelope that later fields extend rather than replace: `{ambient: {…}}`,
`{path}`, `{path, span, message}`, `{html, diagnostics}` with `channels`/`deps` omitted
outright rather than stubbed, since a stub is a shape too.

Four choices went to Eric. Three took the recommendation: **wasm-bindgen-cli directly with
`--target web`** over wasm-pack, because caller-supplied bytes keep loading injectable and
browser-reusable while the repo already owns staging; **facts recorded on `Unit` now**
rather than deferred whole to ∆RGQ; and a **measure-only harness**, since asserting
`.html` would make it a second corpus walker over a tree the Rust gate already pins.

The fourth went the other way and was the more interesting call. The plan proposed a
dependency-injected wrapper with real-artifact specs quarantined in a separate config, to
protect `ci.yaml`'s deliberate "verify has no `needs:`". Eric chose **real wasm in the root
vitest run**, accepting that TypeScript feedback now waits on a cached cargo build. That is
the better trade: the specs exercise the exact bytes that ship, there is one test command
rather than two, and the DI seam survives anyway as `createCore` underneath `loadCore`.
The cost is a local ordering rule — `npm run rust:wasm` before `npm run verify` — which the
loader names in its own error message rather than leaving to documentation.

The structural insight that made the rest cheap: if `link` emits the same `Module` the VM
already ran, `vm.rs` does not change at all. Units became the compiler's output format and
`Module` stayed the VM's input format, with linking the rebase in between. 1313 lines of VM
were never touched, and every corpus expectation stayed byte-identical without blessing.

## Refinement

Import operands needed a representation before the local function count was known, since a
call can be lowered before the file's later `lem-fn`s are reserved. They are tagged with the
high bit during lowering and rewritten to `functions.len() + index` in one pass at the end —
which kept `Op` at 40 variants, so neither `Op::NAMES` nor the corpus's every-op gate moved.

Two things surfaced only by running the real toolchain. The generated glue imports
`__wbg_Error` — not on the old allowlist, and exactly the kind of name the allowlist exists
to make someone look at; reading the shim showed it constructs the `Error` a `JsError`
throws, so it was allowlisted deliberately rather than by pattern. And ESLint tried to parse
the generated `.d.ts` files, which is what `artifacts/**` being ignored now fixes.

A `/code-review high` pass found seven, all fixed. One was a real correctness hole in my own
design: **`link` resolved imports by name and never compared them against the signature the
caller had recorded**, so recompiling only a callee whose parameters had been reordered
relinked cleanly and silently swapped a caller's named arguments — reproduced as `A-B`
becoming `B-A` with zero diagnostics. The fix is what makes `facts` load-bearing instead of
bookkeeping: a mismatch is now a spanned diagnostic at the call site naming both signatures,
which is also precisely the signal the shell needs to know which file to recompile. Until
that check existed, `facts` was write-only — the review's grep said so, and it was right.

The others: `loadCore`'s default path counted `../`s from `src/`, so the **built** package
always failed to find the artifact (specs ran from source and never saw it — now a search
upward, with a spec pinned to the `dist/` layout); a `BTreeMap` fed the signature table in
path order, so a collision blamed the untouched resident file instead of the one just
edited; no `free()` on the wrapper, so every `loadCore` leaked a resident core; an
unguarded empty-scenario divide in the harness; a `try` that reported a broken glue module
as a missing one; and a duplicated CLI version pin, now derived from `cargo metadata`.

Fixing the loader raised a coverage problem worth recording: the natural "searched
everywhere, found nothing" branch is unreachable while the artifact exists. Rather than
reach for a `/* v8 ignore */`, the search became an exported `stagingDirectory(from?)`
returning found-or-nearest, which is directly testable from a fabricated path. The gate
pushed the design somewhere better.

Deferred deliberately, each noted in `docs/architecture.md` rather than left implicit: no
`removeFile` (nothing needs it before the shell exists, and the unresolved-import diagnostic
is what makes adding it safe later), facts do not cross the boundary yet (∆RGQ), and
per-unit interning duplicates strings across the linked pool — a link-time dedup nobody
needs at this size.

Measured, on the corpus fixtures plus a synthetic 101-file site: ~1 µs per-call floor, ~3 µs
mean per page render against a <1 ms/page criterion, and one big compile at 1.24 ms versus
~4.7 ms across 101 single-file calls. Few large calls is validated. ∆xWs records the numbers
that matter against the exit criterion.

## Retrospective

**A field nothing reads is not a fact, it is a comment.** `Unit::facts` was written,
tested for content, and documented as the D25 cache key — and no code consumed it, which is
exactly the gap the silent-argument-swap fell through. I had tested that facts contained the
right thing, never that anything acted on it. When an item's stated purpose is "the compiler
reports X", the test that matters is the one where reporting X changes an outcome; a test
asserting X's shape can pass forever while X is inert.

**Reusing a type across a boundary reuses its invariants too.** Keeping `bytecode::Function`
inside `Unit` is what let `link` be a rebase and left the VM untouched — the right call. But
`Function::file` has no meaning until link fills it in, so the type is now quietly bimodal
and its doc comment describes only the post-link half. Eric caught that by asking where a
unit's bytecode lived. Cheap reuse across a new boundary is worth it; the honest cost is
that some field's invariant now holds only on one side, and the doc should say which.

**The editor complaint was a gate gap wearing a costume.** `#![cfg(target_arch = "wasm32")]`
on the ABI tests looked like a rust-analyzer annoyance. It was: those tests were invisible
to `cargo clippy --all-targets` and to `cargo test`, compiled only by the wasm leg nobody
runs in the inner loop. Removing it changed nothing about what runs where — the macro
already expands to nothing off wasm32 — and put the file inside the normal gate. Worth
suspecting any `cfg` that excludes a whole file from the host build: it may be buying less
than it costs.

**Two of seven review findings were "works in the test, broken in the artifact".** The
built-package path and the resident-file ordering both passed every spec I wrote, because
my specs ran from `src/` and compiled files in the order I happened to hand them over. Tests
written alongside the code inherit the code's assumptions about its environment — the same
shape [[∆c8h]] recorded for a helper's blind spot, one layer out: there the test mirrored
the algorithm, here it mirrored the layout. A spec pinned to the `dist/` layout now exists
because the reviewer, not the author, thought to ask where else this runs.

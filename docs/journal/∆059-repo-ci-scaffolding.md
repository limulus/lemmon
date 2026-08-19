---
id: ∆059
completed: 2026-08-18 22:39:34 -07:00
---

## Backlog item

> - ∆059 Repo + CI scaffolding — Rust workspace (`crates/lemmon-syntax`, `lemmon-vm`,
>   `lemmon-wasm`) with CI building and testing it alongside the existing npm workspace
>   (`packages/core`, `packages/shell`, `.github/workflows/ci.yaml`), so every later item
>   lands in a green harness. Layout per `CLAUDE.md`.

## Planning

Three questions were settled before any code:

1. **The Rust checks fold into `npm run verify`** rather than living as a separate
   command, so a Rust regression cannot sail past the pre-push hook. This later forced a
   second decision — see Refinement.
2. **`lemmon-wasm` is compile-check only.** wasm-pack packaging and the typed JS wrapper
   belong to `∆TNT`, which owns the boundary; doing them here would leave that item
   hollow.
3. **No Rust coverage gate**, deliberately deviating from the project-wide 100% rule.
   Thresholds against placeholder crates measure nothing; the tool and threshold get
   chosen against real code with `∆TEJ`. Eric approved the carve-out explicitly.

The three crates were wired as a real dependency spine — `lemmon-syntax` (`Span`) →
`lemmon-vm` (`RuntimeError`, re-exporting `Span`) → `lemmon-wasm` — so every workspace
edge is exercised by a test rather than leaving three unconnected islands. `Span` and
`RuntimeError` are starting shapes, not commitments; `∆FDQ` and `∆7Ib` are expected to
reshape them.

## Refinement

**The CI shape changed substantially mid-task, at Eric's prompting.** The plan had one
`wasm` job that only cross-compiled. Eric asked whether the node matrix should build the
Rust too, then whether a job should build the wasm into an artifact for the node jobs to
consume. The second idea was right, and the pipeline was rebuilt around it:

| Job | Matrix | Purpose |
|---|---|---|
| `rust` | — | fmt, clippy, `cargo test` on the host — once, not once per node version |
| `wasm` | — | release `wasm32` build → stage → upload artifact |
| `verify` | node 24, 26 | `verify:js` + `build` + coverage. No `needs:`, so TS feedback is not blocked behind cargo |
| `wasm-check` | node 24, 26 | `needs: wasm` → downloads the artifact, compiles it under each Node |

Building a `.wasm` never consults Node, so matrixing the *build* buys nothing; accepting
one does, since enabled WebAssembly proposals move between Node majors. Hence one build,
many consumers — and every runtime is tested against the exact bytes that would ship.

The cost: **CI no longer runs the single `npm run verify` command that pre-push runs.** To
keep that checkable rather than aspirational, `verify` is now *defined* as `verify:js` +
`rust`, and the CI header says so. Adding a leg means updating both.

Four things were wrong and got fixed:

- **`Swatinem/rust-cache` was placed before any toolchain resolution.** Its README is
  explicit that toolchain selection must come first, since the cache keys on the active
  rustc version. A `cargo --version` step now precedes it, which makes rustup honor
  `rust-toolchain.toml` — keeping that file the single source of the version pin instead
  of duplicating it into a setup action.
- **`prebuild` runs `clean`, and `clean` had grown `target`.** So `npm run build` deleted
  the staged wasm and, for anyone without `CARGO_TARGET_DIR` redirected, their entire
  Rust build cache. Split into `clean:js` (what `prebuild` uses) and `clean`.
- **The staging script is `.ts` run through Node's type stripping**, so jobs running it
  need `setup-node`, not the runner's default Node.
- **A failed `cargo build` skipped staging and left the previous artifact in place**, so
  `wasm:check` would go green against stale bytes. `rust:wasm` now clears `artifacts/`
  first.

## Retrospective

**The invariant-1 check was theater, and a code review caught it.**
`scripts/check-wasm.ts` originally allowlisted imports by *module*, and was described —
in its own doc comment, in `CLAUDE.md`, and to Eric — as mechanical enforcement of "no
filesystem, network, clock, randomness". It was not. Adding `js_sys::Date::now()` proved
it: the import modules stay `__wbindgen_placeholder__` / `__wbindgen_externref_xform__`
unchanged, and the clock appears only as an import *name* (`__wbg_now_<hash>`). On
`wasm32-unknown-unknown` there is no WASI module to catch, so the check caught nothing
that would actually happen. It now allowlists hash-stripped import names and fails
closed.

The lesson is not "write a better allowlist" — it is that **a check asserted to enforce an
invariant must be tested against a real violation before it is described as enforcing
anything.** Every other guard in this change was negative-tested (clippy warnings,
misformatting, failing tests, corrupt wasm, missing exports); this one was only
positive-tested, and the confident claim went into three places on the strength of it.

Smaller: `Span::new` accepted a reversed range, where `len()` underflows to roughly four
billion in the release build that ships. A `debug_assert!` at construction, plus a
`#[should_panic]` test, now pins it. Worth remembering that the release wasm profile —
where `debug_assert!` is compiled out — is the one that ships, so guards belong where a
value is *created*, not where it is read.

Deliberately left for later: `∆TNT` must revisit the import allowlist, since
`wasm-bindgen-cli` rewrites the placeholder imports into its own `wbg` shim; and
`scripts/` sits outside the coverage gate, which is documented but sits awkwardly next to
`check-wasm.ts` carrying real logic.

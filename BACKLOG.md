# Backlog

This backlog is managed with the **delto** skill — before adding or changing items, consult
it (and the authoring reference it points to) so the conventions below stay enforced.

Organized as Initiative (`##`) → Epic (`###`) → Item (`-`); initiatives are roughly
priority-ordered, items 5 lines max.

Each item starts with a “deltoid” — `∆` followed by 3 alphanumerics, e.g. `∆7hy`. Deltoids
are immutable and travel with the item into its completed-work journal entry in
`docs/journal/`. To mint a collision-free deltoid, run
`npx @limulus/delto@1 mint --journal-dir docs/journal` (see `delto mint --help`).

Hard prerequisites use a trailing `; needs: ∆aaa[, ∆bbb]` suffix — the only dependency
mechanism: `delto surface` trusts these edges to decide what is eligible to work on.

## Phase 0 — Spike: one real page from the WASM core

Exit criterion (architecture doc 3): limulus.net's `the-rebound` post renders through a
minimal base→page→article layout chain from the WASM core, byte-similar to the 11ty output,
with VM render time under 1 ms/page. Everything here serves that demo.

### Core spike

- ∆FDQ Scanner — logos lexer with text/tag modes per `docs/language.md` §Scanner (triggers
  `<lem.` `<lem-` `</lem-`, first-`>` termination), plus the round-trip property test; it is
  the substrate every stage sits on; needs: ∆059
- ∆7Ib Parser bake-off — one-day lelwel spike against a hand-written recursive-descent start
  (docs/architecture.md §Crates names the candidates); keep whichever gives better error
  recovery for less code, and record the loser and why in the journal; needs: ∆FDQ
- ∆hw4 Parser → lossless CST + typed AST for the spike subset (`lem.`, `lem-if`, `lem-for`,
  both `lem-let` forms, block `lem-fn`), spans everywhere, recovery good enough that a
  half-typed template still yields a tree; needs: ∆7Ib
- ∆TEJ Golden corpus harness — `tests/corpus/` inputs with expected trees/diagnostics (later
  expected HTML), cargo-test driven, so language changes are locked by tests from the first
  week and the future tree-sitter grammar has an agreement target; needs: ∆hw4
- ∆c8h Codegen + VM — stack machine per docs/architecture.md §VM (output buffer,
  Capture/EndCapture for `lem-let` blocks and slots, calls with named-arg reordering,
  spanned runtime errors), enough to run the spike subset; needs: ∆hw4
- ∆Ldn Document mode — fence-aware segmenter, in-core comrak rendering of joined prose,
  `<!--lem:N-->` sentinel splice with lost-sentinel build errors, untyped front matter
  passthrough for now (docs/language.md §Document mode); needs: ∆c8h
- ∆TNT WASM boundary — wasm-bindgen build of the core, `@limulus/lemmon-core` typed wrapper
  (compile/loadData/render), and a small Node harness that renders fixtures, measuring
  boundary marshalling cost so the few-large-calls design is validated early; needs: ∆c8h

### Prove it

- ∆xWs Render `the-rebound` end to end — port minimal base/page/article layouts to `.lem`,
  render the post (document mode included) through the Node harness, diff against the
  11ty-built HTML, and record VM + boundary timings against the <1 ms/page exit criterion;
  needs: ∆Ldn, ∆TNT

## Phase 1 — Language complete, tooling from day one

Seed items for the next phase (architecture doc 3, phase 1); expect these to be split
further at claim time.

- ∆C8d Type checker + schemas — `lem-type`/`lem-enum`, reference fields (`by`), computed
  defaults, narrowing, project-flat + ambient namespaces (docs/language.md §Types, §Names),
  diagnostics with primary/secondary spans; turns limulus.net's build-time throws into
  compile errors; needs: ∆hw4
- ∆Q6L Stdlib — strings/lists/maps, `datetime("…")` (compile-time checked on a literal
  argument; there is no datetime literal) + rfc3339/rfc822 formatting, `slugify`,
  `readingTime`, `url()`, `asset()`, `raw()`, `uniqueId()`, with typed signatures the
  checker understands, since feeds/sitemap/layout ports all block on these; needs: ∆C8d
- ∆Hbw Document mode, full rules — nested `lem-let` capture bodies as sub-documents, typed
  front matter validated at YAML positions, string-literal reference resolution in prose
  calls (docs/language.md §Document mode items 2 and 6); needs: ∆Ldn, ∆C8d
- ∆RGQ Side channels + generators — `lem-style`/`lem-script`/`lem-head`/`lem-emit`
  collection and dedup, `lem-page` (including `for` generators with prev/next), serializable
  modules and render-reported deps for incremental dev; needs: ∆C8d
- ∆5MM tree-sitter-lemmon + LSP skeleton — embedded-template grammar with HTML/markdown
  injections, corpus agreement test wired into CI, `@limulus/lemmon-lsp` serving diagnostics
  and schema completions from `core.analyze()`; editor tooling is a day-one commitment
  (docs/decisions.md D16); needs: ∆TEJ, ∆C8d

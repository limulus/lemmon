# The `the-rebound` spike (∆xWs)

The Phase 0 proof: limulus.net's `/feed/the-rebound/` post rendered end to end from the
WASM core — document mode for the post body, then an explicit base→page→article layout
chain — held against the Eleventy-built page. Run it with:

```sh
npm run rust:wasm          # stage the core, if not already staged
npm run spike:the-rebound
```

The harness (`scripts/render-the-rebound.ts`) compiles everything in one call, renders
the document, feeds its HTML back through `loadData` as `page.content`, renders
`driver.lem`, and diffs the result against `fixtures/reference.html`. Every diff hunk
must match an allowlisted difference class or the run exits non-zero; timings for each
boundary call are reported against the <1 ms/page exit criterion.

## Fixtures

- `fixtures/index.md` — the post source, vendored from limulus-dot-net
  (`www/feed/the-rebound/index.md`) with one transformation: each Liquid
  `{% renderTemplate 'webc' %}<article-photo …>{% endrenderTemplate %}` block becomes a
  single-line lem call, `<lem.articlePhoto("…", placement: "…")>`.
- `fixtures/reference.html` — the Eleventy-built `dist/www/feed/the-rebound/index.html`,
  byte-for-byte.
- `fixtures/data.json` — the ambient payload (`site`, `page`, `build`): every computed
  value the 11ty pipeline derives at build time (dates in all their formats, photo
  renditions from `photos.json`, revision history, reading times, hashed asset URLs, the
  recommended-article cards, SVG-sprite desc counters). Precomputing these in the
  harness is the deal invariant 1 imposes: the core takes them as data, the way the
  shell and asset manifest eventually will.

The reference and `data.json` are a matched pair: the recommended cards, bundle hashes,
and desc counters all come from the same build. To refresh, rebuild limulus-dot-net
(`npm run build` there), copy the built page over `fixtures/reference.html`, and re-derive
the affected `data.json` values from it. `sprite.lem`'s symbol block comes verbatim from
the same build.

## Ported layouts (`site/`)

Page-specific, hardcoded ports of the WebC layouts and components — including WebC's
emitted whitespace, scoped-class hashes, and per-page svg desc counters — parameterized
only where `data.json` feeds them. They are a rendering-fidelity fixture, not the start
of the real limulus.net port (that arrives with the Phase 1 language features:
schemas, stdlib, channels).

Reproducing WebC's whitespace is why the definitions hug their opening tags
(`<lem-fn f()><article …>`): D26 dedents a body laid out on its own lines, and hugging is
what keeps the margin the reference emits. Where a close tag does sit on its own line, the
body carries one newline more than it emits, since D26 takes one off each edge. The
templates read badly because of it — ∆Ma4 replaces the byte comparison with an
HTML-normalizing one so they can be written properly.

## Allowlisted difference classes

Each class is a normalization under which a hunk's two sides agree, plus a guard naming
what the hunk must contain for that class to claim it — without the guard, a class that
collapses whitespace absorbs every unrelated indentation change too:

- `heading-id` — markdown-it-anchor percent-encodes heading ids and adds
  `tabindex="-1"`; comrak emits plain slugs.
- `md-photo-wrap` — markdown-it re-wraps the tail of each in-content `<article-photo>`
  block in a spurious `<p>`; lemmon's sentinel splicing emits the markup unwrapped.
- `attr-escaping` — lemmon escapes `"` inside attribute values; WebC emitted the raw
  character.
- `whitespace` — blank-line-only differences (currently zero hunks). Indentation deltas
  on content lines are deliberately *not* covered: they were driven to zero by matching
  WebC's emitted whitespace, and must fail if they come back.

A note on formatting: the 11ty build's dprint transform silently fails on this page
(the `md-photo-wrap` mangle is unparseable markup) and ships it unformatted, so the
harness compares raw bytes; it only formats both sides when the reference itself can be
formatted.

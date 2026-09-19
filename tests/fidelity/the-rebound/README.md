# The `the-rebound` fidelity harness (∆xWs)

The Phase 0 proof: limulus.net's `/feed/the-rebound/` post rendered end to end from the
WASM core — document mode for the post body, then an explicit base→page→article layout
chain — held against the Eleventy-built page. The comparison is a leg of `verify:js`, so
`npm run verify` runs it. To run it alone:

```sh
npm run rust:wasm          # stage the core, if not already staged
npm run test:fidelity
```

The harness (`scripts/render-the-rebound.ts`) compiles everything in one call, renders
the document, feeds its HTML back through `loadData` as `page.content`, renders
`driver.lem`, and compares the result against `fixtures/reference.html` as rendered HTML
(`scripts/lib/rendered-html.ts`): parse5 trees, attributes by value, text after
browser-style white-space processing. Every remaining difference must be one of the
forgiveness classes below or the run exits non-zero.

The timings are not gated. `npm run test:fidelity -- --time` repeats the loads and
renders, prints a per-call table, and holds the page's render time against the <1 ms/page
exit criterion — a verdict about the machine it runs on, which is why the gate makes one
pass and applies no budget.

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
the same build. A refresh can change how often a forgiveness class fires; if it does, the
harness fails until `EXPECTED_FORGIVEN` in `scripts/render-the-rebound.ts` and the counts
below say the new numbers.

## Ported layouts (`site/`)

Page-specific, hardcoded ports of the WebC layouts and components — including WebC's
scoped-class hashes and per-page svg desc counters — parameterized only where
`data.json` feeds them. They are a rendering-fidelity fixture, not the start of the real
limulus.net port (that arrives with the Phase 1 language features: schemas, stdlib,
channels).

They are written the way an author would write them: a definition on its own lines with
an indented body, one statement per line, a blank line between definitions. D26 takes the
definition's margin off, D27 takes the statement lines, and D28 says the rest of the
whitespace is nobody's product surface — the comparison below cannot see it. Only the
whitespace a browser renders is deliberate in these files: the two licence icons in
`page.lem` sit adjacent because a space between two `svg`s would render, and each
revision message stays on one line because it is inside a `pre`.

## What the comparison sees

`renderedPage()` knows the UA stylesheet: block elements break the line, replaced
elements sit on it like a character, `pre`/`textarea` keep their text byte for byte,
`script`/`style` compare with indentation removed, and collapsible whitespace survives
only where a browser would paint it. It knows nothing of the site's CSS, so the harness
declares the two custom elements whose whitespace that CSS decides — `article-photo` is
a block inside `article-header`, `data-chip` is a flex container — each checked against
the `.webc` source, since a declaration the CSS does not back would hide a real space.

## Forgiveness classes

Each class is one difference the harness accepts, stated narrowly enough that it cannot
absorb one it does not explain — a class that forgave every `p` would also forgive a
dropped paragraph beside the one it meant to excuse. The run reports how often each fired
and fails if a count has moved: the expected counts are 6, 6, and 1, checked against
`EXPECTED_FORGIVEN` in the harness.

- `heading-id` — markdown-it-anchor percent-encodes heading ids and adds
  `tabindex="-1"`; comrak emits plain slugs. Only `id` and `tabindex` are set aside on an
  `h2`; every other attribute must still agree.
- `md-photo-wrap` — markdown-it wraps the tail of each in-content `<article-photo>` block
  in a spurious `<p>`, which the HTML parser's adoption agency turns into a ghost
  `<p><a></a></p>` and an empty `<p>` inside the figure; lemmon's sentinel splicing emits
  the markup unwrapped. Only those two shapes are forgiven, and only in a figure holding
  a `pho-picture`.
- `attr-escaping` — WebC emitted a raw `"` inside a double-quoted attribute value, so the
  parsed reference has the value cut at the quote and the rest as junk attribute names;
  lemmon escapes it. Forgiven only when the reference's value is ours cut at the first
  quote and every junk name is a token of what was cut off.

The 11ty build's dprint transform silently fails on this page (the `md-photo-wrap`
mangle is unparseable markup) and ships it unformatted; that no longer matters, since
nothing here compares bytes.

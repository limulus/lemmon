---
id: ∆D3R
completed: 2026-10-17 14:15:48 -07:00
---

## Backlog item

> - `∆D3R` `lem-style` in the ported layouts
>   - The three CSS files and the components' scoped styles become `lem-style` blocks in the
>     port, replacing the interim `<link>` to passthrough-copied CSS

## Planning

The item's brief, folded here. From `∆rT5` Schemas, layouts, components: `site.lem` types,
the base/page/article/video layouts and the WebC components rewritten as `.lem` functions
with `lem-style` scoping (D20). Scope here: branch work — the layouts and components from
`∆foi` gain `lem-style` scoping and the interim `<link>` goes — serving slice 2's
acceptance: Eric opens the seven posts and sees them identical to the live site, with
their CSS as per-page hashed bundles under `/assets/immutable/`, and `npm run parity`
reports zero unexpected differences for them. `∆F4y`'s handoff added the config's
`plugins: [bundler()]`, `<lem.mark("styles")>` in `base.lem`, and a `css-bundle` parity
class replacing `interim-css`.

Planned in the slice-2 run of 2026-10-17 by a read-only Opus agent against `∆F4y`'s
interfaces, and reviewed by a second that applied the plan's edits to a scratch copy of
the site, built it with the real shell, and ran parity and Chrome screenshots before
anything was written into the branch. The translation rule is mechanical so each block
diffs against its `.webc`: wrap in `.fnName { … }`, `:host` as `&`, `@media` nested,
`@keyframes` after the wrapper — every selector in the twelve sources starts with
`:host`, so none needs WebC's descendant prefix, and `&` keeps WebC's specificity. Order
is the subtle part: Eleventy's bundle is post-order (a component's children, the body
before its layout, each layout before the one it calls), and Lemmon collects in run
order with the body rendered first, so a component's block goes after the calls it makes
and a layout's between its capture and the next layout call. The three global
stylesheets cannot be read as data (a data source is YAML or JSON), so the port carries a
generated copy as `lem-style` blocks, with a drift spec. The review found the planned
screenshot check flaky (lazy images aborted mid-load) and blind on three posts (an
allowance per post and variant), and that "identical to the live site" cannot hold while
Lemmon has none of the site's client scripts (`∆d1N`, slice 4).

## Refinement

Built on the branch in three reviewed commits by Opus agents, the main session committing
each after the branch's `npm test`, `npm run tscc`, lint and `npm run parity` passed.
Checkpoint 1 (`d111d16`): `plugins: [bundler()]`, `mark("styles")` and the generated
`site-styles.lem` in `base.lem`, and `css-bundle` — the stylesheet `href` forgiven only
where both bundles exist, are named by the hash of their bytes, and end with the three
global stylesheets. Checkpoint 2 (`0981029`): every component's and layout's style as a
nested `lem-style` block, `authorChip` calling `dataChip` instead of printing its class,
a spec holding every nested selector to an explicit `&`, and `css-bundle`'s third check —
both bundles, Eleventy's with WebC's hashes renamed to function names, normalize equal
through lightningcss and css-tree. The six article posts share one bundle and
new-york-2024 has its own, as in Eleventy's build. Checkpoint 3 (`6fdc8d6`):
`npm run parity:look`, Chrome screenshots of every post from both builds at 1280 px light,
390 px light and 1280 px dark, after neutralizing on both pages only what parity forgives
by name (recommended cards, footnote markup, the reading-time minute, markdown-it's empty
photo wrappers), with remote images answered by one fixed PNG, client scripts off on both
sides, and software rendering; all 21 match pixel for pixel, byte-identical across runs.
`-- --serve` serves both builds side by side. The porting guide gained a Styles section.

The reviews found, among others: check 2 comparing the generator with itself while
nothing read Eleventy's bundle; specs whose fixtures were the same length on both sides,
so a missing or extra fragment passed; a bare nested selector invisible to the normalizer
yet dropped by browsers inside D20's floor; GPU antialiasing making identical pages
differ; parse5's round trip dropping a `pre`'s leading newline; and neutralizer specs
that never asserted what each side must keep. What the screenshots cannot see by design:
the closed revision history, rules that apply only at another pixel ratio or colour gamut,
and the site's scripts.

## Retrospective

Building the plan's edits in a scratch copy during review, before the branch was touched,
settled the questions reading could not — collection order through the real VM, the
normalized bundles' equality — and left the build itself with no surprises in the CSS.
Every remaining finding was in the checking machinery, where a check that passes for the
wrong reason looks exactly like one that works.

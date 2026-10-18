---
id: ∆ITg
completed: 2026-10-17 21:59:19 -07:00
---

## Backlog item

> - `∆ITg` Computed defaults and the link-post appender
>   - The link-post appender as a site `transformDocument`, penumbra's Liquid include as a lem
>     call, and markdown-it's extras; front matter and defaults landed in `∆tAw`
>   - needs: `∆ETy`, `∆tAw`

## Planning

The item's brief, folded here. From `∆o24` Content + markdown parity: typed front matter
for every `.md`, computed defaults replacing `eleventyComputed.js` (title from `linkTitle`,
`tilTopic` from the path, link and via domains, `readingTime`), the link-post appender as
`transformDocument`, markdown-it's footnotes, task lists, attrs and anchors in comrak
(core work where comrak lacks them, with their decision record), and code by the
highlight.js producer (`∆Nxd`). Scope: slice 4's acceptance. Before it started, `∆tAw`
took the typed front matter and the three defaults the chips read, so what remained was the
appender, the Liquid include four penumbra documents carry, and markdown-it's extras.

Built on the port branch in the slice-4 run of 2026-10-17 by an Opus agent on `∆ETy`'s
`transformDocument`, reviewed read-only by another (`2980f8e`):

- `lemmon/link-posts.ts`, a site plugin, appends what Eleventy's `linkPostContent`
  preprocessor appends — `[link]:`, and with a `viaUrl` the `[via]:` definition and the
  "Via domain." paragraph — to documents whose front matter has a `linkUrl`. Its spec
  extracts Eleventy's own preprocessor from `eleventy.config.ts` and compares the two over
  synthetic cases and every document, so "to the byte" is a claim that can fail.
- The include sits inside a raw HTML block, where a lem call is refused, so each 5-line
  block became one top-level `<lem.demoWithFallback("…")>` and the branch's translator
  (now `eleventyFormsOfLemmonCalls`) restores the exact lines for Eleventy and the revision
  hash: all 25 hashes and Eleventy's output are unchanged.
- Parity's `heading-id` forgives Eleventy's `tabindex="-1"` on a heading whose explicit
  `{#id}` is equal on both sides, as it already did for slug ids.

## Refinement

- markdown-it's extras were counted before anything was built: no document uses a task
  list or an attribute beyond `{#id}`, which comrak's header attributes already handle
  (D08), so neither was built. Footnotes stay comrak's: `footnote-markup` forgives them
  until the cutover, and `∆oAl`'s brief lists what readers will see change — `1` for
  `[1]`, `↩` for `↩︎`, and the separator rule `main.css` draws for `section-separator`
  — for Eric to accept or fix then. That reverses the brief's expectation of core work.
- A `viaUrl` that is not a URL stops the build at the post; a `linkUrl` that is not a
  string is left to the schema, whose error at the key is better than a transform's.
- Clearing the Via paragraph let the comparator reach the rest of three link pages, whose
  rows moved; each was re-derived from where every class fires, and the review re-derived
  two more from the output.
- At the end of the item 21 of the 25 documents compare clean; the other 4 differ only in
  their per-page scripts, `∆d1N`'s.

## Retrospective

The brief planned core work for footnotes, task lists and attributes from the markdown-it
plugins Eleventy loads, not from what the documents use; counting first cut it to nothing
and left one reader-visible choice for Eric. The review found two specs that could not fail
for the reason they claimed — a refusal case that a looser regex also refused, a host whose
`www.` sat where the anchor did not matter — the kind of gap only a perturbation finds.

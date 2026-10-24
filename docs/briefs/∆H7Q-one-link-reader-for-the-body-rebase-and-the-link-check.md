# One link reader for the body rebase and the link check

## Background

Found by a review of the slices on 2026-10-20. The body pass (D46,
`packages/shell/src/lib/rebase.ts`) rebases a document's links by Eleventy's rule over
`@11ty/posthtml-urls`' table, as limulus.net's feeds had them: a `ping` split at commas, a
`srcset` re-read and re-joined through `parse-srcset` 1.0.2 (a candidate whose descriptors
do not parse dropped, which the library reports on standard output, captured in
`links.ts`), and an SVG `href` or `xlink:href` left alone. The link check (D47,
`link-scan.ts`) reads the same markup by HTML's rules: a `ping` split at whitespace, every
SVG element's `href` and `xlink:href`, and the extra rows it adds (a refreshing `meta`,
Open Graph urls, inline CSS). The two share the table and, since `ea7323d`, one element
walker; the differences exist only for byte parity with Eleventy's feeds, which the
cutover retires. So a link the check reads, an SVG `href` say, can stay relative in
`build.bodies`, where a feed reads it on another page.

Eric decided on 2026-10-24 to unify them on HTML's rules.

## Scope and constraints

- The rebase reads a body's links as the link check does, and rewrites the attributes it
  reads; `parse-srcset` and the comma splitter go, and D46's list of divergences from
  Eleventy is revised.
- limulus.net's feeds should not change: verify that no document body holds a `srcset`,
  `ping`, or SVG link the new rule would rewrite, so `lemmon-port`'s parity still passes
  until the cutover merges.

# Link and asset integrity check

## Background

### From `∆7Xs` Output writer

Output writer — HTML to disk with marks resolved, the `styles` producer writing per-page
hashed CSS bundles of what `lem-style` collects under `/assets/immutable/…` (today ten
characters of a sha256; `name.<8hex>.ext` is esbuild's and passthrough's convention,
D13/D20), passthrough copy by glob (`assets/**` plus png/svg/jpg/xsd/xsl/ txt/pdf), and a
link/asset integrity check, CloudFront untouched

## Scope and constraints

- Slice 4's acceptance, which this item serves: Eric builds the branch and all 26 documents,
  code blocks highlighted and client JavaScript bundled to hashed URLs, compare clean.
- The integrity check from the original output writer item; HTML and passthrough are `∆muD`,
  the `styles` producer is `∆F4y`.

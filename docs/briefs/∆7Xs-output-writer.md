# Output writer

## Background

Output writer — HTML to disk with marks resolved, the `styles` producer writing per-page
hashed CSS bundles of what `lem-style` collects under `/assets/immutable/…` (today ten
characters of a sha256; `name.<8hex>.ext` is esbuild's and passthrough's convention,
D13/D20), passthrough copy by glob (`assets/**` plus png/svg/jpg/xsd/xsl/ txt/pdf), and a
link/asset integrity check, CloudFront untouched

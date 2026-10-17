# Mark resolution and the default plugin

## Background

### From `∆7Xs` Output writer

Output writer — HTML to disk with marks resolved, the `styles` producer writing per-page
hashed CSS bundles of what `lem-style` collects under `/assets/immutable/…` (today ten
characters of a sha256; `name.<8hex>.ext` is esbuild's and passthrough's convention,
D13/D20), passthrough copy by glob (`assets/**` plus png/svg/jpg/xsd/xsl/ txt/pdf), and a
link/asset integrity check, CloudFront untouched

### From `∆PgF` Plugin host

Plugin host — the `Plugin` hooks (data, transformDocument, assets, transformOutput, emit,
commands, watch, and `marks`: producers with keys and a payload record, decorators, two
producers on one key a load error, the shell's `styles`/`scripts`/`head` bundler a default
plugin the site lists — D42), plugin `.lem` bound via `lem-import` in `site.lem` (D23, D06),
and a content API for out-of-build tools. Gates the port (D12)

## Scope and constraints

- Slice 2's acceptance, which this item serves: Eric opens the seven posts and sees them
  identical to the live site, with their CSS as per-page hashed bundles under
  `/assets/immutable/`, and `npm run parity` reports zero unexpected differences for them.
- The marks part of the plugin host only: producers with keys and a payload record,
  decorators, claims sent as `HostDecl.marks`, two producers on one name and key a load
  error naming both, and the shell's `styles`, `scripts` and `head` producers as a default
  plugin the site config lists. The other hooks are `∆c4j`.
- The `styles` producer writes what `lem-style` collects under `/assets/immutable/…` with
  the hash convention the original item records (today ten characters of a sha256;
  `name.<8hex>.ext` is esbuild's and passthrough's convention, D13/D20).

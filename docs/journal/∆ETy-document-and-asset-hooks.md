---
id: ∆ETy
completed: 2026-10-17 21:18:00 -07:00
---

## Backlog item

> - `∆ETy` Document and asset hooks
>   - `transformDocument` rewrites a document's text before it compiles, and `assets` hands
>     a plugin's manifest to `compile` with the files it built; `∆c4j`'s journal says why

## Planning

The item's brief, folded here. Split from `∆c4j` Plugin host on 2026-10-17, when Eric
chose to build only the hooks slice 4 consumes (`∆c4j`'s journal entry). The
architecture's sketch had `transformDocument?(doc, ctx): Document`, a pre-markdown text
transform, and `assets?(ctx): Promise<AssetManifest>`; D38 had settled the manifest the
core takes — finished URL strings on the host declaration, `asset("…")` a compile-time fact
— and D42 that mark handlers are synchronous. Its consumers: `∆ITg`'s link-post appender,
which appends reference definitions and a Via paragraph to a link post's markdown, and
`∆d1N`'s esbuild plugin, which hands `compile` its scripts' URLs. Constraints carried in:
the transformed text is what compiles and what `CheckResult.texts` frames diagnostics
against; asset files pass the write's collision checks and are written only after every
check; plugin objects outlive builds in `lem dev`.

The slice plan's review found that `Page.body` and every span are UTF-8 byte offsets and
most documents have non-ASCII front matter, so the hook takes the body after a
byte-identical front-matter prefix, in `discover()` before the text reaches `texts` or
`compile`. Built by an Opus agent in an isolated lemmon worktree — the port branch was
being built against main at the same time, and runs the shell's source live — reviewed
read-only by another, and fast-forwarded onto main (`b20a656`). The decisions are D44:

- `transformDocument` sees `.md` documents only, synchronously, and rewrites the body; the
  front matter it is handed is a frozen copy of the parsed record. A body that would move
  where the core finds the front matter is refused, each cause in its own words. A failure
  is an error at the document's front matter: an untransformed link post renders wrong
  without a word.
- `assets` may be async and runs once per build after discovery, before the core loads,
  with `{input, output, writes, env}`; manifests merge, a path two plugins map an error
  naming both; its files are claimed after passthrough and before pages under their own
  writer kind. A hook's failure is a diagnostic at the config file's start, the place
  `ConfigError` already names.
- D44 refines D06: the shell's trust reaches from what is done to a page after render to
  what plugins hand the core before it compiles.

## Refinement

- A lone surrogate in a transformed body or a manifest key makes `core.compile` throw,
  which would crash `lem` rather than report, so the shell refuses it.
- The review found a body-less document whose closing `---` ends the file refusing every
  append, with a message naming causes that did not apply; the shell now accepts a body
  that leaves the core's block unchanged, which is exactly the rule the core applies. It
  also found the README's example appending with one newline, which a reference definition
  cannot follow inside a paragraph — Eleventy's appender uses two — and that an asset hook
  had to read `process.env` where `env("…")` reads the build's own environment, so the
  context carries `env`.
- Diagnostics at the config file broke `CheckResult.texts`' documented promise that every
  diagnostic's path is held there; the docs name the exception, and the CLI prints them
  without a frame.
- A template is not transformed: every layout compiles against what a page template
  defines, so a transform there would change other files' units (invariant 11), where a
  document's definitions are seen by no other file.

## Retrospective

Most of the cost was refusals and their wording, not the hooks: three of the review's five
findings were about what the shell says when a transform goes wrong. Writing a throwaway
plugin for each consumer against the built API, as the review did, is cheaper than waiting
for `∆ITg` and `∆d1N` to find a shape problem.

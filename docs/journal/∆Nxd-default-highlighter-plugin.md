---
id: ∆Nxd
completed: 2026-10-17 21:33:10 -07:00
---

## Backlog item

> - `∆Nxd` Default highlighter plugin
>   - highlight.js as a `code` producer shipped with the shell, so a new site highlights out
>     of the box and limulus.net's `<pre><code class="hljs">` markup is reproduced exactly
>   - needs: `∆7wK`

## Planning

The item's brief, folded here. Default highlighter plugin: highlight.js as a `code`
producer shipped with the shell and listed by `lem init`, claiming its languages, so a new
site highlights out of the box and limulus.net's `<pre><code class="hljs">` markup is
reproduced exactly (D42); `lem theme` turning a highlight.js or `.tmTheme` theme into CSS;
a syntect-in-wasm producer a plugin a site adds, paying its 2 MB only there. Its need on
the plugin host went when `∆c4j` split: a `code` producer is a mark handler, which the
shell has run since `∆F4y`.

Built in the slice-4 run of 2026-10-17 by an Opus agent in an isolated lemmon worktree,
proved against a copy of the port with the worktree's shell, reviewed read-only by
another, and fast-forwarded onto main (`0c21265`); the port lists it in `b75a010`.
`highlighter(options?: {exclude?: string[]})` claims every highlight.js language name and
alias as a key of the `code` mark (368 keys for 192 languages; D42 forbids a wildcard), and
produces `<pre><code class="hljs">…</code></pre>` from `hljs.highlight(code, {language,
ignoreIllegals: true})` — limulus.net's markup, which is markdown-it's documented example,
so it is the default and there is no markup option. `exclude` rather than an allowlist, for
D42's named case of a site producing one language itself. highlight.js is pinned to exactly
11.9.0, the port's version, since grammar changes move spans and `pre` text compares
exactly; it adds about 5.3 MB unpacked, 1,535 files and no dependencies, and about 30 ms to
loading the shell's package entry, which a config pays when it imports any value from it.

## Refinement

- On a copy of the port all 8 highlighted blocks are byte-identical to Eleventy's; on the
  branch the two TILs compare clean and no parity count moved.
- Claims match as written: highlight.js lower-cases a fence's language and a claim does
  not, so ```` ```TS ```` keeps comrak's block and warns "did you mean `ts`?" where Eleventy
  highlights it. limulus.net writes none.
- Step 7's cache of a code block's output by payload id was not built: the producer costs
  about 1 ms of a 160 ms rebuild (7.6 ms in a cold process's first build, compiling
  grammars). The sentences that said a host caches on the id now say it can.
- The review pinned `ignoreIllegals` with an input whose output it changes, refused an
  `exclude` that is not a list (a string iterated as characters), and renamed the test
  mock that shared the plugin's name.
- `lem theme` was not built and is dropped rather than minted: highlight.js ships its
  themes as CSS, which a site links as limulus.net links `hljs-github.css`; only a
  `.tmTheme` would need converting, for a syntect producer nobody has asked for.
  `lem init`'s listing of the plugin and a theme moved to `∆V7i`'s brief.

## Retrospective

Comparing the copy's output against Eleventy's settled the trailing-newline question before
any test asserted it, and measuring before building the cache saved building it.

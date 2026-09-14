# D40 — Code highlighting takes its grammars from the host

- **Status:** Accepted 2026-09-13 (∆BfR), refining D08
- **Backlog item:** ∆BfR
- **Normative text:** `docs/language.md` § Document mode, Code blocks

## Context

D08 put syntect in the core for class-based highlighting, and ∆Ldn deferred it.
limulus.net has eight fenced blocks in its archive — TypeScript, Rust, Bash, and plain
text — highlighted today at build time by highlight.js through markdown-it, as `hljs-*`
classes styled by a hand-edited stylesheet. Three things came up while planning the item:

- **The size estimate was wrong.** The item said syntect adds about a megabyte to the
  `.wasm`. Measured, syntect with bat's grammars embedded through `two-face` takes the
  artifact from 2.17 MB to 4.82 MB, and syntect's own grammars, which lack TypeScript,
  to 4.22 MB.
- **comrak's bundled `SyntectAdapter` breaks two invariants.** When syntect returns an
  error it writes the code unescaped (invariant 7), and a language it does not know it
  guesses from the code's first line (invariant 10).
- **Eric asked that nothing here make it hard for a plugin to highlight another way.**
  syntect cannot emit highlight.js's classes, so limulus.net's parity may itself be
  such a case.

## Decision

**The host declares the grammars, as `.sublime-syntax` text on the declaration, and the
core embeds none.** They ride on the declaration for the asset manifest's reason (D38):
prose renders while compiling, so that is where they are consumed, and a document whose
code names a language records a digest of the whole set as a compile-time fact (D25). A
host that declares no grammars asks for no highlighting. Who produces the value decides
who declares it: which languages a site writes about, and whether it adds a grammar of
its own, is the site's and the shell's to say, not the core's.

**The declared grammars are parsed together, the first time a block names a language,
into one set linked in declaration order.** Every lookup after that is syntect's own —
a block's language by extension, then name; a grammar's `embed` or `include` by scope or
name — and the grammar declared last with a word wins, so grammars a site adds take
precedence over a default set. Built once and never grown, the set gives a block the
same HTML whatever an instance highlighted before it (invariant 1). The cost is the parse:
limulus.net's four languages take about 0.4 seconds per worker in wasm, TypeScript
nearly all of it, and bat's whole set about five seconds, so a host declares the
languages a site writes about — or, later, hands its workers a precompiled set, which
syntect loads in milliseconds and unpacks a language at a time.

**A grammar syntect refuses is left out, and its header is read** with the YAML loader
syntect uses, so that a block naming its language is told the grammar does not load,
with that file labelled, rather than that no grammar exists.

**The core's highlighter owns only the inside of `<code>`.** comrak's
`<pre><code class="language-x">` is written unchanged, as comrak writes it without a
highlighter; a highlighted block's spans carry the grammar's scope atoms prefixed `syn-`,
since a bare `comment` or `title` would collide with a site's own classes. syntect writes
those atoms unescaped, so a grammar whose scopes hold a character HTML reads as markup is
refused rather than trusted (invariant 7). A block with no language, one no grammar
answers, or one whose grammar fails is written exactly as comrak writes it, and the
latter two are warnings at the language word, with a did-you-mean. The adapter is the
core's own, not comrak's.

**No theme is part of the core.** A theme changes no class, so a stylesheet is the
site's, one page may use another than the next, and generating one from a `.tmTheme`
belongs with the shell's grammar set.

## Alternatives considered

- **Embed bat's grammars through `two-face`**, as architecture.md first listed.
  Rejected: 0.7 MB more `.wasm` than the engine alone, which grew the staged artifact
  from 2.12 MB to 4.10 MB, and a grammar set only a rebuilt core could extend. It does
  avoid parsing YAML — a precompiled set loads in about 2 ms, against a third of a second
  for TypeScript from YAML — which is the case for a compiled form the shell hands its
  workers, later, rather than for embedding.
- **syntect's own default grammars.** Rejected: no TypeScript, which half of
  limulus.net's fences are.
- **Parse a grammar the first time a block names its language**, with everything it
  references. Built first, and dropped: loading a set that grows needs its own index of
  every grammar's header, a dependency walk, a rule replacing a grammar whose scope a
  later one declares, and a rebuild in declaration order at each growth, since syntect's
  lookups prefer what was loaded last and it never relinks — 669 production lines
  against 503, to save a parse a host avoids by declaring its languages and a
  precompiled set avoids entirely.
- **Parse every declared grammar when the declaration arrives**, rather than at the first
  block that names a language. Rejected: a host that renders no code would pay for it.
- **Share the parsed set across workers in shared memory.** Not available: one instance
  per worker and nothing shared is the boundary rule, and sharing Rust structures
  between instances would need wasm threads on a nightly toolchain. Bytes can be shared —
  the grammar text, or a precompiled set — and each worker loads its own from them.
- **comrak's `SyntectAdapter`.** Rejected for the two invariants above; it also drops
  comrak's own `<pre>` attributes in class mode.
- **A `Core` method generating a theme's CSS.** Rejected for the core: a syntect-shaped
  method on a public surface, meaningless to a site that highlights another way.

## Consequences

- A different highlighter has three places to stand, none built: after render, over a
  host that declares no grammars, finding blocks by their `language-x` class; inside the
  core as a pure wasm module (∆SDN), where it would replace only a block's body; or as
  grammars of its own, which are data. A highlighter choice would be a compile-time
  input and ride the same fact as the grammars.
- syntect is still 1.97 MB of the `.wasm` for a site that highlights elsewhere; the two
  modules that name it (`crates/lemmon-vm/src/document/highlight/`) are what a cargo
  feature would gate.
- A grammar arriving for a language that had none leaves the documents that named it
  stale at link, which is how a shell can deliver grammars on demand.
- syntect's regex engine sizes an NFA in bytes, and pointers are half as wide on
  wasm32, so a pattern near the limit can load under native tests and fail in the
  `.wasm`, or the reverse. bat's Hosts File grammar is one; the corpus fixtures stay well
  clear.
- A `lem` fence warns until a host declares a grammar for it.
- limulus.net's parity for its highlighted blocks is ∆o24's choice. markdown-it writes
  `<pre><code class="hljs">` where comrak writes `class="language-x"`, so either the
  harness forgives the class and the inside of `<code>` and the site restyles for `syn-`
  classes, or highlight.js colours the blocks after render, over a host that declares no
  grammars, rewriting the class as it goes — which can match exactly.

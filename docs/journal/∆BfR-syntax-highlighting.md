---
id: ∆BfR
completed: 2026-09-13 19:41:07 -07:00
---

## Backlog item

> - ∆BfR Syntax highlighting for code fences — the core's own comrak adapter classes a block's
>   code `syn-` with the `.sublime-syntax` grammars the host declares (D40); a host declaring
>   none gets comrak's blocks untouched, and theme CSS is ∆Nxd's. Deferred out of document
>   mode as ~2 MB of `.wasm` the spike's page did not need; limulus.net's archive has fences

## Planning

**The item as filed rested on three assumptions that did not hold.** It said syntect adds
about a megabyte to the `.wasm`. Measured, syntect with bat's grammars embedded through
`two-face` took the artifact from 2.17 MB to 4.82 MB, and syntect's own grammars, which
lack TypeScript, to 4.22 MB. It assumed comrak's bundled `SyntectAdapter` would be wired in.
That adapter writes the code unescaped when syntect returns an error (invariant 7) and
guesses a language from the code's first line (invariant 10), so the core has its own
adapter. And a survey of limulus-dot-net showed its 8 fences are coloured by highlight.js
through markdown-it, as `hljs-*` classes in `<code class="hljs">`, which syntect's scope
classes can never reproduce.

**Eric asked that nothing preclude a plugin highlighting another way.** That settled the
markup: the core writes comrak's `<pre><code class="language-x">` unchanged and owns only
the inside of `<code>`, and a host that declares no grammars gets comrak's output
untouched. A post-render plugin, or ∆SDN's pure wasm modules, can then colour blocks from
markup that names the language.

**The grammars moved out of the core at Eric's suggestion.** Asked which grammar set to
embed, he proposed a fourth option: embed none, have the shell provide them, and let sites
extend the set. They ride on the host declaration for the asset manifest's reason (D38):
prose renders while compiling. Prose rendering inside `compile` is synchronous, so a
grammar has to be there before a document compiles. Eric's question about sharing grammars
across workers in shared memory led to parsing a grammar only when a block first names its
language. Step 0 measured that design before any code: the engine alone added 1.95 MB;
indexing bat's 226 grammar headers took 5 ms in wasm; parsing all of them took 4.9 s;
TypeScript alone took about 0.34 s to parse plus 0.13 s for its first highlight; and
two-face's precompiled set loaded in 2 ms. The rest was settled by asking: an unknown
language warns with a did-you-mean, classes carry a `syn-` prefix so a scope atom like
`comment` cannot collide with a site's own class, and theme generation belongs to the
shell item that became ∆Nxd.

## Refinement

**The per-language design shipped in checkpoint 1 at 806 production lines.** Making lazy
loading deterministic took a header index, a dependency walk, a rule replacing a grammar
whose scope a later one declares, and a rebuild in declaration order each time the set
grew, because syntect's lookups prefer whatever was loaded last and it never relinks. The
header index was an event-based YAML reader that re-implemented yaml-rust's scalar typing.
The first review found that syntect writes scope atoms into `class="…"` unescaped, so a
grammar with a quote in a scope could put markup into the page; such grammars are now
refused. Probes showed footnotes that comrak reorders or drops, `math` fences, and nested
containers all keep the per-block bodies aligned.

**Eric's `xhigh` review prompt asked whether the module could be simpler, even against the
plan.** It could, in two steps. First the hand-rolled reader became yaml-rust's
`YamlLoader` (about 20 ms more per worker), which also fixed a real bug: the index compared
raw `scope:` text while syntect trims trailing dots, reopening the load-order dependence.
That brought the module to 669 lines. Then Eric asked what precompiling options exist.
syntect's own serialized set is already linked, loads in milliseconds, and unpacks each
language's rules on first use, which is the laziness the design was paying 300 lines for.
So the core now parses every declared grammar together at the first block that names a
language, and lets syntect's lookups decide: 503 production lines. A grammar syntect
refuses still has its header read, so a block naming its language hears that the grammar
does not load, not that none exists.

**Checkpoint 2 found a crash in checkpoint 1's labels.** A warning whose label points into
a grammar file reached `index_for` at the boundary, which panics for a path it holds no
line index for; in the `.wasm` that traps the worker. It could not happen until the wire
carried grammars. The core now computes a grammar label's positions itself, as front
matter's arrive. The resident `Core` keeps its parsed set when a new declaration repeats
the grammars, and an ABI test highlights a 20,000-line fence, a 100,000-character line, and
brackets nested 5,000 deep on the pinned stack.

**Filed along the way:** ∆aaw (`readingTime` counts code, entity names, and span-split
tokens as words), ∆Nxd (the shell's default grammar set shipped precompiled, site grammars,
`lem theme`), and ∆5pk (`lem` fences highlighted by the core's scanner). ∆o24 was reworded:
code blocks follow D40's restyle-or-highlight.js choice, and it gained `needs: ∆PgF` for a
plugin hook it already used. One platform difference is recorded in D40: syntect's regex
engine sizes an NFA in bytes and pointers are half as wide on wasm32, so bat's Hosts File
grammar loads natively and not in the `.wasm`. The staged artifact grew from 2.12 MB to
4.10 MB.

## Retrospective

**I built the complex design before asking what syntect already provides.** Step 0 timed
two-face's precompiled set loading in 2 ms, which is syntect's lazy per-language format at
work, and I still designed checkpoint 1 around parsing YAML a language at a time. The
question that removed 300 lines came from Eric, after the code existed. When a library
fights a design, look for the form of the library the design is reinventing.

**I named a line count I had not drafted.** I told Eric the simpler shape would be about
330 production lines; it was 503, mostly doc comments and a per-block adapter I had assumed
would shrink. The project's own lesson is to measure a simplification before claiming it.

**I re-implemented a dependency's behaviour and only a review caught it.** The first
header reader copied yaml-rust's typing rules by hand, against CLAUDE.md's "don't re-invent
the wheel", and carried a scope-comparison bug the loader would not have had.

**I reported measurements as tables Eric could not read.** Restated as consequences — what
is paid, when, how often, and what removes it — they got an immediate decision.

**Probing before reporting worked.** The footnote-order question and the scope-escaping
question both became certain within a minute of a throwaway test, one a false alarm and one
a real invariant 7 hole.

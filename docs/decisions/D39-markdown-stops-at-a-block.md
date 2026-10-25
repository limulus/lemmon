# D39 — Markdown stops at a block

- **Status:** Accepted 2026-09-12 (`∆Hbw`), revising D07
- **Backlog item:** `∆Hbw`
- **Normative text:** `docs/language.md` § Document mode, items 1, 2 and 6

## Context

Document mode (D07, `∆Ldn`) splits a `.md` file into prose and lem-blocks, renders the
prose with comrak, and splices each block's output back in. What a block's *body* is
had been left open. §Document mode promised that a capture body —
`<lem-let note>…</lem-let>` — would itself be a document, markdown prose plus nested
blocks, so that a component could be handed prose; the implementation made every body
a template, as in a `.lem` file, and the doc carried a placeholder naming `∆Hbw`.
Building the promise meant a second rule for which bodies are markdown (`lem-if`,
`lem-for` and `lem-fn` bodies would have stayed templates), a per-line source map for
diagnostics inside dedented prose, chunk ownership across nested bodies, and a
fence-in-body limitation of its own. limulus.net's markdown, the first production
site, has no component call with a body: its six calls are void tags, and its four
wrapper elements hold an include, not prose.

## Decision

**A block is a template from its opening tag to its close, body included, and emits
HTML.** Markdown stops at the opening tag: a capture body keeps its asterisks, the
document's link definitions and footnotes do not reach into a body, and a fence inside
one is text like any other, so a tag in it is a tag. The rule an author can state is
that Lemmon emits HTML everywhere, in a `.lem` file and in a `.md` block alike.

**Prose inside an element is written as raw HTML.** CommonMark ends a raw HTML block at
a blank line and resumes markdown after it, and comrak runs with raw HTML passing
through, so an `<aside>` with a blank line on each side of the markdown renders the
prose inside the element, reference links and footnotes included, and a block standing
between those blank lines is spliced as anywhere else. A component that must wrap
prose is split into the element's two halves, or the element is written as that raw
HTML.

## Alternatives considered

- **Every capture body a sub-document**, as §Document mode item 2 first promised.
  Rejected: it is an exception to "Lemmon emits HTML" that needs its own rule for which
  bodies it covers, and adopting it by default would make walking it back a breaking
  change to every document that relied on it. The narrow rule can be widened later; the
  wide one could not have been narrowed.
- **A head-marked capture** — a new `lem-` tag, or a keyword past the name the way
  `from` marks the void source form — whose body comrak renders at compile time with
  its nested blocks spliced at render, joining the document's one comrak run so that
  references stay document-wide. Deferred, not rejected: it is additive and
  head-decidable (invariant 4), and the design above is what it would build. It is the
  form to take if a component ever needs a slot with markup.
- **A `markdown()` function over a string**, rendering at render time, for a
  markdown-formatted string arriving from data. Deferred: a string literal ends at its
  line, so it serves data rather than inline prose; and it would be a second string →
  `html` cast beside `raw()`, which invariant 7 names as the only one, so adopting it
  means amending that invariant and deciding whether raw HTML inside the string passes
  through.

## Consequences

- The corpus pins the rule from the author's seat: `document/blocks/
  capture-body-is-a-template`, `document/blocks/fence-inside-a-capture-body`, and
  `document/prose/raw-html-wrapper-with-blank-lines`, the clean half of
  `document/errors/raw-html-wrapper`.
- No crate changed: the segmenter and the compiler already treat a body as a template,
  and the `.cst`/`.tags` expectations — the tree-sitter agreement target (invariant 9)
  — are unchanged. The eventual grammar injects markdown outside blocks and Lemmon
  inside them, with no markdown nested inside Lemmon nested inside markdown.
- §Document mode item 1 no longer says fences inside a block body are respected, and
  the escape table says a fence protects prose only.
- The deferred-questions registers carry one line each, pointing here.

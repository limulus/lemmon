# D42 — Marks resolve in the host

- **Status:** Accepted 2026-09-13, settled in a planning pass before `∆1Hp` was built,
  superseding D40 and retiring `lem-emit` unbuilt; `∆1Hp` builds the mark and `∆7wK` the
  fence, and either adds a clause here for what the build changes. As built (`∆1Hp`): a
  name no handler claims is a warning, and a host that declares no handlers says nothing;
  `default` is a named argument, which keeps `∆7wK`'s positional payload additive;
  `mark()` lowers to an `Op::Mark` rather than running as a builtin, since a builtin
  cannot reach its call's file and span; a mark and a fragment each carry the file they
  were made in beside their span. As built (`∆7wK`): a claim carries its keys, the payload
  record, and `keyedBy`, the payload field holding a mark's key, since the plugin that
  declares the record is what knows which field it dispatches on; a unit's fact is the
  answer about a name *and* a key rather than one digest per name, so a producer gaining
  a language stales only the files that named it; a payload crossing as data may hold no
  function, at any depth, and no mark; a `math` fence is not a mark after all, since
  comrak renders it before consulting any seam and no option turns that off, so the rest
  are marked through comrak's highlighter seam, which writes comrak's own markup behind a
  sentinel; a payload's id is SipHash-1-3 over the info string and the code; `compile`
  returns the payloads beside its diagnostics, reported by the call that compiled a
  file even where another file's error stops the relink, since a resident unit is not
  compiled again; a producer that lists no keys answers only marks with no key, so
  "no wildcard producer" holds in the wrapper's dispatch as in the claims; a payload
  crosses as JSON, so the VM also refuses one holding a function or a float that is not
  a finite number, and two claims naming different payloads for one name are a
  malformed declaration. As built (`∆F4y`): a handler is an entry of a plugin's `marks` in
  `lemmon.config.ts`, and the claims `lem build` declares are the listed plugins', so an
  omitted list declares an empty claim set and warns at every mark; the shell sends no
  name-only claim — a decorator claims its name with an empty key list, and so does a
  keyless producer — since a claim listing no keys covers every key and would silence the
  warning at a code block's language; two producers on one name and key, and two handlers
  naming different payloads or key fields for one name, are refused at config load, naming
  both plugins, and so are keys for a name no handler keys by a payload field, whose
  `mark()` calls would carry no key for them to match; a handler that throws or returns
  anything but its content is a warning at the mark, which keeps the content it had; a
  code block's handlers see its payload beside its id; a producer writes a file of its
  own through its context's `write(url, content)`, which never throws: the file is queued
  and written after the pages, once where two pages' producers write the same bytes; a url
  or content of the wrong kind, a url naming no file, or other bytes at a produced file is
  an error at the producer's mark, and a clash with a page, a passthrough file or another
  produced file — the same file, the same but for case or Unicode form, or a file where the
  other needs a directory — an error at the later writer, labelled at the first, while what
  a producer that fails wrote is dropped; and the default plugin is `bundler()`, which a
  site lists in `plugins` like any other, so an omitted list claims not even `styles`: its
  `styles`, `scripts` and `head` producers list no keys, and `styles` writes a page's
  collected CSS, joined by newlines, to `/assets/immutable/bundle/<name>.css`, the name the
  first ten base64url characters of its SHA-256 — Eleventy's bundle form — and links it
  from every `styles` mark; no wrapping `lem-fn` ships with it, since a plugin's `.lem` is
  `∆wOn`'s, so a layout writes `mark("styles")`. As built (`∆Nxd`): the shell's highlighter
  is `highlighter()`, a plugin a site lists beside `bundler()`, over highlight.js pinned
  at exactly 11.9.0, since another release's grammars mark the same code differently; it
  produces `code` for each name and alias of every language highlight.js has, each
  matched as written, and writes `<pre><code class="hljs">` with highlight.js's spans —
  limulus.net's markup, which is markdown-it's documented one — with no language class
  and no option for one, since a decorator can add one; a site narrows its claim with
  `exclude`, the keys it leaves to another producer, an unknown key refused; and no
  output is cached by payload id, since highlighting limulus.net's eight blocks takes
  about 1 ms of a 160 ms rebuild. Refined by D46 (`∆zu0`, 2026-10-19): the string a
  template reads is not a page's finished output, rendered after every other page, but a
  document's body rendered alone, its marks resolved, and bound in `build.bodies` before
  every page renders
- **Backlog item:** `∆1Hp` (the mark, claims by name, the collect tags, the boundary);
  `∆7wK` (the `code` mark, keys and payloads, the core's highlighter removed); `∆F4y` (the
  shell's plugins, their handlers, and the default plugin); `∆Nxd` (the shell's highlighter)
- **Normative text:** `docs/language.md` § Marks, § Document mode, Code blocks;
  `docs/architecture.md` § WASM API, § Build pipeline, § Shell plugin API

## Context

Three things met while planning `∆1Hp` (side channels) against D40 (code highlighting).

- The `∆1Hp` plan had settled that an emit is a mark, not text: `<lem-emit styles />`
  writes nothing and records a byte offset, `render` returns the page as bytes beside
  its marks, and the shell splices at each offset. Three fixed names, no payload, no
  span.
- D40 had moved syntect's grammars out of the core and onto the host declaration, and
  its consequences named "a post-render plugin, finding blocks by their `language-x`
  class" as the way a different highlighter would stand — a plugin parsing HTML in the
  shell to find what the core had just written. Eric had asked that nothing make
  highlighting another way hard, and limulus.net's parity is such a case: it writes
  `<pre><code class="hljs">` with highlight.js's classes, which no grammar can produce.
- Eric proposed generalizing the mark: a named marker that can carry a body, emitted by
  a tag and by comrak for a labelled fence, resolved by plugins the shell calls in JS
  or through a plugin's own wasm, so that highlighting, styles, and scripts stop being
  core features shipped to every host.

Four angles were taken before deciding: an inventory of what the core would delete and
add, a survey of what limulus.net needs from such a surface, prior art (Hugo's render
hooks, Eleventy's bundle plugin, Astro and unified, Pandoc filters, Asciidoctor's
processors, Zola, Typst's and Extism's wasm protocols, djot), and an adversarial review
against the invariants. Measured: the core's highlighter and its plumbing are about 680
production and doc lines, 750 test lines, two dependencies, and 1.97 MB of the `.wasm`;
the shell's side of the design is work `∆PgF` and `∆7Xs` schedule anyway.

## Decision

**A mark is a named byte range in the rendered page, with its default content already
in the page, and the host resolves it after render.** The core records `{name, range,
payload?, span}` per mark and returns the page as bytes beside its marks and the
fragments collected under each name. A handler *replaces* the range. Where no handler
claims the name, or one fails, or the host runs no plugins at all — the playground, the
LSP — the page is already correct: a fence holds comrak's plain escaped block, a
placement holds nothing. That keeps D40's property that an unhandled block reads
exactly as comrak writes it, and it is what makes an unclaimed mark a warning rather
than a hole.

**A fence is a `code` mark.** comrak's adapter records a mark for every code block,
fenced or indented, keyed by its language word, with the info string and the literal as
its payload; a `math` fence is one such block. The core embeds no highlighter and takes
no grammars. Because a fence's payload is fixed at compile time, it is reported once,
by content hash, when the document compiles, and a render's mark carries the id:
nothing about the code crosses the boundary per render, and a handler's output is
cached on the same id.

**The template primitive is `mark(name, payload?, default?)`**, an ambient constructor
beside `raw()` and `asset()`, returning `html`. The name is a literal, checked at
compile against the host's claims as `asset()` is checked against the manifest (D38); a
payload is a record the claim names, checked at the call; a default is an `html` value
passed capture-then-call (D22). `<lem-emit>` is retired unbuilt: `mark("styles")` is
its spelling, and D20's row says so. The collect tags — `lem-style`, `lem-head`,
`lem-script` — stay: they collect fragments under a name; `mark()` places.

**The host declares what its handlers claim.** A claim is a name, a mode, and for a
producer the keys it covers. A *producer* makes a mark's content from its payload and
lists its keys — a highlighter its languages, a diagram plugin `pikchr` — with no
wildcard. A *decorator* transforms whatever content a mark has and may claim a
wildcard. The compiler checks a literal name, and a literal key, against the producers'
claims, with a did-you-mean, and records a digest of the claims a unit read as a
compile-time fact (D25) — D40's mechanism with the grammar text replaced by a list of
names — so `lem check` and the editor keep the warning at the word, and a plugin that
starts claiming a language leaves the documents that named it stale. Two producers
claiming one name and key is a plugin-load error naming both. Per mark, the one
producer runs, then every matching decorator in the site's plugin order, each over the
previous output; producers see the page as the core returned it and never each other's
output.

**The shell resolves before it transforms, and feeds render in a second phase.**
_(Refined by D46: no second phase — each document's body renders alone and is finished
in the shell, then bound in `build.bodies` before every page renders, feeds among them.)_
The splice precedes every output transform. A rendered body reaching a template as a
value is a shell-supplied *string* of a page's finished output — rendered, spliced,
transformed — loaded back as a data source for a second render phase; it is never an
`html` value carrying marks. A mark inside a fragment, a payload, or a default is a
render error at the collecting block or the call.

**Trust and determinism are stated boundaries.** A producer's output is spliced
verbatim, so handlers are trusted as `transformOutput` is; the payload arrives raw,
with an escaped copy beside it. Invariant 1 is the core's and is unchanged; the core's
byte-identical promise ends at the splice for a JS handler, and a content-addressed
cache keyed on plugin version and payload assumes a handler is pure, which the shell
can check for a wasm handler with no imports and only document for JS. A plugin that
wants wasm instantiates it inside its own handler; the shell defines no wasm protocol.

## Alternatives considered

- **Point marks with no content**, as the `∆1Hp` plan had them. Rejected: an unclaimed
  name, a failed handler, or a host with no shell leaves a hole where a block was, and
  `readingTime` would count no code at all, deciding `∆aaw` by accident. `∆RGQ` rejected
  ranges on four grounds; its own retrospective found two wrong, and the one that
  survives — a transform before the splice invalidates them — applies to points too.
- **The core calls plugin wasm at render time** (`∆SDN`'s first half). Rejected for
  output: there is no direct wasm-to-wasm call, so the core would cross into JS and
  back twice per block per render, inside its synchronous call, uncacheable by the
  shell, and the boundary would need a host import. Kept for values: a mark's content
  is opaque to the template, so anything a template computes on — plurals, collation —
  still needs the core to call out, which is what `∆SDN` now names.
- **Post-render plugins finding blocks by class**, as D40's consequences allowed.
  Rejected: a regex over HTML fires inside code that quotes the pattern, collides with
  neighbouring transforms, and must re-derive what the core knew when it wrote the
  block. A mark is that knowledge kept.
- **Later declaration wins on a claim collision**, D40's rule for grammars. Rejected by
  Eric for an error: a site that overrides a shipped highlighter's `rust` does so by
  narrowing that plugin's claim through its options, and the dispatch table stays a
  function of the plugin list alone, which `lem check` can print.
- **Wildcard producers.** Rejected: the one plugin that wants every fence — a fallback
  renderer, a copy button — is a decorator over the default content, and a wildcard
  producer would silence the unknown-language check for every plugin.
- **No claims; diagnostics from handlers alone.** Rejected: the unknown-language
  warning would arrive at build time, once per render of a body, and never in the
  editor, against D16's day-one commitment.
- **An `escape()` builtin carrying flagged marks**, so a feed could escape a body in
  the core and the shell escape handler output the same number of times. Rejected: a
  new builtin, a flag on every mark, a rule for each builtin that touches `html`, and
  it still could not show the feed a body after dprint, which limulus.net's feed
  carries today.
- **Feeds written by a shell plugin over final output**, as `eleventy-plugin-rss` does.
  Not rejected, subsumed: the second phase is that, with the feed still a Lemmon
  template. _(Refined by D46: the body pass is, reading documents' bodies rather than
  pages.)_
- **A new tag for a template mark**, `<lem-mark name>body</lem-mark>`. Rejected: a body
  is a rendered template, wrong for raw text, and a raw body needs its own scanner mode
  and head marker either way (`∆M5x`).
- **Call-with-body sugar** now. Deferred by Eric: a raw body's mode must be in the
  head, since the callee is not resolved when the body is scanned, and the sugar
  belongs to both body kinds together, once the port shows whether capture-then-call
  hurts (`∆M5x`).
- **Payloads inline in the render's JSON.** Rejected: `∆RGQ` measured the page inside
  the JSON reply as more than half a render call; a fence's code would pay the same in
  proportion. By reference, nothing crosses per render.

## Consequences

- `∆1Hp` builds the general mark under the three collect tags, `mark()`, claims by name
  on the declaration, the bytes-beside-metadata boundary with marks and fragments, and
  the handler dispatch in `@limulus/lemmon-core`; its plan is revised. `∆7wK` makes
  fences `code` marks with keys, declares `Code` in the shell's `ambient.lem`, and
  removes syntect, the grammars on the declaration, the grammar digest fact, and the
  corpus's grammar fixtures.
- The default highlighter is a shell plugin (`∆Nxd`): highlight.js in JS reproduces
  limulus.net's markup exactly, which collapses the choice `∆o24` inherited from D40.
  `∆SDN` keeps only its value-producing half. `∆5pk` becomes a `code` producer for `lem`
  built from `lemmon-syntax`. `∆aaw` loses the half D40 caused, since a body's default
  content carries no `syn-` spans. `∆ePP` renders feeds in the second phase, which
  answers D30's open question about a rendered body's form _(refined by D46: `∆zu0`
  renders them after a body pass, from `build.bodies`)_. `∆PgF` gains the `marks`
  registration and ships the shell's `styles`/`scripts`/`head` producers as a default
  plugin a site lists, so a site can produce those names itself without a collision.
- A host that claims a name ships an ordinary `lem-fn` wrapping `mark()` for it, so a
  layout reads `<lem.styles()>` and a component `<lem.diagram(…)>`. The core knows
  nothing of those wrappers — they are project or plugin functions (D23) — and they are
  what keeps `mark()` a plugin author's primitive rather than a site author's word.
- The `Mark` shape, the wire, and the corpus notation change before any is blessed;
  the `∆1Hp` plan's `Place`/`Placed` names, which collide with the loader's `Place`
  position and `Fault::Placed`, go with it.
- The 24-byte `Value` budget is prose in `datetime.rs` with no test behind it; a marked
  value rides behind an `Rc`, as the plan already had it.
- Invariant 7 governs the template and the core; a handler's output is the shell's
  trust, and the invariant's wording may gain that clause when it is next touched.
- A raw-text form in templates — a `: string` capture, and call-with-body sugar over
  it — is deferred (`∆M5x`); until then a multi-line source is a fence in a document.

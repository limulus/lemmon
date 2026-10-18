# D44 — Document and asset hooks

- **Status:** Accepted 2026-10-17 (∆ETy), settled while building it; refines D06, whose
  trust in the shell reaches from what is done to a page after the core returns it to
  what plugins hand the core before it compiles: a document's body, and the manifest;
  revised 2026-10-17 by D45 (∆d1N): the asset hook is given the project's root too, the
  config file's directory, since esbuild's hashes name the sources from where it runs
- **Backlog item:** ∆ETy
- **Normative text:** `docs/architecture.md` § Build pipeline (step 2), § Shell plugin
  API; `packages/shell/README.md` § Plugins

## Context

The architecture's plugin sketch carried `transformDocument?(doc, ctx): Document`, a
pre-markdown text transform, and `assets?(ctx): Promise<AssetManifest>`, a logical path
mapped to `{url, hash, deps}`. Neither was built: the shell refused every plugin field but
`name` and `marks`. ∆c4j was split so that slice 4 builds only the hooks its items call:
limulus.net's link-post appender (∆ITg) adds `[link]: url` definitions and a "Via"
paragraph to a link post's markdown before it compiles, and the esbuild pipeline (∆d1N)
bundles client scripts and hands `compile` their URLs.

What the sketch left open was the contract: what a document is to a transform, whether it
may touch front matter or templates, whether either hook may await, where a failure is
placed when a plugin's code has no span in the site, and how several plugins' manifests
meet. Three constraints were already settled. D06: plugins inject no logic into the
language or the core. D38: the manifest rides on the host declaration at `compile`, its
values finished URLs, so the sketch's `{url, hash, deps}` was obsolete. Invariant 8: a
diagnostic is framed against the text that produced its span.

## Decision

**A document transform rewrites a document's body, and nothing else.** `transformDocument`
is given `{path, frontMatter, body}` — the path from the input directory, the front
matter as the core loads it, frozen, and the text after the front matter — and returns
the body. The shell rebuilds the text with the front matter byte for byte, so the spans
the front matter was read with hold and the core finds the same block — the same YAML,
closed by the same line, which, where its `---` ended the file, now ends with the newline
the body starts with. A returned body that would give the text other front matter is
refused, saying which way. The transforms run in plugin order, each over the body the
last returned. The front matter is frozen: a write to it throws in strict-mode code, which
an ES module such as `lemmon.config.ts` is, and is ignored elsewhere — either way it
changes nothing the build reads.

It sees documents only. Its consumer serves markdown — reference definitions, a
paragraph — which a template does not have. And a page template compiles twice, the first
time without its record so that every layout compiles against what it defines (D41), so a
transform over one would change what other files compile against, facts of their units
(invariant 11); a document compiles only in the second call, and no file sees what it
defines, so what a transform does to one stays in that document.

**This refines D06.** D06 keeps plugin logic out of the language and the core, and D42
put what the shell does with a page after the core returns it in the shell's trust. These
two hooks act before the core compiles, and hand it only what any host could: a
transform, source text that compiles and is checked as the author's own, with no plugin
code reachable from the language; an asset hook, the manifest `asset()` is answered from
(D38). Both are the shell's trust, as a mark handler is.

**What it returns is what compiles, and what diagnostics are framed against.** The
transformed text is the document's text from then on: it is what `compile` is sent and what
`CheckResult.texts` holds, so a frame shows what the core saw. An append keeps every
position the author's file has; a transform that changes anything earlier moves them, and
the docs say so rather than mapping positions back.

**A transform is synchronous.** Its one consumer is a function of the document alone, and
what a transform would await — a file, the network — is a dependency no build or `lem dev`
watch can see. An `async` transform is refused at config load; a promise returned anyway is
an error at the document. Allowing a promise later is additive; taking it away would not
be.

**A failed transform is an error at the document**, at its front matter or its start, and
stops the build before it compiles. A failed mark handler is a warning (D42) because the
mark keeps a correct default; an untransformed document has none — a link post without its
definitions renders its references as literal brackets.

**The asset hook runs once per build, and may await.** `assets({input, output, writes,
env})` is called after the transforms and before the first `compile`: the input and
output directories, since a bundler's hashes can depend on where it writes; whether the
build writes, since `lem check` calls it too and writes nothing; and the build's
environment, the values `env("…")` bindings read, as a frozen copy, so that a hook and the
site see one environment — ∆d1N's esbuild plugin is to read its `define` token there — and
a hook can be run without touching `process.env`. It returns `{manifest?, files?}`:
logical path to finished URL, and url to contents. esbuild's API is asynchronous, so the
hook is awaited, in plugin order.

**Manifests merge; a path is one plugin's.** Every plugin's manifest goes on the one
declaration. A logical path two plugins both map is an error naming both, since either
answer would be a guess.

**A hook's failure is an error at the start of the config file.** A plugin's code has no
span in the site, and the file that lists the plugin is where an author goes to change it
— where `ConfigError` already places a refusal of `plugins`. The diagnostic is placed at
the file's start, as a page without front matter is, and printed without a frame, since no
step reads the config as text. As a diagnostic rather than a thrown error, it is counted
with every other error, and a check reports it beside the warnings discovery found before
it; an error in discovery stops the build before any asset hook runs.

**Asset files are output like any other.** They are claimed after the passthrough files and
before any page, so a collision among them or with a passthrough file is an error at the
config's start, and a page or produced file that collides with one is an error at that
page's url or mark, labelled at the config. They are written with the pages, after every
check passes, and counted apart in the summary.

**Every build calls the hooks again.** `lem dev` reads the config once, so a plugin object
outlives builds; nothing the shell holds of a hook's answer survives the build it ran in.

## Alternatives considered

- **Templates transformed too.** Rejected: a template has no markdown for a transform to
  serve, and a page template's definitions are compiled against by every layout, so its
  transform would reach other files' units, where a document's stays in the document. No
  consumer asked.
- **Front matter a transform may change.** The text would have to be read again after the
  transform, with every span placed in front matter the author never wrote, and a computed
  field is a schema default's job (D37). Rejected; the front matter is frozen, so a write
  to it throws in strict-mode code.
- **Async transforms.** Costless to the pipeline, which awaits already, but nothing needs
  it, and it would make a transform's output depend on something outside the document.
  Deferred, since allowing it later breaks no one.
- **A failed transform as a warning, the document compiled as written.** Rejected: unlike a
  mark's default, the untransformed text is wrong output, silently.
- **Mapping diagnostics back through a transform.** Needs a source map from every
  transform; an append needs none. Not built.
- **A hook's failure thrown, as `SiteError` is.** Stops at the first failure, outside the
  error count, and drops the warnings discovery had already found. Rejected.
- **The last plugin to map a path wins, or the first.** Silent, and depends on list order
  for correctness. Rejected.
- **Manifest values `{url, hash, deps}`.** Superseded by D38 before this was built.
- **Running the asset hooks concurrently.** Faster only with several asset plugins, and
  limulus.net has one. Sequential, so the merge and its errors follow the list.

## Consequences

- Hand-offs: ∆ITg writes the appender as a site plugin in the port's config, against the
  exported `DocumentSource`, and ∆d1N an esbuild plugin that returns its outputs in memory
  as `files`, at their hashed URLs, with a manifest from each unhashed one.
- A diagnostic's path may now be the config file, absolute, which `CheckResult.texts` does
  not hold; the CLI prints it without a frame.
- `lem dev` runs every asset hook on every rebuild, which for esbuild is a full bundle;
  ∆d1N measures what that costs before anything caches.
- The content API (∆Gg3) reads front matter, which no transform can change, so a tool
  reading it and a build agree on every field.
- A transform that does more than append moves positions in diagnostics; the shell README
  and §Build pipeline say so.

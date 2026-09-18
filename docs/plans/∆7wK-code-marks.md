# ∆7wK Code marks — plan

Written by the session that claimed ∆7wK on 2026-09-15, after reading D42, D40, the ∆1Hp
journal, and every line of code the item touches, on `main` at `959b860`. The design
points below either are D42's text or follow from it and from how ∆1Hp built the mark;
they are not open unless Eric reopens them. The three that were open when this was
written are settled at the end, answered by Eric the same day. When ∆7wK's journal entry
is written, its Planning section absorbs this file and the file is deleted.

## Start here

1. Read `CLAUDE.md`, the ∆7wK item in `BACKLOG.md`,
   `docs/decisions/D42-marks-resolve-in-the-host.md`, D40 (what this item removes), and
   the Planning section of `docs/journal/∆1Hp-side-channels-and-marks.md` — it says why
   `default` is a named argument, why `mark()` is an op and not a builtin, and why names
   travel as strings; all three shape what is added here.
2. The item is claimed (`.delto-claims.local.jsonl` is per machine; re-claim on another).
3. The staged `.wasm` on this machine is 4,173,736 bytes (`artifacts/wasm/`, built
   2026-09-15). Record the size before checkpoint 2 and after: D42 measured syntect at
   1.97 MB of it, and the number is reported as what the shell stops shipping.
4. Every line number below was checked against `959b860`; functions are named so they can
   be found again after the tree moves.
5. Open checkpoint 1 with a plain-language preview for Eric, then build it.

## What exists today

- **The core highlights while compiling.** `render_prose`
  (`crates/lemmon-vm/src/document/mod.rs:62-217`) finds every `CodeBlock` in comrak's
  tree, computes the language word and its source span (`language_word`, 227-238),
  skips `math`, and hands the list to `highlight::highlight`
  (`document/highlight/mod.rs`, 369 lines), which looks each word up in the host's
  syntect grammars (`document/highlight/grammars.rs`, 635 lines) and returns a comrak
  adapter filling in the inside of `<code>`. The grammars ride on the declaration
  (`Host.grammars`, `with_grammars`, `keeping_grammars_of`, `grammar_digest`,
  `host.rs:112-118, 195-219, 248-256`), cross the wire as `GrammarInput`
  (`crates/lemmon-wasm/src/wire.rs:150-152, 168-175, 220-228`) and `HostDecl.grammars`
  (`packages/core/src/lib/types.ts`), and the set's digest is a unit fact
  (`Unit.grammar_ref`, `unit.rs:66-71, 229-241`, checked at link 397-421). The corpus
  host declares three fixture grammars (`tests/corpus/document/grammars/`,
  `crates/lemmon-vm/tests/corpus.rs:39-72`, `crates/lemmon-corpus/src/lib.rs:413`,
  `testing.rs:51-67`); seven `document/fences` cases carry `syn-` spans.
- **Prose is strings.** `PreparedFile.chunks: Option<Vec<String>>`
  (`compile/surfaces.rs:41-44, 116-128`), one HTML chunk per gap between lem-blocks;
  `lower_document` (`compile/items.rs:72-94`) emits each with `EmitText`; `Unit.body`
  is their concatenation (`compile/mod.rs:171-175`), what the loader binds as `body`.
- **A mark is a name and a range.** `Mark {name, start, end, file, span}`
  (`rendered.rs:17-33`); `Op::Mark { name }` pops an `html` default (`vm.rs:529-560`);
  `call_mark` (`compile/calls.rs:296-362`) takes one positional, the literal name, and
  the named `default`; `claim_ref` (`compile/schema.rs:101-125`) records one
  `ClaimRef {name, state, span}` per name and warns at `Unclaimed` with a did-you-mean
  over `claimed_names()`; link recomputes the state (`unit.rs:423-455`). The wire's
  claim is `{name}` (`wire.rs:160-166`), a mark crosses as `{name, start, end, path,
  span}` (`MarkOut`, 618-628), and the wrapper's `resolveMarks`
  (`packages/core/src/lib/marks.ts:66-116`) dispatches by name alone, one producer per
  name.
- **The corpus shows marks as `⟦mark styles⟧…⟦/mark⟧`** (`tests/corpus.rs:218-232`),
  and `mark()`'s arity is fixed at one positional (`builtins/mod.rs:217-220`,
  `rules::builtin` at `rules.rs:633-640`).
- **comrak 0.54 facts** (`~/.cargo/registry/src/*/comrak-0.54.0/src/html.rs`): the
  highlighter seam (`codefence_syntax_highlighter`, 557-574) is called for every code
  block but a `math` fence, labelled or not, to write the `<pre>` tag, the `<code>`
  tag, and the body; comrak then writes `</code></pre>` and a newline itself. A `math`
  fence goes to `render_math_code_block` (510-511, 1435-1470) before any seam is
  consulted, unconditionally: the `math_code` option gates only the inline
  dollar-backtick syntax (`parser/inlines.rs:1247-1260`), and no option turns the fence
  case off. `codefence_renderers` (517-527) is looked up by language, so an unlabelled
  block never reaches it.
- **limulus.net** (`www/**/*.md`): 16 fences — 8 unlabelled, 4 `typescript`, 2
  `rust`, 1 `txt`, 1 `bash` — no info-string extras, no indented blocks; highlight.js
  through markdown-it writes `<pre><code class="hljs">…` (`eleventy.config.ts:107-112`).

## Working agreements

- Three checkpoints, each leaving `npm run verify` green after `npm run rust:wasm`;
  checkpoint 3 also runs `npm run rust:wasm:test`, `npm run wasm:check`, and
  `npm run spike:the-rebound`. After each: `/code-review medium --fix` on Opus, then
  stop; Eric reviews and commits. Commit a checkpoint before opening the next one
  (∆1Hp's retrospective).
- Docs change in the same pass as the semantics (the list under each checkpoint), and
  the diff's own doc sentences are written last.
- Corpus cases are written from the author's seat; bless, then read every diff line
  against what the source asked for. Every `document/fences` case changes in
  checkpoint 2 and each must be read.
- Every test that claims to pin a rule is run against a broken version of the rule
  before it is trusted — above all the staleness tests, which passed under perturbation
  once in ∆1Hp.

## Design settled for this item

### Claims carry keys, a payload record, and the field that keys a payload

- A claim is `{name, keys?, payload?, keyedBy?}` on the wire (`ClaimInput`,
  `deny_unknown_fields`). `keys` present means a producer's claim and lists what it
  covers, sorted for a deterministic did-you-mean; a decorator's claim is a bare name.
  `payload` names the record a mark of this name carries; `keyedBy` names the field of
  that record whose value is the mark's key (`"language"` for `code`). Several claims
  may share a name — a producer and a decorator, two producers on disjoint keys — and
  the core merges them per name: keys unioned, `payload` and `keyedBy` required to agree
  or the declaration is refused at the boundary as a caller bug naming both. Overlapping
  producer keys are the shell's plugin-load error (∆PgF); the core unions them. There is
  no `mode` field: the compiler needs nothing a bare claim and a keyed one do not say.
- `Host.marks` becomes `Option<BTreeMap<Rc<str>, Claim>>` with `with_claims(…)`;
  `with_marks(names)` stays as the name-only convenience the existing tests use.
- **What a compile consumes is recorded per name and key.** `ClaimRef` becomes
  `{name, key: Option<Rc<str>>, answer, span}`, one per distinct `(name, key)` a unit
  asked about, where `answer` is `Undeclared`, `Unclaimed`, or `Claimed { key_covered:
  Option<bool>, payload: Option<Rc<str>> }` — exactly what the warning and the payload
  check read. Link recomputes the answer under the current declaration and reports the
  first ref that differs, with ∆1Hp's four messages plus "a producer claims `rust` for
  `code` now, and this file was compiled where none did", its converse, and "the
  handler for `code` takes a `Snippet` payload now, and this file was compiled against
  `Code`". This replaces D42's "one digest per name" (question 3): a digest would stale
  every fenced document when a highlighter adds a language; a per-key answer stales the
  documents naming that language and says which. As in ∆1Hp, a did-you-mean in a
  recorded warning may go stale when the key list changes without changing the answer.
- `claim_ref(name, key, span)` is the one predicate: `call_mark` calls it for a template
  mark, and `compile_unit` calls it once per code block for a document (below), so a
  fence's warning is source-ordered with the file's others and lands as the same fact.
  Warnings: the existing two for a name; for a key, "no producer claims `bsah` for
  `code`; did you mean `bash`?" and "no producer claims `lem` for `code`, so this block
  keeps its default content". An undeclared host stays silent about both. A host that
  declares handlers but no `code` producer therefore warns at every labelled fence —
  that is `docs/language.md` § Code blocks as written ("with no producer for the key
  the block stays as written, and the host warns"), and ∆Nxd is what a new site lists.

### `mark(name, payload?, default: html)`

- `call_mark` accepts one or two positionals (its own arity message; `rules::builtin`
  is unchanged for every other builtin). The payload is lowered before the default, so
  the op pops the default then the record beneath it.
- Checking, given the name's answer: `Claimed` naming a record → `check(payload,
  Type::Named(record))`, the record resolved through the index like an annotation and
  recorded as a type fact; a name nothing declares is an error at the call ("the host's
  claim for `code` names a payload `Code`, which nothing declares"); a claim naming no
  record refuses a payload ("the handler for `styles` takes no payload") and one naming
  a record requires it ("the producer for `code` takes a `Code` payload"). `Unclaimed`
  or `Undeclared` → the payload is lowered with no expectation but must be a record
  (declared or `any`). A payload record whose fields, transitively, include a function
  type is refused at the call: nothing can carry it across the wire.
- **The key of a template mark is the `keyedBy` field of its payload** (question 1).
  When the payload is a record literal whose that field is a string literal, the key is
  checked at compile through `claim_ref` at the field's span; otherwise the check is
  the handler's at build (D42's "a computed one by the handler at build"), and the VM
  reads the field from the popped record — `null` is no key. `Op::Mark` carries the
  field name as a constant for that read.
- `Op::Mark` becomes `Mark { name: u32, key: Option<u32>, payload: MarkPayload }` with
  `enum MarkPayload { None, Popped { keyed_by: Option<u32> }, Reported(u32) }`; the
  three constant operands are rebased at link in the spelled-out match
  (`unit.rs:610-625`). `Mark` gains `key: Option<Rc<str>>` and `payload:
  Option<Payload>`, `enum Payload { Reported(Rc<str>), Value(Value) }`; `Buffer::
  push_marked` copies both; `PartialEq for Mark` — what `==` on `html` asks — adds key
  and payload to name and range, since two marks with different payloads place
  different content. A record carrying marked markup anywhere in it is a render error
  at the call, "a mark cannot stand inside a payload", the third of D42's three.

### Every code block is a `code` mark

- **Mechanism: the highlighter seam, with a sentinel.** `render_prose` keeps its tree
  walk — comrak's own split of the info string (the first run of non-space bytes is the
  language; `is_cmark_space` is already there), the full decoded info string, the
  literal, and the mark's span, the language word when there is one, else the block's
  own source range — and keeps registering one adapter as
  `codefence_syntax_highlighter`, as today. The adapter highlights nothing:
  `write_pre_tag` writes a sentinel `<!--lem:code:N-->` and then comrak's
  `write_opening_tag`, `write_code_tag` the same tag as today, and `write_highlighted`
  escapes the literal with `comrak::html::escape`. comrak then writes `</code></pre>`
  and a newline itself, so a block's range runs from its sentinel to the end of that
  closing tag; the adapter either measures what it wrote or writes a closing sentinel
  after the body, the builder's call, and either way a test pins the tail against a
  plain comrak render. The markup stays comrak's, which is what today's adapter already
  keeps, and the parity test — the same document rendered with no plugins equals the
  marked render with its sentinels stripped (today's
  `writes_a_block_no_grammar_colours_exactly_as_comrak_does`, kept as the gate) —
  holds `math` included.
- **A `math` fence is not a mark.** comrak renders it before consulting any seam, and
  no option turns that off (above), so it stays comrak's math block —
  `<pre><code class="language-math" data-math-style="display">` — with no mark, no key,
  and no claim asked. Eric settled this on 2026-09-15: nothing about `math` needs a
  producer, and the design is not reshaped around comrak's special case. The one thing
  given up is a server-side math producer, which no site in front of us wants; the
  alternative, replacing every code block's node with a raw HTML block the core writes
  itself, was drafted and dropped as a second copy of comrak's markup for that one
  case. This amends D42 ("a `math` fence is one such block") and
  `docs/language.md` § Code blocks ("keyed `math`"): both say a `math` fence is
  comrak's own and not a mark, in the "as built" clause D42's status line reserves.
- The sentinel is unforgeable: `<!--lem:` anywhere in the prose is already refused
  (`document/mod.rs:79-89`), and inside a fence the `<` is escaped. It is found by its
  number, not its order: a fence in a footnote renders where the footnotes do
  (`document/fences/block-order` exists for this).
- **Chunks become pieces.** `Prose.chunks` and `PreparedFile.chunks` become
  `Vec<Vec<Piece>>`, `enum Piece { Text(String), Code { html: String, key:
  Option<String>, id: Rc<str>, span: Span } }`, cut at both sentinel kinds with the
  sentinels removed. `lower_document` emits `Text` as today and `Code` as `Const
  { html }`, `Mark { name: "code", key, payload: Reported(id) }` at the block's span,
  `Emit`; the whitespace rule `chunk(…, after_silent)` applies to a chunk's first text
  piece only. `Unit.body` is the pieces' text concatenated, code markup included — the
  loader's `body` is unchanged in content, so `readingTime` still counts code as prose,
  which is ∆aaw's remaining half, not this item's.
- **The payload is reported once, by content hash.** `CodePayload { id, language:
  Option<String>, info: String, code: String }`, deduplicated by id within a unit and
  kept on `Unit.payloads` (the document's own text, a few kilobytes at most; and the
  place a pull-style `payload(id)` would read if the wire ever wants one). `id` is
  sixteen hex characters of a 64-bit hash over the three fields with a length prefix
  each, so `("a", "bc")` and `("ab", "c")` differ. The hash is a fixed, documented
  algorithm rather than `std`'s `DefaultHasher`, whose docs forbid relying on it across
  Rust releases: a shell's persistent handler cache (∆4tG) keys on the id.
  `siphasher` (SipHash-1-3, keys 0,0) is already in the vm crate's tree; use it if it
  still is after syntect leaves and costs the `.wasm` no more than a few kilobytes,
  measured; else `rustc-hash` v2, also in the tree via rowan. Tests: the id is a pure
  function of the three fields, differs when any one changes, and two blocks with the
  same content in one document report one payload and two marks.
- `compile` reports payloads. `Compiled<Unit>` is unchanged; the site-level `compile`
  and the boundary gather `payloads` from the units their call produced. The wire's
  `compile` returns `{diagnostics, payloads}` (question 2), `payloads` one `{id,
  language, info, code}` per distinct id among the files the call compiled; a resident
  unit's payloads were reported by the call that compiled it, as its warnings were.
  ∆Qr2 adds `facts` beside them. The wrapper's `Core.compile` returns `CompileResult`;
  every consumer reads `.diagnostics` (`core.spec.ts`, `load.spec.ts`,
  `scripts/render-the-rebound.ts:257-259`, `scripts/bench-wasm.ts:214-237`).
- **The `code` mark's shape is the core's contract**, not a claim's: name `code`, key
  the language word, payload `{language, info, code}`, span the word or the block. The
  shell's `ambient.lem` declares the same shape as `Code` so a template can build one:

  ```
  <lem-type Code>
    language: string?
    info: string
    code: string
  </lem-type>
  ```

  `language` is optional because an unlabelled or indented block has none. The
  declaration lives in `tests/corpus/host/environment/ambient.lem` (the host fixtures
  are shaped like the shell; `packages/shell` is still a stub), and `docs/language.md`
  § Ambient types names it beside `Page` and `Build`.

### The boundary and the wrapper

- `MarkOut` gains `key?`, `payload?` (a JSON value, for a template mark), `payloadId?`
  (for a fence). A `Value` becomes JSON through a new `json_from_value` beside
  `value_from_json` (`wire.rs:731`): null, bool, int, float, string — `html`, `asset`,
  and `url` as strings — a datetime as RFC 3339, a duration as it prints, an enum
  variant as its name, list, map, record as an object; a closure cannot reach it (the
  compiler refused the record). Document the mapping in `docs/architecture.md` § WASM
  API.
- `HostInput.grammars`, `GrammarInput`, `keeping_grammars_of` at
  `crates/lemmon-wasm/src/lib.rs:221`, and the wrapper's `HostDecl.grammars` and
  `GrammarFile` go; `Claim` gains `keys?`, `payload?`, `keyedBy?`; `Mark` gains `key?`,
  `payload?`, `payloadId?`; `RenderResult` is unchanged; `CompileResult` and
  `CodePayload` are new exports.
- `resolveMarks` dispatches by key. A `MarkHandler` gains `keys?: readonly string[]`.
  For each name: a mark with a key is answered by the producer whose `keys` include it,
  a mark without a key by the producer that declares none; two producers answering one
  mark is a `TypeError` (the shell's load error precedes it); each producer's `produce`
  is called once, with the marks it answers, in page order; every decorator for the
  name then runs per mark, keyed or not. `Resolved.unclaimed` becomes `{name, key?}[]`,
  one per distinct pair that neither a producer covered nor a decorator touched — a
  fence whose language no plugin highlights, under a host that declares handlers, is
  what a build reports there.

### What leaves

`crates/lemmon-vm/src/document/highlight/` (both files); `Host.grammars` and its four
methods; `GrammarRef`, `Unit.grammar_ref`, `PreparedFile.grammar_ref`,
`Prose.grammar_ref`, the link check at `unit.rs:397-421`, and the five grammar tests at
`unit.rs:2106-2265` (their staleness cases are rewritten for keys);
`testing::GRAMMARS`; `tests/corpus/document/grammars/` with the corpus harness's
`grammars()` and `FIXTURES` (`lemmon-corpus/src/lib.rs:413`, and the test at 708-715
that builds a grammar fixture — remove the fixture mechanism if nothing else uses it,
which is the honest fix for an unreached thing); the ABI test
`highlights_large_long_and_deeply_nested_code_without_trapping`
(`crates/lemmon-wasm/tests/boundary.rs:147-185`, which guarded syntect's stack use);
`core.spec.ts:104-124, 231-256`; `syntect` and `yaml-rust` from the workspace and vm
manifests. `wasm:check`'s import allowlist should be unchanged — confirm.

## Checkpoints

### 1. Claims with keys and `mark()` with a payload (`crates/lemmon-vm`)

Host `Claim`/`with_claims`, `ClaimRef` per name and key, `claim_ref(name, key, span)`,
link's new messages, `call_mark`'s payload and key, `Op::Mark`'s operands, `Mark.key`
and `Mark.payload`, the payload-carries-a-mark render error. No comrak change yet.
The boundary keeps compiling (its `MarkOut` reads named fields; `with_marks` stays).

- Unit tests, each perturbed: the payload checked against the claimed record; the four
  refusals; the literal key checked at the field; a computed key read at render;
  `Undeclared` silent on both; one `ClaimRef` per `(name, key)`; link stales a file on
  a key arriving and on a payload record changing, and leaves a file that marks another
  key alone — in both link orders (∆1Hp's lesson).
- Corpus (`render/`): `channels/code-from-a-template.lem` (declares `Code` itself and
  marks `code` with a literal `language`; the notation shows the key and the inline
  payload); `warnings/key-no-producer-claims.lem`; `errors/payload-not-a-record.lem`,
  `errors/payload-where-none-is-taken.lem`, `errors/payload-missing.lem`,
  `errors/payload-record-undeclared.lem`, `errors/mark-inside-a-payload.lem`. The
  untyped corpus host claims `code` with keys `[bash, css, html, sh]`, payload `Code`,
  keyed by `language`, beside the three bare names.
- Corpus notation (`tests/corpus.rs`, `tests/corpus/README.md`): `⟦mark code ts⟧` —
  name, then key when there is one, then an inline payload as JSON when there is one.
- Docs: `docs/language.md` § Marks (drop the placeholder at 798; the key rule; the
  record the claim names; `Code`), § Ambient types (`ambient.lem` declares `Code`);
  `docs/architecture.md` § WASM API's `marks?` line and § VM's `Mark` sentence;
  `docs/artifact-changes.md` 405-409 (`keyedBy`; "one digest per name" → per name and
  key); D42's row and record status line gain an "as built (∆7wK)" clause.

### 2. Fences as marks, and the highlighter removed (core, wire, wrapper minimally)

The node swap, sentinels, pieces, `CodePayload` and its hash, `Unit.payloads`,
`Compiled`'s payloads, `lower_document`'s code piece, `claim_ref` per block; the
highlight module, grammars, `GrammarRef`, and the corpus fixtures deleted; the wire's
`grammars` field, `keeping_grammars_of`, the wrapper's `grammars`, and the two
highlighting specs removed with them so the boundary still builds; `syntect` and
`yaml-rust` out of `Cargo.toml`. `npm run rust:wasm` before `verify`; measure the
staged `.wasm` before and after.

- Unit tests: byte parity with plain comrak over a document with a labelled fence, an
  unlabelled one, an indented block, a `math` fence, a fence in a footnote, and a fence
  in a list item; a `math` fence gets no mark and asks no claim; the sentinel guard
  still refuses a hand-written `<!--lem:code:0-->`; a block's mark span is the word or
  the block; payload dedup; the id's purity.
- Corpus: every `document/fences` case re-blessed and read — no `syn-` span anywhere,
  every block a mark keyed by its word, `block-order`'s `math` block comrak's own with
  no mark and no warning, `unknown-language` warning "no producer claims `bsah` for
  `code`; did you mean `bash`?" and the plain form for `lem`, `block-order`'s footnote
  fence marked where the footnote renders. New: `document/fences/same-code-twice.md`
  (one payload, two marks). The `.html` trailer gains a `--- payloads` section after
  the fragments: one JSON object per reported payload, in id order, so dedup and
  content are read. `host/pages/with-code.md` shows a fence under the typed host.
- Sweep the placeholders naming ∆7wK: `docs/language.md:773, 869-871`, and its § Code
  blocks line 867 ("A `math` fence is a code block like any other, keyed `math`") with
  `docs/artifact-changes.md` 199-204 and D42's decision text, which say the same;
  `docs/architecture.md:53-54, 85-86, 111-114, 182-183, 234-236, 330`; the crates row
  loses syntect; `CLAUDE.md` line 5 ("comrak for markdown and syntect for code
  highlighting"); `tests/corpus/README.md:40-45` (the grammars paragraph becomes the
  claims the untyped host declares); `docs/artifact-changes.md` 418-422 ("∆7wK
  removes it" → removed, with the measured size); the doc comments on `ClaimState`,
  `ClaimRef`, `ClaimInput`, and the wrapper's `Claim` that name ∆7wK. The
  `crates/lemmon-corpus/tests/placeholders.rs` gate fails the build at completion if
  one is missed.

### 3. The wire's new shapes and dispatch by key (`crates/lemmon-wasm`, `packages/core`)

`ClaimInput` with `keys`/`payload`/`keyedBy` and the merge-or-refuse rule; `MarkOut`'s
three fields and `json_from_value`; `compile` returning `{diagnostics, payloads}`;
`Core.compile` → `CompileResult`; `resolveMarks` by key with `unclaimed` as pairs;
every consumer of `compile`'s result. Then `npm run rust:wasm:test`, `wasm:check`,
`spike:the-rebound` (the spike's documents have no fences; it must stay at zero diffs).

- Boundary tests (host-target and ABI): a fence crosses as a `code` mark with its
  `payloadId`, and `compile` reported that id; a template mark crosses with its key and
  inline payload; a declaration whose two `code` claims name different payloads is
  refused naming both; a claim with keys stales a resident document when a key arrives.
- Wrapper specs: a producer with keys answers only its keys and a keyless producer only
  keyless marks; two producers on one key throw; a decorator runs over a keyed mark no
  producer covered; `unclaimed` reports the pair once; `CompileResult.payloads` is what
  the core reported, deduplicated across files.
- Docs: `docs/architecture.md` § WASM API (`compile`'s return, the JSON mapping,
  `MarkOut`'s fields), `docs/artifact-changes.md` (the render and API entries).

## Answered by Eric, 2026-09-15

- **A template mark's key is the payload field the claim names** (`keyedBy`), checked at
  compile when both the payload and that field are literals, and at render otherwise.
  The core sets a fence's key from the language word without reading the claim.
- **Payloads are pushed:** `compile` returns `{diagnostics, payloads}`, the shape ∆Qr2
  adds `facts` to. `Unit.payloads` keeps the text, so a `payload(id)` pull stays
  available later if a host ever shows the reply growing large.
- **Staleness is recorded per name and key**, not as one digest per name: the existing
  fact gains a key and its answer the two new parts, so a producer arriving for one
  language stales the documents that named it and the message says which.

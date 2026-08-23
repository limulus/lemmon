---
id: ∆Ldn
completed: 2026-08-23 14:01:28 -07:00
---

## Backlog item

> - ∆Ldn Document mode — fence-aware segmenter, in-core comrak rendering of joined prose,
>   `<!--lem:N-->` sentinel splice with lost-sentinel build errors, untyped front matter
>   passthrough for now (docs/language.md §Document mode)

## Planning

The item's four clauses hid one real design question: where the _extent_ of a lem-block
comes from. A block starts on a line the segmenter can recognise, but it ends at a matching
close the segmenter would have to find — and finding it means knowing void shapes from block
ones, how closes match, and what an unclosed block is. Writing that in the segmenter would
have been a second grammar to keep in step with the parser, and the first place document
mode and template mode drifted apart.

So the parser answers it. `template::items`' loop body was split out as `item`, and
`parse_item_prefix` runs it exactly once over the slice a block starts at; the root's text
length is the block's extent. Invariant 4 is what makes the question well posed — a tag's
shape is decidable from its head alone, so nothing past the item can change what the item
is. An unclosed block needs no special case: it is the parser's existing error, shifted into
file coordinates.

That decision cascaded. Because the block comes back as a real `Parse`, its **green** nodes
splice straight into a `Root` between the prose's text runs. Rowan green trees carry no
positions, so the assembled document tree has file-absolute spans for free: the compiler's
~95 span sites needed no change, and `dump`/`tag_dump` work on a document unaltered — which
is what let `document/` carry `.cst` and `.tags` expectations, and so become a tree-sitter
agreement target (invariant 9) years before that grammar exists. Only the blocks' own
diagnostics are offset, in one place.

Two scope calls were taken rather than asked. **syntect highlighting was cut** — the spike's
post has no code fences, and it is a megabyte of wasm with its own feature questions; it is
now `∆BfR`. And **`{#id}` heading attributes stayed in**, because they were nearly free once
comrak was configured. Two readings were picked where the docs were silent, both now written
into `docs/language.md`: text after a block's close on its line is an error rather than a
guess at intent (invariant 10), and a void keyword tag takes its line in a document as it
does in a template, while a block keeps the newline after it so two blocks' output lands on
two lines.

## Refinement

comrak needed more configuring than D08 implies. It parses `{#id}` into `Ast::attrs` behind
its `attributes` feature and its HTML renderer then **never reads it** — there is no
built-in path from a parsed heading attribute to rendered HTML. Its own `header_id_prefix`
is no substitute: it ignores `attrs` and injects an `<a class="anchor">` into every heading,
markup limulus.net does not have. So heading ids go through `HeadingAdapter`, one of the
three plugin seams comrak exposes. That trait sees only a heading's level and text, never
the node, so the ids are computed from the tree up front and handed out in document order —
and because the trait is `Send + Sync`, that iterator needs a `Mutex` in a core that never
threads. Both are the adapter API's narrowness showing through.

A `/code-review high` pass over the working tree found eight issues; seven were fixed, each
reproduced with a probe document first. Two were serious:

- **Escaped tags in prose were unbuildable.** The tag-survival check ran against comrak's
  _decoded_ text, so `&lt;lem.x&gt;` and `\<lem.x>` — both correct ways to write a tag in
  prose — were compile errors with no workaround outside a code span. A post about Lemmon
  could not be built. The check now reads the **source bytes** the node came from, with
  CommonMark backslash semantics.
- **Inline-tag spans pointed at the wrong bytes.** An offset into the decoded literal was
  added to a source offset, so any earlier entity shifted the span, and a multibyte
  character could land it off a UTF-8 boundary — a panic waiting for the first consumer to
  slice by it. Both gone with the same rewrite.

The rest: explicit `{#id}`s bypassed the anchorizer and could collide with a generated slug
(they are now reserved before any slug is made); `Fence::closes` ignored indentation, so the
segmenter closed fences CommonMark leaves open — fixed along with CommonMark tab-stop
counting; the "swallowed" message named four causes, none of which applied to the raw-HTML
wrapper that most often triggers it; two inputs of one name could silently bless over each
other now that `.lem` and `.md` are both inputs; and the three structural corpus gates still
walked `syntax/` only, leaving the hand-assembled document trees — the ones that most need
holding to the shape rules — ungated.

One finding was declined: segmenting re-lexes the file's tail per block, so it is O(blocks ×
size). It is microseconds at post scale, and the fix — scan once, hand the parser a token
slice — needs a fallback for a file whose prose leaves the scanner in tag mode. The reason
is recorded at the call site rather than in a backlog item.

Finally `document.rs` was split into `document/mod.rs` and `document/adapters.rs`.
`options()` deliberately did **not** move: `render.unsafe` is not a markdown preference, it
is what lets a sentinel line survive to be cut on, so it belongs beside the splice it
constrains.

## Retrospective

The two serious review findings share one cause: the tag check was written against whatever
comrak handed back, without asking what that string _was_. A decoded literal is not source,
and any span derived from it is a guess. The fourteen document cases written before the
review did include tags standing in prose — but every one of them expected _rejection_. None
wrote `&lt;lem.x&gt;` or `\<lem.x>` and expected it to survive as text, which is the obvious
thing to write about a template language and the case that would have caught both bugs on
the first run. Writing the cases an author would hit, not the cases the implementation
suggests, would have found them sooner.

Blessing was also trusted too readily once. `npm run corpus:bless` produced chunk boundaries
that swallowed the newline after every block, and the diff was read as fine because each
file _looked_ like plausible HTML; it took the `let-then-access` case rendering two values
onto one line to notice. The README already says a blessing is a proposal, not a fix. The
lesson is narrower: read blessed output against what the _source_ asked for, not against
whether it looks well formed.

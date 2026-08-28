# The Lemmon language

Implementation-grade reference, distilled from the architecture set ("The Lemmon
Language" artifact). Normative for the core; where this file and code disagree, one of
them has a bug. Decision IDs (D01…) refer to `docs/decisions/README.md`.

Two source kinds:

- **Templates** (`.lem`) — components, layouts, pages, and the site model (`site.lem`).
- **Documents** (`.md`) — YAML front matter + a body in *document mode* (§ Document
  mode): markdown that may carry block-level Lemmon tags.

## Scanner

Two modes over one buffer (logos `morph()`), plus one for a single tag's body:

- **Text mode** emits raw text runs. It is interrupted only by `<lem.`, `<lem-`, or
  `</lem-`. Everything else — every HTML element, custom elements, a bare `<lem>` — is
  text. `lem` is not an HTML element, so no real markup is ever misread.
- **Tag mode** lexes the expression language between the tag name and the closing `>`:
  identifiers, literals, operators, punctuation.
- **Fields mode** (D29) is tag mode for the body of a `lem-type`, whose fields are
  declarations, not text. The opener's name alone selects it (invariant 4): after a
  `<lem-type …>` head ends, a line ending is a token that ends a field — except inside
  open brackets, where it is whitespace, so a default can be a multi-line literal (D24)
  — and the first `<` hands back to text mode without a diagnostic, so that
  `</lem-type>` lexes as any close tag does and anything else standing there is the
  parser's to report.

Three delimitability rules (D21):

1. **A head never contains `>`.** Ordering is `lt lte gt gte`; lambdas are `|a| expr`;
   type annotations are `: T`. So a tag ends at the first `>` outside a string literal —
   no expression parsing needed to find the boundary.
2. **`lem.` accesses; `lem-*` keywords define.** Inside a keyword head, `= expr` marks
   the void form: `<lem-let x = e />` binds a value (void); `<lem-let x>…</lem-let>`
   binds its rendered body (block). Same for `lem-fn` (block body vs
   `<lem-fn f(…): T = expr />`) and `lem-type` (fields in a block body vs the alias
   `<lem-type Name = T />`, D30). Every tag's shape is decided by its head alone.
3. **The `lem-` element namespace is reserved** in Lemmon files. Every builtin is a
   valid custom-element name; an unrecognized `lem-*` tag is a compile error with a
   did-you-mean. (Client-side custom elements used in templates must not be named
   `lem-*`.)

| Form | Meaning |
|---|---|
| `<lem-name head> … </lem-name>` | Block builtin. Every block closes bare, `lem-fn` included. |
| `<lem-name head />` | Void builtin. The ` />` is canonical (formatter writes it); parser accepts bare `>`. |
| `<lem.expr>` | Access: print a value or call a function — the only call form (D22). Always void, never slashed. |
| `&lt;lem` | Escape for a literal `<lem…` in a template. Code fences handle it in documents. |

Every token carries a byte span.

Scanner rules the parser and the tree-sitter grammar must agree on:

- The tag name after `<lem-` or `</lem-` is any run of `[A-Za-z0-9_-]`, possibly empty.
  Whether it names a builtin is checked by the parser, which reports unknown names with
  a did-you-mean; the scanner never turns a malformed trigger back into text.
- A `<` inside a head ends the head: the tag is reported as unterminated, and scanning
  resumes in text mode at the `<`. So a forgotten `>` costs one diagnostic, not the rest
  of the file.
- A string literal ends at its closing `"` or, failing that, at the end of its line,
  with an unterminated-string error.
- There is no compile-time comment tag. HTML comments are text and pass through to the
  output — and a trigger inside one still fires.
- `lt lte gt gte in is as true false null` are reserved words: they cannot be used as
  identifiers or as named-argument names. `extends`, `by` and `from` are contextual —
  keywords in a `lem-type` head and a field, ordinary names anywhere else.

Parser recovery rules — what a tag's shape is when its terminator or close tag
contradicts it, and what each such mistake costs. The tree-sitter grammar must agree
with the tag boundaries these imply; the diagnostic counts are the Rust parser's
contract. The name wins in every case; nothing is decided by lookahead or by what
follows:

- A block tag's name decides its shape. `<lem-if x />` is reported (a block tag cannot
  be self-closed) but stays a block: its body and close tag are parsed as usual.
- A head the scanner cut short (at a `<` or the end of input) still opens its block,
  so a block whose `>` was forgotten but whose close tag is present costs exactly the
  scanner's diagnostic. Such a block reaching the end of input adds no "unclosed"
  diagnostic: that is a consequence of the cut, not a second mistake.
- A close tag that matches nothing open is never adopted as the innermost block's
  close. It is reported and kept as an error node inside that block's body, and the
  author's real close tag, when it comes, closes the block — it is never "stray". A
  close tag that matches an *outer* open block implicitly closes every block inside
  it, with one diagnostic at the close tag naming the innermost; with nothing open at
  all, a close tag is stray. A close tag the scanner cut short closes the innermost
  block if its name matches and is otherwise kept as an error node — never reaching an
  outer block — adding no diagnostic of its own.
- A block still open at the end of input is reported as unclosed, at its opener, even
  when a mistaken `/>` or a wrong close tag inside it was already reported: the
  parser cannot tell a forgotten close from either, so both mistakes are named.
- An unknown `lem-*` tag is always void, whatever its did-you-mean neighbour's shape
  and whether it ends in `>` or `/>`. A block shape would need its close tag matched
  by name, which a context-free grammar cannot express. Its close tag is an unknown
  tag too, reported with the same did-you-mean wherever it appears.
- A scanner error token inside a head (an unlexable character, an unterminated
  string) is the scanner's to report: whatever the grammar expected in that token's
  place goes unsaid, while mistakes in the tokens that are present — a `/>` on a
  block tag, a reserved word as a name, a close tag after the head — are still
  reported.
- A `lem-type` body holds fields, one per line: `name: Type`, then `by index` if the
  field is a reference, then `from source` if it is a collection or data binding, then
  `= default` if it has one. Whatever else stands on a
  field's line is one diagnostic; a `>` there is a mistake in the field, not its end.
  Anything the scanner handed back to text mode before the close tag — an element, a
  tag — is reported as out of place and still parsed as items, so its tags nest and
  close as usual.
- `lem-case` and `lem-default` are blocks that only mean something directly inside a
  `lem-match`, cases before the default and at most one default; anywhere else, or
  out of order, they are reported and parsed as blocks all the same. A `lem-match`
  body holds only those and whitespace: text or any other tag inside it is reported,
  and still parsed.

## Tags

| Tag | Head | Semantics |
|---|---|---|
| `lem.` | expression | Print/call. Strings HTML-escaped; `html` verbatim; other types formatted by type (datetime → ISO, asset → hashed URL…). Formatter parenthesizes non-path heads: `<lem.(a ?? b)>`. |
| `lem-if` / `lem-else-if` / `lem-else` | `bool` or optional | No truthiness: condition must be `bool`, or an optional (non-null test — narrows `T?` to `T` in the branch). Separators are void. |
| `lem-for` | `x in list` · `x, i in list` · `k, v in map` | Iteration; `<lem-else />` inside gives the empty case. Bodies cannot mutate → parallelizable later. |
| `lem-let` | `name = expr` / `name: T = expr` → void · bare `name` → block | Immutable binding, scoped to the enclosing block. Block form binds the rendered body as `html` (the capture form; in documents the body is a document). Unused bindings warn. |
| `lem-match` / `lem-case` / `lem-default` | subject; patterns | Patterns: subtype (`Til as t`), literal, enum variant (a bare name), `null`. Cases are blocks, the optional `lem-default` last. Exhaustive over enums/closed unions, else `lem-default` required. |
| `lem-fn` | signature | **Definition only** (calls go through `lem.`). Block body returns `html`, closes `</lem-fn>`. Expression body `(…): T = expr` is void, returns any type. |
| `lem-type` | `Name` / `Name extends Base` → block · `Name = T` → void | Record schema; fields in the body (block), one per line, lexed in fields mode (D29). The void form is a type alias (D30). See § Schemas. |
| `lem-enum` | `Name = a \| b \| c` | Enum (void). Bare variant names in expressions where the enum is expected; strings in front matter. |
| `lem-page` | bindings; optional `for x in xs` | Marks a `.lem` file as a page template; carries front-matter-equivalents as `name = expr` bindings (`layout`, `title`, `permalink`…). With `for`: one page per item; `page.prev`/`page.next` are the generated neighbours. Void. |
| `lem-import` | `name from plugin("p")` · `{ names } from plugin("p")` | Bind a plugin's shipped functions/types — qualified or promoted (D23). Site files never import each other. Void; lives in `site.lem`. |
| `lem-style` / `lem-head` | — | Block side channels: collected per page from every function actually called, deduplicated. |
| `lem-script` | `src=asset(…)` | Void side channel: a deduplicated module script tag. |
| `lem-emit` | `styles` / `scripts` / `head` | Void placeholder in a layout, resolved after the whole page renders. |

## Expressions

| Category | Syntax | Notes |
|---|---|---|
| Literals | `"text"`, `"Link: {linkTitle}"`, `42`, `1.5`, `true`, `null`, `[a, b]`, `{k: v}` | String interpolation `{…}` (`{{` for a literal brace — delimiter is a deferred question); until it is settled, an interpolated expression may not itself contain a string literal. No datetime literal: a fixed date is `datetime("2024-11-24")` (stdlib, validated at compile time when the argument is a literal). |
| Access | `a.b`, `a?.b`, `xs[i]`, `m["key"]` | `?.` propagates null; out-of-range list index is a render error, not null. |
| Calls | `f(x)`, `f(x, nolink: true)` | Positional then named. UFCS: `x.f(y)` resolves to `f(x, y)` when `x`'s type has no method `f` — so `photo.rendition(md, jpeg)` reads naturally. |
| Lambdas | `\|a\| a.date`, `\|a, b\| a.date lt b.date` | Parameter types inferred from the expected function type. |
| Arithmetic | `+ - * / %`, unary `-` | `+` also concatenates strings; `string + int` is a type error. |
| Comparison/logic | `== !=`, `lt lte gt gte`, `&& \|\| !`, `a ?? b`, `c ? a : b` | Equality structural. Ordering on numbers, strings, datetimes. |
| Membership | `x in xs` | Lists; map keys. |
| Type tests | `x is LinkPost` | Narrows in the branch, like `lem-match`. |

Every comma-separated list — list and map literals, call arguments, `lem-fn` parameters,
lambda parameters — accepts one optional trailing comma (D24); an empty slot is an error.
Precedence follows JavaScript for shared operators. **Immutability throughout**: `lem-let`
binds, never assigns; no `while`; recursion is allowed.

## Types

`string` · `int` · `float` · `bool` · `datetime` · `duration` · `url` · `html` ·
`asset` · `[T]` · `{K: V}` · `T?` · records (nominal, single `extends`, subtype usable
for base, narrowing via `is`/`lem-match`) · enums · function types `(A, B): T` ·
the well-known records `Page`, `Site`, `Build`, which a site declares (D30, below).

As written, a type is a name, `[T]`, `{K: V}`, or `(A, B): R`, nested freely, each
optionally followed by `?`. The `?` binds to the type just written — `(A): R?` is a
function returning an optional — and there is no grouping form to make a function type
itself optional. Whether a name resolves, and whether a map's key type is `string`, is
the checker's question, not the parser's.

- `html` is trusted markup: produced by block-bodied functions, markdown rendering, and
  `raw()`. Emitted verbatim.
- `asset` resolves against the esbuild manifest at compile/load; printing yields the
  hashed URL; unknown path = error with span. Created by `asset("…")` or `[asset]`
  fields.
- `url` validates at load; attribute-escaped on emit; `url(...)` builtin percent-encodes
  composed URLs.
- Optionals are the only nullable type; the checker forces `?.`, `??`, or narrowing.

### Schemas

```
<lem-type Article>
  title: string
  subhead: string?
  date: datetime
  author: string
  hero: Photo? by site.photos
  vod: Vod?
  layout: Layout = article
  scripts: [asset] = []
  draft: bool = false
  readingTime: duration = readingTime(body)
</lem-type>
```

- **Fields** are one per line, in the order `name: Type`, then `by <index>`, then
  `= <default>`; a default may span lines inside its brackets (D29).
- **Reference fields** (`by <index>`): the loader resolves the front-matter value
  (id or alias) against the named index; a miss is a load error at the YAML position.
- **Computed defaults** evaluate once at load, in dependency order.
  The default expression sees `body`, `path`, `self`, and `site`.
- Plain string keys that name another record (`author: string` → `site.authors`) are
  checked at load; a miss is a load error at the YAML position.
- **Extra front-matter keys are errors** (opt out per type with `extra: {string: any}`).
- Schemas are the source of truth; JSON Schema is derived output (`lem schema export`),
  never authored (rationale: the same types statically check template expressions —
  subtyping, function types, `html`/`asset` semantics, reference fields).

### Well-known types (D30)

The core provides no complex types. `Site`, `Page` and `Build` are *well-known*: the
names are reserved, a site declares each as an ordinary `lem-type`, and the checker holds
each declaration to a contract — `Page` needs `url: url`, `inputPath: string`,
`body: html`, `prev: Page?`, `next: Page?`; `Build` needs `time: datetime`; `Site` needs
nothing. Everything past the contract is the site's, and a name a site never declares
keeps the checker's unknown type: declaring is how a site buys the checking, not how it
earns permission to render.

```
<lem-type Layout = (Page, html): html />

<lem-type Page>
  url: url
  inputPath: string
  body: html
  prev: Page?
  next: Page?
  data: PageData        <!-- the site's own root record: what a layout may read -->
</lem-type>

<lem-type Site>
  posts: [Article] from "feed/*/index.md"
  photos: PhotoIndex from plugin("photos")
  authors: {string: Author} from "_data/authors.json"
  articles: [Article] = concat(posts, links, tils, journal)
</lem-type>

<lem-type Build>
  time: datetime
  generator: string
  env: BuildEnv         <!-- a missing value is a load error, not `undefined` at render -->
</lem-type>
```

- A **`from` field** binds a collection (a glob), a data file, or a plugin's records. It
  stands between `by` and `= default`, so field order is
  `name: Type [by …] [from …] [= default]`; it replaces `lem-collection`/`lem-data`.
- There is no `layout` type. A string in data resolves **by its expected type** — record
  → its `by` index, enum → a variant, `asset` → the manifest, `datetime` → parse, and a
  function type → a project-namespace function of that type — so `layout: Layout = article`
  in a schema and `layout: article` in front matter need no special case.

_Not yet (∆1vS): the parser knows `lem-collection`/`lem-data` as unsupported builtins, and
has neither the `from` clause nor the alias form._

### Checking

Bidirectional with local inference: declared types on parameters and fields; inferred
for bindings and lambda parameters. Generics on builtins only (`filter`, `map`,
`sortBy`, `groupBy`…). Unions limited to subtype hierarchies and `T?`. Diagnostics carry
a primary span plus secondary spans where useful. Unused bindings/parameters warn.

## Names & namespaces (D23)

One value namespace, resolved lexically: **locals/params** (innermost wins) →
**project** → **ambient**. Types are separate, distinguished by capitalization
(`Article` type / `article` value). Builtin `lem-*` tags occupy no namespace.

- **Project-flat:** every `.lem` file in the site shares one namespace; no imports
  between site files. Duplicate definitions are compile errors citing both spans.
  File-top-level `lem-page`/`lem-let` bindings stay local to their file.
- **Plugins qualified:** `site.lem` binds once —
  `<lem-import photos from plugin("photos") />` → `lem.photos.picture(…)`; named
  promotion `<lem-import { picture, Photo, PhotoIndex } from plugin("photos") />`
  brings favourites (and types) into the flat namespace. Promotion collisions are the
  same duplicate error.
- **Ambient:** stdlib (`slugify`, `raw`, `asset`, `url`, `rfc3339`, `rfc822`,
  `readingTime`, `uniqueId`, …) needs no import; site definitions may shadow it (warn).
  `site`, `page`, `build` cannot be redefined as values, though their *types* are the
  site's to declare (D30).

## Functions & components (D04, D22)

- A component is a function. Block body → returns `html`; expression body → any type.
- **One call form:** `<lem.f(args)>`. `<lem.f>` without parens is the function value
  (printing a function is a type error).
- **Slots are parameters** (`slot: html`, more `html` params for "named slots"), filled
  by capture-then-call:

```
<lem-let chips>
  <li><lem.authorChip(article, nolink: true)></li>
  <li><lem.publishedChip(article)></li>
</lem-let>
<lem.chipList(size: small, slot: chips)>
```

- **Layouts wrap by capturing** — no implicit magic variable:

```
<lem-fn article(page: Page, body: html)>
  <lem-let inner>
    <article>…<lem.body>…</article>
  </lem-let>
  <lem.pageLayout(page, inner)>
</lem-fn>
```

- Purity: no clock/filesystem/env access; `build.time` etc. arrive as data.

## Whitespace (D26, D27, D28)

Lemmon optimizes how a `.lem` file reads, not how its HTML reads (D28). Indentation and
the blank lines between tags are not a product surface — a formatter or minifier
downstream owns that (D12's dprint plugin) — so the rules below remove a definition's own
layout from the output and nothing else. Text around block tags is otherwise preserved as
written.

**Statement lines (D27).** A tag that renders nothing where it stands — `lem-let` or
`lem-fn`, void form or block — takes its line with it when nothing else stands on that
line, so a file laid out one statement per line leaks no blank line per statement.
`<lem.…>` renders in place and keeps the line it is written on; `lem-if`/`lem-for` render
their bodies and keep theirs.

**Body normalization (D26).** A `lem-fn` block body and a `lem-let` capture body are
normalized at compile time:

- one newline-and-indent is trimmed at each edge — one per edge, so a deliberate blank
  line survives as one;
- the lines that remain are dedented by the longest whitespace prefix they share, blank
  lines ignored, so a definition's own indentation never reaches the output and relative
  indentation survives;
- a body that opens on its own tag's line (`<lem-fn f()><svg>` …) has no indentation on
  that first line, so the margin is empty and the body stays verbatim — hugging is how a
  definition emits a margin of its own;
- `lem-if`/`lem-for` bodies take no edge trim and no dedent of their own — a loop body's
  leading newline is what separates iterations — but their text lies inside the
  definition around them, so it dedents with it and its lines set the margin like any
  others;
- a nested capture body is a root of its own: normalized on its own, opaque to the body
  around it.

The rule is lexical, over the template's literal text only. It runs after the statement
rule — a line that rule deletes must not set the margin — and never touches interpolated
values, which arrive at run time. No element awareness (invariant 2): `<pre>`/`<textarea>`
content dedents like everything else.

**Rendering (D28).** The renderer never re-indents at a boundary. A multi-line value keeps
its own columns wherever it is substituted — a slot's second line lands where its
definition put it, not at the call's column — and a flow body keeps its newlines;
re-indenting either would make the output depend on a value's shape at run time. What
none of this licenses is touching whitespace HTML renders — inline formatting contexts,
`pre`/`textarea` — which is output semantics and survives byte for byte. A fidelity gate
therefore compares rendered HTML: never raw bytes, and never bytes with whitespace
collapsed blindly (D15's "normalized"; `scripts/lib/rendered-html.ts`).

## Escaping

- `<lem.…>` of `string`: escapes `& < > " '` — safe in text and quoted attributes.
- `html`: verbatim; sources are block-bodied functions, markdown, `raw()` (the single
  grep target for trusted input).
- `url`: attribute-escaped.
- Unquoted attribute interpolation (`class=<lem.x>`) is a lint warning.

## Document mode (D07, D08)

A `.md` body is split by the core **before** markdown runs; comrak (in-core, D08)
renders prose; the VM renders Lemmon blocks; outputs are spliced.

1. A **lem-block** starts on a line whose first non-whitespace is `<lem.` or `<lem-`,
   outside fenced code. It ends at its matching close (or the void tag's own `>`/`/>`),
   possibly many lines later; fences inside a block body are respected. Nothing but
   whitespace may follow the close on its line — a block ends its line, in the source
   and in the output, and text after it is an error rather than a guess at prose.
2. A block's body is itself a **document** — markdown prose plus nested blocks — so
   `<lem-let note>Some *emphasised* text</lem-let>` captures rendered markdown.
   _Not yet (∆Hbw): today a body is a template, as in a `.lem` file, and the fence
   rule in item 1 has nothing to apply to inside one._
3. Prose runs are joined with `<!--lem:N-->` sentinel lines and rendered as **one**
   markdown document (reference links and footnotes resolve document-wide). A sentinel
   line is a CommonMark type-2 HTML block, so splice points are always siblings of
   paragraphs — the `<p>`-wrapping failure of WebC/Hugo cannot occur.
4. The splice replaces each sentinel with its block's output. A missing/escaped sentinel
   (block swallowed by a blockquote or indented code) is a **build error** at the
   block's span; so is any `lem` tag surviving into rendered output (mid-paragraph use —
   unsupported in v1).
5. Prose is static → markdown runs at **compile** time; results are cached in the
   module's constants. Only lem-blocks execute at render.
6. Front matter is a typed record (the collection's schema); the body has `page` and
   `site` in scope. String literals where a record is expected resolve through reference
   indexes at compile time: `<lem.photo("middle-falls-letchworth-state-park",
   placement: right)>` — a typo'd id is a build error. _Not yet (∆Hbw, ∆C8d): the core
   skips a leading `---` block without reading it — the shell's YAML parser owns it —
   and a string literal is a string._
7. A void keyword tag (`<lem-let x = … />`) takes its line with it, as in a template
   (§Tags); an access tag or a block keeps the line ending after it, so that two
   blocks' output stand on two lines.

comrak configuration: CommonMark + footnotes, attribute-style heading ids (`<h2 id="…">`,
slugged from the text and made unique, no anchor link injected), `header_attributes`
(`{#id}`), syntect highlighting (class-based, generated theme CSS — _not yet_, its own
item), `render.unsafe` (raw HTML passes through). A shell-side `markdown` override hook is
reserved but not built.

## Canonical example

```
<lem-fn articleCard(article: Article)>
  <article class="article-card">
    <a href="<lem.article.url>">
      <lem-let preview = article.image ?? article.hero />
      <lem-if preview>
        <lem.picture(preview, [md], class: "preview")>
      <lem-else-if article.vod />
        <img src="<lem.article.vod.poster>" alt="<lem.article.vod.alt>" class="preview">
      </lem-if>
      <div class="text-content">
        <header>
          <h1><lem.article.title></h1>
          <lem-let chips>
            <li><lem.authorChip(article, nolink: true)></li>
            <li><lem.publishedChip(article)></li>
            <lem-match article>
              <lem-case LinkPost as l><li><lem.linkChip(l, nolink: true)></li></lem-case>
              <lem-case Til as t><li><lem.tilChip(t.topic, nolink: true)></li></lem-case>
              <lem-default><li><lem.timeChip(article)></li></lem-default>
            </lem-match>
          </lem-let>
          <lem.chipList(size: small, slot: chips)>
        </header>
        <p><lem.(article.subhead ?? article.teaser ?? "")></p>
      </div>
    </a>
  </article>
  <lem-style>
    .article-card {
      > a { display: block; overflow: hidden; }
      img.preview { display: block; width: 100%; height: auto; }
    }
  </lem-style>
</lem-fn>
```

## Deferred questions

Deliberately undecided (see `docs/decisions/README.md`); when one blocks you, use the
provisional reading in this file and flag it in the PR:

- string interpolation delimiter (`{expr}` vs `${expr}`)
- `int`/`float` vs a single `number`
- a pipe operator (`|>`) — possibly redundant given UFCS
- enum/closed-union syntax details and `lem-match` exhaustiveness rules
- `==`/`&&` vs `eq`/`and` spelling (consistency with `lt`/`gt`)
- inline lem tags in prose (reserved for v2; design must remain compatible)

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
   binds its rendered body (block); `<lem-let x: T from … />` binds a source the host
   fulfils (void, D30). Same for `lem-fn`: block body vs `<lem-fn f(…): T = expr />`.
   Every tag's shape is decided by its head alone.
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
  `extends` in a `lem-type` head, `by` after a field's type, `from` after the name in a
  `lem-let` head or in a `lem-import` head — and ordinary names anywhere else.

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
  field is a reference, then `= default` if it has one. Whatever else stands on a
  field's line is one diagnostic; a `>` there is a mistake in the field, not its end.
  Anything the scanner handed back to text mode before the close tag — an element, a
  tag — is reported as out of place and still parsed as items, so its tags nest and
  close as usual.
- A `lem-let` head with an `=` is the `=` form, whatever else it holds; one with `from`
  past its first token is the source form. The first token is the name, so a capture
  named `from` is still a block. A source form without `: Type` is reported at the
  `from`: the type is what tells the host what to load, and nothing could infer it from
  a glob. The source is one expression; which forms mean anything — a string naming a
  glob or data file, `plugin("…")`, `env("…")` — is the checker's question, as whether a
  type name resolves is.
- `lem-case` and `lem-default` are blocks that only mean something directly inside a
  `lem-match`, cases before the default and at most one default; anywhere else, or
  out of order, they are reported and parsed as blocks all the same. A `lem-match`
  body holds only those and whitespace: text or any other tag inside it is reported,
  and still parsed.

## Tags

| Tag | Head | Semantics |
|---|---|---|
| `lem.` | expression | Print/call. Strings HTML-escaped; `html` verbatim; other types formatted by type (datetime → ISO, asset → hashed URL…). Formatter parenthesizes non-path heads: `<lem.(a ?? b)>`. |
| `lem-if` / `lem-else-if` / `lem-else` | `bool` or optional | No truthiness: condition must be `bool`, or an optional (non-null test — narrows a `T?` path such as `article.hero` to `T` in the branch, as `x is T` narrows `x` to `T`; § Checking, Narrowing). Separators are void. |
| `lem-for` | `x in list` · `x, i in list` · `k, v in map` | Iteration; `<lem-else />` inside gives the empty case. Bodies cannot mutate → parallelizable later. |
| `lem-let` | `name = expr` / `name: T = expr` / `name: T from source` → void · bare `name` → block | Immutable binding, scoped to the enclosing block. Block form binds the rendered body as `html` (the capture form; in documents the body is a document). The `from` form binds what the host loads from a glob, a data file, `plugin("…")`, or `env("…")`; it is allowed only at the top level of `site.lem` (D30). Unused bindings warn — except at the top level of the file an ambient value reads, where every binding is a field of that value's record and declares its type (§ Ambient types). |
| `lem-match` / `lem-case` / `lem-default` | subject; patterns | Patterns: a record type (`Til`, or `Til as t` to bind it), a literal, an enum variant (dotted, `.hero`), `null`. Cases are blocks, tried in order, the optional `lem-default` last. A type case narrows the subject in its body and binds `t` to it. Exhaustive over enums (every variant, and `null` for a `T?`), else `lem-default` required — a record is matched by its subtypes, and that set is open (D35; the exact rules are a deferred question). |
| `lem-fn` | signature | **Definition only** (calls go through `lem.`). Block body returns `html`, closes `</lem-fn>`; a `: html` on it is allowed and nothing else is. Expression body `(…): T = expr` is void and returns the declared `T` — the annotation is required, since a signature is read from the head alone (invariant 11). |
| `lem-type` | `Name` / `Name extends Base` | Record schema (block); fields in the body, one per line, lexed in fields mode (D29). See § Schemas. |
| `lem-enum` | `Name = a \| b \| c` | Enum (void); its variants are written bare here, and nowhere else. In an expression a variant is `.hero` where the enum is expected and `Placement.hero` anywhere; a string in front matter. |
| `lem-page` | bindings; optional `for x in xs` | Marks a `.lem` file as a page template; its `name = expr` bindings are the page's own fields, as front matter is a document's (`layout`, `title`, `url`…), on a record that extends the host's `Page` (D30). With `for`: one page per item; `page.prev`/`page.next` are the generated neighbours. Void. |
| `lem-import` | `name from plugin("p")` · `{ names } from plugin("p")` | Bind a plugin's shipped functions/types — qualified or promoted (D23). Site files never import each other. Void; lives in `site.lem`. |
| `lem-style` / `lem-head` | — | Block side channels: collected per page from every function actually called, deduplicated. |
| `lem-script` | `src=asset(…)` | Void side channel: a deduplicated module script tag. |
| `lem-emit` | `styles` / `scripts` / `head` | Void placeholder in a layout, resolved after the whole page renders. |

## Expressions

| Category | Syntax | Notes |
|---|---|---|
| Literals | `"text"`, `"Link: {linkTitle}"`, `42`, `1.5`, `true`, `null`, `[a, b]`, `{k: v}` | A `{k: v}` written where a record is expected is a value of that record (§ Checking, Literals). String interpolation `{…}` (`{{` for a literal brace — delimiter is a deferred question); until it is settled, an interpolated expression may not itself contain a string literal. No datetime literal: a fixed date is `datetime("2024-11-24")` (stdlib, validated at compile time when the argument is a literal). |
| Access | `a.b`, `a?.b`, `xs[i]`, `m["key"]` | `.` asks the type on its left and nothing else: a record's field, an enum's variant when the name on its left is one (below), or — called — a builtin type's method. A map is indexed. `?.` propagates null and needs an optional on its left; `a?.b.c` needs a second `?.`, since `a?.b` is optional. An out-of-range list index or a missing map key is a render error, not null. |
| Variants | `.hero`, `Placement.hero` | `.hero` is a variant of whichever enum is expected where it stands; where none is, qualify it. A bare `hero` is only ever a name. In a `?:` leave the space — `?.` is one token, so `c ?.hero : .left` reads as a member access. |
| Calls | `f(x)`, `f(x, nolink: true)`, `xs.first()` | Positional then named. A bare name is a `lem-fn` or an ambient function. `x.f(…)` reads the receiver's type (D34): a builtin type's method — `heading.slugify()`, `words.first()`, `m.keys()` — or a record's field `f` called as the value it holds. Nothing consults the enclosing scope, so a project `lem-fn` is never reached through a dot: it is `rendition(photo, md, jpeg)`, not `photo.rendition(md, jpeg)`. On `any` the VM resolves the name at render time. |
| Lambdas | `\|a\| a.date`, `\|a, b\| a.date lt b.date` | Parameter types inferred from the expected function type, or from the builtin whose argument the lambda is: `xs.map(\|a\| …)` types `a` from the element type of `xs` and infers the return from the body (D34); with none expected they are `any` (provisional). |
| Arithmetic | `+ - * / %`, unary `-` | `+` also concatenates strings; `string + int` is a type error. |
| Comparison/logic | `== !=`, `lt lte gt gte`, `&& \|\| !`, `a ?? b`, `c ? a : b` | Equality structural. Ordering on numbers, strings, datetimes. |
| Membership | `x in xs` | Lists; map keys. |
| Type tests | `x is LinkPost` | A `bool`; `LinkPost` is a declared record. As a `lem-if` or `?:` condition it narrows `x` — a local or a field path — to `LinkPost` in the branch, like a `lem-match` case (§ Checking, Narrowing). A test that always holds or never can is an error. At run time it reads the tag a record value carries (D35). |

Every comma-separated list — list and map literals, call arguments, `lem-fn` parameters,
lambda parameters — accepts one optional trailing comma (D24); an empty slot is an error.
Precedence follows JavaScript for shared operators. **Immutability throughout**: `lem-let`
binds, never assigns; no `while`; recursion is allowed.

## Types

`any` · `string` · `int` · `float` · `bool` · `datetime` · `duration` · `url` · `html` ·
`asset` · `[T]` · `{K: V}` · `T?` · records (nominal, single `extends`, subtype usable
for base, narrowing via `is`/`lem-match`) · enums · function types `(A, B): T` ·
the host's ambient records — the standard shell declares `Page` and `Build` — and the
type of `site`, derived from `site.lem` (D30, below).

Records and enums are nominal, by name (D32): `lem-type`/`lem-enum` declare them
project-flat in the type namespace (§Names), and a declared name is what its declaration
says wherever it is written. A record's fields are its own and its ancestors'; `.` reads
one, typed by the declaration, and a field a record does not declare is a compile error.
A subtype may redeclare a field only with the identical type — that changes its default,
nothing else — so a subtype is always usable for its base. An enum's value is one of its
variants, and a variant always carries a dot: `.hero` is a variant of whichever enum is
expected where it stands (an argument, an annotated binding, a declared return, and the
right operand of `==`/`!=`/`in` when the left is an enum), and `Placement.hero` is one
wherever nothing says which enum — including where nothing is expected at all. A bare
`hero` is only ever a name. An enum prints as its variant's name, compares by it, and at
run time is that name as a string; a record has no printed form (provisional readings,
both: the enum question is deferred).

`any` is the spelled unknown: assignable to and from everything, and every operation on
it is `any` again, checked at render time by the VM with a span. It is what an ambient
value the host named without typing has, and what an author writes to opt a value out
of checking. Nothing else is implicit: an `int` is not a `float` (provisional; the number
model is deferred), and a `T` is not a `T?` until the checker sees that it is.

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
- `{K: V}` is always keyed by `string`; any other `K` is an error at the annotation.
  `map`, `list`, and `function` are not type names, and an author who writes one is told
  the Lemmon spelling. Neither they nor the builtin type names can be declared.

### Schemas

```
<lem-type Article extends Page>
  title: string
  subhead: string?
  date: datetime
  author: string
  hero: Photo? by site.photos
  vod: Vod?
  layout: (Article, html): html = article
  scripts: [asset] = []
  readingTime: duration = readingTime(body)
</lem-type>
```

- **Fields** are one per line, in the order `name: Type`, then `by <index>`, then
  `= <default>`; a default may span lines inside its brackets (D29).
- **Reference fields** (`by <index>`): the field's type is a record `R` (or `R?`), and
  the index is an expression of type `{string: R}` — a map on `site`, as a rule —
  checked where the field is declared. The loader resolves the front-matter string
  through it; a miss is a load error at the YAML position.
- **Computed defaults** are checked against their field where they are declared and run
  once at load, when the loaded data omits the field, in dependency order (D37). A
  default sees `self`, the record being built, as the declaring type — a subtype that
  redeclares a field to change its default sees the subtype. On a record that extends
  the host's document type it also sees `path`, the document's path, and `body`, its
  rendered prose as `html`; on any other record those names are undefined. It sees the
  ambient values as any expression does, except the host's document value, which is
  bound to the record only once it is built — read `self`. What a default reads through
  `self` and off an ambient value is recorded with it: a field whose default reads
  itself through others is refused at link, and the loader has an order before it runs
  anything. Only reads written in the default itself count — a field a called function
  reads, or one reached through `self` handed to a function whole, is not part of the
  order, and a default that reaches a field the loader has not computed yet is a load
  error at the read, never a guess. A `layout` default names a function:
  `layout: (Article, html): html = article`.
- **Extra front-matter keys are errors.** A record has exactly its declared fields.
- **The loader reads data by declared type.** A field's value is checked against the
  field's type as a literal's would be, and an omitted field is filled with `null`
  exactly when its type admits one — a `T?`, or `any` — as a literal fills it; a string
  resolves by the type expected of it (see **Ambient types**). Two readings are the
  loader's alone: a whole number in data is a `float` where one is declared, since a
  shell serialising JSON cannot tell `2` from `2.0` — the checker's rule that an `int` is
  not a `float` stands — and a string is never `html`, wherever it came from: `raw()` is
  the only string that becomes one (invariant 7), so an `html` field takes its value
  from a default or not at all.
- **`extends`** names a record — never an enum, a builtin type, or the type itself
  through any chain — and a field an ancestor declares may be redeclared only with the
  same type, to give it a default (D32); a redeclaration that gives none leaves the
  ancestor's default in force. So `layout: (Article, html): html` is declared once, on
  `Article`, and a `LinkPost extends Article` inherits it: a layout that only a link
  post needs takes an `Article` and narrows inside.
- _Not yet (∆Q6L): `datetime`, `duration`, `url`, and `asset` have no run-time values;
  a loaded field of one of those types keeps its string until the stdlib gives them
  one._
- Schemas are the source of truth; JSON Schema is derived output (`lem schema export`),
  never authored (rationale: the same types statically check template expressions —
  subtyping, function types, `html`/`asset` semantics, reference fields).

### Ambient types (D30)

The core knows no type or value by name. A **host** — the shell, the LSP, a playground —
declares its ambient environment when it compiles a program: the names of the ambient
values and their types, written in Lemmon and passed like any other file, so every field
has a span and the checker that validates the site's records validates what the host
sends. A host that names a value without typing it gives that value the checker's
unknown type — which is how the Phase 0 spike, whose host names `page`, `site` and
`build` and types none, renders unannotated.

The standard shell (`@limulus/lemmon-shell`) ships this declaration and binds `page`,
`build`, and `site`:

```
<lem-type Page>
  url: url
  inputPath: string
  prev: Page?
  next: Page?
  draft: bool = false
</lem-type>

<lem-type Build>
  time: datetime
  generator: string
</lem-type>
```

`prev`/`next` are a page's neighbours in the collection whose glob matched it; `draft` is
D19's flag — a draft is skipped and left out of collections unless `--drafts`.

- **`site`** is the record of `site.lem`'s top-level bindings. A `from` binding names a
  source the shell fulfils, and the binding's type decides what is loaded: a list of a
  type that extends `Page` loads the documents its glob matches; anything else parses
  the file as data. A call names a host source instead of a file — `plugin("…")` is a
  plugin's data hook, `env("…")` an environment value the shell passes as data, the
  only way one reaches a program (invariant 1) — and the shell resolves the string the
  environment holds by the binding's type, as it resolves any string in data. A `from`
  binding stands only at the top level of this file, and its source is a string
  literal, `plugin("…")`, or `env("…")` with one string-literal argument; any other
  form is an error at the source. `= expr` bindings are computed at load, in dependency
  order, and declare their type as a `from` binding does — see **Declared, not
  inferred** below. At this file's top level a sibling binding is read by its bare name
  — `concat(posts, links, tils)` below — and that is the same read as `site.posts`,
  recorded for the loader's order; a `lem-fn` or a default in the file reads
  `site.posts` as every other file does. A binding here may not read the host's
  document value: it is computed once, before any page is bound. The host fulfils a
  `from` binding by its name alone, so no two files it reads bindings from may load one
  of the same name. The host's own values — `build` — load first, in the host's
  declaration order, and a bindings file's after them; so a binding may read `build`,
  and a default on the host's own record may read the host's other values and nothing a
  bindings file loads. The host's document value is the one value the loader leaves as
  sent: the envelope carries it for the renders no loaded document stands in for — a
  document no collection matched, whose `page` is unknown-typed, and a template, whose
  `page` is the declared type — so it is bound as sent, not validated.
- _Not yet (∆RGQ): the envelope's document value is one value for every render no
  loaded document stands in for, so a template's `page` is checked by the compiler and
  not by the loader; `plan()` gives each page its own, validated as the declared type._

```
<lem-let posts:    [Article]        from "feed/*/index.md" />
<lem-let photos:   PhotoIndex       from plugin("photos") />
<lem-let authors:  {string: Author} from "_data/authors.json" />
<lem-let siteUrl:  url              from env("SITE_URL") />
<lem-let articles: [Article] = concat(posts, links, tils) />
```

- **A content type is page-backed by `extends Page`**, and its front matter is its own
  fields, flat: `page.title` beside `page.url`. A `from` glob's element type must extend
  `Page`. A document's type is the element type of the collection whose glob matched it;
  a front-matter `type:` key may name a subtype instead, and the checker reports at that
  key if the named type does not extend the element type. The host names that type per
  file, and the file's unit depends on it as a compile-time fact (D25); the core checks
  what it can see — that the name is a declared record, and that it extends the type the
  document value has — and reports at the front matter, which is where the key that may
  have named it stands. A document no glob matches has no schema: `page` is
  unknown-typed, and it renders.
- **`layout`** is a field every page schema declares, `(T, html): html` for its own `T`,
  usually with a default. At load its string value resolves to a project function of that
  type, as any string does where a function type is expected; the shell renders a page by
  calling that function with the page and its rendered body. That is why `body` is the
  layout's parameter and not a `Page` field: nothing has produced it before render. There
  is no `Layout` type and no `layout` builtin. The checker asks for it where a record's
  base *is* the document value's type: a field may be redeclared only with the same type
  (D32), so the entry is declared once, where its self type is, and everything below
  inherits it. Its own type may stand in for `T` — `(Page, html): html` on a schema that
  extends `Page` is a layout the whole family can share.
- **`url`** is the one `Page` field a source may provide — `<lem-page url = "/feed.xml" />`,
  or `url:` in front matter; the shell derives it otherwise. There is no `permalink`.
- A string in data resolves **by its expected type** — record → its `by` index, enum → a
  variant, `asset` → the manifest, `datetime` → parse, function type → a project function
  of that type — so `layout: article` in front matter needs no special case.

The shell reads exactly two names of its own: `type`, a raw key it must read before a
document has a type, and `layout`, a field it requires on every page schema. Everything
else it knows about a page is a field declared with a span, in `ambient.lem` or
`site.lem`.

**Declared, not inferred.** Every surface one file offers another is read from heads
alone, never from a body: a `lem-fn`'s parameters and return, a `lem-type`'s fields, and
the top-level bindings of the file an ambient value reads. That is the rule behind the
annotation an expression-bodied `lem-fn` needs, and the one a top-level `lem-let` in
`site.lem` needs with it. It is what lets a file's unit be a function of its own text
plus the facts it consumed (invariant 11, D25): a type error in one file's body cannot
change how another file is checked, and nothing has to be compiled in dependency order
for a name to have a type. Everywhere else a `lem-let` is a local and infers as usual.

### Checking

Checking is not a pass of its own: the compiler's one walk types each expression as it
lowers it (D31), so every rule below is applied where the code for the construct is
emitted, and a type error is a compile error that leaves the file without a unit. A
construct a rule refuses then takes whatever type keeps the rest of the file checkable —
the type that was expected where one was, `any` where there is nothing better — so one
mistake costs one diagnostic and never a train of follow-ons.

Bidirectional with local inference: declared types on parameters, fields, and annotated
`lem-let`s; inferred for unannotated bindings and lambda parameters. An expectation flows
into literals, lambdas, and `?:`; everything else is inferred and then compared, with one
diagnostic at the expression that does not fit (`expected `int`, got `string``). Generics
on builtins only (`filter`, `map`, `sortBy`, `groupBy`…). Unions limited to subtype
hierarchies and `T?`. Diagnostics carry a primary span plus secondary spans where useful.

- **Assignability.** `any` both ways; `null` and `T` to `T?`; `T?` to `U?` when `T` to
  `U`; lists and maps covariant (values are immutable); functions contravariant in their
  parameters and covariant in their return; a record to any record it extends (D32);
  otherwise the same type.
- **Literals.** `[…]`/`{…}` under an expected list or map check each element against
  it; under an expected `any` the literal is opted out and its elements are `any` too.
  Without either, the first element sets the type, a `null` makes it optional, and an
  element that fits neither is an error — a list's elements share one type, never `any`.
  An empty `[]`/`{}` with nothing expected is `[any]`/`{string: any}`. A `{…}` where a
  record is expected (a `T?` included) is a value of that record — the expected type,
  never a subtype: every key must name a field the record or an ancestor declares, each
  value is checked against its field, an omitted optional field is null, and an omitted
  non-optional field is an error, whether or not the field has a default — defaults are
  the loader's, run once the record and the site are in hand (D35, D37). A map that is not a
  literal never stands for a record.
- **Conditions.** `lem-if`, `lem-else-if`, and `?:` test a `bool` or an optional — no
  truthiness. `&&`, `||`, and `!` take `bool` only. What a condition narrows is below.
- **Narrowing.** A condition that is an optional *path* — a local, or fields read off
  one with plain `.`s (`article.hero`, `a.hero.credit`), parenthesised or not — narrows
  that path to its inner type in the arm it guards; one that is `path is T` narrows it
  to `T`. Values are immutable, so a path narrowed at the top of an arm holds throughout
  it, a lambda written inside included. The narrowing is of that path alone: `a.hero`
  narrowed says nothing of `a.hero.caption`, which needs its own test. It reaches
  neither `lem-else` nor anything built with an operator — `x is T && c`, `!(x is T)`
  — nor a `?.` step, which is optional by construction; an inner binding of the same
  name ends it. Inside such an arm the path is still accepted as a condition and on the
  left of `??`, since it was declared optional. A path may be rooted at an ambient value
  as well as at a binding — no binding may take an ambient name (D23), so the name alone
  tells one root from another, and nothing can shadow one.
- **Type tests.** `x is T` is a `bool`. `T` must be a declared record (an enum is
  compared with `==`; a builtin type is no record), and `x` a record — or an optional
  one, or `any`. The test must be able to go either way: `T` is a subtype of `x`'s type,
  or, when `x` is optional, its type itself (a null test). `x`'s own type on a
  non-optional, an ancestor of it, or an unrelated record is an error. At run time a
  record value carries its declared name and answers by it, `null` is no record, and
  any other value — a map that reached the test through `any` — is a render error:
  nothing but a record carries a type to test (D35).
- **Matching.** `lem-match` evaluates its subject once and tries the cases in order. A
  literal case compares as `==` does; `null` needs a subject that may be null; `.hero`
  needs an enum subject (a `T?` included) and names one of its variants; a type case
  `T` or `T as t` is the type test above, and its body sees the subject — when it is a
  path — narrowed to `T`, and `t` bound to it (`t` warns if unread; drop the `as` to
  not bind). A bare `<lem-case hero>` is a type pattern, so it reports `hero` as the
  unknown type it is; an enum's name there is told to match its variants, where the same
  name under `is` is told to compare with `==` — the one thing the two forms differ on.
  A pattern spelled twice is an error at the repeat, judged by the value it holds rather
  than by how it is written, so two spellings of one string are one pattern. With no
  `lem-default` the cases must cover the subject, and only an enum can be covered —
  every variant, and `null` when it is optional; a record needs a default, since a
  subtype may be declared in any file, and so does anything else (the provisional
  reading; the exact rules are deferred).
- **Printing.** `<lem.…>` prints `string`, `html`, `int`, `float`, `datetime`, `url`,
  `asset`, an enum, and `any`. A `bool`, `null`, a list, a map, a record, a function, or
  an optional is a compile error: narrow the optional, or give it a default with `??`.
- **Fields.** `.` on a record reads a field its declaration or an ancestor's declares,
  with that field's type; on `any`, `any`. A method is not a field: reading one without
  calling it is an error naming the call form, since a method is called and never read.
  A map has no fields (index it), and neither has an enum. `?.` reads through an optional
  record and yields an optional.
- **Variants.** `.hero` is a variant of the enum expected where it stands, and of
  nothing else; where no enum is expected it is an error naming the qualified spelling
  that would work instead. Beside an enum the right operand of `==`/`!=` is lowered
  expecting the enum, and of `in` a list of it, so a dotted variant may stand there; the
  operator then judges both types as it always does. `Placement.hero` needs no
  expectation: `Name.x` is read in the type namespace first, so a qualified variant means
  the same thing wherever it stands, whatever a local of that spelling holds — that local
  is still read as a value by its bare name, and warns as unused if nothing does. A bare
  name is never a variant, and a declared type used as a value says so rather than
  reporting an undefined name.
- **Where an expectation reaches.** The dotted form needs one, so it is refused where
  none arrives: printing (`<lem.…>`), a `lem-if` or `lem-match` subject, the left of
  `==`/`in`/`??`, an unannotated `lem-let`, an argument of an `any` or of a builtin
  whose rule does not shape it (none shapes one to an enum today), and
  every element of a list or map literal that is not itself expected — the first
  included, since what the first element infers is not an expectation for the rest.
  A comparison against an `any` — an ambient value its host named without typing — is
  the common one. The qualified form works in all of them.
- **Operators.** `+ - * / %` on numbers (`int∘int` is `int`, any `float` makes `float`),
  `+` also on two strings; `lt lte gt gte` on numbers, strings, datetimes; `==`/`!=` on
  two numbers or two assignable types, never functions; `??` and `?.` need an optional
  on their left — `null` counts as one with nothing inside, so `null ?? x` is `x` — and
  `T? ?? x` has the type the two sides share; `in` checks its left against the list's element type, or wants a
  `string` for a map.
- **Calls.** Arguments are checked against parameters, named ones after reordering; a
  function value's type checks positionally; a builtin has a typing rule of its own
  (`raw` takes `string` or `html`), and the rule may shape each argument from the ones
  before it (D34): `xs.filter(|a| …)` expects `(T): bool` of its argument and yields `[T]`;
  `xs.map(|a| …)` expects a function of `(T)` and no return in particular — a lambda
  there takes `T` as its parameter and its body is inferred, a function value fits when
  its parameters accept `T` — and yields a list of what that function returns. Calling
  `any` checks nothing. A lambda under an expected function type takes its parameter
  types from it and its body is checked against its return; one with nothing expected
  has `any` parameters (provisional). A builtin that takes a function runs it at render
  time, and an error inside it points at the lambda, not at the call.
  `x.f(…)` lowers `x` first and its type decides (D34): a builtin type's method `f`, or
  a record's field `f` called as its value. A name that is neither is an error naming
  the methods that receiver does carry, and the indexing a map wanted instead. Only `any`
  defers the question to the VM, which tries the field and then the method for the value
  it finds, and errors with the span if neither answers; a receiver known to be null is
  refused where it stands, as reading its field is. A method's receiver is its first argument, so an arity message counts
  as the author wrote it — `xs.first(1)` gives one argument to something that takes none.
  Through `?.` the call is jumped over entirely when the receiver is null, and its result
  is optional. A qualified variant is not a receiver at all: `Placement.hero(…)` is a
  call of the variant, and calling an enum is an error.
- **Declarations.** A type declared twice, in one file or two, is the duplicate error
  functions get; a declaration may not take a builtin type's name; a field or a variant
  repeated within one declaration is an error at the repeat. An `extends` that names an
  enum, an unknown type, or the type itself (through any chain) is an error at the base
  name; a field redeclared with another type is an error at the field, labelled with
  the ancestor's. An unknown type anywhere is an error at the name, and what carries it
  is `any` from then on. Another file's declaration a file checked against is one of its
  unit's compile-time facts (D25): a change to that shape makes the unit stale at link.
- **Iteration.** `lem-for` over `[T]` binds `T` (and an `int` index), over
  `{string: V}` a `string` key and `V`, over `any` two `any`s.
- **Unused.** A `lem-let` (either form) or a `lem-fn`/lambda parameter that nothing reads
  warns at its name; loop bindings are exempt, since an unread index or key is the
  common case. A binding the name rules refused never warns.

Where the checker says `any`, the VM still checks at render time — the runtime errors
are the `any` path, not a second checker.

## Names & namespaces (D23)

One value namespace, resolved lexically: **locals/params** (innermost wins) →
**project** → **ambient**. Types are separate, distinguished by capitalization
(`Article` type / `article` value) — a convention, not a rule: a type and a function may
share a name, and the type namespace is project-flat with the same duplicate rule as
the value one. `Name.x` is the one place the type namespace is consulted first: when
`Name` is a declared enum it is a variant, whatever the value namespace holds. Builtin
`lem-*` tags occupy no namespace, and neither do the method names a builtin type carries
(D34): they belong to the type, so a `lem-fn first` collides with nothing and warns about
nothing.

- **Project-flat:** every `.lem` file in the site shares one namespace; no imports
  between site files. Duplicate definitions are compile errors citing both spans.
  File-top-level `lem-page`/`lem-let` bindings stay local to their file — except
  `site.lem`'s, which are the fields of `site` (D30).
- **Plugins qualified:** `site.lem` binds once —
  `<lem-import photos from plugin("photos") />` → `lem.photos.picture(…)`; named
  promotion `<lem-import { picture, Photo, PhotoIndex } from plugin("photos") />`
  brings favourites (and types) into the flat namespace. Promotion collisions are the
  same duplicate error.
- **Ambient:** the stdlib functions that make or vouch for a value (`raw`, `asset`,
  `url`, `datetime`, `uniqueId`, …) need no import — an entry that works on a value is
  that value's method instead, `heading.slugify()`, `d.rfc3339()`, and occupies no
  namespace (D34); a `lem-fn` or `lem-let` may shadow an ambient name, with a warning at
  the definition when the name it displaces is the stdlib's — a parameter or loop
  binding of that name is not a shadow, and neither is a definition displacing a project
  function, a parameter, or an outer binding; none of those warn.
  The ambient *values* are the host's to name and type (D30) — the standard shell binds
  `site`, `page`, `build` — and cannot be redefined.

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
<lem-fn article(page: Article, body: html)>
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
6. Front matter is a typed record — the schema of the collection whose glob matched the
   file, or the subtype a `type:` key names (D30) — and the body has `page` and `site`
   in scope. String literals where a record is expected resolve through reference
   indexes at compile time:
   `<lem.photo("middle-falls-letchworth-state-park", placement: right)>` — a typo'd id
   is a build error. _Not yet (∆Hbw): the core skips a leading `---` block without
   reading it — the shell's YAML parser owns it — and a string literal is a string._
7. A void keyword tag (`<lem-let x = … />`) takes its line with it, as in a template
   (§Tags); an access tag or a block keeps the line ending after it, so that two
   blocks' output stand on two lines.

comrak configuration: CommonMark + footnotes, attribute-style heading ids (`<h2 id="…">`,
slugged from the text and made unique, no anchor link injected), `header_attributes`
(`{#id}`), syntect highlighting (class-based, generated theme CSS — _not yet_ (∆BfR)),
`render.unsafe` (raw HTML passes through). A shell-side `markdown` override hook is reserved
but not built.

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
- closed-union syntax and `lem-match` exhaustiveness rules (variant spelling is settled: D32)
- `==`/`&&` vs `eq`/`and` spelling (consistency with `lt`/`gt`)
- inline lem tags in prose (reserved for v2; design must remain compatible)

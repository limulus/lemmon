# Mistakes the compiler reports

The diagnostics authors hit most, each with a source that earns it, compiled against the
example site in `SKILL.md` (`Post`, `Photo`, `Placement`, `site.photos`). In a message, `…`
stands for words left out; in a source, a `/` between code spans separates its lines. Every
diagnostic carries a span, so its `path:line:col` points at the token to change.

### While compiling

- `<lem-iff page.draft>` — _unknown tag `lem-iff`; did you mean `lem-if`?_
- `<lem-if page.draft><p>Draft</p>` at the end of a file — _unclosed `lem-if`…_
- `<lem-if site.recent.length() > 0>…</lem-if>` — _…an optional, got `int`_: the head ended
  at the `>`; write `gt`.
- `<lem-fn f(b: bool)><lem-if b />x</lem-if></lem-fn>` — _`lem-if` is a block tag and cannot
  be self-closed…_
- `<lem-fn f(a: bool, b: bool)><lem-if a><lem-else />…<lem-else-if b />…</lem-if></lem-fn>`
  — _`lem-else-if` cannot follow `lem-else`_
- `<lem-fn card()><lem-style />…</lem-fn>` — _`lem-style` is a block tag…_
- `<lem-fn f(xs: [int]): [int] = xs.map(x => x + 1) />` — _expected `)`, found `=`_: a
  lambda is `|x| x + 1`.
- `<lem-fn greet(name: string) = "Hi " + name />` — _an expression-bodied function declares
  its return type…_
- `<lem-fn title(): string>Home</lem-fn>` — _a block-bodied function returns `html`…_
- `<lem-fn f()><lem-fn g(): int = 1 /></lem-fn>` — _`lem-fn` must be at the top level of a
  file_
- `<lem-fn card()><lem-let unused = 1 /></lem-fn>` — _warning: `unused` is never used_
- `<lem-fn link(title: string): string = "Link: {title}" />` — _warning: `title` is never
  used_: interpolation is not implemented; write `"Link: " + title`.
- `<lem-fn url(s: string): string = s />` — _warning: `url` shadows the stdlib function of
  the same name_
- `<lem-fn f(page: Post): string = page.title />` — _`page` is bound by the host and cannot
  be redefined_
- `<lem-fn f(): int = 1 /><lem-fn f(): int = 2 />` — _`f` is already defined_
- `<lem-fn byline()><lem.page.byline></lem-fn>` — _`Page` has no field `byline`_: in a
  layout or component `page` is a `Page`; read the record through a parameter.
- `<lem-fn r(p: Photo): string = p.rendition("md") />` — _`Photo` has no field `rendition`_:
  call a project function as `rendition(p, "md")`.
- `<lem-fn sub(p: Post)><lem.p.subhead></lem-fn>` — _cannot print a `string?`: narrow it
  with `lem-if`, or give it a default with `??`_
- `<lem-fn f(): int = 1 /><lem-fn g()><lem.f></lem-fn>` — _cannot print a `(): int`: call it
  with `(…)`…_
- `<lem-fn t(p: Post)><lem-if p.title>…</lem-if></lem-fn>` — _a condition must be a `bool`
  or an optional, got `string`_
- `<lem-fn n(p: Post): int = p.title?.length() />` — _`?.` needs an optional, got `string`_
- `<lem-fn n(p: Post): int = p.title.length />` — _`length` is a method of `string`: call it
  as `.length()`_
- `<lem-fn alt(): string = site.photos.middle.alt />` — _a `{string: Photo}` has no fields:
  index a map as `["middle"]`_
- `<lem-fn y(p: Post): string = "© " + p.date.year() />` — _cannot apply `+` to `string` and
  `int`_: print the number with its own `<lem.…>`.
- `<lem-fn half(n: int): float = n / 2 />` — _expected `float`, got `int`_
- `<lem-fn list(xs: [int]): string = xs.join(", ") />` — _`.join()` needs a list of
  strings…_
- `<lem-fn h(p: Photo): html = raw(p.src) />` — _raw() takes a string, got `url`_
- `<lem-fn h(s: string): html = raw(s: s) />` — _`raw` does not take named arguments_
- `<lem-fn w(p: Placement): bool = p == "hero" />` — _cannot compare `Placement` with
  `string`_: write `p == .hero`.
- `<lem-fn h(): Placement = hero />` — _undefined name `hero`_: a variant is `.hero`.
- `<lem-fn h()><lem.(.hero)></lem-fn>` — _no enum is expected here: qualify the variant as
  `Enum.hero`_: write `Placement.hero`.
- `<lem-fn f(p: Placement)><lem-match p><lem-case hero>…</lem-case></lem-match></lem-fn>` —
  _unknown type `hero`_: a bare case name is a type pattern; write `.hero`.
- `<lem-fn f(p: Placement)><lem-match p><lem-case .hero>…</lem-case></lem-match></lem-fn>` —
  _`lem-match` over `Placement` misses `.left`, `.right`_
- `<lem-fn d(): datetime = datetime("2026-04-04T10:30:00") />` — _…has a time and no UTC
  offset…_
- `<lem-fn css(which: string): url = asset(which) />` — _`asset()` needs a path written as a
  literal…_
- `<lem-type Memo extends Page>` / `topic: string` / `</lem-type>` — _`Memo` extends `Page`,
  so it declares how a page renders: `layout: (Memo, html): html`_
- `<lem-type Card>` / `banner: string = site.name` / `</lem-type>` — _`site` is out of scope
  in a default…_: a default reads only `self` (and `path`, `body` on a page).
- `<lem-let drafts: [Post] from "drafts/*.md" />` outside `site.lem` — _a `from` binding
  belongs at the top level of `site.lem`…_
- `<p><lem.site.name></p>` in a file with no front matter — _warning: this file has no front
  matter, so it is no page and its top-level output never renders_
- `Hello <lem.page.url> there` in a `.md` — _lem tags in prose must stand alone on a line…_
- `<lem.page.url> and more` in a `.md` — _unexpected text after a lem-block on its line…_
- `<aside>` / `<lem.page.url>` / `</aside>` in a `.md` — _markdown swallowed this
  lem-block…_: put a blank line inside each edge of the element.

Two mistakes compile without a word: a string with `{…}` in it prints the braces, and a
`format("MMMM d")` pattern that is not strftime prints the letters. Both read as text.

### While loading front matter

These surface once data loads, which `lem check` and `lem build` both do.

- `subhed: Off by one` — _`Post` has no field `subhed`; did you mean `subhead`?_
- `tags: [article]` — _`Post` has no field `tags`_: collection membership is the glob.
- `subhead: 3` — _expected `string?` for `subhead`, got `int`_
- `hero: dawn` — _`hero` names "dawn", which its index has no entry for_
- `date: 4 April 2026` — _`4 April 2026` is not a datetime…_
- `layout: articl` — _`layout` names `articl`, and no `lem-fn` has that name_
- omitting `author:` — _missing `author`: `Post` declares it with no default_

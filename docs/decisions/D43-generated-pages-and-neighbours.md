# D43 — Generated pages and their neighbours

- **Status:** Accepted 2026-09-14 while planning ∆c9S; that planning split ∆c9S into ∆Bhz
  and ∆99b. Closes the three points D41 left open, and revises D30 (`Page` no longer
  declares `prev`/`next`, and the shell reads the `for:` key) and D36 (the declaration
  names the url field and the neighbour fields)
- **Backlog item:** ∆Bhz (the url field, `plan()`, page templates and their `body`);
  ∆99b (the generator key, expansion, neighbours)
- **Normative text:** `docs/language.md` § Types › Ambient types; `docs/architecture.md`
  § WASM API, § Build pipeline, § VM

## Context

D41 made a `.lem` file with front matter a page and a generator a page template whose
front matter names a `site` list, and left three things to the item that builds it: the
key's name and shape, whether the shell reads it or the host declaration names it, and
how a string in front matter names a field of `site`. Planning that item against the code
found three more.

- **A record cannot hold its neighbours.** The docs declared `prev: Page?` and `next: Page?`
  on the shell's `Page`. Values are immutable and reference-counted, so a page whose `next`
  holds its successor, while the successor's `prev` holds it, is a cycle no loader can
  build. The type was also too wide: `page.prev.title` is refused, since `Page` has no
  `title`, and a subtype may not redeclare the field as `PhotoPage?` (D32), so every use
  would narrow first. And for a document, "its neighbours in the collection whose glob
  matched it" meant the order the shell listed files in, which is no order an author chose.
  limulus.net reads neighbours in one place, its photo pages, for their url and title;
  nothing reads a collection's.
- **Nothing reports where a page goes.** The shell writes each page at its url. It
  derives that url for a document and for a static page template, but a generated page's
  url is a default computed in the core, from a record the shell never sees, so the core
  has to report it. D36 had said the declaration needs nothing beyond `ambient`,
  `document`, and `entry`.
- **A page template has no `body` for its defaults.** A default on a page-backed record
  sees `body`, the document's prose rendered at compile time (D37). A template has no
  prose: its output exists only once it renders, reading the record the defaults are
  still building.

## Decision

**The key is `for:`, the shell reads it, and it is checked at load.** A generator's front
matter carries `for:` with exactly one entry: the name of a field its page type declares,
and the list whose items fill it, written `<ambient value>.<field>` — the same pair
`by site.photos` names in a schema, never an expression.

```
---
type: TilTopicPage
for:
  topic: site.tilTopics
---
```

The shell reads and strips the key, as it strips `type:`, and sends the pair beside the
page's front matter with the positions of both. The loader checks it there: the ambient
value is one the host binds, the field is declared on it as a list, the page type
declares the named field, and the list's element is assignable to it. The item is the
list's value as loaded, not a string resolved again as data. A generator's front matter
gives neither that field nor the url field, and names its type with `type:`. Its pages
are built once every bindings file has loaded, so no `from` glob can hold them, and a
glob that matches a generator is a load error at its `for:` key. An empty list makes no
pages.

**The declaration names the url field, and `plan()` reports it.** The host names the field a
page's url is in, as it names the entry field; the document value's declared type must
declare it as `url`. `plan()` lists every loaded page — each collection's in the order the
loader builds them, then the pages the envelope carries on their own in its order, a
generator expanded where it stands — as a reference with that field's value. A reference is
`{path, index?}`, `index` numbering a generator's pages from zero in list order. Two loaded
pages at one url is a load error at the second, labelled at the first.

**Neighbours belong to generated pages, declared by their own type.** The host declaration
names the two neighbour fields. The shell's `Page` declares neither; a page type whose pages
have neighbours declares them itself, each typed as an optional of that type or one of its
ancestors — `prev: PhotoPage?` on `PhotoPage` — which the compiler checks where the field is
declared, as it checks the entry field. On a generated page the loader fills them with the
previous and next pages of its generator, in list order; the first page has no previous, the
last no next, and every page that is not generated has neither. A page of one generator
always has that generator's type, so the declared type holds, and a photo template reads
`page.prev.title` with nothing narrowed.

**A neighbour is a reference into the page table, read through when the field is read.**
The loader fills a neighbour field with a reference to a page in the store, and reading
the field yields that page's record, so no record holds another, no cycle is built, and
`render` takes the page table to read through. A default may not read a neighbour field,
since the loader fills them after every page of the generator is built. The read is
refused where it stands, or at link when it goes through a call, as a default's read of
`page` is. Front matter cannot give one, since a page record never nests in another's
front matter (D37).

**A record whose defaults read `body` cannot type a page template.** The compiler records
where each default reads `body`, and the link step, which holds every unit, refuses a page
template compiled as a record whose plan runs such a default: one error at the template's
front matter, labelled at each default that reads it. A document of the same record is
not refused.

## Alternatives considered

- **Neighbours on the shell's `Page`, typed `Page?`, for every page.** Rejected: every use
  narrows before reading a field the page type declares, and a collection's neighbours
  follow the order its files were listed in.
- **No neighbour fields: a template finds its neighbours in the list it pages over**, with
  list methods, and composes their urls through a `lem-fn` its type's url default also
  calls. Rejected: it works over an item such as a photo but not over page records, whose
  entry field holds a function that `==` cannot compare, and a neighbour page's computed
  fields are out of reach.
- **Neighbour records one level deep**, whose own neighbours are null. Rejected:
  `page.prev.next` would be a silent null rather than the page itself (invariant 10).
- **The host declaration naming the generator key, read by the core.** Rejected: the
  core would read front matter for one key, under a name that is still the shell's
  convention. D37 declined the same arrangement for `type:` as more machinery than
  stripping a key the shell reads anyway.
- **Sending and checking the pair at compile, beside `record`.** Rejected: it would be a
  compile input that changes no compiled code, and every other front-matter mistake is
  already a load error at its YAML position, which is where the language server validates
  front matter too (D16).
- **An empty `body` for a page template, or its output rendered at load.** Rejected: an
  empty body is a guess a default would compute a wrong value from (invariant 10), and a
  render at load reads a `page` whose record is the one being built.
- **A generator's pages joining a collection.** Rejected: the pages exist only once the
  list they are made from has loaded, and a collection holding them would have to load
  after a list that may itself be computed from that collection.

## Consequences

- D41's open points are closed. The shell reads two front-matter keys of its own, `type`
  and `for`, and the declaration names three kinds of field: the entry (`layout`), the url
  (`url`), and the neighbours (`prev`, `next`), which only a generated page's type
  declares.
- D36's declaration grows two optional names; a unit that reads a neighbour field depends
  on the declaration as a unit that read an ambient name does.
- The store keeps its pages in the order `plan()` reports them, each with its record and
  reference, rather than one record per document path.
- The VM gains a reference value that only a neighbour field holds and a field read
  resolves. `render` needs the store's page table beside the globals.
- ∆Bhz builds the url field, `plan()` for static pages and documents, page templates (D41),
  and the `body` refusal; ∆99b builds `for:`, expansion, neighbours, and `index` on the
  reference. ∆ePP ports the photo and TIL topic pages on both. The shell's half — reading
  and stripping `for:`, and sending every page no glob holds in the envelope's own section —
  belongs to the shell's front-matter reader (∆4tG).
- `docs/artifact-changes.md` records what the three artifacts must say: the photo page
  port's front matter and its `PhotoPage` neighbour fields, the declaration's new names,
  and `plan()`'s result.

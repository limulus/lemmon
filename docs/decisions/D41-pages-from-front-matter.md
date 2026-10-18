# D41 — Pages from front matter

- **Status:** Accepted 2026-09-13 (∆RGQ), revising D30; the generator key's name and who
  reads it, left to ∆c9S, settled 2026-09-14 by D43 — the planning that split ∆c9S. As
  built (∆Bhz, 2026-09-18): one textual rule finds the block in both modes, and a
  block that never closes is an error at its opening line rather than a rule and
  prose, in a `.md` too; a page template renders through its layout with its output
  as the body, in one VM run with the layout —
  settled with Eric 2026-09-17. As built (∆99b, 2026-10-18): generators, with the
  refinements D43 records
- **Backlog item:** ∆RGQ (settled it); ∆Bhz (page templates) and ∆99b (generators) build
  it
- **Normative text:** `docs/language.md` § Types › Ambient types

## Context

The docs gave a `.lem` file a page-header tag, `lem-page`, whose `name = expr` bindings
were the page's own fields — the template's counterpart to a document's front matter —
and whose `for x in xs` clause made one page per item. Planning ∆RGQ asked why a template
needs a tag when a document already becomes a page through front matter.

limulus.net answered it. Besides its markdown, the site has eight static pages written
in WebC — the home page, 404, and the index pages for the feed, links, photos, TILs,
Penumbra, and Penumbra's journal — plus the sitemap. (The About page is WebC too, but its
content is markdown, and the port makes it a document.) Every one carries plain YAML front
matter: a title, a layout, a teaser, sometimes an image or a url. None computes a value.
Only its two generators, the photo pages and the TIL topic pages, need more: JavaScript
front matter for Eleventy's pagination, a filter and sort over the list, and a title and
url computed per item. Eleventy's arrangement has one property worth keeping: the file
that generates the pages sits where those pages live.

## Decision

**A template is a page when it has front matter.** A `.lem` file that opens with a `---`
block is typed as a document is, by the collection whose glob matched it or by a `type:`
key; the loader validates its front matter as the page's record; and the template's own
output is the body its layout receives. From there a template page and a document are
rendered the same way. A `.lem` file with no front matter is not a page. `lem-page` is
retired.

**A generator is a page template whose front matter names a list.** It makes one page per
item of a `site` list, with the item in the field the front matter names. Neither of the
two things Eleventy's generators needed JavaScript for lives in the front matter.
Filtering and sorting the list is a computed binding in `site.lem`. Each page's other
fields, such as its title and url, are defaults on its page type, which read the item
through `self` (D37). A generated page's `url` must come from its type, since every page
of one file would otherwise derive the same one, and `prev`/`next` follow the list's
order.

`docs/language.md` § Ambient types shows the shape, with the key written `for:`
provisionally.

## Alternatives considered

- **Keep `lem-page`, with expression bindings and a `for` clause.** Rejected: its only
  advantage over front matter is expressions, which no static page uses; it is a second way
  to build a page record beside front matter; and a generator would mix a tag with the
  front matter its static fields could have used.
- **Generated pages as computed bindings in `site.lem`**, a list of a `Page` subtype whose
  markup is its layout function. Rejected: it loses the file where the pages come from; it
  needs a rule for which `Page` records are pages, since a binding like
  `articles = concat(posts, links, tils)` re-lists pages that already exist; it contradicts
  D37's rule that a page record comes only from a document of its own; and a generated page
  would have no `inputPath`.
- **Eleventy's pagination vocabulary in front matter** — a data path, a size, an alias, a
  filter, and a permalink template. Rejected: it is a second language inside YAML, which
  D06 rules out, and each of its expressions already has a home in Lemmon.

## Consequences

- `<lem-page>` is an unknown tag, like the retired `lem-collection` and `lem-data`
  (`syntax/unknown/misspelt-tags`). D30's `url` clause now names front matter alone.
- Page templates compile with the documents, not before them. The shell parses a `.lem`
  file's front matter before compiling it, as it does a document's, because a `type:` key
  or a matching glob decides the record the file compiles as; the core parses no YAML
  (D37), skips the leading `---` block, and validates what the shell sends at the shell's
  positions. So the order D30 describes becomes: templates without front matter first,
  `ambient.lem` and `site.lem` among them; then documents and page templates, matched
  against the `from` globs `site.lem` reports and compiled with their records (∆4tG).
- This record left three things open, and D43 settled them: the key is `for:`, with one
  entry naming a field and an `<ambient value>.<field>` list, as a `by` head names a map;
  the shell reads and strips it as it strips `type:`; the loader checks it. D43 also
  moved neighbours from the shell's `Page` to the page types that want them.
- ∆Bhz makes the scanner treat a `.lem` file's leading `---` block as front matter and
  renders a page template through its layout; ∆99b expands generators and gives `plan()`
  their pages.
- A page with several items, such as ten posts per archive page, is additive later; both
  limulus.net generators make one page per item.
- ∆ePP ports the photo and TIL topic pages as generator templates _(∆ePP is now ∆hvm)_.
- `docs/artifact-changes.md` records what the three artifacts must say about page
  templates and generators.

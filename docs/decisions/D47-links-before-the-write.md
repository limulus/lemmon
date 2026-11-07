# D47 — Every link resolves before anything is written

- **Status:** Accepted 2026-10-19, planned the same day and settled while building `∆yOJ`;
  moves the architecture's link/asset integrity check from Post-process, after the write,
  to a Check step before it, and retires Post-process. Revised 2026-11-07 (`∆H7Q`): a
  `srcset` that `@prettier/parse-srcset` refuses is an error, as an XML document whose
  links cannot be read is, and the body pass reads a body's links as the check reads them
- **Backlog item:** `∆yOJ`; `∆H7Q` (the revision)
- **Normative text:** `docs/architecture.md` § Build pipeline (step 8);
  `docs/shell.md` § `lem build` (step 7), and the config's `served`

## Context

The architecture put a link/asset integrity check in step 9, Post-process, after the
write — against the shell's rule that every check runs before anything is written, and
that `lem check` reports what a build would. Mapped over the limulus-dot-net port's output
on 2026-10-18, 115 of 4,025 references were broken, across four targets; the feeds (`∆zu0`)
cleared 112. What remained was a redirect, `https://limulus.net/@eric`, which CloudFront's
function answers with no file; a live content bug, `[wonderful essay](link)`; and
`#fn2`, a footnote comrak names by its label. Reading the feeds as XML on 2026-10-19 found
two more: the Atom feeds' alternate links, `https://limulus.net/tils` and `/penumbra`,
which CloudFront's dot-less rewrite serves and no file does. S3 and CloudFront match names
exactly, while `lem dev` serves from a file system that may fold case and Unicode form,
so a mis-cased link works in the dev server and fails in production.

## Decision

**The check runs at the end of `renderSite()`, over what the build holds in memory, and
only when nothing before it failed** (architecture step 8): every page, asset file and
produced file, the claims on every output file, passthrough files' among them, and the
merged manifest with the plugin that mapped each entry. So `lem build` writes nothing on a
broken link, `lem check` reports it, and `lem dev` reports it and keeps serving the last
build. A page that failed to render has claimed its file and holds no links, so a check
after it would pass links into it and miss its own: D37's rule, the author's error and not
one downstream of it.

**A link is what the output's kind makes one, read as that kind is read, never by a
pattern over the text**, chosen by the file's extension. HTML (`.html`) is parsed with
parse5 as a document with scripting off, the parse the body pass uses (D46), and its links
are the attributes of `@11ty/posthtml-urls`' table, which `links.ts` now holds for both —
a `srcset` candidate by candidate, a `ping`'s urls split at whitespace as HTML splits them
(the body pass keeps posthtml-urls' commas; _revised by `∆H7Q`: it splits them so too, and
reads every url here an element's attributes hold_), `<base href>` setting the base the
rest resolve against rather than being checked — and three rows the table lacks for this
use: a refreshing `meta`'s url, by HTML's refresh steps; any SVG element's `href` and
`xlink:href` (`use`, `image`, `feImage`, `a`); and the `content` of Open Graph's url
properties, a vocabulary, since titles such as "TILs: git" parse as URLs too. A
`<style>`'s text and a `style` attribute are CSS. CSS (`.css`, and inline) is tokenized by
CSS Syntax 3 with `@csstools/css-tokenizer`: a `url()`, a `url(` or `src(` with a string,
and `@import`'s string, against the stylesheet's url — so an SVG `data:` URI holding
`url(%23clip)` is one string. XML (`.xml`) is parsed with `@xmldom/xmldom`: an
`xml-stylesheet` instruction's `href` before the root, every element's `href` and
`xlink:href`, and the text of an RSS `link` without `href`, an RSS channel image's `url`,
a sitemap `loc` or `*_loc`, and a `guid` unless `isPermaLink="false"`, by local name,
against the nearest `xml:base`. An Atom `id` is an identifier, not a link, and a feed's
escaped item content is its page's body, whose links its page's check reads. Nothing else
is read: JavaScript, an inline script's text, SVG paint `url(#…)`s, and passthrough files
of any kind are targets only. Every url an asset hook's manifest maps must name a claimed
file too, which covers an `asset()` url that appears only inside a script; one that does
not is said once, at the config, and no page that prints it is faulted for it again. Every
tree is walked with a stack of its own, the body pass's too, since a page may nest deeper
than the call stack goes (invariant 8).

**An XML document whose links cannot be read is an error.** That is one the XML parser
refuses — at an error, and at a warning too, each of which marks markup that is not
well-formed, but the one at a U+FFFD, which UTF-8 may hold; one that declares an encoding
other than UTF-8, which is all the check reads, so that ISO-8859-1 bytes are not misread as
links to names that do not exist; and one that is not UTF-8. xmldom expands no entity a
document declares, so a document using its own is refused, a billion-laughs one in 2 ms. It
is not a strict parser in every place, though: a bare `&` before whitespace, a NUL and a
lone surrogate pass it, which a feed reader's parser would refuse, so a feed the check
passes is not thereby proven well-formed. _(Revised by `∆H7Q`: a `srcset` that
`@prettier/parse-srcset` refuses, which it does whole where any candidate's descriptors do
not parse, is an error too, once per page for each element, attribute and value, at the
page's writer. No link in it can be read, so none can be checked, while a browser drops only
the candidate at fault and loads the rest; `parse-srcset` 1.0.2 had dropped that candidate
too, and the check read the others.)_

**A link resolves as a browser resolves it, and names a file exactly.** A url the URL
parser refuses even against its base is skipped — a body keeps such a link and is warned
at (D46), and a mistyped one costs no build — and so is one in a scheme other than `http`
or `https`, and one on another host than the config's `origin` — every absolute one, where
the site names none. The origin's host on either scheme is the site's own:
`http://limulus.net/x` is checked as `/x`, since the same files answer it. A path the
config lists as `served` is answered. Any other names the file its path maps to as a
page's url does, which some writer must claim exactly, case and Unicode form included. One
that differs only in those is an error that says so; so is a dot-less link to a directory
whose `index.html` is written, since no host rule is modelled: CloudFront's dot-less
rewrite is limulus.net's, and a page served at `/x` resolves its relative links against
`/`. A fragment into an HTML page the build writes must name an element by HTML's rule for
the element a fragment indicates — an `id`, or an HTML `a`'s `name`, as written or
percent-decoded; the empty fragment and `top` the top; a `:~:` text directive dropped; an
id inside a `<template>` none — and one into any other file is not checked. The page is
read as it is written, so a fragment only a script satisfies — an id a script inserts,
`#comments`, or a client route, `#/settings`, `#!` — is an error, and nothing exempts one
today; a site that needs one gets an additive step later.

**`served` is a top-level config field**, beside `origin`: the paths on the origin the
host answers without a file the build writes — a redirect or a rewrite — each a path from
`/` as a link writes it, matched exactly, and read by nothing but the check. It is named
for what it states about the host: `redirects` would misname a rewrite, and `links.ignore`
an allowance, where the host does answer. It is validated at load as `origin` is, each
fault refused for what it is, and normalized to the URL parser's path, two entries naming
one path refused. A path it lists that names a file the build writes is a warning at the
config's start, labelled at that file's writer: the entry is stale, and a link there, a
fragment into the page among it, goes unchecked.

**Each fault is an error, once per output file per url it resolves to**, naming each way
the url is written there and every place it stands, with how often, so a link a layout
writes reports on each page, as a layout's runtime error does (D43); one url written alike
under two `xml:base`s resolves twice, and is said twice. It stands at the writer: a page's
`url` key or its front matter, a generated page's `for:` entry, a producer's mark, the
config's start for an asset file or a manifest entry (invariant 8). Nothing searches the
source for the url, which may come from a layout, a component, data or prose, and placing
it by text would be a guess (invariant 10). A near miss is labelled at the writer of the
file it names, and a missing fragment at the writer of the page it points into, but where
that page is its own.

## Alternatives considered

- **After the write, as Post-process.** Rejected: a broken site would be on disk, and
  `lem check` could not report it.
- **A pattern over the text**, as the prototype began. Rejected: a url in a comment, a
  string, a `data:` URI or escaped content is no link, and the table needs the parse.
- **Matching folded, as the dev server's file system does.** Rejected: the host matches
  exactly, and this check is where the difference is caught.
- **Modelling the host's dot-less rewrite.** Rejected: it is one host's rule, and the
  page it serves resolves its relative links elsewhere; limulus.net's feeds are fixed to
  end in `/` instead, their `<id>`s kept.
- **`served` with prefixes**, as CloudFront's `/@eric` rule is one. Deferred: exact paths
  cover every link the site writes, and a `/*` form is additive.
- **Any `meta` whose content parses as a url.** Rejected: six titles on limulus.net parse.
- **Scanning feed content.** Rejected: 12 ms on limulus.net, and every body's faults
  reported twice, the pages checking the same links rebased against the same origin.
- **Refused XML as a warning that skips the file.** Rejected: its links would go unread,
  and a feed reader's stricter parser refuses it too.
- **Grouping by the url as written.** Rejected: `/x/` and `https://limulus.net/x/` on one
  page are one fault, and one url under two `xml:base`s two.
- **One diagnostic per broken target, across pages, with labels.** Rejected for D43's
  page-by-page precedent.
- **A did-you-mean.** Not offered: the shell has none, and the policy lives in
  `lemmon_syntax::nearest`.
- **A hand-written CSS scanner and XML reader.** Rejected: two small dependencies with
  none of their own — 18 KB of JavaScript for the tokenizer, about 280 KB for xmldom —
  against reimplementing a tokenizer and an XML parser. css-tree, a full parser, was
  measured as an upper bound and is more than a tokenizer needs.
- **`@xmldom/xmldom` 0.9.5, the port's pinned version.** Rejected: every 0.9 release
  before 0.9.12 carries published high-severity advisories, and nothing here needs parity
  with the port's copy.

## Consequences

- `∆yOJ` builds `links.ts` (the table and the `srcset` and `ping` splitting rebase.ts held,
  whose walk now keeps a stack of its own too),
  `link-scan.ts`, `link-check.ts`, `OutputFiles.find`, the url on each `Output`, the
  manifest's `mappedBy`, and `served`; the shell depends on `@csstools/css-tokenizer` 4
  and `@xmldom/xmldom` 0.9.
- The pipeline's step 8 is the Check and step 9 the Write; Post-process is gone. An output
  transform (`∆fwn`), when built, runs between Resolve and the Check, since the check reads
  what is written.
- The limulus-dot-net port lists `served: ['/@eric']`; its feeds' alternate links end in
  `/`; make-ai-boring-again links `#fn-out-of-scope`; the essay's `(link)` is fixed on
  limulus.net's main.
- `lem dev` writes nothing while a page links to one the site does not have, until it does.
- Not read, by design, each additive later: an HTML page whose file does not end `.html`
  — `/page` with no extension, `p.htm` — which is neither scanned nor a fragment's
  target; SVG presentation `url()`s (605 on limulus.net, all into the logo); urls in an
  inline script not from `asset()`; passthrough contents; fragments into anything but an
  HTML page the build writes; `imagesrcset`; CSS `image-set()` strings; Atom `icon` and
  `logo`; RSS `enclosure` and `comments`. A manifest url resolves against the origin's
  root.
- Cost, on a copy of the port: about 4,100 links in 59 pages, 37 asset files and 10
  stylesheets; the check takes about 65 ms in a fresh process and 40–46 ms warm, so
  `lem build` goes from about 0.75 s to 0.82 s and a `lem dev` rebuild from about 440 ms
  to 490 ms, still under the second at which `∆xBO` opens.

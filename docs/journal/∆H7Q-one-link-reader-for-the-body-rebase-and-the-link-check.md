---
id: ∆H7Q
completed: 2026-11-07 09:52:24 -07:00
---

## Backlog item

> - `∆H7Q` One link reader for the body rebase and the link check
>   - The body pass rebases a body's links by the HTML rules the link check reads them by;
>     the Eleventy rules kept for parity, and `parse-srcset` with them, go

## Planning

This section folds in the item's brief. The problem was found by a review of the slices
on 2026-10-20. Two parts of the shell read the same markup by different rules:

- **The body pass** (D46, `rebase.ts`) rebased a document's links by Eleventy's rule, over
  `@11ty/posthtml-urls`' table, as limulus.net's feeds had them:
  - a `ping` was split at commas;
  - a `srcset` was re-read and re-joined through `parse-srcset` 1.0.2, which reported any
    candidate it dropped on standard output;
  - an SVG `href` or `xlink:href` was left alone.
- **The link check** (D47, `link-scan.ts`) read the same markup by HTML's rules, plus rows
  of its own: a refreshing `meta`, Open Graph urls and inline CSS.

The differences existed only for byte parity with Eleventy's feeds, which the cutover
retires. They meant that a link the check read could stay relative in `build.bodies`,
where a feed reads it on another page. On 2026-10-24 Eric decided to unify the two on
HTML's rules, with limulus.net's feeds unchanged.

The plan was written on 2026-11-01 against `9927735` and revised on 2026-11-06 against
`8c03716`. It put four questions to Eric. He took the recommendations with one change:
"use `@prettier/parse-srcset` instead" of a hand-written parser. The four answers:

- **Fragment-only urls in embeds stay as written.** This covers an SVG `href` on any
  element but `a`, and a CSS `url()`. Rebased onto the page's url, `<use href="#icon">`
  would name a document no reader loads. An `<a href="#fn1">` is a navigation, so it is
  rebased as before.
- **A `<style>`'s text is left as written**, though the check reads it.
- **A `srcset` the parser refuses is kept as written.**
- **Urls are trimmed by HTML's ASCII whitespace**, through the check's `stripped()`, not
  by JavaScript's `trim()`.

`@prettier/parse-srcset` 3.1.0 gives each candidate's offset, which splicing in place
needs. It differs from HTML in two ways:

- It refuses a whole value where HTML drops only the bad candidate.
- It throws on a value of only whitespace and commas, so the wrapper returns no
  candidates for one without calling it.

A differential run checked it against `parse-srcset` 1.0.2 over every `srcset` in
`lemmon-port`'s 55 built pages (933 attributes, 1,029 candidates). The two listed the
same urls in every one.

Parity could not change:

- no `.md` on the port holds a `srcset`, `ping`, SVG, `style`, `meta`, `base` or `url(`;
- the 18 `srcset`s in its feed bodies are all absolute urls on `pho.limulus.net`.

## Refinement

The change was built on 2026-11-06 as lemmon `1849242`, `refactor(shell)!`:

- One `attributeLinks()` in `link-scan.ts` yields every url an element's attributes
  hold, read as the check reads them, along with what the rebase needs to splice it.
- The rebase writes only the urls that change, each over itself in its attribute, so a
  `srcset`'s descriptors and a `ping`'s spacing stay as written.

The commit is production +310/−206 lines, tests 474, docs 319. It was no line
simplification: the refusal handling that went was small against the CSS and refresh
splicing that came.

The diff's review found:

- **A refused `srcset` hid its good candidates from the check**, since the library
  refuses the whole value. The plan's answer to the third question had been reasoned for
  one bad candidate, which HTML drops anyway. Eric chose an error: the check reports such
  a `srcset` once per page, naming the attribute, and the body pass keeps it without a
  second message. This is the commit's breaking change.
- **A rebased refresh url holding `'` ended at that quote**, since HTML's refresh steps
  end a quoted url at its quote. Quotes are now percent-encoded there.
- **Specs were missing** for `escaped()`'s `"`, for the fragment rule applied to the
  stripped url, and for `\f` and `\r` in a CSS string.
- **A doc sentence overclaimed:** it said every other byte of a body survives, which the
  parser's own repairs do not promise.

Two bugs older than the item, which it made reachable through more attributes, landed as
their own commits:

- **`e893517`:** an element parse5 reconstructs carries the location of the one it
  copied, such as an `<a>` reopened after a misnested `</b>`. So a url the URL parser
  refused there was warned at twice. Each attribute is now taken once, by where it starts
  in the text.
- **`bccb701`:** parse5 8.0.1 ended an attribute at its name where a quoted value was
  followed directly by another attribute. Rewriting `src` in `<img src="./a.png"alt="x">`
  therefore wrote `src="…"="./a.png"alt="x"`, and a new `attributeEnd()` re-read the end
  from the text. On 2026-11-07 a search upstream found the bug already reported, as
  inikulin/parse5#1899 on 2026-10-27, and fixed in parse5 8.1.0, released 2026-11-01.
  `407a85c` raises the shell's floor to `^8.1.0`. It was meant to delete the workaround
  as well, but the diff's review found that 8.1.0 still ends an attribute at its name
  where its `=` has no value (`<a href=>`, `<a href = >`). The `>` branch of parse5's
  before-attribute-value state emits the tag without ending the attribute, the same
  family of bug. So `attributeEnd()` stays, its comment now naming that case, and the
  spec pins it: without the function, the rebase writes `<a href="…"=>`. No upstream
  report of that case turned up.

`npm run verify` passed before each commit.

Eric accepted the item on 2026-11-07, after a parity run the main session made at his
request:

- At lemmon `bccb701`, `dist/lemmon` was byte-identical to the
  `lemmon-port/dist/parity-final/` reference from 2026-10-19. So parity could not speak
  for the item either way.
- The run was shown able to fail: fifteen defects planted in a copy of the output were
  each reported. They included a feed entry's link left relative and an image's alt text
  dropped.
- The output and the parity report came out the same under parse5 8.1.0, at `407a85c`,
  and so did every planted defect's report.

One gap was left unfiled. A body whose refused `srcset` reaches only a feed, and no HTML
page, is reported by nothing, since the check does not read a feed's escaped content. In
practice a document's own page prints its body, so the error shows there.

## Retrospective

- The plan carried an answer Eric gave for one library over to another without
  re-deriving it, and only the review caught what that hid.
- The parse5 workaround was built and reviewed before anyone looked upstream, where the
  fix had been released five days earlier. A bug in a dependency gets an upstream search
  before a workaround.
- The offer to delete the workaround was then made from the one shape the upstream fix
  named, and the deletion passed `verify`. The workaround read every shape from the
  text, and the specs pinned only the shape that had been reported. The review found the
  second shape by probing the others.
- A parity pass was never evidence for a refactor that leaves the site unchanged. What
  supported accepting this one was the byte comparison and the planted defects.

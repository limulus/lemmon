# Refusing a render on purpose

## Background

Found in the limulus.net port (`∆foi`, 2026-10-15). `new-york-2024` is the one feed post
with a video, so the port gave it `VideoPost extends Post` with `vod: Vod` required and
`layout: (Post, html): html = video`. Two things the author wanted to write were refused:

- `layout: (VideoPost, html): html` — D32's same-type rule says a redeclared field keeps
  its type, and `layout` is declared `(Post, html): html` on `Post`.
- A `video(post: Post, body)` that renders only a `VideoPost`: a `lem-match` over `post`
  must have a `lem-default` arm, and nothing in the language lets that arm fail the render
  deliberately. So the arm falls back to `article(post, body)`, and a plain `Post` whose
  front matter names `layout: video` renders as an article, silently.

Invariant 10 says errors over guesses; here the author has no way to write the error.

A second case, from the photo pages (`∆hvm`, 2026-10-18): `photo-meta.webc` throws on a
lens or camera its tables do not know, and the port's `photoMeta` can fail only through
the table lookup itself, `no key "…"`, which names the key but not the table or the fix.

## Scope and constraints

- A design question for Eric: a `fail("…")` stdlib function whose call is a render error
  with its span (the smallest form), or letting a subtype narrow `layout`'s parameter
  type (a D32 change, since `layout` is read through the base type), or both. Whichever
  it is updates `docs/language.md`, the decision index and `docs/artifact-changes.md`.
- Not a shell concern.

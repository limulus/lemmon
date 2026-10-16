# `docs/language.md` describes three forms the compiler lacks

## Background

Found while writing the Lemmon authoring skill (∆pXO), whose every example is compiled
through the staged core. Where the doc and the compiler disagree today:

- **String interpolation.** The doc shows `"Link: {linkTitle}"` as interpolation, with only
  the delimiter deferred. The compiler keeps `{…}` as literal text
  (`crates/lemmon-vm/src/compile/calls.rs`, "unimplemented"). The doc carries no
  placeholder note, so the gate in `crates/lemmon-corpus/tests/placeholders.rs` cannot see
  the gap.
- **`lem-import`.** Described as working; the compiler refuses it as not supported yet
  (∆c4j, `tests/corpus/syntax/unsupported/builtins.lem`). Again no note in the doc.
- **`concat(posts, links, tils)`** in §Ambient types' `site.lem` example names a stdlib
  function that does not exist, and `+` on two lists is a type error: nothing concatenates
  lists today, which the port of limulus.net's `collections.article` will want.
- The **canonical example** writes `[md]` and `size: small`, undefined names; the corpus
  copy (`render/canonical/article-card.lem`) has `["md"]` and `size: "small"`.
- The "unquoted attribute is a lint warning" rule has no linter behind it.

## Scope and constraints

- A docs fix for the first four, each as a placeholder note (the phrasing the gate reads)
  naming the item that will land the form; interpolation and list concatenation have none,
  so decide whether each is a deferred question, a new item, or a sentence to delete. The
  canonical example is made to compile, and the lint sentence either names the
  editor-tooling item (∆5MM) or goes.
- Not a language change: nothing here decides the interpolation delimiter.

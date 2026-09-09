---
type: Article
title: The Rebound
hero: sunset
---

The schema says this field is a `Photo`, and the loader tagged what it resolved
`hero` to through `site.photos`, so the test answers (D35, D37).

<lem-if page.hero is Photo>
  <figcaption><lem.page.hero.caption></figcaption>
</lem-if>

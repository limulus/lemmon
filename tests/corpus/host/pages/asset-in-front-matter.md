---
type: Article
title: The Gallery
scripts:
  - scripts/gallery.js
---

A `[asset]` field resolves against the same manifest a template's `asset("…")` does, so
a path is one asset whether a document named it or a layout did:

<lem-for script in page.scripts>
  <script type="module" src="<lem.script>"></script>
</lem-for>

---
type: Article
title: New York 2024
date: 2026-04-04T10:30:00-07:00
runtime: 1490
---

A date in front matter is a datetime by the time a template reads it, printed in the
offset it was written in:

<lem.page.date>

<lem-if page.runtime>
  <time datetime="<lem.page.date>">when this went up</time>
  <p>There is a video on this one.</p>
</lem-if>

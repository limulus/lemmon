# Content + markdown parity

## Background

Content + markdown parity — typed front matter for every `.md` (globs replace `tags`, schema
defaults replace `links.json`), computed defaults replacing `eleventyComputed.js` (title
from linkTitle, tilTopic from path, link/via domains, readingTime), the link-post appender
as `transformDocument`, markdown-it's footnotes, task lists, attrs, and anchors in comrak,
and code by the highlight.js producer (`∆Nxd`)

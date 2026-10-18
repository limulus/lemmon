# `lem init`

## Background

`lem init` — scaffold a working blog as plain site code: `site.lem` with a `Post extends
Page` schema, a `posts` glob binding, a `layout` default, and base/post layouts, plus an
`index.lem` page. The core has no content conventions (D30), so this is where a new site
gets them — readable and editable, never built in. Its `lemmon.config.ts` lists the
shell's plugins, `plugins: [bundler(), highlighter()]`, so a new site's layouts place
their styles and its code blocks are highlighted out of the box (∆Nxd); the shell ships
no highlight.js theme, so the scaffold's styles carry one

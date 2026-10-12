# Build pipeline + CLI

## Background

Build pipeline + CLI — `lem build` walks the input, classifies files, parses front matter
with YAML positions, loads `lemmon.config.ts`, and drives the core across a worker pool:
compile `ambient.lem` + templates without front matter, match documents and page templates
to the reported `from` globs and compile them with their record (D30, D41), loadData, plan,
render; `lem check` prints spanned diagnostics. The shell's backbone

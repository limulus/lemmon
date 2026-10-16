# `lem check` and `--drafts`

## Background

### From `∆4tG` Build pipeline + CLI

Build pipeline + CLI — `lem build` walks the input, classifies files, parses front matter
with YAML positions, loads `lemmon.config.ts`, and drives the core across a worker pool:
compile `ambient.lem` + templates without front matter, match documents and page templates
to the reported `from` globs and compile them with their record (D30, D41), loadData, plan,
render; `lem check` prints spanned diagnostics. The shell's backbone

## Scope and constraints

- Slice 3's acceptance, which this item serves: Eric runs `lem dev` in the branch, edits a
  post or a layout and sees the change in the browser, sees `lem check` report a deliberate
  error with its span, and sees a draft under `--drafts` that `lem build` omits.
- `lem check` from the original build item; `--drafts` from the `lem dev` item, where D19
  places it as a field of the shell's `Page` declaration, not a reserved key.

# `lem check`

## Background

Split from `∆1rD` on 2026-10-17, when Eric moved drafts to Someday/Maybe (`∆Han`). From
`∆4tG` Build pipeline + CLI, by way of `∆1rD`: "`lem check` prints spanned diagnostics".
D33 settles what it prints from: a diagnostic's `range`, computed in the core, so the shell
converts nothing.

## Scope and constraints

- Slice 3's acceptance, which this item serves: Eric runs `lem dev` in the branch, edits a
  post or a layout and sees the change in the browser, and sees `lem check` report a
  deliberate error with its span.
- `lem build` already prints each diagnostic as `path:line:col: severity: message` with
  its labels beneath (`packages/shell/src/lib/cli.ts`), and writes nothing on an error, so
  `lem check` is that pipeline without the writing. It renders every page too, since a
  runtime error and two pages at one file are found only there.
- "With its span": `path:line:col` names where a span starts, not what it covers. Showing
  the source line beneath with the span underlined is the plan's to propose, and serves
  `lem build` and `lem dev` as much as `lem check`.

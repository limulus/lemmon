---
id: ∆Uqg
completed: 2026-10-17 18:11:49 -07:00
---

## Backlog item

> - `∆Uqg` `lem check`
>   - The build without the writing: compile, load and render every page, and print each
>     diagnostic with its span, so an author checks a change without touching the output
>   - needs: `∆muD`

## Planning

The item's brief, folded here. Split from `∆1rD` on 2026-10-17, when Eric moved drafts to
Someday/Maybe (`∆Han`); from `∆4tG` Build pipeline + CLI, by way of `∆1rD`: "`lem check`
prints spanned diagnostics". D33 settles what it prints from: a diagnostic's `range`,
computed in the core. Scope: slice 3's acceptance, in which Eric sees `lem check` report a
deliberate error with its span. `lem build` already printed `path:line:col` and wrote
nothing on an error, so `lem check` is that pipeline without the write, rendering every
page, since a runtime error and two pages at one file are found only there; "with its
span" meant showing what the span covers, not only where it starts.

Planned in the slice-3 run of 2026-10-17 by the main session and reviewed by a read-only
Opus agent, whose probes of `@babel/code-frame` 7.29.7 retired it: it splits lines on
`\r`, U+2028 and U+2029 where the core and the shell's `SourceText` split on `\n` only, so
it frames the wrong line after a lone `\r`; it marks a multi-line span's last line one
column long, draws a caret on the next line for a span ending at a line start (a real
corpus shape, `host/sections/teaser-that-is-no-string.lem:3:1..6:1`), and throws on a
multi-line span past the end; and its v8 raises the engine floor to 24.11. The shims were
as long as a frame, so the frame was written over `SourceText`'s own line model. The same
review moved the frame's text from a re-read of the file to the text the build read,
which a re-read in `lem dev` would race.

## Refinement

Built in `fd7cb4b` by an Opus builder and reviewed by a second read-only Opus agent, the
main session committing after `npm run verify` passed (492 tests). One internal pipeline,
`renderSite`, ends in `check()` or `build()`'s write; `CheckResult.texts` carries every
file the build read once each, by the path its diagnostics name: the walk's documents and
templates, `ambient.lem`, and each data file a binding read. `SourceText.frame(range)` is
41 lines of code: a `>` gutter of line numbers, `^` under what the span covers, a span
ending at a line's start leaving that line out, an empty span one caret, tabs kept under
the carets, control characters and a byte order mark shown as U+FFFD, a span over more
than five lines as its first three and its last, and a line over 100 code units cut to a
window starting 20 before the span, since limulus.net's `svg-sprite.lem` has lines of
2931. The reviewer compiled all 257 corpus files, as written, with CRLF endings, with a
non-ASCII prefix on every line, and without a trailing newline, through the staged core
and compared every frame with the bytes its span covers: none of 4049 differed. The
review round also found a spec that could not fail (it changed a file after the build
rather than during it), a data file's text overwritten when a binding named a walked
document, and docs claiming `lem check` reports everything a build would: a write the
file system refuses is found only by `lem build`. Production +292/−60 lines (173 of code),
tests +559/−13, docs +66/−28.

Later in the slice (`bf34900`), a trap in the core reaches `lem build` and `lem check` as
`CoreTrapped`, printed as one `lem:` line. On the port branch `lem check` says `checked 25
pages and 2 produced files` in about 0.36 s, with eight warnings for code blocks no
handler claims until `∆Nxd`.

## Retrospective

The first choice, a well-used package over our own code, was the rule's, and it was wrong
here: the package's idea of a line disagreed with the core's. A probe of the package
against the core's own spans found that before anything was built, which is what made the
hand-written frame cheap. A spec that claims to prevent a race has to make the race happen,
which the first version of the texts spec did not.

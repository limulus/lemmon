# Drafts

## Background

Split from `∆1rD` on 2026-10-17, when Eric moved drafts to Someday/Maybe: limulus.net has
no drafts today (Eleventy builds every post, and an unfinished one stays uncommitted), so
neither the port nor the cutover needs them. D19: `draft: bool` and a `--drafts` dev flag;
D30: `draft` is a field of the shell's `Page` declaration (`packages/shell/ambient.lem`,
`draft: bool = false`), not a reserved key. No scheduled publishing in static mode.

## Scope and constraints

- Leaving a draft out of the build means leaving it out of its collections too, or a
  draft shows in another page's list with a link to a page that was never written. The
  shell can drop it at discovery (`discover` in `packages/shell/src/lib/site.ts`), before
  anything compiles or loads: the page is then never compiled with its record, never in
  the envelope, never rendered, which the core already allows ("a page the envelope does
  not carry is not loaded and not an error", docs/language.md §Ambient types).
- Reading `draft` from the front matter as parsed is the shell reading its own field.
  Since D19 rules out scheduling, nothing computes `draft`, so the value as written is the
  value as loaded. A value other than `true` is not a draft, and a typed page then fails
  its load for a `draft` that is not a `bool`. An untyped page is bound as sent and not
  validated, so a mistyped `draft` there goes unreported.
- A page template with `draft: true` still compiles in the first call, recordless, as
  every template does, so what it defines stays visible to layouts; only its page is
  dropped.
- Whether `--drafts` is accepted by `lem build` and `lem check` as well as `lem dev`, and
  whether a build says how many drafts it left out, is the plan's to settle.

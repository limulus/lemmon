---
id: ∆1rD
completed: 2026-10-17 14:42:57 -07:00
---

## Backlog item

> - `∆1rD` `lem check` and `--drafts`
>   - `lem check` prints spanned diagnostics without writing; the `draft` field on `Page` and
>     the `--drafts` flag (D19) show a draft in `lem dev` that `lem build` omits
>   - needs: `∆muD`, `∆Vhi`

## Planning

The item's brief, folded here. From `∆4tG` Build pipeline + CLI: "`lem check` prints
spanned diagnostics"; `--drafts` came from the `lem dev` item, where D19 places `draft` as a
field of the shell's `Page` declaration, not a reserved key. Its scope was slice 3's
acceptance as it then stood: Eric runs `lem dev` in the branch, edits a post or a layout and
sees the change in the browser, sees `lem check` report a deliberate error with its span,
and sees a draft under `--drafts` that `lem build` omits.

## Refinement

Split on 2026-10-17, before anything was built, as slice 3 started. While the slice was
being planned Eric made drafts a Someday/Maybe feature: limulus.net has no drafts today
(Eleventy builds every post, and an unfinished one stays uncommitted), so neither the port
nor the cutover needs them. `lem check` stays in slice 3 as `∆Uqg`, its need on `∆Vhi`
dropped, since checking a site does not use the dev server; drafts are `∆Han`, which keeps
that need, since D19 makes `--drafts` a `lem dev` flag. Slice 3's acceptance lost its
draft clause. What planning had found about drafts is in `∆Han`'s brief: the shell can drop
a draft at discovery, before anything compiles, so it leaves its collections too, and
reading `draft` from the front matter as written is sound because D19 rules out anything
computing it. The placeholders naming `∆1rD` now name `∆Uqg` or `∆Han`.

## Retrospective

Nothing was built, so there is little to regret. The draft clause sat in slice 3's
acceptance because D19 settled the flag long before the slices were cut. Nobody asked
whether the port needed drafts, and it did not.

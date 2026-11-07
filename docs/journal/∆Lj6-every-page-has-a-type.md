---
id: ∆Lj6
completed: 2026-11-06 22:44:49 -07:00
---

## Backlog item

> - `∆Lj6` Every page has a type
>   - A page no glob collects and no `type:` names is an error naming the fix, where today it
>     renders without its layout and silently; retires the shell's per-page reload

## Planning

The item's brief, folded here. D30 had a document that no glob matches and no `type:` names
render unknown-typed, and D37 had the loader bind the envelope's document value as sent for
it, one value per load. So such a page rendered its own output with no layout and its front
matter unchecked, and `renderSite` called `loadData` once per page like that, each call
re-sending and re-validating the whole envelope, every rendered body included since D46. It
was a silent fallback: through slice 3 the limulus.net port's `site.lem` collected only
`feed/*/index.md`, and its 18 other documents rendered as bare markdown while `lem check`
stayed clean. The shell already refused a missing `site.lem` for that reason (`SiteError`),
and generators already needed a type (D43). Eric questioned on 2026-10-24 whether untyped
pages should exist, and agreed to remove them from the standard shell: a page with neither
a collecting glob nor a `type:` is an error naming the fix, every page loads once with the
site, and the core may keep rendering unknown-typed documents for hosts with no schemas (a
playground, an editor). The brief measured the change against `b2d4350`: the reload loop and
a non-string url path go, `CompiledPage.record` becomes required, and the shared fixture's
untyped `notes/loose.md` reached 131 specs.

The plan, checked against `9927735` on 2026-11-01:

- The refusal sits in `compileSite`'s page loop, between the two compile calls: the first
  call is the only place the globs exist, and after the second the page has already
  compiled unknown-typed. `untyped()` absorbs D43's refusal, so a page gets one
  diagnostic: D43's at a `for:` key, or the new one at its front matter, or the file's
  start where it has none.
- The fixture's `notes/loose.md` gets a glob rather than a `type:`, through a `Memo` type
  whose layout passes its body through (it reads `m.draft`, or `∆gsV`'s unused-parameter
  warning fires), so its text, spans and output stay the same and the same glob types the
  ad hoc `notes/*.md` pages the specs write. `BARE_LEM` replaces the empty `site.lem` of
  the minimal sites.
- The corpus keeps the core's view: its `host/unmatched/` cases stay, since a schema-less
  host needs the path and the core cannot tell a missing glob from a host with none.
- D48 records it, revising D30 and D37. limulus.net has 40 pages on `lemmon-port` and
  `lemmon-cutover`, none untyped, so nothing there changes.
- Against the brief: about 24 specs rewritten, not 32 to 36; a page-against-page collision
  spec needs distinct urls, since two loaded pages at one url is a core load error; and no
  `artifact-changes.md` entry, since that ledger is frozen.

## Refinement

Built on 2026-11-01 as lemmon `dd01484`, `refactor(shell)!: every page has a type`, with a
`BREAKING CHANGE:` footer naming both fixes. The shared page's output came out
byte-identical under its glob. One perturbation in the plan was inert: dropping the
`continue` after a refusal failed nothing, because the build stops before the second call
on any error, so nothing after it could report again. The builder made a real double report
instead (a typeless generator reported by D43 and then D48), which both D43 specs caught.

The diff's review found:

- A document whose front matter never closes reads as having none, so it was refused as
  untyped, even when its author had written `type:`, and the core's own error about the
  unclosed front matter was never reported. Now such a document goes to the second call
  without a record, and the core's error stops the build.
- The docs said a refused page never reaches the core, but a page template compiles once
  in the first call, with the templates. They now say it is left out of the second call.
- A collision case for a page with no front matter had been lost in the rewrite. It came
  back as a `notes/a.md` with `url: /notes/z/` against a bare `notes/z.md`, rather than
  the pair the review suggested, which would have tripped the core's url check first.
- D37's new sentence read as the premise of a "therefore" that does not follow from it,
  and D46's Status line did not name D48.

`npm run verify` passed before the commit. Eric accepted it on 2026-11-06. His
`npm run parity` on `lemmon-port`, which `∆GJ9` waits on, is still to run.

## Retrospective

The plan's double-report probe was reasoned from the loop's shape, not from where the build
stops, and only running it showed it pinned nothing. The review's real finding was another
recovery path: a mistake in the author's front matter, met before the rule under change,
which the rule then misreported.

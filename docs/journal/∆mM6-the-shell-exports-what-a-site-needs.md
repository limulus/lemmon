---
id: ∆mM6
completed: 2026-11-07 12:29:55 -07:00
---

## Backlog item

> - `∆mM6` The shell exports what a site needs
>   - Its API stops exporting internals such as `walk`, `readData` and the front-matter
>     readers, so the first release promises only what a site's config and plugins use

## Planning

This section folds in the item's brief. On 2026-11-01 Eric decided that the shell's export
list is cut to what a site needs, before `∆PiI` publishes the packages, since removing an
export after a release is a major version and before the first one costs nothing. The
brief counted about 40 names, internals among them, and listed what limulus.net's cutover
branch imports.

No plan file was written; the plan was made in the session on 2026-11-07, against
`c01b71f`. The barrel then exported 53 names, and nothing in this repository imported it,
since the `lem` bin imports the CLI's module. The line it drew:

- keep the 12 names the cutover branch imports, in its config, its plugins and their
  specs;
- keep the names of the types those already expose, 13 of them, since their shapes are
  promised through the kept exports anyway;
- keep the errors `build()` is documented to throw.

`SiteError` and `WriteError`, which the brief listed as internals, stay by the third rule:
`build()` throws the first, and the second is a result's `failure`, so a spec that catches
or checks either needs the class. `Diagnostic`, the type of a result's diagnostics, is not
added; a spec can name it when one needs to, in a minor release.

`check()`, `loadConfig()` and `ConfigError` were left to Eric, who asked for their use
cases. No site calls them from code: limulus.net runs them through `lem check` and
`lem build`, `lem init` (`∆V7i`) scaffolds a site the CLI drives, and the plugin commands
(`∆wOn`, `∆x1B`) load the config inside the CLI. The likely callers from code, the LSP
(`∆5MM`) and the content API (`∆Gg3`), would live in this repository and add what they
need, in the shape they need, as a minor release. Eric dropped all three.

The question turned up a gap. Only `loadConfig()` checked a config's plugins, so a
plugin's spec that hands `build()` a `LoadedConfig` of its own skipped every check that
`lem build` makes, and a plugin could pass its spec and be refused by the site that lists
it. Eric had the fix folded into this item.

## Refinement

Built on 2026-11-07 as two commits:

- `ef10367`, `refactor(shell)!`, cuts the barrel to 27 named exports, with no
  `export *`, of which 7 exist at runtime, down from 18. Its `BREAKING CHANGE:` footer
  names the 26 removed. `docs/shell.md` §Plugins now lists the API by who uses it: the
  config, a plugin, and a plugin's spec.
- `c8e560c`, `fix(shell)`, has `build()` and `check()` run `validatePlugins` before
  anything else, and throw its `TypeError` before any plugin runs.

The check found the gap inside this repository first: two of the shell's own build specs
listed two plugins named `shipping`, which no config file could list, and passed only
because nothing checked. The second is now `other`, as its neighbours' is.

The spec's first case, two producers for one key, did not show the gap: the core's
declaration refuses it too, with a vaguer message. Two plugins of one name, which builds
without the check, does. Perturbed, the specs fail as they should: without the call the
build succeeds, and with it moved after discovery the document transforms run three times
before the refusal.

The diff's review, by a read-only Sonnet agent, found nothing blocking. Two notes were
taken: a `check()` spec for its `@throws` line, and the `Plugin` type's doc, which said
only the config's load refuses an unknown field. Left: a plugin that changes itself after
loading, which is theoretical, and exporting `Diagnostic`.

The evidence:

- `npm run verify` passed before the review and after it, with 890 tests and the fidelity
  check's 575 nodes.
- Against this checkout, the cutover branch typechecks, and its `lem check` checks 59
  pages, 37 asset files and 10 produced files.

Checking the cutover branch also found 9 of its plugin specs failing against lemmon main,
with or without this diff. They had failed since `∆Lj6` (`dd01484`), which landed after
the branch's tip, `28dde17`. Its rule that every page has a type (D48) refused the small
sites the `svgSprite` and `linkPosts` specs build, which wrote an empty `site.lem` above
untyped pages. `∆Lj6`'s journal says nothing on `lemmon-cutover` changes; it had counted
the site's 40 pages and not its specs' sites. Eric had it fixed there: `aa2be3f` on
`lemmon-cutover` gives each spec's site a `Bare` page type whose layout passes its body
through, and the branch's `npm run verify` passes, with 142 tests. The branch stays
unpushed.

Eric accepted the item on 2026-11-07.

## Retrospective

A check that lives only in the loader is skipped by every other way in, and making
`build()` public made that a promise. The shell's own specs had the bug, unnoticed,
because nothing read the plugins they built with.

Counting a consumer's pages is not counting what it runs against the shell: `∆Lj6` read
the site and not its specs. The cutover branch's own `npm run verify`, run against this
checkout, is that check, and it ran only when this item needed it. Run after `∆Lj6`, it
would have caught the failures six days sooner.

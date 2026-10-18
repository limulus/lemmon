---
id: ∆IvY
completed: 2026-10-17 21:33:11 -07:00
---

## Backlog item

> - `∆IvY` svg-sprite plugin
>   - Inline sprite from `assets/svg/{common,photos}` as a mark producer replacing
>     `eleventy-plugin-svg-sprite` and the port's hand-copied `svg-sprite.lem`

## Planning

The item's brief, folded here. From `∆F9l` svg-sprite + dprint plugins: an inline sprite
from `assets/svg/{common,photos}` replacing `eleventy-plugin-svg-sprite`; the dprint half,
`∆Oyi`, was retired. Scope: slice 4's acceptance, Eric builds the branch and all 25
documents, code blocks highlighted and client JavaScript bundled to hashed URLs, compare
clean. When `∆c4j` split, a mark producer was found enough, so its need on the plugin host
went; the port had inlined a hand-copied sprite since slice 1, which a new icon would have
had to be pasted into.

Built on the port branch in the slice-4 run of 2026-10-17 by an Opus agent, reviewed
read-only by another (`a0bc9ec`, after a lockfile resync left over from `∆Vhi` landed as
its own commit). `lemmon/svg-sprite.ts` is a site plugin written only against the shell's
public exports — limulus.net-specific, so it lives in the site, and the first plugin outside
this repo to exercise the API. `svgSprite({mark: directory})` builds each sprite with
svg-sprite 2.0.4, as eleventy-plugin-svg-sprite calls it, and places it where `page.lem`
writes `<lem.mark("sprite")>`; the photos sprite is one more entry, left for `∆hvm`.

## Refinement

- The plan review found two differences a raw svg-sprite run has from what parity
  accepts: the `<style>` WebC hoists out of the `svg-ldn` symbol (the port carries those
  rules in `page.lem`) and the wrapper style's trailing `;`. The plugin strips both.
- svg-sprite prefixes ids inside each file by the order files were added, and `page.lem`'s
  copied rules name `#elogo-p3`. Eleventy's glob adds files reverse-sorted — on every Unix,
  since libuv sorts `readdir` — so the plugin does too. The letter counts the icons sorting
  after `ldn.svg`, so adding or removing one renames the ids: the plugin's spec and parity's
  `css-bundle` catch that and `lem build` does not, and `∆oAl`'s brief says to keep the
  `<style>` in the sprite and delete `page.lem`'s copy once no Eleventy output must match.
- The review found that a producer's failure is a warning per page, so a broken sprite
  shipped every page without icons and exited 0. The sprite is now built in the plugin's
  `assets` hook, which may be async and whose failure stops the build, and the producer
  places what that build's hook made; the hook returns no files. A file svg-sprite drops
  as unreadable is an error naming it, dotfiles are skipped as glob skips them, and the
  cache across `lem dev` rebuilds keys on path, size, mtime and ctime.
- It adds about 90 ms to a cold `lem build`, mostly loading svg-sprite; `lem dev` pays
  that once, and an unchanged rebuild only lists and stats the files.

## Retrospective

The plan's review compared rendered HTML, where the id namespacing was invisible because
both sides happened to say `e`; comparing raw bytes against the reference implementation
found it in one run. Using the `assets` hook as a build's async preparation step is a
stretch of its name, but it is the one hook whose failure stops a build; if another
plugin needs the same, that is a sign the hook wants a broader name.

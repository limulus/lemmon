# Strikethrough in Markdown

## Value

Someone writing for a Lemmon site in the Lemmon app can paste struck-through text into a
post and have it published struck. Until this lands it is published as the text between
two pairs of tildes.

## Background

Eric, 2026-11-15, while deciding what the Lemmon app (`limulus/lemmon-app`) does with
formatted text pasted into a post: a strike is written `~~text~~`. He chose that knowing
the site does not render it yet, and asked for this item so that it will. The app's work is
its backlog item `∆TwX`.

Checked by the agent that wrote this brief, on 2026-11-15, not said by Eric:

- `options()` in `crates/lemmon-vm/src/document/mod.rs` turns on comrak's `footnotes` and
  `header_attributes` and `render.unsafe`, and nothing else. D08's row in
  `docs/decisions/README.md` lists the same.
- The app's editor already draws `~~text~~` struck, so what its writer sees and what the
  site publishes disagree today.
- No Markdown file under limulus.net's `www` has `~~` in it, so turning this on changes no
  page there.

## Scope and constraints

- `~~text~~` in a document's prose renders as struck-through text. That is all Eric has
  decided; anything else the extension brings is not settled here.

## Implementation suggestions

- comrak has a strikethrough extension among its options. Check its name and what else it
  accepts, a single tilde for one, against the comrak version in `Cargo.toml`.
- D08 lists the extensions that are on, so its row changes with this.

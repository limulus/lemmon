# Plugin data, commands and `.lem`

## Background

Split from `∆c4j` Plugin host on 2026-10-17 and placed in slice 5 beside the photos plugin
(`∆pP5`), the first consumer of a plugin's data and of a `lem` subcommand (`lem photos
add`). From the architecture's plugin sketch: `data?(ctx)`, typed via `site.lem`
declarations, and `commands?: Command[]`; plugins ship `.lem` files (functions and types)
bound via `lem-import` in `site.lem` (D23), as the shell ships its own `ambient.lem` (D30),
and cannot inject build-time JS into the language or the core (D06). A plugin claiming a
mark name ships the `lem-fn` wrapping `mark()` for it, so a layout reads `<lem.styles()>`
(D42).

Parked in Someday/Maybe on 2026-10-18, as slice 5 started, with nothing built. Mapping the
slice found no consumer for any part, in slice 5 or at the cutover: photos load as a typed
data file whose derivation `npm run photos` reruns and a spec keeps from drifting (so a
data hook would only replace that script), `npm run photos` needs no Eleventy (so a
`lem photos add`, now `∆x1B`, adds a name, not a capability), and the port's components
are site files, with D42's two mark wrappers written once each. A plugin subcommand would
also need `lem` to load the config before it dispatches, where `cli.ts` refuses an unknown
command before reading it. D23's qualified names are now a deferred question in
`docs/decisions/README.md`, with the options found then. Promote this item when a plugin
ships values or `.lem` a site cannot write itself.

## Scope and constraints

- Confirm at planning that `∆pP5` consumes each part. Data already loads as a typed data
  file (`<lem-let photos: {string: Photo} from "_lemmon/photos.json" />` on the port
  branch, derived by `npm run lemmon:photos`), so the data hook earns its place only by
  replacing that script; plugin `.lem` has no consumer the port shows yet.
- D23's qualified access, `lem.photos.picture(…)`, meets D34, where `.` asks the receiver's
  type and nothing else; docs/language.md lets only an enum name answer `Name.x` first, and
  its own example uses `photos` both as a `from plugin("photos")` binding and as an import
  name (∆bah). Settle that first, with its decision record.
- The core already takes `plugin("…")` sources: parsed, reported as
  `FromBinding.kind: 'plugin'`, fulfilled by `{value}` in `DataInput.sources`, a load
  error placed at the binding (D37). The shell refuses them today (`load.ts`); how a
  binding names a plugin (by `name`, presumably) and what the hook returns (the sketch's
  record of many values, or one value per binding) are open.
- How a plugin's `.lem` reaches `compile` (an absolute path, as `AMBIENT_FILE` does, and a
  plugin tag on the source) and the CLI loading the config before it can tell whether a
  subcommand exists are open too. `lem-import` is an `UnsupportedTag` in the parser today.

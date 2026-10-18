# `lem photos add`

## Background

Split from `∆pP5` Photos plugin on 2026-10-18 and parked with `∆wOn`, whose `commands`
hook it would be the first to use. limulus.net adds photos with `npm run photos`, which
runs `bin/process-photos.js photos/*` (resize with sharp, blurhash, an FTPS upload, then a
rewrite of `www/_data/photos.json`) and, on the port branch, `npm run lemmon:photos`. The
script imports nothing from Eleventy, so the command would add a name under `lem`, not a
capability; it was parked for that reason.

## Scope and constraints

- `process-photos.js` does its work on import, so a command wraps it by spawning it or
  after a refactor into a function.
- `lem`'s `cli.ts` refuses a command other than `build`, `check` and `dev` before it loads
  the config; a plugin's command needs the config loaded first, with its own positionals,
  options and help.
- A photo's alias is minted by `@sindresorhus/slugify`, never the core's `slugify` (the
  journal entry for `∆pP5`).

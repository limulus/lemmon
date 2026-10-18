# Revisions plugin

## Background

Revisions plugin — `revisions.json` as data feeding `lastModified`, plus the
`update-revisions` command reimplemented on the content API instead of spinning up Eleventy
(`11ty/get-articles.ts`)

## Scope and constraints

- Moved from slice 4 to slice 6 on 2026-10-17 (`∆c4j`'s journal entry): the data half
  already works without a plugin — the port branch binds `revisions.json` as a typed data
  file and its layouts read `lastModified` from it — and the first thing to run the
  command is the cutover's deploy job (`.github/workflows/cd.yaml` runs
  `npm run update-revisions` before the build). `lastModified` cannot be a schema default,
  since a default reads no ambient value (D37).
- The command needs each article's computed title, which is `∆Gg3`'s.

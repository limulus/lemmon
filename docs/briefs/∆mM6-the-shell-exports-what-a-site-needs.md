# The shell exports what a site needs

## Value

A site's config and plugins see a small API, and changes inside the shell stop being
changes to what it promises.

## Background

On 2026-11-01 `packages/shell/src/index.ts` exports about 40 names, internals among them:
`walk` and its entry types, `discover` and `Site`, `readData`, the front-matter readers
(`readPage`, `findFrontMatter`, `opensFrontMatter`, `derivedUrl`, `forEntry` and their
types), `SiteError` and `WriteError`, and the parity comparator, which `∆GJ9` removes.

limulus.net's cutover branch (`lemmon-cutover` at `28dde17`) imports `Config`, `Plugin`,
`DocumentSource`, `JsonValue`, `Mark`, `bundler`, `esbuild` and `highlighter`, and in its
plugins' specs `build`, `BuildResult` and `LoadedConfig`.

Once the packages are published, every export is part of what a release promises, and
removing one is a major version; before the first release it costs nothing (Eric,
2026-11-01).

## Scope and constraints

- The shell's export list is cut to what a site needs (Eric, 2026-11-01).
- Lands before `∆PiI` publishes the packages (Eric, 2026-11-01).

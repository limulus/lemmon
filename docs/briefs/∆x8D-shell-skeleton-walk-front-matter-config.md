# Shell skeleton: walk, front matter, config

## Background

### From `∆4tG` Build pipeline + CLI

Build pipeline + CLI — `lem build` walks the input, classifies files, parses front matter
with YAML positions, loads `lemmon.config.ts`, and drives the core across a worker pool:
compile `ambient.lem` + templates without front matter, match documents and page templates
to the reported `from` globs and compile them with their record (D30, D41), loadData, plan,
render; `lem check` prints spanned diagnostics. The shell's backbone

## Scope and constraints

- Slice 1's acceptance, which this item serves: Eric runs `lem build` in the limulus-dot-net
  port branch, opens the eight `feed/` posts from `dist/` in a browser, and `npm run parity`
  there reports them against the 11ty `dist/` with every difference in a named forgiveness
  class.
- Discovery only: the walk, the classification (document, template, data, passthrough),
  front matter parsed into `LoadedValue.spans` with positions, and a `lemmon.config.ts`
  holding input, output and passthrough globs. No plugin list yet.
- No worker pool: the rebuild time is measured in slice 3 before any parallelism is built.

## Implementation suggestions

- The `bin` runs the `src/` entry directly, since Node runs `.ts`, so the port branch can
  depend on this checkout with `file:` and see every edit without a build step.
- `scripts/render-the-rebound.ts` already reads sources and front matter for one page; start
  from what it does.

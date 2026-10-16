# @limulus/lemmon-shell

The Node/TypeScript shell around the Lemmon core (see `docs/architecture.md` §Shape and
§Build pipeline). It does the first step of a build, discovery, and nothing after it yet:
it compiles and renders nothing, and writes no output.

- `loadConfig()` imports a site's `lemmon.config.ts` and validates it: `input` and `output`
  directories, relative to the config file, and `passthrough` globs, relative to `input`
  and with Eleventy's glob semantics (picomatch, dotfiles included, case-insensitive).
- `walk()` lists every file under `input`, sorted by path, as a `document` (`.md`), a
  `template` (`.lem`, marked as a page when it opens with front matter, D41), a
  `passthrough` file (a glob matched it, whatever its extension), or `ignored`. Like
  Eleventy it never enters a `node_modules` or `.git` directory; unlike Eleventy it does
  not follow a symbolically linked directory, so no glob reaches the files under one.
- `readPage()` reads a page's front matter with the `yaml` package into the
  `LoadedValue` the core loads — `type:` and `for:` read and stripped, `url` and
  `inputPath` (the path from the config's directory, `www/feed/x/index.md`) derived where
  it gives none — with a span for every field path, in bytes and UTF-16 positions as the
  core counts them. Bad YAML is a `Diagnostic`, never a throw. `findFrontMatter()` finds
  the block by the core scanner's rule.
- `discover()` does all three and returns the site: entries, pages by path, diagnostics.

```ts
// lemmon.config.ts
import type { Config } from '@limulus/lemmon-shell'

export default {
  input: 'www',
  output: 'dist/www',
  passthrough: ['assets/**/*', '**/*.{png,svg,jpg,jpeg,xsd,xsl,txt,pdf}'],
} satisfies Config
```

`lem build [--config <path>]` discovers the site, prints a summary and every page's url,
and prints each diagnostic as `path:line:col: severity: message`, exiting 1 on an error.

## Running from a checkout

The `lem` bin is `bin/lem.ts`, which Node runs directly, under the `source` export
condition so that this package and `@limulus/lemmon-core` load from their `src/`. A site
can therefore depend on a checkout with `file:` and see every edit without a build:

```json
{
  "devDependencies": { "@limulus/lemmon-shell": "file:../lemmon/packages/shell" }
}
```

npm links the directory and leaves its dependencies to the checkout's own `node_modules`,
`@limulus/lemmon-core` among them, so nothing is fetched. Importing the package from a
site's own code needs the condition too (`node --conditions=source`, and
`customConditions: ["source"]` for its typecheck), or a `npm run build` here first.

# D45 — The esbuild plugin

- **Status:** Accepted 2026-10-17 (`∆d1N`), settled while building it; revises D44, whose
  asset hook is now given the project's root. Revised 2026-10-18 (`∆EEa`) to follow
  limulus.net's Eleventy build, which changed first (its `d1dc734`): a bundle is named by
  what it ships as — its placeholders resolved first, so a loader's name follows its
  worker's, and its map naming the checkout's path relative to itself — and no longer
  by esbuild's `[hash]`
- **Backlog item:** `∆d1N`; `∆EEa` (the revision)
- **Normative text:** `docs/shell.md` § `esbuild(options?)`, § Assets;
  `docs/architecture.md` § Build pipeline (step 2), § Shell plugin API

## Context

D09 settled esbuild through its JS API with limulus.net's conventions ported 1:1, and D10
kept the `XXXXXXXX` placeholder in client TypeScript; D38 put the manifest on the host
declaration as finished URLs, and D44 built the `assets` hook to carry it. What was left
was the plugin itself, and limulus.net's parity harness compares script URLs exactly, so
its bundles have to be byte for byte the ones `11ty/esbuild.ts` writes: every name in
them is a hash of what esbuild was given. _(Revised by `∆EEa`: of what they ship as.)_

Measured on the port branch, three things move every hash. The esbuild release, and the
wasm loader's, since both write into the bundle. The output directory's depth, since a
source map names each source by its path from the bundle. And — found only by probing —
esbuild's working directory: with every byte of a bundle and its map unchanged, its
`[hash]` changes when `absWorkingDir` does, because esbuild hashes each source by its
path from there. Eleventy runs from the project's root; `lem` can be started anywhere,
and D44's context gave a hook the input and output directories, not the root.
_(Revised by `∆EEa`: those three moved esbuild's `[hash]`, which no longer names a bundle.
Of them, the releases still move a name, and the output's depth, since the map is
hashed with the bundle; the working directory does not.)_

## Decision

**The shell ships `esbuild(options?)`, an `assets` hook, with D09's conventions as its
defaults** — every `index.ts` and `worker.ts` under the input an entry, `/lib/*` left to
the browser, a `.wasm` import instantiated by `esbuild-plugin-wasm`, each bundle written
beside its source as `[dir]/[name].[hash]` — minified ES modules with source maps. A site
passes what is its own: entry globs, externals, entry and asset names, and `env`, the
environment variables its scripts read as `process.env.NAME`. The asset names default to
`/assets/immutable/esbuild/`; limulus.net passes Eleventy's `assets/immutable/11ty/esbuild/`
to keep its `.wasm` URL, and with it the hash of the worker that names it.

**esbuild 0.24.0 and `esbuild-plugin-wasm` 1.1.0 are pinned exactly.** 0.24.0 is what
limulus.net's Eleventy build runs, not the 0.28 the architecture first named; the loader
writes its own code into a worker's bundle.

**The hook is given the project's root, and esbuild runs from it.** `AssetsContext` gains
`root`, the directory of the config file, which `input` and `output` are relative to, so
a bundle's name is a function of the project and never of the directory `lem` was started
in. The hook resolves `root`, `input` and `output` through symbolic links first, as
esbuild resolves each source, so a project reached through a link — `/tmp` on macOS is
one — builds the same bytes. _(Revised by `∆EEa`: a name is now a hash of the bundle and
its map, and neither holds the root's path where esbuild writes it as it is, so the root
moves no name; esbuild still runs from it so that its messages name each source from the
root, and the map's checkout path is made relative from it.)_

**The manifest is keyed by the URL an entry's source names**: `www/lib/x/index.ts` is
`/lib/x/index.js`, as `11ty/esbuild.ts` keys its map, mapped to the URL the bundle ships
at. A CSS bundle an entry's imports produce is mapped beside it, `/lib/x/index.css`. Two
bundles at one logical path — two entries, or an entry's styles and a CSS entry — are an
error naming both sources.

**Each bundle is named by what it ships as, as Eleventy names it** _(revised by `∆EEa`;
until then placeholders resolved after esbuild's hashing, and a bundle's name did not
change when only a bundle it names did)_. esbuild writes each entry's bundle with the
placeholder `XXXXXXXX` where `[hash]` is in `entryNames`, as `11ty/esbuild.ts` writes
`[dir]/[name].XXXXXXXX`, and its own `[hash]` after the name, which keeps two bundles
apart until they are named — an `entryNames` without `[dir]` would give two `index.ts`
one path — and is cut off before anything is hashed. Then each bundle is named, and a
bundle named by a placeholder is named first:

- **Its placeholders are resolved.** A string naming `….XXXXXXXX.js` names the entry at
  that path — from `/` where it starts with `/`, from the bundle's own logical path where
  it does not — and the file name that entry ships under is spliced into the string as
  written, as Eleventy splices the hash in: `'worker.…'` stays without a `./`, and the
  string keeps its length, so the map's columns hold. Only where `entryNames` has moved
  the bundles, so that the spliced string would name another file, does it become the
  URL the entry ships at, relative to the bundle's own where it was relative.
- **Its map names the checkout's path relative to itself.** Every `<root>/` in the map's
  text becomes the way from the map's directory to the root, as `11ty/esbuild.ts`
  rewrites its working directory — `./` from a map in the root itself, where Eleventy's
  would write `/`: `esbuild-plugin-wasm` names its module by an absolute path, in the
  sources and in the import line of the code it writes, which the map holds. A root
  holding a `"`, a `\` or a character outside ASCII is escaped in the map's JSON, where
  the text is not found, and stays absolute, as it does in Eleventy's.
- **Its `[hash]` is the first eight characters of the RFC 4648 base32 of the SHA-256 of
  the bundle and its map**, as UTF-8, one after the other, with the comment that ends the
  bundle still naming its map `….XXXXXXXX.js.map`; that comment alone is renamed after.
  The styles an entry's imports gather are named the same way, by what they ship as,
  where Eleventy does not name them: it writes every output but an entry's own as
  esbuild left it, so it would ship them as `index.XXXXXXXX.css`. limulus.net's scripts
  import no CSS, so parity never sees the difference.

A placeholder naming no entry fails the hook at the source that wrote it — found in the
bundle's map, and placed as esbuild places its own errors, from the root with its line
and column — or at the bundle where no source holds it, as when a `define` put it there.
So does one that names its own bundle, directly or through the bundles it names, since
each would be named by the other's name: the error names the cycle alone,
`` `/lib/b/index.js` -> `/lib/c/index.js` -> `/lib/b/index.js` ``, where Eleventy's lists
every bundle being named when it closed.

**No unhashed stub is written.** `11ty/esbuild.ts` writes `/lib/x/index.js` re-exporting
the hashed bundle beside every entry, added when hashing came in; every reference in its
output is a translated placeholder, and in Lemmon's an `asset()` or a placeholder, so
nothing loads one.

**A variable `env` names that the environment lacks fails the hook, naming it.** esbuild
would throw on the `undefined` define anyway, with a message about a define value rather
than a variable; the build's environment is the frozen copy D44 hands the hook, so a site
loads `.env` in its config and the hook sees it.

**esbuild's errors fail the hook; its warnings go to standard error.** The hook runs
esbuild silent: an error throws esbuild's own message, with its file and position, which
D44 reports at the config's start. A warning has no channel in `Assets`, so the plugin
writes esbuild's own formatting of it to standard error and the build goes on.

**esbuild is loaded when the hook first runs.** The shell's entry point re-exports the
plugin, so a module-level import would load esbuild for every config that imports a value
from the shell, and fail the config of a checkout without it; loaded in the hook, only a
site listing the plugin pays.

## Alternatives considered

- **The wasm loader as an option the site passes** (`plugins: [wasmLoader()]`). Keeps a
  third-party loader out of the shell; but D09 names the loader among the conventions
  that port, it is 28 KB with no dependencies, and inert until a script imports a
  `.wasm`. Built in; a `plugins` option stays additive.
- **esbuild from `process.cwd()`.** What Eleventy does, and byte-identical when `lem`
  runs from the project's root — and different names when it does not, the same site
  shipping other URLs by where a shell was. Rejected. _(Revised by `∆EEa`: the working
  directory no longer moves a name; run from it, esbuild's messages would name the
  sources from wherever a shell was, and a map's path would be made relative to it,
  absolute where the root is not under it.)_
- **`dirname(input)` as the root.** Right for limulus.net, whose input is a child of the
  config's directory, and a coincidence for any other. Rejected for the context field.
- **A `root` option the site passes.** Every site would have to write
  `fileURLToPath(new URL('.', import.meta.url))` to get what the shell already knows.
- **Writing the stubs.** File-set parity with Eleventy's `dist/`; but they are 18 URLs
  nothing loads, and an unhashed URL is what `asset()` exists to keep out of pages.
- **Resolving placeholders before hashing**, so a loader's name follows its worker's.
  Right for a CDN that keeps every hashed name forever, and a different name for every
  bundle that names another, so the port could not compare clean. Not built; the
  consequence is recorded below. _(Built by `∆EEa` once limulus.net's Eleventy build
  named its bundles so, the two compared byte for byte again.)_
- **`entryNames` with the placeholder alone where `[hash]` is**, as `11ty/esbuild.ts`
  passes `[dir]/[name].XXXXXXXX` (`∆EEa`). Byte for byte the same names, and esbuild
  refuses two outputs at one path before anything is named: an `entryNames` without
  `[dir]` gives every `index.ts` one, and an entry's styles and a CSS entry beside it
  share theirs, which the manifest's check names better. esbuild's hash after the name
  costs a cut of nine characters.
- **The whole chain in a cycle's error**, as Eleventy writes it (`∆EEa`). The bundles that
  led into the cycle are not in it, and renaming them fixes nothing.
- **Writing every placeholder as the absolute URL.** Simpler, and a relative
  `new URL('./worker.….js', import.meta.url)` would change bytes Eleventy writes.
- **`define` passed through, values computed in the config.** The config runs before the
  hook is given its environment, so the site would read `process.env` itself, and an
  unset variable would reach esbuild as the `undefined` it refuses.
- **esbuild's own logging.** Its errors would print twice, once from esbuild and once as
  the hook's failure, and outside every test's reach, since esbuild writes them from its
  child process.
- **A `diagnostics` field on `Assets`**, each warning and error with the file and
  position esbuild gives it, so that the shell prints them through its `Io` with every
  other diagnostic, counted, rather than a warning written to standard error past `Io`
  and every error folded into one at the config's start. The way forward, and a change to
  D44's contract every asset hook shares; not built here.
- **Writing a relative placeholder from the bundle's own URL always.** One rule rather
  than two, and `'worker.XXXXXXXX.js'` would become `"./worker.….js"`, a string Eleventy
  never wrote, two bytes longer, moving every map column after it.

## Consequences

- The shell depends on `esbuild` 0.24.0 — 148 KB of JavaScript and one native binary of
  about 9.3 MB for the platform, from 25 optional packages the lockfile lists — and on
  `esbuild-plugin-wasm` 1.1.0, 28 KB with no dependencies, both loaded the first time
  the hook runs. npm 12 blocks esbuild's postinstall unless allowed, and the JS API runs
  without it.
- On the port branch the hook's 37 files — 18 bundles, their maps, and the `.wasm` — are
  byte for byte Eleventy's, called at the branch's own paths with limulus.net's `11ty/`
  asset names. A copy of the site elsewhere matches 34: the `.wasm` worker's map names
  its module as `wasm-module:<absolute path>`, so its hash, and its loader's bytes,
  depend on where the checkout is — Eleventy's as much as Lemmon's. _(Revised by `∆EEa`:
  that path is relative on both sides now, since the branch merged limulus.net's
  `d1dc734`, and a copy of the site elsewhere matches all 37.)_
- Every build bundles again: about 0.2 s of each `lem dev` rebuild on the port branch
  (0.17 s without the plugin, 0.37 s with). Nothing caches; esbuild's incremental
  `context()` is `∆xBO`'s question.
- Nothing removes stale output, so each `lem dev` edit to a script leaves its earlier
  bundles in the output beside the new ones.
- A loader's name does not follow its worker's: an edit to a worker alone ships a new
  worker and the loader under its old name, with new bytes. limulus.net's CloudFront
  serves every `/*.????????.*` as immutable for a year, so a browser holding the old
  loader goes on asking for the old worker. Eleventy ships the same today, and the port
  inherits it unchanged (D13); the cutover (`∆oAl`) is where to weigh it. _(Revised by
  `∆EEa`: fixed on limulus.net's main first, and followed here, so an edit to a worker
  renames every bundle that names it, through any chain of names.)_
- A bundle cannot name itself, nor two bundles each other: each would need the other's
  name to be named (`∆EEa`). A worker that starts another of itself names its own URL at
  run time, `import.meta.url`, not by a placeholder.
- `∆Zg9`'s whole-site comparison meets 18 stubs in Eleventy's `dist/` that Lemmon's lacks.

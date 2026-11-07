// What a site's config, its plugins and their specs use, and the types those name
// (docs/shell.md §Plugins): every name here is promised, and one taken back is a major
// release, so nothing is exported that no site uses.

// What a plugin's hooks are given, so that a site's config can type them from here.
export type { Fragment, JsonValue, Mark, MarkContext } from '@limulus/lemmon-core'

export {
  build,
  CoreTrapped,
  type BuildOptions,
  type BuildResult,
  type CheckResult,
} from './lib/build.ts'
export { bundler } from './lib/bundler.ts'
export type { Config, DevConfig, LoadedConfig } from './lib/config.ts'
export { esbuild, type EsbuildOptions } from './lib/esbuild.ts'
export { highlighter, type HighlighterOptions } from './lib/highlighter.ts'
export type { Environment } from './lib/load.ts'
export type {
  Assets,
  AssetsContext,
  DecoratorContext,
  DocumentSource,
  Plugin,
  PluginHandler,
  ProducerContext,
} from './lib/plugins.ts'
export { SiteError } from './lib/program.ts'
export { WriteError } from './lib/write.ts'

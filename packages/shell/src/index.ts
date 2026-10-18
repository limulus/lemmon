// What a plugin's hooks are given, so that a site's config can type them from here.
export type { Fragment, JsonValue, Mark, MarkContext } from '@limulus/lemmon-core'

export {
  build,
  check,
  CoreTrapped,
  type BuildOptions,
  type BuildResult,
  type CheckResult,
} from './lib/build.ts'
export { bundler } from './lib/bundler.ts'
export { highlighter, type HighlighterOptions } from './lib/highlighter.ts'
export {
  ConfigError,
  loadConfig,
  type Config,
  type DevConfig,
  type LoadedConfig,
} from './lib/config.ts'
export * from './lib/front-matter.ts'
export type { Environment } from './lib/load.ts'
export type {
  Assets,
  AssetsContext,
  DocumentSource,
  Plugin,
  PluginHandler,
  ProducerContext,
} from './lib/plugins.ts'
export { SiteError } from './lib/program.ts'
export * from './lib/rendered-html.ts'
export * from './lib/site.ts'
export {
  walk,
  type DocumentEntry,
  type Entry,
  type EntryKind,
  type IgnoredEntry,
  type PassthroughEntry,
  type TemplateEntry,
} from './lib/walk.ts'
export { WriteError } from './lib/write.ts'
export { readData, type DataRead } from './lib/yaml-values.ts'

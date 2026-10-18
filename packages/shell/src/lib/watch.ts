import { watch as chokidar, type FSWatcher } from 'chokidar'

import type { LoadedConfig } from './config.ts'
import { skipped } from './walk.ts'

/**
 * A watch on a site's input, until it is closed, with the files and directories it
 * watches: by each directory, the names in it.
 */
export type Watcher = Pick<FSWatcher, 'close' | 'getWatched'>

/**
 * Watches the config's input directory, resolving once it is watched: `changed` is
 * called with the path of each file or directory added, changed, or removed there that
 * the walk does not pass over ({@link skipped}), and `failed` with each error the watch
 * meets — a directory it may not read, say — after which it goes on watching the rest.
 * A symbolic link is watched as itself, so an edit reaches a build through the file it
 * names only where that file is under the input too.
 *
 * chokidar rather than Node's recursive `fs.watch`, which on Linux misses every edit
 * after an editor's atomic save (a temporary file renamed over the original) and every
 * edit under a renamed directory.
 */
export async function watch(
  config: Pick<LoadedConfig, 'input' | 'output'>,
  changed: (path: string) => void,
  failed: (error: unknown) => void
): Promise<Watcher> {
  const watcher = chokidar(config.input, {
    ignoreInitial: true,
    ignored: (path) => skipped(config, path),
    // As the walk, which follows no linked directory: one leading to the output would
    // report every file a build writes as a change, and rebuild without end.
    followSymlinks: false,
  })
  watcher.on('all', (_event, path) => {
    changed(path)
  })
  // An emitter with no `error` listener throws what it emits, which would end `lem dev`
  // over one unreadable directory.
  watcher.on('error', failed)
  // Not `once()` from node:events, which rejects on the first `error`: the watch goes on
  // after one, and is ready once it has read what it can.
  await new Promise<void>((resolve) => {
    watcher.once('ready', resolve)
  })
  return watcher
}

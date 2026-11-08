#!/usr/bin/env node
// Plain JavaScript, since Node strips no types from a file under `node_modules`. `#cli` is
// this package's source under the `source` condition, which a checkout runs with, and its
// build otherwise, which is what an installed package runs.
import { run } from '#cli'

process.exitCode = await run(process.argv.slice(2), {
  stdout: process.stdout,
  stderr: process.stderr,
  cwd: () => process.cwd(),
  env: process.env,
  // Installed only when `lem dev` asks, so that Ctrl-C ends any other command as it
  // would any program; and removed by the first signal, so that a second ends `lem dev`
  // that way too.
  interrupted() {
    const controller = new AbortController()
    const signals = ['SIGINT', 'SIGTERM']
    const stop = () => {
      for (const signal of signals) process.off(signal, stop)
      controller.abort()
    }
    for (const signal of signals) process.on(signal, stop)
    return controller.signal
  },
})

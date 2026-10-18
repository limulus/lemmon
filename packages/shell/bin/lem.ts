#!/usr/bin/env -S node --conditions=source
import { run } from '../src/lib/cli.ts'

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
    const signals = ['SIGINT', 'SIGTERM'] as const
    const stop = () => {
      for (const signal of signals) process.off(signal, stop)
      controller.abort()
    }
    for (const signal of signals) process.on(signal, stop)
    return controller.signal
  },
})

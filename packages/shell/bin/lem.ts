#!/usr/bin/env -S node --conditions=source
import { run } from '../src/lib/cli.ts'

process.exitCode = await run(process.argv.slice(2), process)

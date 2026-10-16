import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { onTestFinished } from 'vitest'

/**
 * A fresh directory under the system's temporary one, holding `files` — keyed by paths
 * relative to it, with `/` separators — and removed when the test finishes.
 */
export function tempDir(files: Record<string, string> = {}): string {
  const dir = mkdtempSync(join(tmpdir(), 'lemmon-shell-'))
  onTestFinished(() => {
    rmSync(dir, { recursive: true, force: true })
  })
  for (const [path, text] of Object.entries(files)) {
    const file = join(dir, ...path.split('/'))
    mkdirSync(dirname(file), { recursive: true })
    writeFileSync(file, text)
  }
  return dir
}

/**
 * The rule both wasm harnesses apply to a boundary call's diagnostics: which of them
 * fails the run. One definition, because the last change to it (`∆VNk`, severity and
 * labels) had to be made twice — the two scripts held byte-identical copies.
 */

import type { Diagnostic } from '../../packages/core/src/index.ts'

/**
 * Throws when any diagnostic is an error, naming what the call was doing. A warning does
 * not fail a compile and is not a harness's concern (`∆VNk`).
 */
export function expectClean(diagnostics: Diagnostic[], doing: string): void {
  const errors = diagnostics.filter((d) => d.severity === 'error')
  if (errors.length > 0) {
    throw new Error(`unexpected diagnostics ${doing}: ${JSON.stringify(errors)}`)
  }
}

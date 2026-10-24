/** The one test of a diagnostic list the shell stops on: whether it holds an error. */

import type { Diagnostic } from '@limulus/lemmon-core'

/** Whether any of `diagnostics` is an error. */
export function hasError(diagnostics: Diagnostic[]): boolean {
  return diagnostics.some((diagnostic) => diagnostic.severity === 'error')
}

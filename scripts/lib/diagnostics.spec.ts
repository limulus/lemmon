import { describe, expect, it } from 'vitest'

import { expectClean } from './diagnostics.ts'
import type { Diagnostic, Severity } from '../../packages/core/src/index.ts'

/** A diagnostic in the shape the boundary returns one, spans and all. */
function diagnostic(severity: Severity, message: string): Diagnostic {
  return {
    path: 'render/text/statement-lines.lem',
    span: {
      start: 20,
      end: 24,
      range: { start: { line: 19, character: 9 }, end: { line: 19, character: 13 } },
    },
    severity,
    message,
    labels: [],
  }
}

const error = diagnostic('error', 'unknown type `hero`')
const warning = diagnostic('warning', '`last` is never used')

describe('expectClean()', () => {
  it('should not throw when there are no diagnostics at all', () => {
    expect(() => {
      expectClean([], 'compiling nothing')
    }).not.toThrow()
  })

  it('should not throw when every diagnostic is a warning', () => {
    expect(() => {
      expectClean([warning, warning], 'compiling a warning case')
    }).not.toThrow()
  })

  it('should throw when a diagnostic is an error, naming what the call was doing', () => {
    expect(() => {
      expectClean([error], 'compiling render/text/plain.lem')
    }).toThrow(/compiling render\/text\/plain\.lem/)
  })

  it('should throw when errors and warnings are mixed', () => {
    expect(() => {
      expectClean([warning, error], 'compiling the site')
    }).toThrow()
  })

  it('should carry the offending diagnostics in the thrown message', () => {
    expect(() => {
      expectClean([error], 'rendering a page')
    }).toThrow(/unknown type `hero`/)
  })

  it('should not carry a warning that rode along with an error', () => {
    let thrown = ''
    try {
      expectClean([warning, error], 'compiling the site')
    } catch (caught) {
      thrown = (caught as Error).message
    }
    expect(thrown).not.toMatch(/is never used/)
  })
})

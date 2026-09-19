import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'

import { cleanFixtures, type CorpusFixture } from './corpus-fixtures.ts'

const MARKER = '--- diagnostics'
const PAGE = '<h1>top!</h1>'

// Real trailer lines, copied from the corpus rather than invented: an error and a
// warning as `lemmon_vm::Diagnostic`'s `Display` writes them, and a secondary label as
// it indents one.
const ERROR = 'render/errors/case-refusals.lem:16:13..16:17: unknown type `hero`'
const WARNING =
  'render/text/statement-lines.lem:20:10..20:14: warning: `last` is never used'
const LABEL = '  render/errors/compile.lem:1:9..1:10: first defined here'
// A span in another file stays in bytes, since the harness has nothing to place it
// against (tests/corpus/README.md). This is the text of
// `host/errors/template-typed-by-a-record-that-reads-body.html`'s label, as a
// diagnostic of its own would carry it.
const BYTE_SPAN = 'host/environment/site.lem:516..520: reads `body` here'

/** An expectation in the corpus's shape: the rendered page, then the trailer. */
function expectation(trailer: string[], page = PAGE): string {
  return `${[page, '', MARKER, ...trailer].join('\n')}\n`
}

function fixture(path: string, trailer: string[], page?: string): CorpusFixture {
  return { path, text: `<h1>${path}</h1>`, expectation: expectation(trailer, page) }
}

/** Whether a case with this trailer is one a harness may compile. */
function keeps(trailer: string[], page?: string): boolean {
  return cleanFixtures([fixture('render/text/plain.lem', trailer, page)]).length === 1
}

describe('cleanFixtures()', () => {
  describe('what the trailer says', () => {
    it('should keep a case whose trailer is empty', () => {
      expect(keeps([])).toBe(true)
    })

    it('should drop a case whose trailer holds an error', () => {
      expect(keeps([ERROR])).toBe(false)
    })

    it('should keep a case whose trailer holds only warnings', () => {
      expect(keeps([WARNING, WARNING, WARNING])).toBe(true)
    })

    it('should drop a case whose trailer mixes a warning with an error', () => {
      expect(keeps([WARNING, ERROR, WARNING])).toBe(false)
    })
  })

  describe('a directory name, which decides nothing', () => {
    it('should drop a case under a feature directory whose expectation errors', () => {
      // ∆Bhz: `document/front-matter/unclosed.md` became an error case in place.
      expect(
        cleanFixtures([fixture('document/front-matter/unclosed.md', [ERROR])])
      ).toEqual([])
    })

    it('should keep a case under an errors/ directory whose expectation is clean', () => {
      expect(cleanFixtures([fixture('render/errors/still-renders.lem', [])])).toEqual([
        {
          path: 'render/errors/still-renders.lem',
          text: '<h1>render/errors/still-renders.lem</h1>',
        },
      ])
    })
  })

  describe('finding the trailer', () => {
    it('should read only the lines after the last marker', () => {
      // A rendered page is free to print the marker as ordinary output.
      expect(keeps([], `${PAGE}\n${MARKER}\n${ERROR}`)).toBe(true)
    })

    it('should read the last marker even when an earlier section is clean', () => {
      expect(keeps([ERROR], `${PAGE}\n${MARKER}`)).toBe(false)
    })

    it('should ignore the blank lines in a trailer', () => {
      expect(keeps(['', WARNING, '', WARNING, ''])).toBe(true)
    })

    it('should throw, naming the path, for an expectation with no trailer', () => {
      const orphan = { path: 'render/text/plain.lem', text: PAGE, expectation: PAGE }
      expect(() => cleanFixtures([orphan])).toThrow(/render\/text\/plain\.lem/)
    })
  })

  describe('reading one diagnostic line', () => {
    it('should skip a secondary label, which is not a diagnostic of its own', () => {
      expect(keeps([WARNING, LABEL])).toBe(true)
    })

    it('should still drop the error a secondary label belongs to', () => {
      expect(keeps([ERROR, LABEL])).toBe(false)
    })

    it('should read an error whose span is bytes and whose path is another file', () => {
      expect(keeps([BYTE_SPAN])).toBe(false)
    })

    it('should read a warning whose span is bytes and whose path is another file', () => {
      expect(
        keeps(['host/environment/site.lem:516..520: warning: reads `body` here'])
      ).toBe(true)
    })

    it('should read the severity of a line whose message holds `..`', () => {
      expect(keeps(['render/text/plain.lem:2:5..2:9: unknown name `a..b`'])).toBe(false)
      expect(keeps(['render/text/plain.lem:2:5..2:9: warning: `a..b` is never used'])).toBe(
        true
      )
    })

    it('should read the severity of a line whose path holds `..`', () => {
      expect(keeps(['../shared/site.lem:2:5..2:9: warning: `last` is never used'])).toBe(
        true
      )
      expect(keeps(['../shared/site.lem:2:5..2:9: `last` is never bound'])).toBe(false)
    })

    it('should not read a `warning: ` later in an error message as the severity', () => {
      expect(
        keeps(['render/errors/marks.lem:1:35..1:43: no handler claims `warning: x`'])
      ).toBe(false)
    })

    it('should throw, naming the path and the line, for a line with no span', () => {
      expect(() => keeps(['the stage said something with no span at all'])).toThrow(
        "render/text/plain.lem: its expectation's trailer holds a line with no span: " +
          'the stage said something with no span at all'
      )
    })

    it('should throw for a line whose only `..` is in its message', () => {
      expect(() => keeps(['render/text/plain.lem: unknown name `a..b`'])).toThrow(
        'a line with no span'
      )
    })

    it('should throw for a line whose span is followed by no message', () => {
      expect(() => keeps(['render/text/plain.lem:2:5..2:9'])).toThrow('a line with no span')
    })
  })

  // The two literals this module reads by are copies of Rust constants. These cases read
  // what the Rust side really wrote, so a change to either constant fails here once the
  // corpus is blessed again, where a count of kept fixtures would only shrink.
  describe('the corpus as it is blessed', () => {
    const blessed = (path: string): CorpusFixture => ({
      path,
      text: '',
      expectation: readFileSync(
        new URL(`../../tests/corpus/${path.replace(/\.lem$/, '.html')}`, import.meta.url),
        'utf8'
      ),
    })

    it('should keep a case the corpus blessed with warnings alone', () => {
      expect(cleanFixtures([blessed('render/warnings/unused.lem')])).toHaveLength(1)
    })

    it('should drop a case the corpus blessed with errors', () => {
      expect(cleanFixtures([blessed('render/errors/misspelled-field.lem')])).toHaveLength(0)
    })
  })

  describe('what it returns', () => {
    it('should keep the inputs in the order it was given them', () => {
      const given = [
        fixture('render/text/a.lem', []),
        fixture('render/errors/b.lem', [ERROR]),
        fixture('document/prose/c.md', [WARNING]),
      ]
      expect(cleanFixtures(given).map(({ path }) => path)).toEqual([
        'render/text/a.lem',
        'document/prose/c.md',
      ])
    })

    it('should return the input alone, without the expectation riding along', () => {
      // An unknown field in a `compile` payload is refused by the boundary's serde.
      expect(cleanFixtures([fixture('render/text/a.lem', [])])).toEqual([
        { path: 'render/text/a.lem', text: '<h1>render/text/a.lem</h1>' },
      ])
    })
  })
})

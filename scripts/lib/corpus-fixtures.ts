/**
 * Which corpus inputs a harness may compile and render clean, decided from each case's
 * blessed diagnostics rather than from its directory name.
 *
 * `tests/corpus/README.md` says it plainly: whether a case is clean is _data_, and the
 * directory a case sits in says what it is about, not what it produces — `host/pages/`
 * holds error cases, and a case that a language change turns from clean to broken stays
 * where it is. A harness that reads the path instead compiles a deliberate error and
 * dies on it, which is ∆Bhz: `document/front-matter/unclosed.md` became an error case in
 * place, and the benchmark broke at the commit that blessed it and stayed broken until
 * someone ran it by hand.
 *
 * The file system stays in the caller: this module is given the text of each input and
 * of its expectation, so its spec needs no directory of its own.
 */

/** One corpus input, as the boundary knows the file. */
export interface CorpusInput {
  /** Corpus-relative path, as the boundary knows the file. */
  path: string
  /** The input's text. */
  text: string
}

/** A corpus input beside the expectation the corpus blessed for it. */
export interface CorpusFixture extends CorpusInput {
  /** The blessed `.html` expectation beside the input. */
  expectation: string
}

// `lemmon_syntax::DIAGNOSTICS_TRAILER` and `lemmon_syntax::WARNING_PREFIX`
// (crates/lemmon-syntax/src/diagnostic.rs). A TypeScript reader cannot import them, so
// these are copies; change them together.
const DIAGNOSTICS_TRAILER = '--- diagnostics'
const WARNING_PREFIX = 'warning: '

/** What separates a diagnostic's locator from its message, and a label's from its note. */
const MESSAGE_SEPARATOR = ': '

/** What a secondary label is indented by (`lemmon_vm::Diagnostic`'s `Display`). */
const LABEL_INDENT = '  '

/**
 * The trailer's diagnostic lines: what follows the **last** marker line, blank lines and
 * secondary labels dropped.
 *
 * The last, because a rendered page is free to print the marker as ordinary output and
 * the trailer a stage appends is always the final one — the rule
 * `lemmon_corpus::trailer_start` applies, and one a reader here has to mirror rather
 * than observe, since no case in the corpus prints it today.
 */
function diagnosticLines(expectation: string, path: string): string[] {
  const lines = expectation.split('\n')
  const marker = lines.lastIndexOf(DIAGNOSTICS_TRAILER)
  if (marker === -1) {
    throw new Error(
      `${path}: its expectation has no \`${DIAGNOSTICS_TRAILER}\` trailer, so it is ` +
        `not one`
    )
  }
  return lines
    .slice(marker + 1)
    .filter((line) => line !== '' && !line.startsWith(LABEL_INDENT))
}

/**
 * Whether a diagnostic line is an error, read as `lemmon_corpus::split_trailer_line` reads
 * one: `path:span: [warning: ]message` (`crates/lemmon-vm/src/diagnostics.rs`), where the
 * locator is everything before the first `': '` and holds the span's `'..'`. That accepts
 * both span forms — `line:col..line:col` against the case's own source, `start..end`
 * bytes when the line names another file — without parsing either.
 *
 * Throws, naming the path and the line, for a line with no span before its message.
 * Invariant 8 says no diagnostic is without one and the corpus gate refuses to bless such
 * a line, so it is a damaged expectation; reading it as an error would drop the case and
 * shrink what the harness covers with nothing said.
 */
function isError(line: string, path: string): boolean {
  const separator = line.indexOf(MESSAGE_SEPARATOR)
  if (separator === -1 || !line.slice(0, separator).includes('..')) {
    throw new Error(`${path}: its expectation's trailer holds a line with no span: ${line}`)
  }
  return !line.slice(separator + MESSAGE_SEPARATOR.length).startsWith(WARNING_PREFIX)
}

/**
 * The fixtures whose blessed expectation shows no error — the ones a harness may compile
 * and render without a diagnostic. Warnings do not disqualify a case (∆VNk): cases
 * under `render/` and `document/` warn and render clean, some of them outside any
 * `warnings/` directory.
 *
 * Throws, naming the path, for an expectation with no diagnostics trailer, and for a
 * trailer line with no span: every `.html` the corpus blesses has the one and none has
 * the other, so either is a damaged file, and dropping the case would hide it.
 */
export function cleanFixtures(fixtures: CorpusFixture[]): CorpusInput[] {
  const clean: CorpusInput[] = []
  for (const { path, text, expectation } of fixtures) {
    // Rebuilt rather than passed through, so the expectation cannot ride along into a
    // `compile` payload, where the boundary's serde refuses an unknown field.
    const lines = diagnosticLines(expectation, path)
    if (!lines.some((line) => isError(line, path))) clean.push({ path, text })
  }
  return clean
}

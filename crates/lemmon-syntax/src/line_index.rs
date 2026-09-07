//! Turning a byte offset into the line and column a person — or an editor — reads.
//!
//! The core speaks in byte [`Span`]s and always will (invariant 8): they are what the
//! scanner produces, and what makes a diagnostic reproducible. But nothing downstream
//! displays a byte offset. The corpus trailers need a line and column to be reviewable by
//! eye, `lem check` needs one to print, and the LSP needs a [`Position`]. Doing that
//! conversion here — once, in the core, where the bytes are — is what keeps the shell and
//! the language server from each re-deriving it in JavaScript, where a byte offset is the
//! awkward direction to convert from.
//!
//! The arithmetic is rust-analyzer's `line-index`, which stores line starts and a map of
//! the non-ASCII characters on each line rather than the text itself, so a resident index
//! costs nothing for an all-ASCII file. What this module adds is the naming, and the
//! refusal: `line-index` reports a bad offset as `None`, and here that is a panic, because
//! an offset that is not a character boundary in this file did not come from its scanner.

use line_index::{TextSize, WideEncoding};

use crate::Span;

/// A place in a file, in the shape the Language Server Protocol defines.
///
/// **Both fields are zero-based**, and `character` counts UTF-16 code units from the start
/// of the line — not bytes, and not characters. That is the LSP's definition, kept exactly
/// so a [`Span`] can reach an editor without a conversion; an off-by-one here is a bug the
/// user sees on every diagnostic.
///
/// A display that counts from one — the corpus trailers, `lem check` — adds one to each
/// field as it prints. It does not get to redefine what this type means.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Position {
    /// Zero-based line, counting the `\n`s before it.
    pub line: u32,
    /// Zero-based offset into the line, in UTF-16 code units.
    pub character: u32,
}

/// A [`Span`] as two [`Position`]s — the LSP's `Range`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

/// Where every line of a file starts, so that any offset into it can be placed.
///
/// Built once per file and kept for as long as the file is resident. It holds line starts
/// and the non-ASCII characters on each line, never the text, so a build worker can afford
/// one per file and a language server one per open document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineIndex(line_index::LineIndex);

impl LineIndex {
    /// Indexes `text`.
    ///
    /// Only `\n` starts a line. A `\r` is an ordinary character of the line it ends, as it
    /// is in the LSP, so a CRLF file's columns keep agreeing with its byte spans instead of
    /// quietly losing one per line.
    #[must_use]
    pub fn new(text: &str) -> Self {
        Self(line_index::LineIndex::new(text))
    }

    /// The position of `offset`.
    ///
    /// # Panics
    ///
    /// If `offset` is past the end of the file or falls inside a character. Either means a
    /// span that did not come from this file, which is a bug in whatever produced it rather
    /// than something to round off — a diagnostic pointing into the middle of an `é` would
    /// otherwise be silently wrong forever (invariant 10).
    #[must_use]
    pub fn position(&self, offset: u32) -> Position {
        let line_col = self
            .0
            .try_line_col(TextSize::from(offset))
            .unwrap_or_else(|| {
                panic!("offset {offset} is past the end of the file or inside a character")
            });
        let wide = self
            .0
            .to_wide(WideEncoding::Utf16, line_col)
            .expect("a line and column the index just produced");
        Position {
            line: wide.line,
            character: wide.col,
        }
    }

    /// Both ends of `span` as positions.
    #[must_use]
    pub fn range(&self, span: Span) -> Range {
        Range {
            start: self.position(span.start),
            end: self.position(span.end),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str, offset: u32) -> (u32, u32) {
        let Position { line, character } = LineIndex::new(text).position(offset);
        (line, character)
    }

    #[test]
    fn places_the_first_byte_of_a_file_at_the_origin() {
        assert_eq!(at("hello", 0), (0, 0));
    }

    #[test]
    fn counts_columns_from_the_start_of_the_line() {
        assert_eq!(at("hello", 3), (0, 3));
    }

    #[test]
    fn treats_an_empty_file_as_one_empty_line() {
        assert_eq!(at("", 0), (0, 0));
    }

    #[test]
    fn puts_the_byte_after_a_newline_at_the_start_of_the_next_line() {
        assert_eq!(at("ab\ncd", 3), (1, 0));
    }

    #[test]
    fn leaves_the_newline_itself_on_the_line_it_ends() {
        assert_eq!(at("ab\ncd", 2), (0, 2));
    }

    #[test]
    fn places_an_offset_on_a_later_line() {
        assert_eq!(at("a\nbb\nccc", 7), (2, 2));
    }

    #[test]
    fn places_the_end_of_the_file() {
        assert_eq!(at("a\nbb", 4), (1, 2));
    }

    #[test]
    fn opens_a_trailing_empty_line_after_a_final_newline() {
        assert_eq!(at("a\n", 2), (1, 0));
    }

    /// A `\r` is a character of the line it ends, so a CRLF file's columns still agree
    /// with its byte spans rather than quietly dropping one per line.
    #[test]
    fn counts_a_carriage_return_as_a_character_of_its_own_line() {
        assert_eq!(at("ab\r\ncd", 2), (0, 2));
        assert_eq!(at("ab\r\ncd", 4), (1, 0));
    }

    /// `’` is three bytes and one UTF-16 code unit: the column must not be the byte
    /// offset.
    #[test]
    fn counts_a_multibyte_character_as_the_one_code_unit_it_is() {
        assert_eq!(at("I’m", 4), (0, 2));
    }

    /// `🎉` is four bytes, one character, and *two* UTF-16 code units — the case that tells
    /// the LSP's convention apart from counting characters.
    #[test]
    fn counts_an_astral_character_as_two_code_units() {
        assert_eq!(at("a🎉b", 5), (0, 3));
    }

    #[test]
    fn counts_code_units_only_since_the_start_of_the_line() {
        assert_eq!(at("🎉\nab", 6), (1, 1));
    }

    #[test]
    #[should_panic(expected = "offset 1 is past the end of the file or inside a character")]
    fn refuses_an_offset_inside_a_character() {
        let _ = at("’", 1);
    }

    #[test]
    #[should_panic(expected = "offset 6 is past the end of the file or inside a character")]
    fn refuses_an_offset_past_the_end_of_the_file() {
        let _ = at("hello", 6);
    }

    #[test]
    fn spans_both_ends_of_a_range() {
        let index = LineIndex::new("ab\ncdef");
        assert_eq!(
            index.range(Span::new(1, 5)),
            Range {
                start: Position {
                    line: 0,
                    character: 1
                },
                end: Position {
                    line: 1,
                    character: 2
                },
            }
        );
    }
}

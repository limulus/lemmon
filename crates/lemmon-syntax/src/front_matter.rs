//! Front matter: the `---` block a file may open with, found the same way in both modes.
//!
//! The rule is textual and reads no YAML. A file whose first line is exactly `---`
//! opens a block that runs through the next line that is exactly `---` (a carriage
//! return before the newline allowed on either); what stands between is the shell's to
//! parse (D41), and the core never lexes it, so a `<lem.` there is not a tag. A block
//! that never closes is an error at its opening line rather than a thematic break and
//! some prose: an author who typed `---` on line one meant front matter, and guessing
//! otherwise is what invariant 10 forbids. Both parsers go through [`leading`] and keep
//! the block in the tree as one [`FrontMatter`](SyntaxKind::FrontMatter) token, so the
//! tree stays lossless and the `.tags` projection shows the boundary a tree-sitter
//! grammar has to agree with.

use rowan::{GreenNode, GreenToken, NodeOrToken};

use crate::syntax_kind::SyntaxKind;
use crate::{Diagnostic, Span, index_u32};

/// What a file opens with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrontMatter {
    /// The first line is not `---`.
    None,
    /// A block a later `---` line closes.
    Closed {
        /// The opening line through the closing line, both included.
        block: Span,
        /// The text between the two lines: the YAML, for a host that parses it.
        inner: Span,
    },
    /// An opening `---` line that no later line closes: the three dashes.
    Unclosed(Span),
}

/// Finds the front matter `src` opens with, if any.
#[must_use]
pub fn front_matter(src: &str) -> FrontMatter {
    let mut pos = 0;
    let mut opened: Option<usize> = None;
    while pos < src.len() {
        let line_end = src[pos..].find('\n').map_or(src.len(), |i| pos + i + 1);
        let is_rule = src[pos..line_end].trim_end_matches(['\r', '\n']) == "---";
        match opened {
            None if !is_rule => return FrontMatter::None,
            None => opened = Some(line_end),
            Some(inner_start) if is_rule => {
                return FrontMatter::Closed {
                    block: Span::new(0, index_u32(line_end)),
                    inner: Span::new(index_u32(inner_start), index_u32(pos)),
                };
            }
            Some(_) => {}
        }
        pos = line_end;
    }
    opened.map_or(FrontMatter::None, |_| {
        FrontMatter::Unclosed(Span::new(0, 3))
    })
}

/// What both parsers do with a file's opening: the closed block's span, to place as a
/// token and parse past, or the diagnostic for an unclosed one, whose text is then
/// parsed as it stands.
pub(crate) fn leading(src: &str) -> (Option<Span>, Option<Diagnostic>) {
    match front_matter(src) {
        FrontMatter::None => (None, None),
        FrontMatter::Closed { block, .. } => (Some(block), None),
        FrontMatter::Unclosed(opening) => (
            None,
            Some(Diagnostic::new(
                "front matter opened here has no closing `---` line",
                opening,
            )),
        ),
    }
}

/// The block as the token that stands first under a `Root`.
pub(crate) fn token(text: &str) -> NodeOrToken<GreenNode, GreenToken> {
    NodeOrToken::Token(GreenToken::new(SyntaxKind::FrontMatter.into(), text))
}

#[cfg(test)]
mod tests {
    use rowan::Language as _;

    use super::*;
    use crate::spans::{all_of, at};
    use crate::syntax_kind::Lemmon;

    #[test]
    fn finds_none_in_a_file_that_does_not_open_with_a_rule() {
        assert_eq!(front_matter(""), FrontMatter::None);
        assert_eq!(front_matter("text\n---\na: 1\n---\n"), FrontMatter::None);
    }

    #[test]
    fn finds_a_closed_block_with_the_yaml_between_its_lines() {
        let src = "---\na: 1\n---\nbody";
        assert_eq!(
            front_matter(src),
            FrontMatter::Closed {
                block: at(src, "---\na: 1\n---\n"),
                inner: at(src, "a: 1\n"),
            }
        );
    }

    #[test]
    fn accepts_a_carriage_return_before_each_lines_newline() {
        let src = "---\r\na: 1\r\n---\r\nx";
        assert_eq!(
            front_matter(src),
            FrontMatter::Closed {
                block: at(src, "---\r\na: 1\r\n---\r\n"),
                inner: at(src, "a: 1\r\n"),
            }
        );
    }

    #[test]
    fn closes_at_the_very_next_rule_line_even_when_nothing_stands_between() {
        let src = "---\n---\n";
        assert_eq!(
            front_matter(src),
            FrontMatter::Closed {
                block: all_of(src),
                inner: at(src, "---\n«»---\n"),
            }
        );
    }

    #[test]
    fn reports_an_opening_line_no_later_line_closes() {
        let src = "---\na: 1\n";
        assert_eq!(front_matter(src), FrontMatter::Unclosed(at(src, "---")));
        assert_eq!(front_matter("---"), FrontMatter::Unclosed(all_of("---")));
    }

    #[test]
    fn takes_only_a_line_that_is_exactly_three_dashes_as_a_rule() {
        assert_eq!(front_matter("--- \na: 1\n---\n"), FrontMatter::None);
        assert_eq!(front_matter("\n---\na: 1\n---\n"), FrontMatter::None);
        assert_eq!(front_matter("----\na: 1\n---\n"), FrontMatter::None);
        let trailing_space = "---\na: 1\n--- \n";
        assert_eq!(
            front_matter(trailing_space),
            FrontMatter::Unclosed(at(trailing_space, "«---»\n"))
        );
    }

    #[test]
    fn gives_a_parser_the_closed_block_or_the_diagnostic_for_an_unclosed_one() {
        assert_eq!(leading("a\n"), (None, None));
        let closed = "---\n---\nx";
        assert_eq!(leading(closed), (Some(at(closed, "---\n---\n")), None));
        let unclosed = "---\na: 1\n";
        let (block, diagnostic) = leading(unclosed);
        assert_eq!(block, None);
        let diagnostic = diagnostic.expect("an unclosed block is reported");
        assert_eq!(diagnostic.span, at(unclosed, "---"));
        assert_eq!(
            diagnostic.message,
            "front matter opened here has no closing `---` line"
        );
    }

    #[test]
    fn makes_the_block_a_token_of_its_own_kind() {
        let NodeOrToken::Token(token) = token("---\n---\n") else {
            panic!("a token");
        };
        assert_eq!(Lemmon::kind_from_raw(token.kind()), SyntaxKind::FrontMatter);
        assert_eq!(token.text(), "---\n---\n");
    }
}

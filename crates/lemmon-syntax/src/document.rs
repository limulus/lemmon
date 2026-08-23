//! Document mode's segmenter: where the lem-blocks of a `.md` file are.
//!
//! A document is markdown prose with block-level Lemmon tags in it
//! (`docs/language.md` §Document mode). This module finds the blocks and leaves
//! everything else — the prose, and any front matter — alone: a block starts on a
//! line whose first non-whitespace is `<lem.`, `<lem-`, or `</lem-` outside a fenced
//! code block, and extends as far as the parser says that one item reaches, its close
//! tag included. Rendering the prose is the compiler's job (`lemmon-vm`), since
//! markdown runs at compile time and its output lives in the module's constants.
//!
//! The result is an ordinary [`Parse`] over the whole file, so every later stage reads
//! a document through the same `Root` → items view it reads a template through, with
//! file-absolute spans. The tree is assembled rather than parsed: each block is parsed
//! on its own from the slice it starts at, and its nodes are spliced between the
//! prose, which stands as text runs. Rowan's green trees carry no positions, so the
//! splice costs nothing and nothing needs offsetting but the blocks' diagnostics.

use rowan::{GreenNode, GreenToken, NodeOrToken};

use crate::parser::{Parse, parse_item_prefix};
use crate::syntax_kind::SyntaxKind;
use crate::{Diagnostic, Span};

/// A parsed document: the tree over the whole file, plus where its parts are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentParse {
    /// The tree, with prose as text runs and each lem-block as the item it parsed to.
    pub parse: Parse,
    /// The front matter, `---` lines included, when the file opens with one. The core
    /// does not read it; the shell parses it as YAML.
    pub front_matter: Option<Span>,
    /// Each lem-block, opener through close, in document order.
    pub blocks: Vec<Span>,
    /// The prose runs between them: one more than there are blocks, the first
    /// starting after any front matter, any of them possibly empty.
    pub prose: Vec<Span>,
}

/// Segments a document and parses its lem-blocks.
///
/// Parse diagnostics come from the blocks; the segmenter adds one of its own when
/// something other than whitespace follows a block's close on its line, since what
/// that text would mean — prose, or part of the block — is not something to guess.
#[must_use]
pub fn parse_document(src: &str) -> DocumentParse {
    let front_matter = front_matter(src);
    let body_start = front_matter.map_or(0, |span| span.end as usize);
    let mut children: Vec<NodeOrToken<GreenNode, GreenToken>> = Vec::new();
    let mut diagnostics = Vec::new();
    let mut blocks = Vec::new();
    let mut prose = Vec::new();
    let mut fence: Option<Fence> = None;
    let mut text_start = 0;
    let mut prose_start = body_start;
    let mut pos = body_start;
    while pos < src.len() {
        let line_end = src[pos..].find('\n').map_or(src.len(), |i| pos + i + 1);
        let line = &src[pos..line_end];
        let (indent, content) = strip_indent(line);
        if let Some(open) = fence {
            if open.closes(content, indent) {
                fence = None;
            }
        } else if let Some(open) = Fence::opens(content, indent) {
            fence = Some(open);
        } else if is_block_start(content) {
            let start = pos + (line.len() - content.len());
            push_text(&mut children, &src[text_start..start]);
            prose.push(Span::new(index_u32(prose_start), index_u32(start)));
            // Each block re-lexes the rest of the file, so segmenting is O(blocks ×
            // size). Bounded by a post's length and measured in microseconds; the fix,
            // if it ever matters, is to scan once and hand the parser a token slice —
            // which needs a fallback for the file whose prose left the scanner in tag
            // mode, so it is not worth carrying until a profile asks for it.
            let block = parse_item_prefix(&src[start..]);
            let end = start + usize::from(block.green.text_len());
            children.extend(block.green.children().map(|child| child.to_owned()));
            diagnostics.extend(block.diagnostics.into_iter().map(|d| Diagnostic {
                span: shift(d.span, start),
                ..d
            }));
            blocks.push(Span::new(index_u32(start), index_u32(end)));
            let rest_end = src[end..].find('\n').map_or(src.len(), |i| end + i);
            let rest = &src[end..rest_end];
            let trailing = rest.trim_start_matches([' ', '\t', '\r']);
            if !trailing.is_empty() {
                let at = rest_end - trailing.len();
                diagnostics.push(Diagnostic::new(
                    "unexpected text after a lem-block on its line: a block ends its line",
                    Span::new(index_u32(at), index_u32(rest_end)),
                ));
            }
            text_start = end;
            prose_start = end;
            pos = end;
            continue;
        }
        pos = line_end;
    }
    push_text(&mut children, &src[text_start..]);
    prose.push(Span::new(index_u32(prose_start), index_u32(src.len())));
    diagnostics.sort_by_key(|d| (d.span.start, d.span.end));
    DocumentParse {
        parse: Parse {
            green: GreenNode::new(SyntaxKind::Root.into(), children),
            diagnostics,
        },
        front_matter,
        blocks,
        prose,
    }
}

/// Whether a line's content (its indent removed) opens a lem-block. A `</lem-` is
/// included so that a stray close is the parser's error, not prose.
fn is_block_start(content: &str) -> bool {
    content.starts_with("<lem.")
        || content.starts_with("<lem-")
        || content.starts_with("</lem-")
}

/// A line's indentation in columns and what follows it. Tabs advance to the next
/// four-column stop, as CommonMark counts them, so a tab-indented fence or code block
/// is the same one here as it is to comrak.
fn strip_indent(line: &str) -> (usize, &str) {
    let mut columns = 0;
    for (at, c) in line.char_indices() {
        match c {
            ' ' => columns += 1,
            '\t' => columns += 4 - columns % 4,
            _ => return (columns, &line[at..]),
        }
    }
    (columns, "")
}

/// A text run node for `text`, unless there is none.
fn push_text(children: &mut Vec<NodeOrToken<GreenNode, GreenToken>>, text: &str) {
    if text.is_empty() {
        return;
    }
    let token = GreenToken::new(SyntaxKind::Text.into(), text);
    children.push(
        GreenNode::new(SyntaxKind::TextRun.into(), [NodeOrToken::Token(token)]).into(),
    );
}

/// A `---` first line through the next `---` line. Without a closing line there is no
/// front matter, and the opening `---` is the thematic break markdown takes it for.
fn front_matter(src: &str) -> Option<Span> {
    let mut pos = 0;
    let mut first = true;
    while pos < src.len() {
        let line_end = src[pos..].find('\n').map_or(src.len(), |i| pos + i + 1);
        let is_rule = src[pos..line_end].trim_end_matches(['\r', '\n']) == "---";
        if first && !is_rule {
            return None;
        }
        if !first && is_rule {
            return Some(Span::new(0, index_u32(line_end)));
        }
        first = false;
        pos = line_end;
    }
    None
}

/// An open fenced code block: what it takes to close it.
#[derive(Clone, Copy)]
struct Fence {
    marker: char,
    len: usize,
}

impl Fence {
    /// The fence a line's content opens, if any: three or more backticks or tildes,
    /// indented less than four spaces, with no backtick in a backtick fence's info
    /// string (CommonMark §4.5).
    fn opens(content: &str, indent: usize) -> Option<Self> {
        if indent >= 4 {
            return None;
        }
        let marker = content.chars().next().filter(|c| matches!(c, '`' | '~'))?;
        let len = content.chars().take_while(|&c| c == marker).count();
        if len < 3 || (marker == '`' && content[len..].contains('`')) {
            return None;
        }
        Some(Self { marker, len })
    }

    /// Whether a line's content closes this fence: at least as many of its marker,
    /// nothing but whitespace after, and — like an opener — indented less than four
    /// columns (CommonMark §4.5).
    fn closes(self, content: &str, indent: usize) -> bool {
        if indent >= 4 {
            return false;
        }
        let len = content.chars().take_while(|&c| c == self.marker).count();
        len >= self.len && content[len..].trim().is_empty()
    }
}

fn shift(span: Span, by: usize) -> Span {
    Span::new(span.start + index_u32(by), span.end + index_u32(by))
}

fn index_u32(index: usize) -> u32 {
    u32::try_from(index).expect("sources are smaller than 4 GiB")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{AstNode, Item, Root, Spanned};

    fn items(doc: &DocumentParse) -> Vec<String> {
        Root::cast(doc.parse.syntax())
            .expect("a root")
            .items()
            .map(|item| {
                let kind = match &item {
                    Item::Text(_) => "Text",
                    _ => "Tag",
                };
                format!("{kind}@{}..{}", item.span().start, item.span().end)
            })
            .collect()
    }

    #[test]
    fn keeps_a_file_without_blocks_as_one_prose_run() {
        let doc = parse_document("Just *prose*.\n");
        assert_eq!(doc.blocks, vec![]);
        assert_eq!(doc.prose, vec![Span::new(0, 14)]);
        assert_eq!(items(&doc), vec!["Text@0..14"]);
        assert!(doc.parse.diagnostics.is_empty());
    }

    #[test]
    fn finds_a_void_tag_on_its_own_line() {
        let doc = parse_document("a\n<lem.x>\nb\n");
        assert_eq!(doc.blocks, vec![Span::new(2, 9)]);
        assert_eq!(doc.prose, vec![Span::new(0, 2), Span::new(9, 12)]);
        assert_eq!(items(&doc), vec!["Text@0..2", "Tag@2..9", "Text@9..12"]);
    }

    #[test]
    fn reproduces_the_source_byte_for_byte() {
        let src = "---\nt: 1\n---\n\n# H\n\n  <lem-if x>\n  y\n  </lem-if>\n\ntail";
        let doc = parse_document(src);
        assert_eq!(doc.parse.syntax().text().to_string(), src);
    }

    #[test]
    fn extends_a_block_to_its_close_many_lines_later() {
        let doc = parse_document("<lem-if x>\none\n\ntwo\n</lem-if>\n");
        assert_eq!(doc.blocks, vec![Span::new(0, 29)]);
        assert_eq!(doc.prose, vec![Span::new(0, 0), Span::new(29, 30)]);
    }

    #[test]
    fn leaves_tags_inside_a_fence_as_prose() {
        let doc = parse_document("```lem\n<lem.x>\n```\n<lem.y>\n");
        assert_eq!(doc.blocks, vec![Span::new(19, 26)]);
    }

    #[test]
    fn closes_a_fence_only_with_a_long_enough_marker_of_its_kind() {
        let doc = parse_document("````\n```\n<lem.x>\n~~~~\n<lem.y>\n````\n<lem.z>\n");
        assert_eq!(doc.blocks, vec![Span::new(35, 42)]);
    }

    #[test]
    fn does_not_close_a_fence_from_four_columns_of_indent() {
        let doc = parse_document("```\ncode\n    ```\n<lem.x>\n```\n<lem.y>\n");
        assert_eq!(doc.blocks, vec![Span::new(29, 36)]);
    }

    #[test]
    fn counts_a_tab_to_the_next_four_column_stop() {
        let doc = parse_document("```\n\t```\n<lem.x>\n```\n<lem.y>\n");
        assert_eq!(doc.blocks, vec![Span::new(21, 28)]);
    }

    #[test]
    fn does_not_open_a_fence_from_indented_code_or_a_backtick_info_string() {
        let doc = parse_document("    ```\n<lem.x>\n``` a`b\n<lem.y>\n");
        assert_eq!(doc.blocks.len(), 2);
    }

    #[test]
    fn starts_a_block_under_any_indent_so_that_markdown_can_reject_it() {
        let doc = parse_document("    <lem.x>\n");
        assert_eq!(doc.blocks, vec![Span::new(4, 11)]);
        assert_eq!(doc.prose[0], Span::new(0, 4));
    }

    #[test]
    fn skips_front_matter_and_starts_prose_after_it() {
        let doc = parse_document("---\na: 1\n---\nbody\n");
        assert_eq!(doc.front_matter, Some(Span::new(0, 13)));
        assert_eq!(doc.prose, vec![Span::new(13, 18)]);
        assert_eq!(items(&doc), vec!["Text@0..18"]);
    }

    #[test]
    fn treats_an_unclosed_front_matter_as_prose() {
        let doc = parse_document("---\na: 1\n");
        assert_eq!(doc.front_matter, None);
        assert_eq!(doc.prose, vec![Span::new(0, 9)]);
    }

    #[test]
    fn accepts_crlf_front_matter_lines() {
        let doc = parse_document("---\r\na: 1\r\n---\r\nx");
        assert_eq!(doc.front_matter, Some(Span::new(0, 16)));
    }

    #[test]
    fn shifts_a_block_diagnostic_to_file_coordinates() {
        let doc = parse_document("p\n<lem-if x>\nbody\n");
        assert_eq!(doc.parse.diagnostics.len(), 1);
        assert_eq!(doc.parse.diagnostics[0].span, Span::new(2, 9));
    }

    #[test]
    fn drops_scanner_diagnostics_from_the_prose_after_a_block() {
        let doc = parse_document("<lem.x>\n<lem-if \"unterminated\n");
        let spans: Vec<Span> = doc.parse.diagnostics.iter().map(|d| d.span).collect();
        assert!(spans.iter().all(|s| s.start >= 8), "{spans:?}");
        assert_eq!(doc.blocks.len(), 2);
    }

    #[test]
    fn reports_text_after_a_block_on_its_line() {
        let doc = parse_document("<lem.x> tail \nmore\n");
        assert_eq!(doc.blocks, vec![Span::new(0, 7)]);
        assert_eq!(doc.parse.diagnostics.len(), 1);
        assert_eq!(doc.parse.diagnostics[0].span, Span::new(8, 13));
        assert!(
            doc.parse.diagnostics[0]
                .message
                .contains("after a lem-block")
        );
    }

    #[test]
    fn allows_trailing_whitespace_after_a_block() {
        let doc = parse_document("<lem.x>  \r\nmore");
        assert!(doc.parse.diagnostics.is_empty());
        assert_eq!(doc.prose[1], Span::new(7, 15));
    }

    #[test]
    fn takes_a_stray_close_as_the_parsers_error() {
        let doc = parse_document("</lem-if>\n");
        assert_eq!(doc.blocks, vec![Span::new(0, 9)]);
        assert!(doc.parse.diagnostics[0].message.contains("stray"));
    }

    #[test]
    fn ends_a_block_at_the_end_of_input() {
        let doc = parse_document("<lem.x>");
        assert_eq!(doc.blocks, vec![Span::new(0, 7)]);
        assert_eq!(doc.prose, vec![Span::new(0, 0), Span::new(7, 7)]);
    }
}

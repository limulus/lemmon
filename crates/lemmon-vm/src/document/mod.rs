//! Document mode's markdown stage: the prose of a `.md` file, rendered at compile time.
//!
//! The segmenter (`lemmon_syntax::parse_document`) says where the lem-blocks are; this
//! module renders everything between them. The prose runs are joined with
//! `<!--lem:N-->` sentinel lines and rendered by comrak as one document, so that
//! reference links and footnotes resolve across blocks, then the output is cut at the
//! sentinels into one HTML chunk per gap. A sentinel line is a CommonMark type-2 HTML
//! block, so when it is where it should be it renders verbatim as a sibling of the
//! paragraphs around it; when markdown swallowed it — into a paragraph, a list, a
//! blockquote, indented code — the block's output would land somewhere its author did
//! not put it, and that is a compile error at the block (`docs/language.md` §Document
//! mode, item 4). So is a lem tag left in the prose: tags are block-level only in v1.
//!
//! [`options`] lives here rather than with the adapters because it is not only a set of
//! markdown preferences: `render.unsafe` is what lets a sentinel line through to be cut
//! on, so it is part of the splice below. What comrak cannot do at all is supplied
//! through its plugin seams: heading ids by [`adapters`], and the marking of every code
//! block by [`code`] (D42) — the core highlights nothing.

mod adapters;
mod code;

use comrak::nodes::{NodeValue, Sourcepos};
use comrak::{Arena, Options, format_html_with_plugins, options::Plugins, parse_document};
use lemmon_syntax::{DocumentParse, Span};

use crate::diagnostics::Diagnostic;
use adapters::Headings;
pub use code::CodePayload;
use code::{Block, CodeFences};
pub(crate) use code::{CODE, Code, ProsePiece};

/// comrak's configuration (D08): CommonMark plus footnotes, `{#id}` heading attributes,
/// and raw HTML passed through — prose is the author's own. Heading ids are
/// [`Headings`](adapters::Headings)' job: comrak parses `{#id}` but its renderer does
/// not apply it, and its own id option appends an anchor link to every heading
/// besides.
fn options() -> Options<'static> {
    let mut options = Options::default();
    options.extension.footnotes = true;
    options.extension.header_attributes = true;
    options.render.r#unsafe = true;
    options
}

fn sentinel(index: usize) -> String {
    format!("<!--lem:{index}-->")
}

/// A document's prose, rendered.
pub(crate) struct Prose {
    /// The HTML of each gap between the document's lem-blocks, cut into text and the
    /// code blocks to mark: the chunk at `i` stands before lem-block `i`, the last one
    /// after every lem-block.
    pub chunks: Vec<Vec<ProsePiece>>,
    /// What the document's code blocks hand their producers, one per distinct block, in
    /// the order the document first writes each (D42).
    pub payloads: Vec<CodePayload>,
}

/// Renders a document's prose, every code block in it written as comrak writes it and
/// cut out to be marked.
///
/// # Errors
///
/// One per lem-block that markdown swallowed, per lem tag that reached the prose, and
/// per reserved sentinel written into the prose by hand.
pub(crate) fn render_prose(
    path: &str,
    src: &str,
    doc: &DocumentParse,
) -> Result<Prose, Vec<Diagnostic>> {
    let mut errors = Vec::new();
    let error = |errors: &mut Vec<Diagnostic>, span: Span, message: &str| {
        errors.push(Diagnostic::error(path, span, message));
    };

    // The joined prose, and where each run of it came from.
    let mut joined = String::new();
    let mut runs: Vec<(usize, Span)> = Vec::new();
    for (index, run) in doc.prose.iter().enumerate() {
        let text = &src[run.start as usize..run.end as usize];
        if let Some(at) = text.find("<!--lem:") {
            let at = run.start + index_u32(at);
            error(
                &mut errors,
                Span::new(at, at + 8),
                "`<!--lem:` is reserved for document mode's own sentinels",
            );
        }
        runs.push((joined.len(), *run));
        joined.push_str(text);
        if index < doc.blocks.len() {
            joined.push_str(&sentinel(index));
            joined.push('\n');
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    let arena = Arena::new();
    let root = parse_document(&arena, &joined, &options());
    let lines = line_starts(&joined);
    // Every span below is measured on the *source* the node came from, never on the
    // text comrak decoded from it: `&lt;lem.x&gt;` and `\<lem.x>` are a tag once
    // decoded and are not one as written, and an entity earlier in a line shifts every
    // offset after it besides.
    let source_range = |pos: Sourcepos| -> (usize, usize) {
        let start = (lines[pos.start.line - 1] + pos.start.column - 1).min(joined.len());
        let mut end = (lines[pos.end.line - 1] + pos.end.column).min(joined.len());
        while end < joined.len() && !joined.is_char_boundary(end) {
            end += 1;
        }
        (start, end.max(start))
    };
    let to_source = |offset: usize, len: usize| -> Span {
        let &(joined_start, run) = runs
            .iter()
            .rev()
            .find(|(start, _)| *start <= offset)
            .expect("every offset follows the first run");
        let start = run.start + index_u32(offset - joined_start);
        Span::new(start.min(run.end), (start + index_u32(len)).min(run.end))
    };

    let mut found = vec![false; doc.blocks.len()];
    for node in root.descendants() {
        let data = node.data.borrow();
        let at_root = match &data.value {
            NodeValue::HtmlBlock(_) => node.parent().is_some_and(|p| p.parent().is_none()),
            NodeValue::HtmlInline(_) | NodeValue::Text(_) => false,
            _ => continue,
        };
        let (start, end) = source_range(data.sourcepos);
        if at_root
            && let Some(index) = joined[start..end]
                .trim_end()
                .strip_prefix("<!--lem:")
                .and_then(|rest| rest.strip_suffix("-->"))
                .and_then(|digits| digits.parse::<usize>().ok())
        {
            found[index] = true;
            continue;
        }
        if let Some(at) = tag_start(&joined, start..end) {
            let len = joined[at..end].find('>').map_or(end - at, |i| i + 1);
            error(
                &mut errors,
                to_source(at, len),
                "lem tags in prose must stand alone on a line: inline tags are not supported",
            );
        }
    }
    for (index, block) in doc.blocks.iter().enumerate() {
        if !found[index] {
            error(
                &mut errors,
                *block,
                "markdown swallowed this lem-block: a block must stand at the top level of \
                 the document — not inside a paragraph, list, blockquote, indented code, or \
                 a raw HTML block, which a blank line above and below the block ends",
            );
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    let blocks: Vec<Block> = root
        .descendants()
        .filter_map(|node| {
            let data = node.data.borrow();
            let NodeValue::CodeBlock(block) = &data.value else {
                return None;
            };
            // comrak's own split of the info string: the language is the first word.
            let word = block.info.split(is_cmark_space).next().unwrap_or_default();
            if word == "math" {
                // Rendered by comrak itself, without asking the adapter: no mark.
                return None;
            }
            let (start, end) = source_range(data.sourcepos);
            let span = if word.is_empty() {
                to_source(start, end - start)
            } else {
                let (at, len) = language_word(&joined, start);
                to_source(at, len)
            };
            Some(Block {
                payload: CodePayload::new(&block.info, &block.literal),
                span,
            })
        })
        .collect();

    let headings = Headings::new(root);
    let fences = CodeFences::new();
    let mut plugins = Plugins::default();
    plugins.render.heading_adapter = Some(&headings);
    plugins.render.codefence_syntax_highlighter = Some(&fences);
    let mut html = String::new();
    format_html_with_plugins(root, &options(), &mut html, &plugins)
        .expect("writing to a String cannot fail");
    let mut chunks = Vec::with_capacity(doc.blocks.len() + 1);
    let mut rest = html.as_str();
    for index in 0..doc.blocks.len() {
        let marker = sentinel(index);
        let at = rest
            .find(&marker)
            .expect("a sentinel found in the tree is in the output");
        chunks.push(code::pieces(&rest[..at], &blocks));
        // The sentinel's own newline stays with the chunk after it: a block is a line
        // of the document, and its output ends that line the way the prose's does.
        rest = &rest[at + marker.len()..];
    }
    chunks.push(code::pieces(rest, &blocks));
    let mut payloads: Vec<CodePayload> = Vec::new();
    for block in blocks {
        if payloads.iter().all(|kept| kept.id != block.payload.id) {
            payloads.push(block.payload);
        }
    }
    Ok(Prose { chunks, payloads })
}

/// A byte CommonMark counts as whitespace where it splits an info string.
const fn is_cmark_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\u{b}' | '\u{c}' | '\r')
}

/// Where the language word of the fenced block opening at `start` is written in
/// `text`, and how many bytes it takes: past the fence's indentation, its run of
/// backticks or tildes, and the spaces after it, up to the next whitespace. Measured on
/// the source rather than the decoded info string, where an escape or an entity would
/// shift it.
fn language_word(text: &str, start: usize) -> (usize, usize) {
    let line = &text[start..];
    let line = &line[..line.find('\n').unwrap_or(line.len())];
    let fence = line.trim_start_matches([' ', '\t']);
    let marker = fence.as_bytes()[0];
    let info = fence
        .trim_start_matches(char::from(marker))
        .trim_start_matches([' ', '\t']);
    let at = start + (line.len() - info.len());
    (at, info.find(is_cmark_space).unwrap_or(info.len()))
}

/// The byte offset of each line's first byte.
fn line_starts(text: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(i, _)| i + 1))
        .collect()
}

/// Where a lem tag starts within `text[range]`, as an offset into `text`, if one does.
///
/// Takes the whole text rather than the slice because the byte that decides whether a
/// tag is a tag can lie outside it: comrak starts an escaped node *at* the `<`, leaving
/// the backslash that escaped it behind. An odd number of backslashes escapes the `<`,
/// as CommonMark says; an even number is escaped backslashes and a real tag.
fn tag_start(text: &str, range: std::ops::Range<usize>) -> Option<usize> {
    let (start, end) = (range.start, range.end);
    let mut from = start;
    while let Some(at) = text[from..end].find('<').map(|i| from + i) {
        let backslashes = text[..at].bytes().rev().take_while(|&b| b == b'\\').count();
        if backslashes % 2 == 0
            && ["<lem.", "<lem-", "</lem-"]
                .iter()
                .any(|tag| text[at..].starts_with(tag))
        {
            return Some(at);
        }
        from = at + 1;
    }
    None
}

fn index_u32(index: usize) -> u32 {
    u32::try_from(index).expect("sources are smaller than 4 GiB")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::at;

    /// Each chunk as the page first holds it: its text and its code blocks' elements,
    /// the sentinels gone.
    fn render(src: &str) -> Result<Vec<String>, Vec<Diagnostic>> {
        let doc = lemmon_syntax::parse_document(src);
        render_prose("doc.md", src, &doc).map(|prose| {
            prose
                .chunks
                .iter()
                .map(|pieces| pieces.iter().map(ProsePiece::text).collect())
                .collect()
        })
    }

    fn messages(result: Result<Vec<String>, Vec<Diagnostic>>) -> Vec<String> {
        result
            .expect_err("an error")
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    #[test]
    fn renders_prose_without_blocks_as_one_chunk() {
        assert_eq!(
            render("Hi *there*\n").unwrap(),
            vec!["<p>Hi <em>there</em></p>\n"]
        );
    }

    #[test]
    fn splits_the_output_around_each_block() {
        let chunks = render("a\n\n<lem.x>\n\nb\n").unwrap();
        assert_eq!(chunks, vec!["<p>a</p>\n", "\n<p>b</p>\n"]);
    }

    #[test]
    fn leaves_an_empty_chunk_where_blocks_are_adjacent() {
        let chunks = render("<lem.x>\n<lem.y>\n").unwrap();
        assert_eq!(chunks, vec!["", "\n", "\n"]);
    }

    #[test]
    fn resolves_reference_links_across_blocks() {
        let chunks = render("[a]\n\n<lem.x>\n\n[a]: /a\n").unwrap();
        assert_eq!(chunks[0], "<p><a href=\"/a\">a</a></p>\n");
    }

    #[test]
    fn writes_an_escaped_tag_in_prose_as_text() {
        let chunks = render("Writing &lt;lem.x&gt; and \\<lem.y> in prose.\n").unwrap();
        assert_eq!(
            chunks[0],
            "<p>Writing &lt;lem.x&gt; and &lt;lem.y&gt; in prose.</p>\n"
        );
    }

    #[test]
    fn takes_an_even_run_of_backslashes_before_a_tag_as_a_tag() {
        let src = "a \\\\<lem.x> b\n";
        let msgs = messages(render(src));
        assert_eq!(msgs.len(), 1);
        let expected = format!("doc.md:{}: lem tags", at(src, "<lem.x>"));
        assert!(msgs[0].starts_with(&expected), "{msgs:?}");
    }

    #[test]
    fn points_at_the_tag_itself_after_an_entity_earlier_in_the_line() {
        let src = "&amp;&amp;é <lem.x> here\n";
        let msgs = messages(render(src));
        assert_eq!(msgs.len(), 1);
        let expected = format!("doc.md:{}: lem tags", at(src, "<lem.x>"));
        assert!(msgs[0].starts_with(&expected), "{msgs:?}");
    }

    #[test]
    fn passes_raw_html_through() {
        assert_eq!(render("<div>x</div>\n").unwrap(), vec!["<div>x</div>\n"]);
    }

    #[test]
    fn rejects_a_tag_inside_a_blockquote_as_prose() {
        let src = "> quote\n> <lem.x>\n";
        let msgs = messages(render(src));
        assert_eq!(msgs.len(), 1);
        let expected = format!("doc.md:{}: lem tags in prose", at(src, "<lem.x>"));
        assert!(msgs[0].starts_with(&expected), "{msgs:?}");
    }

    #[test]
    fn rejects_a_block_inside_indented_code() {
        let src = "text\n\n    <lem.x>\n";
        let msgs = messages(render(src));
        let expected = format!("doc.md:{}: markdown swallowed", at(src, "<lem.x>"));
        assert!(msgs[0].starts_with(&expected), "{msgs:?}");
    }

    #[test]
    fn rejects_a_block_inside_a_list_item() {
        let msgs = messages(render("- item\n  <lem.x>\n"));
        assert!(msgs[0].contains("swallowed"), "{msgs:?}");
    }

    #[test]
    fn lets_a_block_interrupt_a_paragraph() {
        assert_eq!(
            render("text\n<lem.x>\n").unwrap(),
            vec!["<p>text</p>\n", "\n"]
        );
    }

    #[test]
    fn rejects_a_tag_in_the_middle_of_a_paragraph() {
        let src = "---\nf: 1\n---\nsee <lem.x> here\n";
        let msgs = messages(render(src));
        assert_eq!(msgs.len(), 1);
        let expected = format!("doc.md:{}: lem tags in prose", at(src, "<lem.x>"));
        assert!(msgs[0].starts_with(&expected), "{msgs:?}");
    }

    #[test]
    fn rejects_a_keyword_tag_in_a_list_item() {
        let src = "- <lem-if x>\n";
        let msgs = messages(render(src));
        let expected = format!("doc.md:{}: lem tags", at(src, "<lem-if x>"));
        assert!(msgs[0].starts_with(&expected), "{msgs:?}");
    }

    #[test]
    fn allows_tags_inside_code() {
        let chunks = render("`<lem.x>`\n\n```\n<lem-if y>\n```\n").unwrap();
        assert!(chunks[0].contains("&lt;lem.x&gt;"));
        assert!(chunks[0].contains("&lt;lem-if y&gt;"));
    }

    /// Every code block the document holds, as the prose cut them out.
    fn codes(src: &str) -> Vec<Code> {
        let doc = lemmon_syntax::parse_document(src);
        render_prose("doc.md", src, &doc)
            .unwrap_or_else(|e| panic!("{e:?}"))
            .chunks
            .into_iter()
            .flatten()
            .filter_map(|piece| match piece {
                ProsePiece::Code(code) => Some(code),
                ProsePiece::Text(_) => None,
            })
            .collect()
    }

    #[test]
    fn writes_every_code_block_exactly_as_comrak_does_with_no_adapter() {
        // The markup contract a producer and a page with no producer both depend on
        // (D42): marking a block changes nothing a reader sees until a handler runs. A
        // fenced block with and without a language, an indented one, a `math` fence, and
        // fences inside a list item, a blockquote, and a footnote.
        const SRC: &str = "Code[^n]:\n\n```sh {1}\necho \"<b>\" & done\n```\n\n\
                           ```\nplain\n```\n\n    indented <i>\n\n```math\nx^2\n```\n\n\
                           - item\n\n  ```css\n  a {}\n  ```\n\n> ```html\n> <p>\n> ```\n\n\
                           [^n]: A note:\n\n    ```sh\n    ls\n    ```\n";
        let plain = comrak::markdown_to_html(SRC, &options());
        assert_eq!(render(SRC).unwrap(), [plain]);
    }

    #[test]
    fn cuts_out_every_code_block_but_a_math_fence() {
        // comrak renders a `math` fence before it asks any seam, so it is no mark and
        // its block stays comrak's own; every other block, indented or fenced, labelled
        // or not, is one. The `math` fence comes first, so a walk that counted it would
        // pair every later element with the block before it.
        const SRC: &str = "```math\nx^2\n```\n\n```sh\na\n```\n\n```\nb\n```\n\n    c\n";
        let codes = codes(SRC);
        assert_eq!(
            codes
                .iter()
                .map(|code| code.key.as_deref())
                .collect::<Vec<_>>(),
            [Some("sh"), None, None]
        );
        assert!(codes.iter().all(|code| !code.html.contains("math")));
        assert!(
            codes[0]
                .html
                .starts_with("<pre><code class=\"language-sh\">")
        );
        assert!(codes[0].html.ends_with("</code></pre>"));
    }

    #[test]
    fn points_a_blocks_mark_at_its_language_word_or_at_the_whole_block() {
        // The word as written, entity and all, since that is where an editor puts the
        // warning; a block with no word has nothing smaller to point at.
        const SRC: &str = "```bs&#97;h\necho\n```\n\n```\nplain\n```\n";
        let codes = codes(SRC);
        assert_eq!(codes[0].key.as_deref(), Some("bsah"));
        assert_eq!(codes[0].span, at(SRC, "bs&#97;h"));
        assert_eq!(codes[1].span, at(SRC, "```\nplain\n```"));
    }

    #[test]
    fn pairs_a_block_in_a_footnote_with_its_own_payload() {
        // The footnote is written before the `css` fence and rendered after it, since
        // comrak puts footnotes last. The walk that finds each block's payload has to
        // meet them in the order comrak renders them, or the `sh` element would be
        // marked with the `css` block's key and payload.
        const SRC: &str = "Text[^n].\n\n[^n]: See:\n\n    ```sh\n    ls\n    ```\n\n\
                           ```css\na {}\n```\n";
        let codes = codes(SRC);
        assert_eq!(
            codes
                .iter()
                .map(|code| (code.key.as_deref(), code.html.contains("ls")))
                .collect::<Vec<_>>(),
            [(Some("css"), false), (Some("sh"), true)]
        );
    }

    #[test]
    fn pairs_blocks_in_footnotes_comrak_drops_or_reorders() {
        // Each element is paired with the walk's block by position, which holds because
        // comrak takes an unreferenced footnote out of the tree and puts the rest in the
        // order they are referenced before it renders. A comrak that stopped doing
        // either would mark a footnote's block with another block's key and payload.
        const UNUSED: &str = "Text[^b].\n\n[^a]: Unused:\n\n    ```css\n    a {}\n    ```\n\n\
                              [^b]: Used:\n\n    ```sh\n    ls\n    ```\n";
        const REVERSED: &str = "Text[^b] and[^a].\n\n[^a]: A:\n\n    ```css\n    a {}\n    \
                                ```\n\n[^b]: B:\n\n    ```sh\n    ls\n    ```\n";
        let paired = |src: &str| {
            codes(src)
                .iter()
                .map(|code| {
                    (
                        code.key.as_deref().map(str::to_owned),
                        code.html.contains("ls"),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(paired(UNUSED), [(Some("sh".to_owned()), true)]);
        assert_eq!(
            paired(REVERSED),
            [
                (Some("sh".to_owned()), true),
                (Some("css".to_owned()), false)
            ]
        );
        // A block comrak never renders reports no payload either.
        let doc = lemmon_syntax::parse_document(UNUSED);
        assert_eq!(
            render_prose("doc.md", UNUSED, &doc).unwrap().payloads.len(),
            1
        );
    }

    #[test]
    fn reports_one_payload_for_a_block_written_twice() {
        // Two marks, one payload: the id is the content, so the host produces it once.
        const SRC: &str = "```sh\nls\n```\n\n```sh\nls\n```\n\n```sh\npwd\n```\n";
        let doc = lemmon_syntax::parse_document(SRC);
        let prose = render_prose("doc.md", SRC, &doc).unwrap();
        let codes = codes(SRC);
        assert_eq!(codes.len(), 3);
        assert_eq!(codes[0].id, codes[1].id);
        assert_eq!(
            prose
                .payloads
                .iter()
                .map(|payload| &*payload.code)
                .collect::<Vec<_>>(),
            ["ls\n", "pwd\n"]
        );
        assert_eq!(prose.payloads[0].id, codes[0].id);
    }

    #[test]
    fn rejects_a_hand_written_sentinel() {
        // A code block's sentinel too, which would otherwise mark a range of the author's
        // choosing: `<!--lem:` is reserved wherever it is written in the prose.
        for src in [
            "x\n\n<!--lem:0-->\n\n<lem.y>\n",
            "x\n\n<!--lem:code:0--><pre><code></code></pre>\n",
        ] {
            let msgs = messages(render(src));
            let expected =
                format!("doc.md:{}: `<!--lem:` is reserved", at(src, "<!--lem:"));
            assert!(msgs[0].starts_with(&expected), "{msgs:?}");
        }
    }
}

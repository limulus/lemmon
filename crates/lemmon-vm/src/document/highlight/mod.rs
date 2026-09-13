//! Code blocks, highlighted by the grammars the host declares (D40).
//!
//! comrak writes a code block's `<pre>` and `<code>` itself unless given a highlighter,
//! and then asks the highlighter for all three parts. The one here keeps comrak's own
//! markup — the same tags, attributes, and escaping, so that a block no grammar colours
//! reads exactly as it would with no highlighter at all — and fills in only the inside
//! of `<code>`. That is the whole of what a highlighter owns, which is what leaves room
//! for a different one: a host that declares no grammars gets comrak's output untouched,
//! for anything after the core to colour as it likes.
//!
//! Every block is looked up before comrak renders, from the tree, as
//! [`Headings`](super::adapters::Headings) computes ids: comrak's adapter sees neither a
//! block's position nor anywhere to put a warning. comrak renders blocks in tree order,
//! and hands the adapter every one but a `math` fence, which it renders itself.
//!
//! Only this module and [`grammars`] name syntect.

mod grammars;

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

use comrak::adapters::SyntaxHighlighterAdapter;
use lemmon_syntax::Span;

use crate::compile::Diagnostic;
use crate::unit::GrammarRef;
pub(crate) use grammars::Grammars;
use grammars::Lookup;

/// One code block in the document, as comrak will render it.
pub(super) struct Block {
    /// The first word of the info string, decoded — what comrak calls the language —
    /// and where it is written; `None` for a block without one.
    pub language: Option<(String, Span)>,
    /// The code, as comrak will pass it.
    pub code: String,
}

/// What highlighting a document's code blocks produced.
pub(super) struct Highlighted {
    /// The adapter to give comrak, or `None` to leave comrak's output alone.
    pub adapter: Option<CodeBlocks>,
    pub diagnostics: Vec<Diagnostic>,
    /// The grammars the blocks were highlighted against, when any names a language.
    pub grammar_ref: Option<GrammarRef>,
}

/// Highlights `blocks`, in document order, with the host's `grammars`.
///
/// A block with no language word is left as comrak writes it. A word no grammar
/// answers leaves the block the same way, and says so; so does a grammar that the word
/// names but that does not load, or that fails on the code. Without `grammars` nothing
/// is highlighted and nothing is said: the host has asked for no highlighting.
pub(super) fn highlight(
    path: &str,
    blocks: &[Block],
    grammars: Option<&Grammars>,
) -> Highlighted {
    let grammar_ref =
        blocks
            .iter()
            .find_map(|block| block.language.as_ref())
            .map(|&(_, span)| GrammarRef {
                digest: grammars.map(Grammars::digest),
                span,
            });
    let Some(grammars) = grammars else {
        return Highlighted {
            adapter: None,
            diagnostics: Vec::new(),
            grammar_ref,
        };
    };
    let mut diagnostics = Vec::new();
    let bodies = blocks
        .iter()
        .map(|block| {
            let (word, at) = block.language.as_ref()?;
            let (body, diagnostic) = body(path, grammars, word, &block.code, *at);
            diagnostics.extend(diagnostic);
            body
        })
        .collect::<Vec<_>>();
    Highlighted {
        adapter: Some(CodeBlocks {
            bodies: Mutex::new(bodies.into_iter()),
        }),
        diagnostics,
        grammar_ref,
    }
}

/// One block's highlighted HTML, or `None` to leave it as comrak writes it, and what
/// there is to say about it.
fn body(
    path: &str,
    grammars: &Grammars,
    word: &str,
    code: &str,
    at: Span,
) -> (Option<String>, Option<Diagnostic>) {
    let warning = |message: String| Diagnostic::warning(path, at, message);
    match grammars.lookup(word) {
        Lookup::Found(syntax) => match grammars.highlight(syntax, code) {
            Ok(html) => (Some(html), None),
            Err(error) => (
                None,
                Some(warning(format!(
                    "the grammar for `{word}` failed on this code, so it is not \
                     highlighted: {error}"
                ))),
            ),
        },
        Lookup::Refused(failure) => (
            None,
            Some(
                warning(format!(
                    "the grammar for `{word}` does not load, so this code is not \
                     highlighted"
                ))
                .with_label(
                    &*failure.path,
                    failure.span,
                    format!("it does not load: {}", failure.message),
                ),
            ),
        ),
        Lookup::Missing {
            suggestion,
            unreadable,
        } => {
            let message = match suggestion {
                Some(near) => format!(
                    "the host declares no grammar for `{word}`; did you mean `{near}`?"
                ),
                None => format!(
                    "the host declares no grammar for `{word}`, so this code is not \
                     highlighted"
                ),
            };
            let labelled = unreadable.iter().fold(warning(message), |d, f| {
                d.with_label(
                    &*f.path,
                    f.span,
                    format!(
                        "this grammar could not be read, so the languages it names are \
                         unknown: {}",
                        f.message
                    ),
                )
            });
            (None, Some(labelled))
        }
    }
}

/// comrak's highlighter seam, filled in with bodies computed before it renders.
pub(super) struct CodeBlocks {
    bodies: Mutex<std::vec::IntoIter<Option<String>>>,
}

impl SyntaxHighlighterAdapter for CodeBlocks {
    fn write_highlighted(
        &self,
        output: &mut dyn fmt::Write,
        _: Option<&str>,
        code: &str,
    ) -> fmt::Result {
        let body = self
            .bodies
            .lock()
            .expect("no panic holds the bodies")
            .next()
            .expect("one body per code block in the tree");
        match body {
            Some(html) => output.write_str(&html),
            None => comrak::html::escape(output, code),
        }
    }

    fn write_pre_tag(
        &self,
        output: &mut dyn fmt::Write,
        attributes: HashMap<&'static str, Cow<'_, str>>,
    ) -> fmt::Result {
        comrak::html::write_opening_tag(output, "pre", attributes)
    }

    fn write_code_tag(
        &self,
        output: &mut dyn fmt::Write,
        attributes: HashMap<&'static str, Cow<'_, str>>,
    ) -> fmt::Result {
        comrak::html::write_opening_tag(output, "code", attributes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Host;
    use crate::document::{Prose, render_prose};
    use crate::testing::{GRAMMARS, at};

    /// Blocks are checked through the whole prose stage, since that is what finds them
    /// in the tree and wires the adapter to comrak.
    fn render(src: &str, host: &Host) -> Prose {
        let doc = lemmon_syntax::parse_document(src);
        render_prose("doc.md", src, &doc, host).unwrap_or_else(|e| panic!("{e:?}"))
    }

    fn highlighting() -> Host {
        Host::new().with_grammars(GRAMMARS.iter().copied())
    }

    fn warnings(prose: &Prose) -> Vec<String> {
        prose.warnings.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn leaves_every_block_alone_for_a_host_that_declares_no_grammars() {
        const SRC: &str = "```bsah\necho\n```\n";
        let prose = render(SRC, &Host::new());
        assert_eq!(
            prose.chunks,
            ["<pre><code class=\"language-bsah\">echo\n</code></pre>\n"]
        );
        assert!(prose.warnings.is_empty());
        assert_eq!(
            prose.grammar_ref,
            Some(GrammarRef {
                digest: None,
                span: at(SRC, "bsah"),
            })
        );
    }

    #[test]
    fn writes_a_block_no_grammar_colours_exactly_as_comrak_does() {
        // The markup contract a different highlighter depends on: the adapter changes
        // what is inside `<code>` for a block a grammar colours, and nothing else.
        const SRC: &str =
            "```\n<b> & \"q\"\n```\n\n    indented <i>\n\n```bsah\necho\n```\n";
        assert_eq!(
            render(SRC, &highlighting()).chunks,
            render(SRC, &Host::new()).chunks
        );
    }

    #[test]
    fn warns_at_a_language_no_grammar_answers_as_the_word_is_written() {
        // An entity decodes to `bsah`, but the warning points at what the author typed.
        const SRC: &str = "```bs&#97;h\necho\n```\n";
        assert_eq!(
            warnings(&render(SRC, &highlighting())),
            [format!(
                "doc.md:{}: warning: the host declares no grammar for `bsah`; did you mean \
                 `bash`?",
                at(SRC, "bs&#97;h")
            )]
        );
    }

    #[test]
    fn warns_nothing_for_a_block_that_names_no_language() {
        let prose = render("```\necho\n```\n\n    indented\n", &highlighting());
        assert!(prose.warnings.is_empty());
        assert_eq!(prose.grammar_ref, None);
    }

    #[test]
    fn records_the_grammars_at_the_first_block_that_names_a_language() {
        const SRC: &str = "```\nplain\n```\n\n```sh\necho\n```\n\n```css\na {}\n```\n";
        let host = highlighting();
        assert_eq!(
            render(SRC, &host).grammar_ref,
            Some(GrammarRef {
                digest: host.grammar_digest(),
                span: at(SRC, "sh"),
            })
        );
    }

    #[test]
    fn labels_a_grammar_that_could_not_be_read_where_a_language_is_missing() {
        const BAD: &str = "name: é\nscope: x: y\n";
        const SRC: &str = "```x\ncode\n```\n";
        let host = Host::new().with_grammars([("bad.sublime-syntax", BAD)]);
        assert_eq!(
            warnings(&render(SRC, &host)),
            [format!(
                "doc.md:{}: warning: the host declares no grammar for `x`, so this code is \
                 not highlighted\n  bad.sublime-syntax:{}: this grammar could not be read, \
                 so the languages it names are unknown: Invalid YAML file syntax: mapping \
                 values are not allowed in this context at line 2 column 9",
                at(SRC, "x"),
                at(BAD, "x«»: y"),
            )]
        );
    }

    #[test]
    fn leaves_a_block_whose_grammar_does_not_load_as_comrak_writes_it() {
        const BROKEN: &str =
            "name: B\nscope: source.b\nfile_extensions: [b]\ncontexts: {}\n";
        const SRC: &str = "```b\n<code>\n```\n";
        let host = Host::new().with_grammars([("b.sublime-syntax", BROKEN)]);
        let prose = render(SRC, &host);
        assert_eq!(prose.chunks, render(SRC, &Host::new()).chunks);
        assert_eq!(
            warnings(&prose),
            [format!(
                "doc.md:{}: warning: the grammar for `b` does not load, so this code is \
                 not highlighted\n  b.sublime-syntax:{}: it does not load: Context 'main' \
                 is missing",
                at(SRC, "«b»\n"),
                at(BROKEN, BROKEN),
            )]
        );
    }

    #[test]
    fn tells_only_a_block_that_names_a_grammar_that_it_does_not_load() {
        // An HTML block embedding the broken CSS still highlights, its `<style>` body
        // plain; the failure is said where a block asks for CSS by name.
        const CSS: &str = "name: CSS\nscope: source.css\nfile_extensions: [css]\n\
                           contexts: {}\n";
        const SRC: &str = "```html\n<style>a {}</style>\n```\n\n```css\na {}\n```\n";
        let host = Host::new().with_grammars([("css", CSS), ("html", GRAMMARS[1].1)]);
        let prose = render(SRC, &host);
        assert!(
            prose.chunks[0].contains("syn-entity"),
            "{}",
            prose.chunks[0]
        );
        assert_eq!(
            warnings(&prose),
            [format!(
                "doc.md:{}: warning: the grammar for `css` does not load, so this code is \
                 not highlighted\n  css:{}: it does not load: Context 'main' is missing",
                at(SRC, "```«css»"),
                at(CSS, CSS),
            )]
        );
    }

    #[test]
    fn leaves_a_block_a_grammar_fails_on_as_comrak_writes_it() {
        const NOWHERE: &str = "name: N\nscope: source.n\nfile_extensions: [n]\n\
                               contexts:\n  main:\n    - match: x\n      push: nowhere\n";
        const SRC: &str = "```n\nx\n```\n";
        let host = Host::new().with_grammars([("n", NOWHERE)]);
        let prose = render(SRC, &host);
        assert_eq!(prose.chunks, render(SRC, &Host::new()).chunks);
        let warnings = warnings(&prose);
        assert_eq!(warnings.len(), 1);
        let expected = format!(
            "doc.md:{}: warning: the grammar for `n` failed on this code, so it is not \
             highlighted: ",
            at(SRC, "«n»\n")
        );
        assert!(warnings[0].starts_with(&expected), "{warnings:?}");
    }
}

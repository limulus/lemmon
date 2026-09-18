//! Code blocks as marks (D42): each written exactly as comrak writes it, and marked so
//! the host's producer for its language can replace the element after render.
//!
//! comrak writes a code block's element itself, asking its highlighter seam for the
//! opening `<pre>` and `<code>` tags and for the body, then closing both. The adapter
//! here highlights nothing. It writes comrak's own tags and escaped body — what comrak
//! writes with no adapter at all — and a sentinel before the `<pre>`, so that the
//! element can be found in the output: it runs from the sentinel to the first
//! `</code></pre>` after it, which an escaped body cannot contain.
//!
//! A `math` fence is no mark. comrak renders it before consulting any seam, and no
//! option turns that off, so the block stays comrak's own.
//!
//! Every block is also found in the tree beforehand, as
//! [`Headings`](super::adapters::Headings) finds headings: the adapter sees neither where
//! a block is written nor its whole info string, and the mark's span and the payload
//! need both. comrak renders blocks in tree order — footnote definitions included, which
//! it has moved to the end of the tree by then — so a sentinel's number is the block's
//! index in that walk.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::hash::Hasher;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use comrak::adapters::SyntaxHighlighterAdapter;
use lemmon_syntax::Span;
use siphasher::sip::SipHasher13;

use super::is_cmark_space;

/// The mark name every code block is placed under (D42): the core's contract with the
/// host, not a name any claim chooses.
pub(crate) const CODE: &str = "code";

/// What a code block hands its producer (D42): the language word, the whole info string,
/// and the code. Reported once, when the document compiles, and named by its id wherever
/// a render marks the block, so nothing about the code crosses per render.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodePayload {
    /// A hash of the rest, as sixteen hex digits. The same block has the same id wherever
    /// it stands, which is what a host caches its producer's output on.
    pub id: Rc<str>,
    /// The first word of the info string, decoded — what the block's mark is keyed by.
    /// `None` for a block that names none, which no producer can claim.
    pub language: Option<Rc<str>>,
    /// The whole info string, decoded: `ts {1,3}` for a fence opening `` ```ts {1,3} ``.
    pub info: Rc<str>,
    /// The code, as comrak read it.
    pub code: Rc<str>,
}

impl CodePayload {
    /// The payload of a block whose info string is `info` and whose code is `code`.
    pub(super) fn new(info: &str, code: &str) -> Self {
        let language = info
            .split(is_cmark_space)
            .next()
            .filter(|word| !word.is_empty());
        Self {
            id: id(info, code),
            language: language.map(Rc::from),
            info: Rc::from(info),
            code: Rc::from(code),
        }
    }
}

/// The id a payload is named by: SipHash-1-3 with zero keys over the info string and the
/// code, each prefixed by its length so that no two ways of splitting the same bytes
/// collide. The language is the info string's first word, so hashing the info covers it.
///
/// A fixed algorithm rather than `std`'s `DefaultHasher`, whose output is free to change
/// between Rust releases: a host keeps what its producers made across builds of the core.
fn id(info: &str, code: &str) -> Rc<str> {
    let mut hasher = SipHasher13::new_with_keys(0, 0);
    for part in [info, code] {
        hasher.write(&(part.len() as u64).to_le_bytes());
        hasher.write(part.as_bytes());
    }
    Rc::from(format!("{:016x}", hasher.finish()))
}

/// One code block of the document, as the walk over comrak's tree found it.
pub(super) struct Block {
    pub payload: CodePayload,
    /// Where the block's mark points: its language word as written, or the whole block
    /// where it names none.
    pub span: Span,
}

/// A run of a document's rendered prose: text, or a code block to mark.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Piece {
    Text(String),
    Code(Code),
}

impl Piece {
    /// What this piece puts in the page before any handler runs: the text, or the
    /// block's element as comrak wrote it.
    pub(crate) fn text(&self) -> &str {
        match self {
            Self::Text(text) => text,
            Self::Code(code) => &code.html,
        }
    }
}

/// A code block standing in a document's prose: its element as comrak wrote it — the
/// mark's default content — and what the mark carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Code {
    pub html: String,
    /// The language word the mark is keyed by, where the block names one.
    pub key: Option<Rc<str>>,
    /// The id of the payload the document reported for this block.
    pub id: Rc<str>,
    pub span: Span,
}

/// comrak's highlighter seam, writing what comrak writes without one, and a numbered
/// sentinel before each block.
pub(super) struct CodeFences {
    next: AtomicUsize,
}

impl CodeFences {
    pub(super) const fn new() -> Self {
        Self {
            next: AtomicUsize::new(0),
        }
    }
}

impl SyntaxHighlighterAdapter for CodeFences {
    fn write_highlighted(
        &self,
        output: &mut dyn fmt::Write,
        _: Option<&str>,
        code: &str,
    ) -> fmt::Result {
        comrak::html::escape(output, code)
    }

    fn write_pre_tag(
        &self,
        output: &mut dyn fmt::Write,
        attributes: HashMap<&'static str, Cow<'_, str>>,
    ) -> fmt::Result {
        let index = self.next.fetch_add(1, Ordering::Relaxed);
        write!(output, "{OPEN}{index}-->")?;
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

/// What opens a block's sentinel. It cannot come from an author: `<!--lem:` anywhere in
/// the prose is refused before comrak runs, and inside a code block the `<` is escaped.
const OPEN: &str = "<!--lem:code:";

/// What comrak closes every block it gave the adapter with.
const CLOSE: &str = "</code></pre>";

/// `chunk` cut into its text and the code blocks its sentinels mark, the sentinels
/// themselves dropped: what the chunk puts in the page is unchanged by the cutting.
pub(super) fn pieces(chunk: &str, blocks: &[Block]) -> Vec<Piece> {
    let mut pieces = Vec::new();
    let mut rest = chunk;
    while let Some(at) = rest.find(OPEN) {
        if at > 0 {
            pieces.push(Piece::Text(rest[..at].to_owned()));
        }
        let numbered = &rest[at + OPEN.len()..];
        let digits = numbered
            .find("-->")
            .expect("the adapter closes its sentinel");
        let index: usize = numbered[..digits]
            .parse()
            .expect("the adapter numbers its sentinels");
        let element = &numbered[digits + "-->".len()..];
        let end = element
            .find(CLOSE)
            .expect("comrak closes every block it gives the adapter")
            + CLOSE.len();
        let block = &blocks[index];
        pieces.push(Piece::Code(Code {
            html: element[..end].to_owned(),
            key: block.payload.language.clone(),
            id: block.payload.id.clone(),
            span: block.span,
        }));
        rest = &element[end..];
    }
    if !rest.is_empty() {
        pieces.push(Piece::Text(rest.to_owned()));
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_a_payload_by_its_content_alone() {
        // The id is what a host caches on, so it is a function of the payload and of
        // nothing else: the same block anywhere is one id, and changing either field is
        // another.
        let one = CodePayload::new("ts {1,3}", "let x = 1\n");
        assert_eq!(one, CodePayload::new("ts {1,3}", "let x = 1\n"));
        assert_ne!(one.id, CodePayload::new("ts", "let x = 1\n").id);
        assert_ne!(one.id, CodePayload::new("ts {1,3}", "let x = 2\n").id);
        assert_eq!(one.id.len(), 16);
        assert_eq!(
            (one.language.as_deref(), &*one.info),
            (Some("ts"), "ts {1,3}")
        );
    }

    #[test]
    fn keeps_two_splits_of_the_same_bytes_apart() {
        // Without the length prefixes, `a b` + `c` and `a` + ` bc` would hash the same
        // bytes in the same order.
        assert_ne!(
            CodePayload::new("a b", "c").id,
            CodePayload::new("a", " bc").id
        );
    }

    #[test]
    fn pins_the_id_across_builds_of_the_core() {
        // A host's cache outlives one build, so the id is a fixed algorithm and this
        // value may only change with a note to every host that keeps one. It was checked
        // against SipHash-1-3 written independently from the paper, over the same
        // length-prefixed bytes, so the doc above names what actually runs.
        assert_eq!(&*CodePayload::new("sh", "echo hi\n").id, "4429e5aa07a9b5e3");
    }

    #[test]
    fn keys_a_block_that_names_no_language_by_nothing() {
        let payload = CodePayload::new("", "plain\n");
        assert_eq!((payload.language, &*payload.info), (None, ""));
    }
}

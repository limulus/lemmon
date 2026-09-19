//! The comrak plugin seams the core fills in.
//!
//! comrak renders most of a document itself; what it cannot do is supplied here through
//! the adapter traits it exposes, and passed to it as `Plugins` by
//! [`render_prose`](super::render_prose). Here that is heading ids; the seam that marks
//! every code block is a module of its own, [`code`](super::code). Options comrak *does*
//! honour belong with the splice they constrain, in [`super`], not here.

use std::collections::HashSet;
use std::fmt;
use std::sync::Mutex;

use comrak::Anchorizer;
use comrak::adapters::{HeadingAdapter, HeadingMeta};
use comrak::nodes::{NodeValue, Sourcepos};

/// Renders headings as `<hN id="…">`: the `{#id}` the author wrote, or a slug of the
/// heading text. The ids are computed from the tree up front — the adapter sees only a
/// heading's level and text — and handed out in document order, which is the order
/// comrak renders in.
///
/// Ids the author wrote are taken as written and reserved before any is generated, so a
/// slug never collides with one wherever in the document it stands. Two headings given
/// the *same* explicit id keep it, and the duplicate is the author's to fix: silently
/// renaming what someone linked to would be worse than the invalid HTML.
pub(super) struct Headings {
    ids: Mutex<std::vec::IntoIter<String>>,
}

impl Headings {
    pub(super) fn new(root: comrak::nodes::Node<'_>) -> Self {
        let headings: Vec<(String, Option<String>)> = root
            .descendants()
            .filter_map(|node| {
                let data = node.data.borrow();
                let NodeValue::Heading(_) = &data.value else {
                    return None;
                };
                let explicit = data.attrs.as_ref().and_then(|attrs| attrs.id.clone());
                Some((node.collect_text(), explicit))
            })
            .collect();
        let mut used: HashSet<String> = headings
            .iter()
            .filter_map(|(_, explicit)| explicit.clone())
            .collect();
        let ids: Vec<String> = headings
            .iter()
            .map(|(text, explicit)| match explicit {
                Some(id) => id.clone(),
                None => {
                    // A fresh anchorizer slugs without deduplicating; uniqueness is
                    // settled here instead, where the explicit ids are known too.
                    let base = Anchorizer::new().anchorize(text);
                    let mut id = base.clone();
                    let mut n = 1;
                    while used.contains(&id) {
                        id = format!("{base}-{n}");
                        n += 1;
                    }
                    used.insert(id.clone());
                    id
                }
            })
            .collect();
        Self {
            ids: Mutex::new(ids.into_iter()),
        }
    }
}

impl HeadingAdapter for Headings {
    fn enter(
        &self,
        output: &mut dyn fmt::Write,
        heading: &HeadingMeta,
        _: Option<Sourcepos>,
    ) -> fmt::Result {
        let id = self
            .ids
            .lock()
            .expect("no panic holds the ids")
            .next()
            .expect("one id per heading in the tree");
        write!(output, "<h{} id=\"", heading.level)?;
        comrak::html::escape(output, &id)?;
        output.write_str("\">")
    }

    fn exit(&self, output: &mut dyn fmt::Write, heading: &HeadingMeta) -> fmt::Result {
        writeln!(output, "</h{}>", heading.level)
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostics::Diagnostic;

    /// Headings are checked through the whole prose stage, since that is what wires
    /// the adapter to comrak — an adapter tested apart from `Plugins` would prove
    /// nothing about the HTML a document actually gets.
    fn render(src: &str) -> Result<Vec<String>, Vec<Diagnostic>> {
        let doc = lemmon_syntax::parse_document(src);
        super::super::render_prose("doc.md", src, &doc).map(|prose| {
            prose
                .chunks
                .iter()
                .map(|pieces| pieces.iter().map(super::super::Piece::text).collect())
                .collect()
        })
    }

    #[test]
    fn renders_footnotes_and_heading_ids() {
        let chunks = render("# Title {#t}\n\n## Other\n\nx[^1]\n\n[^1]: note\n").unwrap();
        assert!(
            chunks[0].starts_with("<h1 id=\"t\">Title</h1>\n<h2 id=\"other\">Other</h2>"),
            "{}",
            chunks[0]
        );
        assert!(chunks[0].contains("class=\"footnotes\""), "{}", chunks[0]);
    }

    #[test]
    fn keeps_a_slug_clear_of_an_id_the_author_wrote_anywhere() {
        let chunks = render("# Bar\n\n# Foo {#bar}\n").unwrap();
        assert_eq!(
            chunks[0],
            "<h1 id=\"bar-1\">Bar</h1>\n<h1 id=\"bar\">Foo</h1>\n"
        );
    }

    #[test]
    fn keeps_repeated_heading_ids_unique() {
        let chunks = render("# A\n\n# A\n").unwrap();
        assert_eq!(chunks[0], "<h1 id=\"a\">A</h1>\n<h1 id=\"a-1\">A</h1>\n");
    }
}

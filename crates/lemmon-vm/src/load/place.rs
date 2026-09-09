//! Where a load diagnostic points.
//!
//! The data the loader validates comes from files the core does not hold — front
//! matter, data files — and from sources with no file at all. The host, which holds
//! the text, sends positions with the data: per loaded value, a table from field path
//! to a complete span (D37). A [`Place`] is one position a diagnostic can be given, and
//! an [`At`] is the loader's cursor into a value: the table, the field path so far, and
//! the declaration to fall back to — of the field, binding, or type that expected the
//! value — when the table has nothing for the path.

use std::collections::BTreeMap;

use lemmon_syntax::{Range, Span};

use crate::compile::Diagnostic;

/// A span in a file the host holds, placed by the host: the byte offsets the core
/// works in and the same two points as positions, since no resident index can place
/// them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostSpan {
    pub span: Span,
    pub range: Range,
}

/// The positions the host sends with one loaded value, by field path: `title`,
/// `hero.caption`, `tags.2`. The empty path is the value as a whole. A path's span is
/// its whole entry — the key through the value — so that a diagnostic about either
/// reads the same way.
pub type Spans = BTreeMap<String, HostSpan>;

/// The field path of `key` under `prefix`, as a [`Spans`] table keys it: `hero` under
/// nothing is `hero`, `caption` under `hero` is `hero.caption`, and an element of
/// `tags` is `tags.2`. The one spelling every producer of a table and the loader share.
#[must_use]
pub fn field_path(prefix: &str, key: &str) -> String {
    if prefix.is_empty() {
        key.to_owned()
    } else {
        format!("{prefix}.{key}")
    }
}

/// One position a diagnostic can be given: a file, a span in it, and the host's
/// positions for that span when the file is one the host holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Place {
    pub(super) path: String,
    pub(super) span: Span,
    pub(super) range: Option<Range>,
}

impl Place {
    /// A place in a resident file, which an index places.
    pub(super) fn resident(path: &str, span: Span) -> Self {
        Self {
            path: path.to_owned(),
            span,
            range: None,
        }
    }

    /// An error here.
    pub(super) fn error(&self, message: impl Into<String>) -> Diagnostic {
        Diagnostic::error(&self.path, self.span, message).placed_by_host(self.range)
    }
}

/// The loader's cursor into a loaded value.
#[derive(Clone, Debug)]
pub(super) struct At<'a> {
    /// The value's table, with the path of the file it describes — `None` for a value
    /// with no file behind it.
    table: Option<(&'a str, &'a Spans)>,
    /// The field path from the value's root, as the table keys it.
    key: String,
    /// What a message names the root: the binding or ambient name. A document's root
    /// is unnamed, so its fields read bare.
    root: String,
    /// Where to point when the table says nothing about the path: the declaration of
    /// what was expected here.
    declared: Place,
}

impl<'a> At<'a> {
    /// The root of a value: named `root` in messages, placed by `table` when there is
    /// one, and by `declared` otherwise.
    pub(super) fn root(
        table: Option<(&'a str, &'a Spans)>,
        root: &str,
        declared: Place,
    ) -> Self {
        Self {
            table,
            key: String::new(),
            root: root.to_owned(),
            declared,
        }
    }

    /// The field or element `key` of this value, expected where `declared` says.
    pub(super) fn child(&self, key: &str, declared: Place) -> Self {
        Self {
            table: self.table,
            key: field_path(&self.key, key),
            root: self.root.clone(),
            declared,
        }
    }

    /// The element `key` of this list or map, expected where this value was.
    pub(super) fn element(&self, key: &str) -> Self {
        self.child(key, self.declared.clone())
    }

    /// How a message names this value: the root's name, then the path.
    pub(super) fn label(&self) -> String {
        if self.key.is_empty() {
            self.root.clone()
        } else {
            field_path(&self.root, &self.key)
        }
    }

    /// Where a diagnostic about this value goes: the table's span for the path, else
    /// for its nearest ancestor, else for the value as a whole; and with no table entry
    /// at all, the declaration of what was expected.
    pub(super) fn place(&self) -> Place {
        if let Some((path, spans)) = self.table {
            let mut key = self.key.as_str();
            loop {
                if let Some(found) = spans.get(key) {
                    return Place {
                        path: path.to_owned(),
                        span: found.span,
                        range: Some(found.range),
                    };
                }
                if key.is_empty() {
                    break;
                }
                key = key.rfind('.').map_or("", |dot| &key[..dot]);
            }
        }
        self.declared.clone()
    }

    /// An error about this value.
    pub(super) fn error(&self, message: impl Into<String>) -> Diagnostic {
        self.place().error(message)
    }
}

#[cfg(test)]
mod tests {
    use lemmon_syntax::Position;

    use super::*;

    fn host_span(start: u32, end: u32) -> HostSpan {
        HostSpan {
            span: Span::new(start, end),
            range: Range {
                start: Position {
                    line: 1,
                    character: start,
                },
                end: Position {
                    line: 1,
                    character: end,
                },
            },
        }
    }

    fn declared() -> Place {
        Place::resident("site.lem", Span::new(3, 8))
    }

    #[test]
    fn places_a_path_by_its_own_entry_when_the_table_has_one() {
        let spans: Spans = [
            (String::new(), host_span(0, 40)),
            ("hero".to_owned(), host_span(10, 30)),
            ("hero.caption".to_owned(), host_span(18, 30)),
        ]
        .into_iter()
        .collect();
        let root = At::root(Some(("a.md", &spans)), "", declared());
        let caption = root.child("hero", declared()).child("caption", declared());
        let place = caption.place();
        assert_eq!(
            (place.path.as_str(), place.span),
            ("a.md", Span::new(18, 30))
        );
        assert_eq!(place.range.map(|r| r.start.character), Some(18));
        assert_eq!(caption.label(), "hero.caption");
    }

    #[test]
    fn falls_back_to_the_nearest_ancestor_then_the_value_as_a_whole() {
        let spans: Spans = [
            (String::new(), host_span(0, 40)),
            ("hero".to_owned(), host_span(10, 30)),
        ]
        .into_iter()
        .collect();
        let root = At::root(Some(("a.md", &spans)), "", declared());
        let hero = root.child("hero", declared());
        assert_eq!(
            hero.child("caption", declared()).place().span,
            Span::new(10, 30)
        );
        assert_eq!(
            hero.child("caption", declared()).element("0").place().span,
            Span::new(10, 30)
        );
        assert_eq!(
            root.child("title", declared()).place().span,
            Span::new(0, 40)
        );
    }

    #[test]
    fn falls_back_to_the_declaration_when_the_table_says_nothing() {
        let spans: Spans = Spans::new();
        let root = At::root(Some(("a.md", &spans)), "", declared());
        assert_eq!(root.child("title", declared()).place(), declared());
        let unplaced = At::root(None, "build", declared());
        assert_eq!(unplaced.label(), "build");
        assert_eq!(unplaced.child("time", declared()).place(), declared());
        assert_eq!(unplaced.child("time", declared()).label(), "build.time");
        assert_eq!(field_path("", "title"), "title");
        assert_eq!(field_path("tags", "2"), "tags.2");
    }

    #[test]
    fn writes_an_error_at_the_place_with_the_hosts_positions() {
        let spans: Spans = [("title".to_owned(), host_span(4, 9))]
            .into_iter()
            .collect();
        let root = At::root(Some(("a.md", &spans)), "", declared());
        let error = root.child("title", declared()).error("bad");
        assert_eq!(error.to_string(), "a.md:4..9: bad");
        assert_eq!(error.range.map(|r| r.end.character), Some(9));
        let fallback = root.child("other", declared()).error("bad");
        assert_eq!(fallback.to_string(), "site.lem:3..8: bad");
        assert_eq!(fallback.range, None);
    }
}

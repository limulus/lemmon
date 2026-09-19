//! Helpers shared by this crate's unit tests.
//!
//! [`at`] and [`all_of`] give a diagnostic's expected span as the source text it covers,
//! and [`shown`] prints diagnostics for comparison. The rest are fixtures: the hosts
//! these tests compile against, the declarations those hosts read, and the calls that
//! compile one file into a [`Unit`] or compile and link a whole set of them.

use std::ops::Range;

use lemmon_syntax::Span;

use crate::SourceFile;
use crate::bytecode::Module;
use crate::compile::{compile_unit, prepare};
use crate::diagnostics::{Compiled, Diagnostic};
use crate::exports::ExportIndex;
use crate::host::{Ambient, Host};
use crate::unit::{Unit, link};

/// The span of `needle` in `source`.
///
/// `needle` is literal source text that `source` must contain exactly once, so a test
/// asserts the span it means rather than the byte offsets it counted. Bracket part of
/// the needle with `«…»` to span only that part, leaving the rest as context that makes
/// the match unique — the only way to name one of several identical names, as
/// `at(src, "f(«a»: int")` names the first `a` of `f(a: int, a: int)`. An empty bracket
/// gives the zero-width span at that point.
///
/// A stray or doubled bracket needs no check of its own: it stays in the text looked
/// for, which `source` then does not contain.
///
/// # Panics
///
/// If a bracket is opened and never closed, or if `source` does not hold the needle
/// exactly once — a repeat would otherwise assert whichever occurrence came first.
pub(crate) fn at(source: &str, needle: &str) -> Span {
    let (text, marked) = unbracketed(needle);
    // Every start, not `match_indices`' non-overlapping ones: `--` occurs twice in
    // `---`, and a needle the source repeats has to be refused however the repeats sit.
    let mut starts = source
        .char_indices()
        .map(|(start, _)| start)
        .chain(std::iter::once(source.len()))
        .filter(|&start| source[start..].starts_with(&text));
    let found = match (starts.next(), starts.next()) {
        (Some(found), None) => found,
        (None, _) => panic!("`{text}` is not in the source"),
        // Two are already taken off the iterator; the rest are what is left.
        (Some(_), Some(_)) => panic!(
            "`{text}` is in the source {} times: bracket the span with `«…»` inside a \
             longer needle that is not",
            2 + starts.count()
        ),
    };
    let start = offset(found + marked.start);
    Span::new(start, start + offset(marked.len()))
}

/// The span of all of `source`, which is what a host sends for a value as a whole.
pub(crate) fn all_of(source: &str) -> Span {
    Span::new(0, offset(source.len()))
}

/// `needle` with its brackets removed, and the byte range they marked in what is left.
fn unbracketed(needle: &str) -> (String, Range<usize>) {
    let Some((before, rest)) = needle.split_once('«') else {
        return (needle.to_owned(), 0..needle.len());
    };
    let (marked, after) = rest
        .split_once('»')
        .unwrap_or_else(|| panic!("`{needle}` opens a span it never closes"));
    (
        format!("{before}{marked}{after}"),
        before.len()..before.len() + marked.len(),
    )
}

fn offset(bytes: usize) -> u32 {
    u32::try_from(bytes).expect("fixtures are smaller than 4 GiB")
}

/// The host a test compiles against unless it needs another: the three names the
/// standard shell binds, none of them typed.
pub(crate) fn untyped_host() -> Host {
    Host::untyped(["site", "page", "build"])
}

/// A host shaped like the standard shell's, reading `site` from `site.lem`: `page` is a
/// declared `Page`, and every page renders through `layout`. It names no url field;
/// [`addressed_host`] is the same host with the url check armed.
pub(crate) fn shell_host() -> Host {
    Host::new()
        .bind("page", Ambient::named("Page"))
        .bind("site", Ambient::bindings("site.lem"))
        .documents("page")
        .rendered_through("layout")
}

/// [`shell_host`] with its pages addressed by their `url` field (D43) — the host the link
/// step's tests use, so that a page's url is checked unless a test says otherwise.
pub(crate) fn addressed_host() -> Host {
    shell_host().addressed_by("url")
}

/// `shell_host`'s document type, as a host really passes it: an ordinary file, compiled
/// with the program.
pub(crate) const AMBIENT: &str = "<lem-type Page>\n  url: url\n</lem-type>";

/// A page schema extending `Page` with a title and a layout field.
pub(crate) const POST: &str = "<lem-type Post extends Page>\n  title: string\n  \
     layout: (Post, html): html\n</lem-type>";

/// Compiles the file at `path` into a unit against an index of every given file's
/// exports.
pub(crate) fn unit_of(path: &str, index_of: &[(&str, &str)]) -> Unit {
    let prepared: Vec<_> = index_of
        .iter()
        .map(|(path, text)| {
            prepare(&SourceFile::new(*path, *text), &untyped_host()).unwrap()
        })
        .collect();
    let mut index = ExportIndex::new();
    for p in &prepared {
        index.offer(p.path(), p.exports());
        index.offer_types(p.path(), p.types());
    }
    let this = prepared
        .iter()
        .find(|p| p.path() == path)
        .expect("the file compiles against an index that includes it");
    compile_unit(this, &index, &untyped_host()).expect_output()
}

/// Compiles the file at `path` against `host`, and against an index of every given
/// file's exports.
pub(crate) fn unit_against(host: &Host, path: &str, files: &[(&str, &str)]) -> Unit {
    let prepared: Vec<_> = files
        .iter()
        .map(|(path, text)| prepare(&SourceFile::new(*path, *text), host).unwrap())
        .collect();
    let mut index = ExportIndex::new();
    for p in &prepared {
        index.offer(p.path(), p.exports());
        index.offer_types(p.path(), p.types());
    }
    let this = prepared
        .iter()
        .find(|p| p.path() == path)
        .expect("the file compiles against an index that includes it");
    compile_unit(this, &index, host).expect_output()
}

/// Every file compiled against `host` and linked: the module, or what stopped it.
pub(crate) fn linked(host: &Host, files: &[(&str, &str)]) -> Compiled<Module> {
    let units: Vec<Unit> = files
        .iter()
        .map(|(path, _)| unit_against(host, path, files))
        .collect();
    link(&units, &ExportIndex::of(&units), host)
}

/// Every diagnostic as the author is shown it.
pub(crate) fn shown(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics.iter().map(ToString::to_string).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_a_needle_the_source_holds_once() {
        let src = "<lem-let raw: string = \"x\" />";
        assert_eq!(at(src, "raw"), Span::new(9, 12));
        assert_eq!(&src[9..12], "raw");
    }

    #[test]
    fn spans_only_what_the_brackets_hold() {
        let src = "<lem-fn f(a: int, a: int): int = a />";
        assert_eq!(at(src, "f(«a»: int"), Span::new(10, 11));
        assert_eq!(at(src, ", «a»: int"), Span::new(18, 19));
    }

    #[test]
    fn counts_the_bytes_a_multibyte_source_puts_before_the_needle() {
        // The offsets this replaces were counted through two entities and an é.
        let src = "&amp;&amp;é <lem.x> here\n";
        assert_eq!(at(src, "<lem.x>"), Span::new(13, 20));
    }

    #[test]
    fn gives_an_empty_bracket_the_zero_width_span_at_that_point() {
        assert_eq!(
            at("---\ntype: Nope\n---\nhi", "«»---\ntype"),
            Span::new(0, 0)
        );
    }

    #[test]
    fn spans_a_whole_source() {
        assert_eq!(all_of("{\"a\": 1}"), Span::new(0, 8));
        assert_eq!(all_of(""), Span::new(0, 0));
    }

    #[test]
    #[should_panic(expected = "`nope` is not in the source")]
    fn refuses_a_needle_the_source_does_not_hold() {
        at("<lem.x>", "nope");
    }

    #[test]
    #[should_panic(expected = "`t` is in the source 10 times")]
    fn refuses_a_needle_the_source_holds_more_than_once() {
        at(
            "<lem-type A>\n  t: string = self.nope\n</lem-type>\n\
             <lem-type B>\n  t: string = \"1\"\n  t: string = self.nope\n</lem-type>",
            "t",
        );
    }

    #[test]
    #[should_panic(expected = "`--` is in the source 2 times")]
    fn refuses_a_needle_whose_repeats_overlap() {
        // `match_indices` sees one `--` in `---` and would take the needle for unique.
        at("---", "--");
    }

    #[test]
    #[should_panic(expected = "`f(«a: int` opens a span it never closes")]
    fn refuses_a_bracket_that_is_never_closed() {
        at("<lem-fn f(a: int): int = a />", "f(«a: int");
    }

    #[test]
    #[should_panic(expected = "`lem«.x»` is not in the source")]
    fn leaves_a_doubled_bracket_in_the_text_it_looks_for() {
        // No check of its own: the second pair stays in the needle and finds nothing.
        at("<lem.x>", "«lem»«.x»");
    }
}

//! The page table: what a load made of each page, and where that page stands.
//!
//! [`Loader::page`] loads one page's front matter as the record its file was compiled
//! as (D41), keeping it with the positions the host sent for it, and [`Store::page`]
//! reads it back by path for the render that binds it. Where a page stands is the url
//! field the host declaration names (D43): a load reports a page whose record holds no
//! url there, and two pages that hold the same one, and [`Store::plan`] lists every
//! loaded page with the url it holds.

use std::collections::BTreeMap;
use std::rc::Rc;

use lemmon_syntax::Span;

use crate::diagnostics::Diagnostic;
use crate::host::Host;
use crate::value::Value;

use super::place::{At, Place};
use super::{Doc, Loaded, Loader, Spans, Store};

/// One page as loaded: its path, the record its front matter was loaded as — what a
/// render of the page binds the host's document value to — and where a diagnostic
/// about it after the load can point.
#[derive(Clone, Debug, PartialEq)]
pub struct LoadedPage {
    pub path: String,
    pub record: Value,
    /// Where each field path of its front matter stands in the file (D37).
    pub spans: Spans,
    /// Where its record was named, for what the table cannot place — a field a default
    /// derived.
    pub record_span: Span,
}

/// One page as `plan()` lists it (D43): its path, and its url where the host names the
/// field it is read from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedPage {
    pub path: String,
    pub url: Option<String>,
}

impl Store {
    /// The loaded page at `path`, if one is.
    #[must_use]
    pub fn page(&self, path: &str) -> Option<&LoadedPage> {
        self.pages.iter().find(|page| page.path == path)
    }

    /// Every loaded page in load order, each with the url its record holds in the
    /// field `host` names — none where the host names no field.
    #[must_use]
    pub fn plan(&self, host: &Host) -> Vec<PlannedPage> {
        self.pages
            .iter()
            .map(|page| PlannedPage {
                path: page.path.clone(),
                url: host
                    .url()
                    .and_then(|field| page_url(page, field))
                    .map(ToString::to_string),
            })
            .collect()
    }
}

/// The url of a loaded page: what the field `field` of its record holds, when that is
/// a url. The link step saw the document type declare the field as `url`, and a load
/// validated the record against it, so a loaded page without one is a bug rather than
/// an author's mistake — which is why this answers `None` and its callers say so, and
/// none of them `expect` (invariant 8).
fn page_url<'a>(page: &'a LoadedPage, field: &str) -> Option<&'a Rc<str>> {
    let Value::Record(record) = &page.record else {
        return None;
    };
    match record.fields.get(field) {
        Some(Value::Url(url)) => Some(url),
        _ => None,
    }
}

/// Where a diagnostic about the url of `page` goes: the table's entry for `field`,
/// else the page's own value, else where its record was named — as for a url a
/// default derived, which no key in the file gave.
fn url_place(page: &LoadedPage, field: &str) -> Place {
    let declared = Place::resident(&page.path, page.record_span);
    At::root(Some((&page.path, &page.spans)), "", declared.clone())
        .child(field, declared)
        .place()
}

/// Every loaded page checked against the others for the url in `field` (D43): one url
/// is one page, so two at one url is an error at the second page's url, labelled at
/// the first's. A page whose record holds no url is an error too, at the same place.
pub(super) fn url_diagnostics(pages: &[LoadedPage], field: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut first: BTreeMap<&str, &LoadedPage> = BTreeMap::new();
    for page in pages {
        let Some(url) = page_url(page, field) else {
            diagnostics.push(url_place(page, field).error(format!(
                "`{}` holds no url in `{field}`, so it stands nowhere",
                page.path
            )));
            continue;
        };
        match first.get(&**url) {
            None => {
                first.insert(url, page);
            }
            Some(taken) => {
                let there = url_place(taken, field);
                diagnostics.push(
                    url_place(page, field)
                        .error(format!(
                            "`{}` and `{}` are both at `{url}`",
                            page.path, taken.path
                        ))
                        .with_placed_label(
                            there.path,
                            there.span,
                            there.range,
                            "the first page at this url",
                        ),
                );
            }
        }
    }
    diagnostics
}

impl Loader<'_> {
    /// Loads one page: its front matter as the record it was compiled as, with its
    /// path and, for a document, its rendered prose in reach of the record's defaults.
    /// A member of a glob binding's `collection` — the element type it collects and
    /// the binding's name — must be of a record extending that type. The record is
    /// kept, in load order, for the render that binds it, with the positions the host
    /// sent for what is said about the page after it loaded.
    pub(super) fn page(
        &mut self,
        loaded: Loaded,
        collection: Option<(&Rc<str>, &str)>,
    ) -> Option<Value> {
        let path = loaded.path.as_deref().expect("checked by `check_envelope`");
        let compiled = self.files[path];
        let compiled = self.module.documents[compiled]
            .as_ref()
            .expect("checked by `check_envelope`");
        let record = compiled
            .record
            .clone()
            .expect("checked by `check_envelope`");
        if let Some((element, binding)) = collection
            && !self.module.is_a(&record, element)
        {
            self.diagnostics
                .push(Place::resident(path, compiled.record_span).error(format!(
                    "`{record}` does not extend `{element}`, the type `{binding}` collects"
                )));
            return None;
        }
        let at = At::root(Some((path, &loaded.spans)), "", self.type_place(&record));
        let Value::Map(given) = &loaded.value else {
            self.diagnostics.push(at.error(format!(
                "expected the front matter as a mapping of `{record}` fields, got `{}`",
                loaded.value.type_name()
            )));
            return None;
        };
        let doc = Doc {
            path,
            body: compiled.body.as_ref(),
        };
        let value = self.record(given, &record, &at, Some(&doc))?;
        self.pages.push(LoadedPage {
            path: path.to_owned(),
            record: value.clone(),
            spans: loaded.spans,
            record_span: compiled.record_span,
        });
        Some(value)
    }
}

/// The url rules and the plan. [`Loader::page`]'s two refusals — a document whose record
/// is not what the binding collects, and front matter that is no mapping — are tested in
/// `load::tests`, beside the bindings fixtures they need.
#[cfg(test)]
mod tests {
    use lemmon_syntax::spans::at;

    use super::*;
    use crate::bytecode::Module;
    use crate::load::tests::{build, document, errors, object, page, shell, spans};
    use crate::load::{Data, Source, load};
    use crate::value::Map;
    use crate::{SourceFile, compile};

    /// A document type placing pages by `url`, and two page schemas: `Article`, whose
    /// url its front matter gives, and `Note`, whose url a default derives.
    const PLACED: &str = "<lem-type Page>\n  url: url\n</lem-type>\n\
         <lem-type Build>\n  generator: string\n</lem-type>";
    const ROUTED: &str = "<lem-type Article extends Page>\n  title: string\n  \
         layout: (Article, html): html = f\n</lem-type>\n\
         <lem-type Note extends Page>\n  url: url = url(\"/n/\")\n  \
         layout: (Note, html): html = g\n</lem-type>\n\
         <lem-fn f(a: Article, b: html)><lem.b></lem-fn>\n\
         <lem-fn g(n: Note, b: html)><lem.b></lem-fn>\n\
         <lem-let posts: [Article] from \"pages/*.md\" />\n\
         <lem-let notes: [Note] from \"notes/*.md\" />";
    const HOME: &str = "---\ntype: Article\ntitle: Home\nurl: /a/\n---\n";
    const ABOUT: &str = "---\ntype: Article\ntitle: About\nurl: /a/\n---\n";
    const NOTE: &str = "---\n---\nnote";

    /// The shell, placing pages by `url`.
    fn placed() -> Host {
        shell().addressed_by("url")
    }

    /// The `ROUTED` site with a member of each collection, a second note, and two
    /// page templates no glob holds, compiled under the placing host.
    fn routed_module() -> Module {
        let files = [
            SourceFile::new("ambient.lem", PLACED),
            SourceFile::new("site.lem", ROUTED),
            document(
                "pages/a.md",
                "---\ntitle: A\nurl: /a/\n---\nbody",
                "Article",
            ),
            document("notes/n.md", NOTE, "Note"),
            document("notes/m.md", NOTE, "Note"),
            document("home.lem", HOME, "Article"),
            document("about.lem", ABOUT, "Article"),
        ];
        compile(&files, &placed()).expect_output()
    }

    /// A page with its front matter's `url` placed by a table over `text`.
    fn placed_page(path: &str, text: &str, front: &[(&str, Value)]) -> Loaded {
        Loaded {
            value: object(front),
            path: Some(path.to_owned()),
            spans: spans(text, &[("url", at(text, "url: «/a/»"))]),
        }
    }

    fn article(url: &str) -> Value {
        object(&[("title", Value::str("A")), ("url", Value::str(url))])
    }

    #[test]
    fn refuses_two_pages_at_one_url_across_a_glob_and_the_envelope() {
        // `pages/a.md` came with no positions, so the label falls to where its record
        // was named; `home.lem` came with them, so the error is at its `url:` key.
        let module = routed_module();
        let data = Data {
            ambient: vec![build()],
            sources: vec![
                (
                    "posts".to_owned(),
                    Source::Many(vec![page("pages/a.md", article("/a/"))]),
                ),
                ("notes".to_owned(), Source::Many(vec![])),
            ],
            pages: vec![placed_page(
                "home.lem",
                HOME,
                &[("title", Value::str("Home")), ("url", Value::str("/a/"))],
            )],
        };
        let a = module.document("pages/a.md").expect("a page source");
        assert_eq!(
            errors(&module, &placed(), data),
            [format!(
                "home.lem:{}: `home.lem` and `pages/a.md` are both at `/a/`\n  \
                 pages/a.md:{}: the first page at this url",
                at(HOME, "url: «/a/»"),
                a.record_span
            )]
        );
    }

    #[test]
    fn refuses_two_pages_at_one_url_the_envelope_carried_written() {
        let module = routed_module();
        let data = Data {
            ambient: vec![build()],
            sources: vec![
                ("posts".to_owned(), Source::Many(vec![])),
                ("notes".to_owned(), Source::Many(vec![])),
            ],
            pages: vec![
                placed_page(
                    "home.lem",
                    HOME,
                    &[("title", Value::str("Home")), ("url", Value::str("/a/"))],
                ),
                placed_page(
                    "about.lem",
                    ABOUT,
                    &[("title", Value::str("About")), ("url", Value::str("/a/"))],
                ),
            ],
        };
        assert_eq!(
            errors(&module, &placed(), data),
            [format!(
                "about.lem:{}: `about.lem` and `home.lem` are both at `/a/`\n  \
                 home.lem:{}: the first page at this url",
                at(ABOUT, "url: «/a/»"),
                at(HOME, "url: «/a/»")
            )]
        );
    }

    #[test]
    fn refuses_two_pages_whose_defaults_derive_one_url() {
        // Neither note's front matter has a `url:` key for the table to place, so
        // both ends point at where each record was named.
        let module = routed_module();
        let data = Data {
            ambient: vec![build()],
            sources: vec![
                ("posts".to_owned(), Source::Many(vec![])),
                (
                    "notes".to_owned(),
                    Source::Many(vec![
                        page("notes/n.md", object(&[])),
                        page("notes/m.md", object(&[])),
                    ]),
                ),
            ],
            pages: Vec::new(),
        };
        let named = |path: &str| module.document(path).expect("a page source").record_span;
        assert_eq!(
            errors(&module, &placed(), data),
            [format!(
                "notes/m.md:{}: `notes/m.md` and `notes/n.md` are both at `/n/`\n  \
                 notes/n.md:{}: the first page at this url",
                named("notes/m.md"),
                named("notes/n.md")
            )]
        );
    }

    #[test]
    fn plans_every_loaded_page_in_store_order_with_its_url() {
        let module = routed_module();
        let data = Data {
            ambient: vec![build()],
            sources: vec![
                (
                    "posts".to_owned(),
                    Source::Many(vec![page("pages/a.md", article("/a/"))]),
                ),
                (
                    "notes".to_owned(),
                    Source::Many(vec![page("notes/n.md", object(&[]))]),
                ),
            ],
            pages: vec![page("home.lem", article("/home/"))],
        };
        let store = load(&module, &placed(), data).unwrap().expect_output();
        let planned = |host: &Host| -> Vec<(String, Option<String>)> {
            store
                .plan(host)
                .into_iter()
                .map(|page| (page.path, page.url))
                .collect()
        };
        assert_eq!(
            planned(&placed()),
            [
                ("pages/a.md".to_owned(), Some("/a/".to_owned())),
                ("notes/n.md".to_owned(), Some("/n/".to_owned())),
                ("home.lem".to_owned(), Some("/home/".to_owned())),
            ]
        );
        // A host that names no url field places nothing.
        assert_eq!(
            planned(&shell()),
            [
                ("pages/a.md".to_owned(), None),
                ("notes/n.md".to_owned(), None),
                ("home.lem".to_owned(), None),
            ]
        );
    }

    #[test]
    fn checks_no_url_for_a_host_that_names_no_url_field() {
        let module = routed_module();
        let data = Data {
            ambient: vec![build()],
            sources: vec![
                (
                    "posts".to_owned(),
                    Source::Many(vec![page("pages/a.md", article("/a/"))]),
                ),
                ("notes".to_owned(), Source::Many(vec![])),
            ],
            pages: vec![page("home.lem", article("/a/"))],
        };
        assert_eq!(errors(&module, &shell(), data), Vec::<String>::new());
    }

    #[test]
    fn reports_a_loaded_page_that_holds_no_url_rather_than_trusting_the_link() {
        // Nothing a load builds can reach here — the link step saw the document type
        // declare the field, and the record was validated against it — so this is
        // the check that the pass would say so rather than trap, were it to happen.
        let unplaced = LoadedPage {
            path: "x.md".to_owned(),
            record: Value::record("Article", Map::default()),
            spans: Spans::new(),
            record_span: Span::new(4, 11),
        };
        let shown: Vec<String> = url_diagnostics(&[unplaced], "url")
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            shown,
            ["x.md:4..11: `x.md` holds no url in `url`, so it stands nowhere"]
        );
    }
}

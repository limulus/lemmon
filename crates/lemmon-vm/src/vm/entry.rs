//! The three ways a module is run: a file's entry ([`render`]), a loaded page through
//! its record's entry field ([`render_page`], D43), and one function for the loader
//! ([`invoke`]).
//!
//! Each builds a [`Vm`], runs its loop, and takes what it produced off it; the loop
//! itself, and everything it needs to run, are the parent module's.

use crate::bytecode::Module;
use crate::load::LoadedPage;
use crate::rendered::{Buffer, Rendered};
use crate::{RuntimeError, Value};

use super::Vm;

/// Renders the file at `path` from `module`, with `globals` bound to the ambient names
/// (`page`, `site`, `build`) the templates may read, and returns the page with the
/// fragments its collect blocks gathered. It takes no page table: a file that is no
/// loaded page binds no record a neighbour stands in, and a collection's members hold
/// none.
///
/// # Errors
///
/// The first runtime error, pointing at the instruction's source.
///
/// # Panics
///
/// If the module has no file at `path`: which files a module holds is the caller's to
/// know, so this is a caller bug rather than a template error.
pub fn render(
    module: &Module,
    path: &str,
    globals: &[(&str, Value)],
) -> Result<Rendered, RuntimeError> {
    let mut vm = Vm::new(module, globals);
    vm.run_entry(entry_of(module, path))?;
    Ok(vm.finish())
}

/// Renders the loaded page `page` from `module` through its record's entry field
/// `entry` (D43), with `globals` as [`render`] takes them, and `pages` the store's
/// page table, which a generated page's neighbour fields name pages of and a read of
/// one resolves through. The file's own output is captured as the body, and the
/// function the field holds is called with the record and that body in the same run:
/// `uniqueId` counts on from the body into the layout, the collect blocks of both land
/// in one collection, and the marks of both stand in one page in print order. With no
/// `entry` — a host that renders pages through no field — the page is its own output,
/// as a file that is no page is.
///
/// # Errors
///
/// The first runtime error, pointing at the instruction's source. A failure with no
/// instruction to blame — the record has no entry field, or what it holds cannot be
/// called with the record and the body — points at where the page's record was named.
///
/// # Panics
///
/// If the module has no file at the page's path, as [`render`] does.
pub fn render_page(
    module: &Module,
    page: &LoadedPage,
    pages: &[LoadedPage],
    entry: Option<&str>,
    globals: &[(&str, Value)],
) -> Result<Rendered, RuntimeError> {
    let path = page.path.as_str();
    let at_record = |message: String| RuntimeError::new(path, page.record_span, message);
    let mut vm = Vm {
        pages,
        ..Vm::new(module, globals)
    };
    vm.out.push(Buffer::default());
    vm.run_entry(entry_of(module, path))?;
    let body = vm.out.pop().expect("the body's buffer").into_value();
    let rendered = match entry {
        None => body,
        Some(entry) => {
            let Value::Record(record) = &page.record else {
                return Err(at_record(format!(
                    "`{path}` was loaded as a {}, not a record",
                    page.record.type_name()
                )));
            };
            let Some(layout) = record.fields.get(entry).cloned() else {
                return Err(at_record(format!(
                    "`{}` has no `{entry}` to render `{path}` through",
                    record.name
                )));
            };
            vm.stack.push(page.record.clone());
            vm.stack.push(body);
            vm.call_value(layout, 2).map_err(at_record)?;
            vm.run_until(0)?;
            vm.pop()
        }
    };
    let root = vm.out.last_mut().expect("the root output buffer");
    rendered.emit(root).map_err(at_record)?;
    Ok(vm.finish())
}

/// The entry function of the file at `path`.
///
/// # Panics
///
/// If the module has no file at `path`: which files a module holds is the caller's to
/// know, so this is a caller bug rather than a template error.
fn entry_of(module: &Module, path: &str) -> u32 {
    module
        .entry(path)
        .unwrap_or_else(|| panic!("no file `{path}` in the module"))
}

/// Calls `functions[func]` of `module` with `args`, `globals` bound to the ambient
/// names, and returns what it returned. How the loader runs a default or a computed
/// binding (D37): a loader-run function is an ordinary function, and this is a
/// render's machinery with a call in place of an entry. It takes no page table: a
/// page's neighbours are filled once every default has run, and the records the
/// bindings hold have none.
///
/// # Errors
///
/// The first runtime error, pointing at the instruction's source.
///
/// # Panics
///
/// If `args` are not as many as the function's parameters — the compiler fixed both,
/// so a mismatch is a bug in the caller rather than anything a template can provoke.
pub(crate) fn invoke(
    module: &Module,
    func: u32,
    args: Vec<Value>,
    globals: &[(&str, Value)],
) -> Result<Value, RuntimeError> {
    let argc = u16::try_from(args.len()).expect("a function takes fewer than 65536");
    let mut vm = Vm {
        stack: args,
        fragments: None,
        ..Vm::new(module, globals)
    };
    vm.call(func, argc, &[])
        .unwrap_or_else(|message| panic!("invoking `{func}` failed: {message}"));
    vm.run_until(0)?;
    Ok(vm.pop())
}

#[cfg(test)]
mod tests {
    use lemmon_syntax::Span;

    use super::super::tests::{function, module};
    use super::*;
    use crate::bytecode::Op;

    #[test]
    fn refuses_to_collect_while_the_loader_runs_a_function() {
        let collects = function(
            "styled",
            &[],
            &[],
            vec![
                Op::Capture,
                Op::EndCapture,
                Op::Collect { name: 0 },
                Op::Const { index: 0 },
                Op::Return,
            ],
        );
        let module = module(vec![Value::str("styles")], vec![collects]);
        let error = invoke(&module, 0, Vec::new(), &[]).unwrap_err();
        assert_eq!(
            (error.span, error.message.as_str()),
            (
                Span::new(20, 25),
                "only a page render collects fragments, and this block ran while the data \
                 was loading"
            )
        );
    }

    #[test]
    #[should_panic(expected = "no file `missing.lem` in the module")]
    fn panics_on_a_file_the_module_does_not_have() {
        let _ = render(
            &module(vec![], vec![function("t.lem", &[], &[], vec![])]),
            "missing.lem",
            &[],
        );
    }
}

/// A page rendered through its layout: the loader's fixtures, with a layout that
/// numbers its heading, collects a style, and places a mark before the body.
#[cfg(test)]
mod pages {
    use lemmon_syntax::Span;
    use lemmon_syntax::spans::at;

    use super::*;
    use crate::load::tests::{build, object, shell};
    use crate::load::{Data, Loaded, Source, Spans, Store, load};
    use crate::rendered::Fragments;
    use crate::{SourceFile, compile};

    const AMBIENT: &str = "<lem-type Page>\n  url: url\n</lem-type>\n\
         <lem-type Build>\n  generator: string\n</lem-type>";
    const SITE: &str = "<lem-type Article extends Page>\n  title: string\n  \
         layout: (Article, html): html = article\n</lem-type>\n\
         <lem-fn article(a: Article, body: html)>\
         <h1 id=\"<lem.uniqueId(\"h\")>\"><lem.a.title></h1>\
         <lem-style>h1 {}</lem-style><lem.mark(\"head\")><lem.body></lem-fn>\n\
         <lem-let posts: [Article] from \"pages/*.md\" />";
    const FAULTY: &str = "<lem-type Article extends Page>\n  title: string\n  \
         layout: (Article, html): html = article\n</lem-type>\n\
         <lem-fn article(a: Article, body: html)>\
         <lem-let z: int = 0 /><lem.1 / z></lem-fn>\n\
         <lem-let posts: [Article] from \"pages/*.md\" />";
    const HOME: &str = "---\ntype: Article\ntitle: Home\n---\n\
         <p id=\"<lem.uniqueId(\"h\")>\">body</p>\
         <lem-style>p {}</lem-style><lem.mark(\"styles\")>";

    /// `home.lem`, a page template of `Article`, compiled with `site` and loaded.
    fn loaded(site: &str) -> (Module, Store) {
        let files = [
            SourceFile::new("ambient.lem", AMBIENT),
            SourceFile::new("site.lem", site),
            SourceFile::new("home.lem", HOME)
                .with_record("Article")
                .named_at(at(HOME, "Article")),
        ];
        let module = compile(&files, &shell()).expect_output();
        let data = Data {
            ambient: vec![build()],
            sources: vec![("posts".to_owned(), Source::Many(vec![]))],
            pages: vec![Loaded {
                value: object(&[("title", Value::str("Home")), ("url", Value::str("/"))]),
                path: Some("home.lem".to_owned()),
                spans: Spans::new(),
                generator: None,
            }],
        };
        let store = load(&module, &shell(), data).unwrap().expect_output();
        (module, store)
    }

    fn rendered(site: &str, entry: Option<&str>) -> Result<Rendered, RuntimeError> {
        let (module, store) = loaded(site);
        let page = store.page("home.lem", None).expect("a loaded page");
        let host = shell();
        let globals = store.globals(&host, "home.lem", None);
        render_page(&module, page, &store.pages, entry, &globals)
    }

    fn texts(fragments: &Fragments, name: &str) -> Vec<String> {
        fragments
            .get(name)
            .iter()
            .map(|f| f.text.to_string())
            .collect()
    }

    #[test]
    fn passes_the_pages_output_to_its_layout_as_the_body() {
        let html = rendered(SITE, Some("layout")).unwrap().html;
        assert!(html.starts_with("<h1 "), "{html}");
        assert!(html.ends_with("<p id=\"h-0\">body</p>"), "{html}");
    }

    #[test]
    fn counts_ids_on_from_the_body_into_the_layout() {
        // One run: the body's id is given out first, and the layout's heading
        // takes the next, which a second machine would have started over.
        let html = rendered(SITE, Some("layout")).unwrap().html;
        assert_eq!(html, "<h1 id=\"h-1\">Home</h1><p id=\"h-0\">body</p>");
    }

    #[test]
    fn collects_the_fragments_of_both_halves_into_one_collection() {
        // What `invoke` could not do: its machine has nowhere to collect, so a
        // `lem-style` in a layout run through it would fault.
        let rendered = rendered(SITE, Some("layout")).unwrap();
        assert_eq!(texts(&rendered.fragments, "styles"), ["p {}", "h1 {}"]);
    }

    #[test]
    fn places_the_marks_of_both_halves_in_one_page_in_print_order() {
        let Rendered { html, marks, .. } = rendered(SITE, Some("layout")).unwrap();
        let names: Vec<&str> = marks.iter().map(|mark| &*mark.name).collect();
        assert_eq!(names, ["head", "styles"]);
        let body = u32::try_from(html.find("<p ").unwrap()).unwrap();
        assert_eq!(marks[0].start, body);
        assert_eq!(marks[1].start, u32::try_from(html.len()).unwrap());
        assert!(marks[0].start <= marks[1].start);
    }

    #[test]
    fn keeps_a_layouts_error_at_the_layouts_own_span() {
        let error = rendered(FAULTY, Some("layout")).unwrap_err();
        assert_eq!(
            (error.path.as_str(), error.span, error.message.as_str()),
            ("site.lem", at(FAULTY, "1 / z"), "division by zero")
        );
    }

    #[test]
    fn renders_a_page_as_its_own_output_under_a_host_with_no_entry() {
        let rendered = rendered(SITE, None).unwrap();
        assert_eq!(rendered.html, "<p id=\"h-0\">body</p>");
        assert_eq!(texts(&rendered.fragments, "styles"), ["p {}"]);
        assert_eq!(rendered.marks.len(), 1);
    }

    #[test]
    fn points_a_failure_before_the_layout_runs_at_the_pages_record() {
        // A field the record has not got has no instruction to blame — the
        // link step keeps this from happening, and the render says where it
        // would have been rather than trapping.
        let error = rendered(SITE, Some("template")).unwrap_err();
        assert_eq!(
            (error.path.as_str(), error.span, error.message.as_str()),
            (
                "home.lem",
                at(HOME, "Article"),
                "`Article` has no `template` to render `home.lem` through"
            )
        );
        let (module, _) = loaded(SITE);
        let unloaded = LoadedPage {
            path: "home.lem".to_owned(),
            record: Value::Null,
            spans: Spans::new(),
            record_span: Span::new(4, 11),
            generated: None,
        };
        let error = render_page(&module, &unloaded, &[], Some("layout"), &[]).unwrap_err();
        assert_eq!(
            (error.path.as_str(), error.span, error.message.as_str()),
            (
                "home.lem",
                Span::new(4, 11),
                "`home.lem` was loaded as a null, not a record"
            )
        );
    }

    /// A page whose neighbours are references into a table built by hand, as a
    /// generator's loader would fill them (D43): `home.lem` at position 0, its `prev`
    /// position 1, and that page's `next` position 0 again.
    mod neighbours {
        use std::rc::Rc;

        use super::*;
        use crate::value::Map;

        const SITE: &str = "<lem-type Article extends Page>\n  title: string\n  \
             prev: Article?\n  next: Article?\n  \
             layout: (Article, html): html = article\n</lem-type>\n\
             <lem-fn article(a: Article, body: html)><lem.body></lem-fn>\n\
             <lem-let posts: [Article] from \"pages/*.md\" />";

        fn article(title: &str, prev: Value, next: Value) -> Value {
            Value::record(
                "Article",
                Map::from_unique([
                    (Rc::from("title"), Value::str(title)),
                    (Rc::from("prev"), prev),
                    (Rc::from("next"), next),
                    (Rc::from("url"), Value::url("/")),
                ]),
            )
        }

        fn table() -> Vec<LoadedPage> {
            let page = |record: Value| LoadedPage {
                path: "home.lem".to_owned(),
                record,
                spans: Spans::new(),
                record_span: Span::new(4, 11),
                generated: None,
            };
            vec![
                page(article("Home", Value::Neighbour(1), Value::Null)),
                page(article("Before", Value::Null, Value::Neighbour(0))),
            ]
        }

        /// `home.lem` with `body`, rendered as the first page of `pages`.
        fn rendered(body: &str, pages: &[LoadedPage]) -> Result<String, RuntimeError> {
            let text = format!("---\ntype: Article\ntitle: x\n---\n{body}");
            let files = [
                SourceFile::new("ambient.lem", AMBIENT),
                SourceFile::new("site.lem", SITE),
                SourceFile::new("home.lem", &text)
                    .with_record("Article")
                    .named_at(at(&text, "Article")),
            ];
            let module = compile(&files, &shell()).expect_output();
            let globals = [("page", pages[0].record.clone())];
            render_page(&module, &pages[0], pages, None, &globals).map(|r| r.html)
        }

        #[test]
        fn reads_a_neighbours_fields_through_the_page_table() {
            assert_eq!(
                rendered("<lem.page.prev?.title ?? \"none\">", &table()),
                Ok("Before".to_owned())
            );
            assert_eq!(
                rendered("<lem.page.next?.title ?? \"none\">", &table()),
                Ok("none".to_owned())
            );
        }

        #[test]
        fn reads_a_neighbours_neighbour_as_the_page_itself() {
            // Not a record one level deep whose own neighbours are null (D43).
            assert_eq!(
                rendered(
                    "<lem-if page.prev><lem.page.prev.next?.title ?? \"none\"></lem-if>",
                    &table()
                ),
                Ok("Home".to_owned())
            );
        }

        #[test]
        fn faults_at_a_reference_the_table_it_was_given_does_not_hold() {
            let error =
                rendered("<lem.page.prev?.title ?? \"none\">", &table()[..1]).unwrap_err();
            assert_eq!(
                error.message,
                "a neighbour names page 1 of the page table, and this render was given 1"
            );
        }

        #[test]
        fn resolves_a_reference_a_call_through_any_finds_in_a_field() {
            // The field is read for the call as any field is, so the error is about the
            // record it names, not the reference.
            let error =
                rendered("<lem-let x: any = page /><lem.x.prev()>", &table()).unwrap_err();
            assert_eq!(error.message, "cannot call a `Article` record");
        }
    }
}

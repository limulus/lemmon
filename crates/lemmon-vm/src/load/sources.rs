//! What a host loads: every `from` binding of the program's bindings files, the source
//! each names, and whether that source collects pages or is one value
//! (`docs/language.md` §Ambient types).
//!
//! A host needs the answer before it can build an envelope — which glob a document
//! matched is the record it compiles the document as (D30), and which bindings it
//! fulfils, and how, is the program's to say — and the loader needs the same answer
//! as it validates what the host sent. [`collects`] is the one rule both ask.

use std::rc::Rc;

use lemmon_syntax::Span;

use crate::bytecode::Module;
use crate::decls::Decls as _;
use crate::host::Host;
use crate::plan::{Fulfilled, SourceKind};
use crate::types::Type;

/// One `from` binding of a bindings file, as a host fulfils it: by its name, from the
/// source it names, with every page a collecting glob matches loaded as its record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FromBinding {
    /// The binding's name: what [`Data::sources`](crate::Data::sources) fulfils it by.
    pub name: Rc<str>,
    /// The literal after `from`, by its form.
    pub kind: SourceKind,
    /// The record each page the glob matches is, where the binding collects pages: a
    /// file source whose type is a list of a record extending the host's document
    /// type. `None` for a data file, a plugin, or an environment value, each of which
    /// is fulfilled by one value.
    pub collects: Option<Rc<str>>,
    /// The bindings file the binding stands in.
    pub path: String,
    /// The binding's name in that file — where a host reports a source it cannot
    /// fulfil, as the loader reports a value of the wrong type with none of its own.
    pub span: Span,
}

/// Every `from` binding of `module`'s bindings files under `host`: each file in the
/// host's declaration order, and its bindings in the order the file declares them.
/// A computed binding is the loader's to run, so it is not among them; a program with
/// no bindings file has none.
#[must_use]
pub fn from_bindings(module: &Module, host: &Host) -> Vec<FromBinding> {
    let mut bindings = Vec::new();
    for plan in &module.bindings {
        let path = &module.files[plan.file as usize];
        let start = bindings.len();
        for step in &plan.steps {
            let Fulfilled::ByHost(kind) = &step.fulfilled else {
                continue;
            };
            let ty = module
                .field(&plan.record, &step.name)
                .expect("a binding is a field of its record");
            bindings.push(FromBinding {
                name: step.name.clone(),
                kind: kind.clone(),
                collects: collects(module, host, kind, &ty),
                path: path.clone(),
                span: step.span,
            });
        }
        // The plan's order is the load order, which a `by` index can rearrange; a
        // host reporting on the bindings reads them as the author wrote them.
        bindings[start..].sort_by_key(|binding| binding.span.start);
    }
    bindings
}

/// The record a `from` binding collects, when its source `kind` and type `ty` make it
/// a glob: a file source whose type is a list of a record extending the host's
/// document type — strictly, as `extends` is: a list of the document type itself is a
/// compile error at the binding, so nothing loads one as pages. Any other file source
/// is a data file — the rule a host tells the two apart by (`docs/language.md`
/// §Ambient types), which a host reads from [`from_bindings`] rather than repeating.
pub(super) fn collects(
    module: &Module,
    host: &Host,
    kind: &SourceKind,
    ty: &Type,
) -> Option<Rc<str>> {
    let SourceKind::File(_) = kind else {
        return None;
    };
    let Type::List(element) = ty else {
        return None;
    };
    let Type::Named(record) = &**element else {
        return None;
    };
    let document = host.document_type()?;
    (*record != document && module.is_a(record, &document)).then(|| record.clone())
}

#[cfg(test)]
mod tests {
    use lemmon_syntax::spans::at;

    use super::super::tests::{AMBIENT, module, shell};
    use super::*;
    use crate::host::Ambient;

    /// A bindings file declaring every form of source. `posts` holds records that look
    /// a string up in `photos`, declared after it, so the loader runs `photos` first.
    const SITE: &str = "<lem-type Photo>\n  caption: string\n</lem-type>\n\
         <lem-type Post extends Page>\n  title: string\n  hero: Photo? by site.photos\n  \
         layout: (Post, html): html\n</lem-type>\n\
         <lem-type Note extends Post>\n  mood: string?\n</lem-type>\n\
         <lem-let motto: string from env(\"MOTTO\") />\n\
         <lem-let posts: [Post] from \"posts/*.md\" />\n\
         <lem-let notes: [Note] from \"notes/*.md\" />\n\
         <lem-let photos: {string: Photo} from \"_data/photos.json\" />\n\
         <lem-let gallery: [Photo] from \"_data/gallery.json\" />\n\
         <lem-let more: string from plugin(\"more\") />\n\
         <lem-let count: int = 1 />";

    fn site() -> Module {
        module(&shell(), &[("ambient.lem", AMBIENT), ("site.lem", SITE)])
    }

    /// The bindings as `(name, kind, collects)`, for the assertions that read neither
    /// the path nor the span.
    fn shown(bindings: &[FromBinding]) -> Vec<(&str, SourceKind, Option<&str>)> {
        bindings
            .iter()
            .map(|b| (&*b.name, b.kind.clone(), b.collects.as_deref()))
            .collect()
    }

    fn file(pattern: &str) -> SourceKind {
        SourceKind::File(Rc::from(pattern))
    }

    #[test]
    fn reports_every_from_binding_in_the_order_the_file_declares_it() {
        let module = site();
        let loads: Vec<&str> = module.bindings[0]
            .steps
            .iter()
            .map(|step| &*step.name)
            .collect();
        assert!(
            loads.iter().position(|name| *name == "photos")
                < loads.iter().position(|name| *name == "posts"),
            "the load order this test is about: {loads:?}"
        );
        assert_eq!(
            shown(&from_bindings(&module, &shell())),
            [
                ("motto", SourceKind::Env(Rc::from("MOTTO")), None),
                ("posts", file("posts/*.md"), Some("Post")),
                ("notes", file("notes/*.md"), Some("Note")),
                ("photos", file("_data/photos.json"), None),
                ("gallery", file("_data/gallery.json"), None),
                ("more", SourceKind::Plugin(Rc::from("more")), None),
            ]
        );
    }

    #[test]
    fn places_each_binding_at_its_name_in_the_bindings_file() {
        let bindings = from_bindings(&site(), &shell());
        let posts = bindings.iter().find(|b| &*b.name == "posts").unwrap();
        assert_eq!(posts.path, "site.lem");
        assert_eq!(posts.span, at(SITE, "<lem-let «posts»: [Post]"));
    }

    #[test]
    fn reads_a_glob_as_a_data_file_for_a_host_that_does_not_type_its_documents() {
        // With no document type there is nothing for an element type to extend, so
        // the loader takes the glob's binding as one value — and the host is told so.
        let untyped = Host::new()
            .bind("page", Ambient::Untyped)
            .bind("site", Ambient::bindings("site.lem"))
            .documents("page");
        let module = module(
            &untyped,
            &[
                ("ambient.lem", AMBIENT),
                ("site.lem", "<lem-let posts: [any] from \"posts/*.md\" />"),
            ],
        );
        assert_eq!(
            shown(&from_bindings(&module, &untyped)),
            [("posts", file("posts/*.md"), None)]
        );
    }

    #[test]
    fn collects_a_type_that_extends_the_document_type_and_not_the_type_itself() {
        let module = site();
        let glob = file("pages/*.md");
        let list = |name: &str| Type::list(Type::Named(Rc::from(name)));
        assert_eq!(
            collects(&module, &shell(), &glob, &list("Post")).as_deref(),
            Some("Post")
        );
        assert_eq!(
            collects(&module, &shell(), &glob, &list("Note")).as_deref(),
            Some("Note")
        );
        assert_eq!(collects(&module, &shell(), &glob, &list("Page")), None);
    }

    #[test]
    fn reports_none_for_a_program_with_no_bindings_file() {
        let module = module(&shell(), &[("ambient.lem", AMBIENT), ("x.lem", "")]);
        assert_eq!(from_bindings(&module, &shell()), []);
    }
}

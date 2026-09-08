//! The ambient environment a host declares (D30, `docs/language.md` §Ambient types).
//!
//! The core knows no type or value by name. A host — the shell, the LSP, a playground —
//! says which names every template may read and what each one's type is, and the types
//! themselves are ordinary declarations in files it compiles with the program, so every
//! field has a span and the rules that check the site's records check what the host
//! sends. A [`Host`] is that declaration as the compiler reads it: names to bindings,
//! plus the two names a host that renders documents adds — which ambient value a file's
//! record retypes, and the field a page is rendered through.
//!
//! A host that names nothing compiles a program with no ambient values: `page` is then
//! an undefined name like any other, not a reserved one.

use std::rc::Rc;

use crate::types::Type;

/// What a host binds one ambient name to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ambient {
    /// Named without a type: `any`, checked by the VM at run time. A host is free to
    /// declare nothing about a value, which is how the Phase 0 spike renders.
    Untyped,
    /// A type the program declares.
    Named(Rc<str>),
    /// The record of a file's top-level bindings — `site` from `site.lem`. The
    /// declaration is synthesized where that file is prepared, and named by its path,
    /// which no `lem-type` can collide with.
    Bindings(Rc<str>),
}

impl Ambient {
    /// Bound to the declared type `name`.
    #[must_use]
    pub fn named(name: &str) -> Self {
        Self::Named(Rc::from(name))
    }

    /// Bound to the record of the top-level bindings of the file at `path`.
    #[must_use]
    pub fn bindings(path: &str) -> Self {
        Self::Bindings(Rc::from(path))
    }

    /// The type a value bound this way has.
    #[must_use]
    pub fn ty(&self) -> Type {
        match self {
            Self::Untyped => Type::Any,
            Self::Named(name) | Self::Bindings(name) => Type::Named(name.clone()),
        }
    }
}

/// Everything a host declares about the environment its programs compile in — and,
/// in the view [`for_file`](Host::for_file) makes, what one file of them sees.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Host {
    /// Every ambient name, in the order the host gave them. A `Vec` rather than a map:
    /// a host declares a handful of names, and the order is what makes a diagnostic
    /// listing them deterministic.
    ambient: Vec<(Rc<str>, Ambient)>,
    /// The ambient value a file's `record` retypes — `page` in the standard shell.
    /// A host that renders no documents names none.
    document: Option<Rc<str>>,
    /// The field a page is rendered through — `layout` in the standard shell — required
    /// on every record that extends the document value's type.
    entry: Option<Rc<str>>,
    /// What the document value is in the file this is a view of: that file's record, or
    /// the unknown type for a document the host matched to nothing. Never set in the
    /// declaration itself, which is why the type a page schema extends stays the
    /// host's however a file was compiled.
    document_binding: Option<Ambient>,
}

impl Host {
    /// A host that names nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A host that names each of `names` without typing it — the shape a host has
    /// before it declares anything, and what the Phase 0 spike passes.
    #[must_use]
    pub fn untyped<'a>(names: impl IntoIterator<Item = &'a str>) -> Self {
        Self {
            ambient: names
                .into_iter()
                .map(|name| (Rc::from(name), Ambient::Untyped))
                .collect(),
            ..Self::new()
        }
    }

    /// Adds an ambient name.
    #[must_use]
    pub fn bind(mut self, name: &str, ambient: Ambient) -> Self {
        self.ambient.push((Rc::from(name), ambient));
        self
    }

    /// Names the ambient value a file's record retypes.
    #[must_use]
    pub fn documents(mut self, name: &str) -> Self {
        self.document = Some(Rc::from(name));
        self
    }

    /// Names the field a page is rendered through.
    #[must_use]
    pub fn rendered_through(mut self, field: &str) -> Self {
        self.entry = Some(Rc::from(field));
        self
    }

    /// How `name` is bound where this is read, if the host names it at all: the
    /// declaration's binding, or — for the document value in a file's view — what that
    /// file's record made it.
    #[must_use]
    pub fn binding(&self, name: &str) -> Option<&Ambient> {
        let declared = self
            .ambient
            .iter()
            .find(|(bound, _)| &**bound == name)
            .map(|(_, ambient)| ambient)?;
        match &self.document_binding {
            Some(binding) if self.document.as_deref() == Some(name) => Some(binding),
            _ => Some(declared),
        }
    }

    /// The ambient value a file's record retypes, if the host renders documents.
    #[must_use]
    pub fn document(&self) -> Option<&str> {
        self.document.as_deref()
    }

    /// The field a page is rendered through, if the host renders pages.
    #[must_use]
    pub fn entry(&self) -> Option<&str> {
        self.entry.as_deref()
    }

    /// Every name the host binds, in declaration order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.ambient.iter().map(|(name, _)| &**name)
    }

    /// Whether the top-level bindings of the file at `path` are an ambient value's
    /// fields — the file whose bindings need declaring, and the one whose record is
    /// synthesized as it is prepared.
    #[must_use]
    pub fn binds_from(&self, path: &str) -> bool {
        self.ambient.iter().any(
            |(_, ambient)| matches!(ambient, Ambient::Bindings(file) if &**file == path),
        )
    }

    /// The host as the file at `path` sees it: a file the host gave a `record` reads
    /// the document value as that record, and a document the host gave none reads it
    /// unknown-typed — D30's document that no glob matched, which renders all the same.
    ///
    /// The declaration itself is never rewritten. The type a page schema extends and
    /// the field a page renders through stay what the host declared, whatever one file
    /// was compiled as.
    #[must_use]
    pub fn for_file(&self, path: &str, record: Option<&str>) -> Self {
        let mut seen = self.clone();
        let Some(document) = seen.document.clone() else {
            return seen;
        };
        if !seen.ambient.iter().any(|(name, _)| *name == document) {
            return seen;
        }
        seen.document_binding = match record {
            Some(record) => Some(Ambient::Named(Rc::from(record))),
            None if crate::is_document(path) => Some(Ambient::Untyped),
            None => return seen,
        };
        seen
    }

    /// The type every page schema extends: what the host *declared* its document value
    /// to be, never what one file's record made it. `None` unless the host both names a
    /// document value and types it.
    #[must_use]
    pub fn document_type(&self) -> Option<Rc<str>> {
        let document = self.document.as_deref()?;
        let declared = self
            .ambient
            .iter()
            .find(|(name, _)| &**name == document)
            .map(|(_, ambient)| ambient)?;
        match declared {
            Ambient::Named(name) => Some(name.clone()),
            Ambient::Untyped | Ambient::Bindings(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell() -> Host {
        Host::new()
            .bind("page", Ambient::named("Page"))
            .bind("build", Ambient::named("Build"))
            .bind("site", Ambient::bindings("site.lem"))
            .documents("page")
            .rendered_through("layout")
    }

    #[test]
    fn names_nothing_by_default() {
        assert_eq!(Host::new().names().count(), 0);
        assert_eq!(Host::new().binding("page"), None);
    }

    #[test]
    fn gives_an_untyped_value_the_unknown_type() {
        let host = Host::untyped(["page", "site", "build"]);
        assert_eq!(host.binding("page"), Some(&Ambient::Untyped));
        assert_eq!(host.binding("page").map(Ambient::ty), Some(Type::Any));
    }

    #[test]
    fn types_a_declared_value_by_the_name_the_host_gave() {
        assert_eq!(
            shell().binding("page").map(Ambient::ty),
            Some(Type::Named(Rc::from("Page")))
        );
    }

    #[test]
    fn types_a_bindings_value_by_the_path_it_reads() {
        assert_eq!(
            shell().binding("site").map(Ambient::ty),
            Some(Type::Named(Rc::from("site.lem")))
        );
    }

    #[test]
    fn keeps_the_ambient_names_in_the_order_the_host_gave_them() {
        assert_eq!(
            shell().names().collect::<Vec<_>>(),
            vec!["page", "build", "site"]
        );
    }

    #[test]
    fn knows_which_file_a_value_takes_its_bindings_from() {
        assert!(shell().binds_from("site.lem"));
        assert!(!shell().binds_from("layout.lem"));
    }

    #[test]
    fn takes_the_page_schemas_base_from_the_document_value() {
        assert_eq!(shell().document_type().as_deref(), Some("Page"));
    }

    #[test]
    fn reads_a_files_own_record_as_the_document_value() {
        let seen = shell().for_file("feed/a/index.md", Some("Article"));
        assert_eq!(
            seen.binding("page"),
            Some(&Ambient::Named(Rc::from("Article")))
        );
    }

    #[test]
    fn leaves_a_document_no_record_matched_unknown_typed() {
        let seen = shell().for_file("feed/a/index.md", None);
        assert_eq!(seen.binding("page"), Some(&Ambient::Untyped));
    }

    #[test]
    fn leaves_a_templates_document_value_as_the_host_declared_it() {
        let seen = shell().for_file("layout.lem", None);
        assert_eq!(
            seen.binding("page"),
            Some(&Ambient::Named(Rc::from("Page")))
        );
    }

    #[test]
    fn keeps_the_page_schema_base_when_a_file_is_compiled_as_a_record() {
        let seen = shell().for_file("feed/a/index.md", Some("Article"));
        assert_eq!(seen.document_type().as_deref(), Some("Page"));
    }

    #[test]
    fn has_nothing_to_retype_for_a_host_that_renders_no_documents() {
        let host = Host::untyped(["page"]);
        assert_eq!(host.for_file("a.md", Some("Article")), host);
    }

    #[test]
    fn retypes_nothing_for_a_document_value_the_host_never_bound() {
        let host = Host::untyped(["site"]).documents("page");
        let seen = host.for_file("a.md", Some("Article"));
        assert_eq!(seen.binding("page"), None);
    }

    #[test]
    fn has_no_page_schema_base_without_a_document_value() {
        assert_eq!(Host::untyped(["page"]).document_type(), None);
    }

    #[test]
    fn has_no_page_schema_base_when_the_document_value_is_untyped() {
        let host = Host::untyped(["page"]).documents("page");
        assert_eq!(host.document_type(), None);
    }

    #[test]
    fn has_no_page_schema_base_when_the_document_value_is_not_bound() {
        assert_eq!(shell().documents("nothing").document_type(), None);
    }
}

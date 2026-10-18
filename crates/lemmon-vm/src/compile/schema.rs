//! What a file is checked against once every file's declarations are in: the host's
//! declaration (D30, D36) and the project's one export index (D32).
//!
//! [`surfaces`](super::surfaces) reads a file's own heads and can say nothing about the
//! names in them — whether a type any of them mentions is declared anywhere, whether the
//! record the host compiled this file as is a schema it may be. Those questions need the
//! index, so they are asked here: at the head of [`compile_unit`](super::compile_unit),
//! and again wherever an annotation resolves mid-walk.
//!
//! Every answer is also a fact of the unit (D25). A type consumed from another file is
//! recorded with the declaration it was checked against, an ambient value with the type
//! the host bound it to, so the link step can tell a unit that has gone stale by name
//! from one that has not. A name nothing declares is reported once and reads as `any`
//! from there, so one mistake costs one diagnostic.

use std::rc::Rc;

use lemmon_syntax::ast::TypeExpr;
use lemmon_syntax::{Span, nearest};

use crate::decls::{
    DeclKind, Declaration, Decls, EntryFault, FieldDecl, Shape, entry_fault,
    misdeclared_neighbour, misshaped_entry_field, neighbour_fits, no_entry_field,
};
use crate::diagnostics::Diagnostic;
use crate::host::{ClaimAnswer, no_asset};
use crate::types::Type;
use crate::unit::{AmbientRef, AssetRef, ClaimRef, ImportRef, TypeRef};

use super::surfaces::PreparedFile;
use super::{Compiler, annotations, duplicate_named};

impl Compiler<'_> {
    /// `ty` with every undeclared name read as `any`: the file that wrote the name was
    /// told, and nothing more is known about it, so nothing more is said of what
    /// carries it (one mistake, one diagnostic).
    pub(super) fn known(&self, ty: Type) -> Type {
        ty.rewrite_named(&|name| {
            self.index.lookup_type(name).is_none().then_some(Type::Any)
        })
    }

    /// Resolves an annotation written in this file, now that the index can say which
    /// names are declared.
    pub(super) fn annotation(&mut self, ty: &TypeExpr) -> Type {
        let mut uses = Vec::new();
        let ty = annotations::resolve(self.path, ty, &mut self.diagnostics, &mut uses);
        self.check_type_uses(&uses);
        self.known(ty)
    }

    /// Reports each use of a type the project does not declare, and records each use
    /// of one it does as a fact of this unit.
    pub(super) fn check_type_uses(&mut self, uses: &[ImportRef]) {
        for reference in uses {
            if self.index.lookup_type(&reference.name).is_none() {
                self.error(reference.span, format!("unknown type `{}`", reference.name));
            } else {
                self.note_type(&reference.name, reference.span);
            }
        }
    }

    /// The URL `asset("path")` resolves to, recorded as a fact of this unit the first
    /// time the file asks for that path (D25, D10).
    ///
    /// The lookup is the whole of `asset()`: the URL becomes a constant in the bytecode
    /// and nothing is looked up at render. That is why the entry has to be recorded —
    /// a rebuild that re-hashes the asset must leave this unit stale, and the link step
    /// has nothing else to notice it by (invariant 11).
    ///
    /// A path the manifest has not got is an error naming the nearest it has, or saying
    /// there are none — a host with no build declares no assets, and `asset()` under one
    /// is a mistake worth its own sentence rather than a did-you-mean over nothing.
    pub(super) fn asset_ref(&mut self, path: &str, span: Span) -> Option<Rc<str>> {
        if let Some(reference) = self
            .unit
            .asset_refs
            .iter()
            .find(|reference| &*reference.path == path)
        {
            return Some(reference.url.clone());
        }
        let Some(url) = self.host.asset(path) else {
            self.error(span, no_asset(self.host, path));
            return None;
        };
        let url: Rc<str> = Rc::from(url);
        self.unit.asset_refs.push(AssetRef {
            path: Rc::from(path),
            url: url.clone(),
            span,
        });
        Some(url)
    }

    /// Records what the host's claims say about the mark name `name` — asked about
    /// `key`, where the mark has one while compiling — and warns at a name no handler
    /// claims, or a key no producer covers (D42).
    ///
    /// The one place either question is asked, so a `mark()` call and a code block get
    /// the same answer and record the same fact. A fact of the unit for `asset_ref`'s
    /// reason: the answer is given while this file compiles, so a declaration that
    /// changes it has to leave the unit stale. A host that declares no handlers at all
    /// says nothing — the playground and the editor render pages no plugin ever touches,
    /// and every mark in them is meant to keep its default content.
    pub(super) fn claim_ref(
        &mut self,
        name: &str,
        span: Span,
        key: Option<(&str, Span)>,
    ) -> ClaimAnswer {
        let answer = self.host.answer(name, key.map(|(key, _)| key));
        let asked: Option<(Rc<str>, Span)> = key.map(|(key, at)| (Rc::from(key), at));
        if !self.unit.claim_refs.iter().any(|reference| {
            &*reference.name == name
                && reference.key.as_ref().map(|(key, _)| key)
                    == asked.as_ref().map(|(key, _)| key)
        }) {
            self.unit.claim_refs.push(ClaimRef {
                name: Rc::from(name),
                span,
                key: asked,
                answer: answer.clone(),
            });
        }
        match &answer {
            ClaimAnswer::Unclaimed => {
                let message = match nearest(name, self.host.claimed_names()) {
                    Some(near) => {
                        format!("no handler claims `{name}`; did you mean `{near}`?")
                    }
                    None => format!(
                        "no handler claims `{name}`, so this mark keeps its default \
                         content"
                    ),
                };
                self.warn(span, message);
            }
            // A producer lists the keys it covers and no wildcard (D42), so a key none
            // of them lists is the author's mistake, said where the key is written.
            ClaimAnswer::Claimed {
                key: Some(false), ..
            } => {
                let (key, at) = key.expect("a key answer comes of asking about a key");
                let claimed = self
                    .host
                    .claim(name)
                    .into_iter()
                    .flat_map(crate::Claim::keys);
                let message = match nearest(key, claimed) {
                    Some(near) => format!(
                        "no producer claims `{key}` for `{name}`; did you mean `{near}`?"
                    ),
                    None => format!(
                        "no producer claims `{key}` for `{name}`, so this mark keeps its \
                         default content"
                    ),
                };
                self.warn(at, message);
            }
            ClaimAnswer::Undeclared | ClaimAnswer::Claimed { .. } => {}
        }
        answer
    }

    /// The type an ambient value has here, recorded as a fact of this unit the first
    /// time the file reads one (D25) — what the host bound the name to, which is not
    /// what the declaration behind it says and so is not covered by a [`TypeRef`].
    ///
    /// A type the host names that no file declares is reported once, at that first
    /// read, and the value is `any` from there: the mistake is the host's, and one
    /// mistake is one diagnostic.
    pub(super) fn ambient_type(&mut self, name: &str, ty: Type, span: Span) -> Type {
        let recorded = self
            .unit
            .ambient_refs
            .iter()
            .find(|reference| &*reference.name == name)
            .map(|reference| reference.ty.clone());
        if let Some(ty) = recorded {
            return self.known(ty);
        }
        self.unit.ambient_refs.push(AmbientRef {
            name: Rc::from(name),
            span,
            ty: ty.clone(),
        });
        let Type::Named(declared) = &ty else {
            return ty;
        };
        if self.index.lookup_type(declared).is_none() {
            self.error(
                span,
                format!("the host binds `{name}` to `{declared}`, which no file declares"),
            );
            return Type::Any;
        }
        self.note_type(declared, span);
        ty
    }

    /// The entry field on a page schema (D30): a record that extends the type the
    /// host's document value has renders through a field the host names, declared
    /// `(Self, html): html`.
    ///
    /// Asked only of a record whose *base* is that type, because a field may be
    /// redeclared only with the same type (D32): the entry is declared once, where its
    /// self type is, and everything below inherits it. So one missing declaration is
    /// one diagnostic, however deep the schemas go.
    ///
    /// Where it is declared is the host's business as much as the site's: a host whose
    /// document type carries the field itself has already answered for every schema
    /// below it, so the field is looked up through the chain and only a declaration
    /// this file makes is judged for its shape — the file that declares one is the file
    /// that hears about it.
    fn check_entry_field(&mut self, decl: &Declaration, base: &str) {
        let (Some(entry), Some(document)) = (self.host.entry(), self.host.document_type())
        else {
            return;
        };
        if *base != *document {
            return;
        }
        match entry_fault(self.index, &decl.name, entry) {
            Some(EntryFault::Undeclared) => {
                self.error(decl.name_span, no_entry_field(&decl.name, &document, entry))
            }
            // A field an ancestor declared is the ancestor's to hear about.
            Some(EntryFault::Misshaped(owner, field)) if owner.name == decl.name => {
                self.error(field.span, misshaped_entry_field(&decl.name, entry));
            }
            Some(EntryFault::Misshaped(..)) | None => {}
        }
    }

    /// The neighbour fields a page schema declares (D43): each the host names, on a
    /// record that extends the document type, is an optional of that record or of one
    /// it extends ([`neighbour_fits`]). Only this file's own fields are judged, as the
    /// entry field's are; the document type itself, which has no base, is the link
    /// step's to judge, which asks of every page record on every link. A field of a
    /// record that is no page is no neighbour, whatever it is called. A type nothing
    /// declares was reported where it was written, and is not a second mistake here.
    fn check_neighbour_fields(&mut self, decl: &Declaration, fields: &[FieldDecl]) {
        let Some(document) = self.host.document_type() else {
            return;
        };
        if !self.index.extends(&decl.name, &document) {
            return;
        }
        for field in fields {
            if self.host.is_neighbour(&field.name)
                && self.known(field.ty.clone()) == field.ty
                && !neighbour_fits(self.index, &decl.name, &field.ty)
            {
                self.error(field.span, misdeclared_neighbour(&decl.name, &field.name));
            }
        }
    }

    /// The record the host compiled this file as, against what the project declares
    /// (D30). The binding it makes is recorded here, where the record is, so that the
    /// first read of the document value finds the fact already made and says nothing
    /// this has not.
    pub(super) fn check_record(&mut self, prepared: &PreparedFile) {
        let Some(record) = prepared.record.as_deref() else {
            return;
        };
        let span = prepared.record_span();
        // A record would type the document value as itself, and the render binds the
        // envelope's unchecked value to a file that is no page (invariant 10).
        if !prepared.source.page_shaped() {
            self.error(
                span,
                "a template with no front matter is no page, so it takes no record",
            );
            return;
        }
        // Named *and* bound: a host that names a document value it never bound has no
        // value for the record to type, and recording the fact anyway would leave the
        // unit looking as though the host had dropped a binding it never made.
        let document = self
            .host
            .document()
            .filter(|name| self.host.binding(name).is_some())
            .map(str::to_owned);
        let Some(document) = document else {
            self.error(
                span,
                format!(
                    "this file is compiled as `{record}`, and the host binds no \
                     document value to be one"
                ),
            );
            return;
        };
        let ty = self.record_type(record, &document, span);
        self.unit.ambient_refs.push(AmbientRef {
            name: Rc::from(&*document),
            span,
            ty,
        });
    }

    /// The type a file compiled as `record` reads its document value as: that record,
    /// or `any` once what is wrong with the name has been reported — a name no file
    /// declares, or one that is an enum. A record that is simply not a page schema is
    /// reported and still read as itself: its fields are declared, whatever the host
    /// should have matched it to.
    fn record_type(&mut self, record: &str, document: &str, span: Span) -> Type {
        let Some(export) = self.index.lookup_type(record) else {
            self.error(span, format!("unknown type `{record}`"));
            return Type::Any;
        };
        if export.decl.kind() == DeclKind::Enum {
            self.error(span, format!("`{record}` is an enum, not a record"));
            return Type::Any;
        }
        self.note_type(record, span);
        if let Some(base) = self.host.document_type()
            && !self.index.extends(record, &base)
        {
            self.error(
                span,
                format!(
                    "`{record}` does not extend `{base}`, which is what `{document}` \
                     is declared as"
                ),
            );
        }
        Type::Named(Rc::from(record))
    }

    /// The rules on this file's own declarations that need every file's declarations
    /// in: the project kept another file's declaration of the name; the base is an enum
    /// (an unknown base was reported as an unknown type); the chain of bases returns to
    /// the type; a field an ancestor declares is redeclared with another type — the
    /// same type is allowed, since that only changes its default (D32).
    pub(super) fn check_declarations(&mut self, types: &[Declaration]) {
        for decl in types {
            let owner = self
                .index
                .lookup_type(&decl.name)
                .expect("a declaration is in the index");
            if owner.path != self.path {
                self.diagnostics.push(duplicate_named(
                    self.path,
                    decl.name_span,
                    &decl.name,
                    &owner.path,
                    owner.decl.name_span,
                ));
                continue;
            }
            let Shape::Record {
                base: Some((base, base_span)),
                fields,
            } = &decl.shape
            else {
                continue;
            };
            let Some(base_export) = self.index.lookup_type(base) else {
                continue;
            };
            if base_export.decl.kind() == DeclKind::Enum {
                self.error(*base_span, format!("`{base}` is an enum, not a record"));
                continue;
            }
            let chain = self.index.chain(base);
            if chain.iter().any(|ancestor| ancestor.name == decl.name) {
                let through = if chain[0].name == decl.name {
                    String::new()
                } else {
                    format!(" through `{base}`")
                };
                self.error(
                    *base_span,
                    format!("`{}` extends itself{through}", decl.name),
                );
                continue;
            }
            self.check_entry_field(decl, base);
            self.check_neighbour_fields(decl, fields);
            for field in fields {
                let redeclared = chain.iter().find_map(|ancestor| match &ancestor.shape {
                    Shape::Record { fields, .. } => fields
                        .iter()
                        .find(|f| f.name == field.name)
                        .map(|f| (ancestor, f)),
                    Shape::Enum { .. } => None,
                });
                if let Some((ancestor, first)) = redeclared
                    && first.ty != field.ty
                {
                    let owner = self
                        .index
                        .lookup_type(&ancestor.name)
                        .expect("an ancestor is in the index");
                    self.diagnostics.push(
                        Diagnostic::error(
                            self.path,
                            field.span,
                            format!(
                                "`{}` is a `{}` in `{}` and cannot be redeclared as a `{}`",
                                field.name, first.ty, ancestor.name, field.ty
                            ),
                        )
                        .with_label(
                            &owner.path,
                            first.span,
                            "declared here",
                        ),
                    );
                }
            }
        }
    }

    /// Records the type `name` — reached at `span` — as a fact of this unit when
    /// another file declares it, along with every type its declaration reaches: its
    /// bases and the types its fields name, since checking against it may walk any of
    /// them. Over-recording costs a needless recompile, never a wrong one. A foreign
    /// type whose base is not declared is reported here: its fields and its place in
    /// the chain cannot be known, and the file that declares it has already said so.
    pub(super) fn note_type(&mut self, name: &str, span: Span) {
        if self.unit.type_refs.iter().any(|r| &*r.name == name) {
            return;
        }
        let mut pending: Vec<Rc<str>> = vec![Rc::from(name)];
        let mut seen: Vec<Rc<str>> = Vec::new();
        while let Some(name) = pending.pop() {
            if seen.contains(&name) {
                continue;
            }
            seen.push(name.clone());
            let Some(export) = self.index.lookup_type(&name) else {
                continue;
            };
            let foreign = export.path != self.path;
            if foreign && !self.unit.type_refs.iter().any(|r| r.name == name) {
                self.unit.type_refs.push(TypeRef {
                    name: name.clone(),
                    span,
                    fact: export.decl.fact(),
                });
            }
            if let Shape::Record { base, fields } = &export.decl.shape {
                if let Some((base, _)) = base {
                    if foreign && self.index.lookup_type(base).is_none() {
                        self.error(
                            span,
                            format!("`{name}` extends `{base}`, which is not declared"),
                        );
                    }
                    pending.push(base.clone());
                }
                for field in fields {
                    field.ty.named_types(&mut pending);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use lemmon_syntax::spans::at;

    use crate::testing::{AMBIENT, shell_host, shown};
    use crate::{Host, SourceFile, compile};

    /// Compiles `files` against `host` and shows every diagnostic.
    fn diagnostics_with(host: &Host, files: &[SourceFile]) -> Vec<String> {
        shown(&compile(files, host).diagnostics)
    }

    /// A layout reading the page: what a template that is no page does with the
    /// document value.
    fn layout(body: &str) -> String {
        format!("<lem-fn layout()>{body}</lem-fn>")
    }

    #[test]
    fn types_an_ambient_value_from_the_hosts_declaration() {
        let files = [
            SourceFile::new("ambient.lem", AMBIENT),
            SourceFile::new("t.lem", layout("<lem.page.url>")),
        ];
        assert!(diagnostics_with(&shell_host(), &files).is_empty());
    }

    #[test]
    fn refuses_a_field_the_ambient_values_type_does_not_declare() {
        let nope = layout("<lem.page.nope>");
        let files = [
            SourceFile::new("ambient.lem", AMBIENT),
            SourceFile::new("t.lem", &nope),
        ];
        assert_eq!(
            diagnostics_with(&shell_host(), &files),
            [format!(
                "t.lem:{}: `Page` has no field `nope`",
                at(&nope, "page.nope")
            )]
        );
    }

    #[test]
    fn reports_a_type_the_host_names_that_no_file_declares() {
        // The mistake is the host's, so it is reported where the file first reads the
        // value, once, and the value is `any` from there.
        let src = layout("<lem.page.a><lem.page.b>");
        assert_eq!(
            diagnostics_with(&shell_host(), &[SourceFile::new("t.lem", &src)]),
            [format!(
                "t.lem:{}: the host binds `page` to `Page`, which no file declares",
                at(&src, "«page».a")
            )]
        );
    }

    /// A page schema beside the host's declaration, for a file to be compiled as.
    const ARTICLE: &str = "<lem-type Page>\n  url: url\n</lem-type>\n\
                           <lem-type Article extends Page>\n  title: string\n  \
                           layout: (Article, html): html\n</lem-type>";

    #[test]
    fn refuses_a_record_for_a_template_with_no_front_matter() {
        // The record would type `page` as an `Article` while the render binds the
        // envelope's unchecked value: a template with no front matter is no page.
        let files = [
            SourceFile::new("ambient.lem", ARTICLE),
            SourceFile::new("t.lem", layout("<lem.page.title>")).with_record("Article"),
        ];
        assert_eq!(
            diagnostics_with(&shell_host(), &files),
            [
                "t.lem:0..0: a template with no front matter is no page, so it takes no \
              record"
            ]
        );
    }

    #[test]
    fn reads_the_document_value_as_a_page_templates_own_record() {
        // Front matter makes the template a page: it takes the record, reads `page` as
        // it, and a field the record does not declare is refused at the read.
        const NOPE: &str = "---\ntitle: x\n---\n<lem.page.title><lem.page.nope>";
        let files = [
            SourceFile::new("ambient.lem", ARTICLE),
            SourceFile::new("t.lem", NOPE).with_record("Article"),
        ];
        assert_eq!(
            diagnostics_with(&shell_host(), &files),
            [format!(
                "t.lem:{}: `Article` has no field `nope`",
                at(NOPE, "page.nope")
            )]
        );
    }

    #[test]
    fn leaves_a_page_templates_document_value_unknown_typed_without_a_record() {
        // No glob matched and no `type:` named it: the template renders its own
        // output, and `page` is whatever the envelope carries.
        let files = [
            SourceFile::new("ambient.lem", ARTICLE),
            SourceFile::new("t.lem", "---\n---\n<lem.page.anything>"),
        ];
        assert!(diagnostics_with(&shell_host(), &files).is_empty());
    }

    #[test]
    fn reads_the_site_files_bindings_as_the_record_that_value_has() {
        let files = [
            SourceFile::new("site.lem", "<lem-let name: string = \"limulus.net\" />"),
            SourceFile::new("t.lem", layout("<lem.site.name>")),
        ];
        assert!(diagnostics_with(&shell_host(), &files).is_empty());
    }

    #[test]
    fn refuses_a_binding_the_site_file_does_not_make() {
        let nope = layout("<lem.site.nope>");
        let files = [
            SourceFile::new("site.lem", "<lem-let name: string = \"limulus.net\" />"),
            SourceFile::new("t.lem", &nope),
        ];
        assert_eq!(
            diagnostics_with(&shell_host(), &files),
            [format!(
                "t.lem:{}: `site.lem` has no field `nope`; did you mean `name`?",
                at(&nope, "site.nope")
            )]
        );
    }

    #[test]
    fn requires_a_type_on_a_top_level_binding_of_the_site_file() {
        const SRC: &str = "<lem-let name = \"limulus.net\" />";
        assert_eq!(
            diagnostics_with(&shell_host(), &[SourceFile::new("site.lem", SRC)]),
            [format!(
                "site.lem:{}: a binding the host reads declares its type: \
                 `<lem-let name: T = … />`",
                at(SRC, "name")
            )]
        );
    }

    #[test]
    fn asks_no_type_of_a_binding_in_a_file_the_host_reads_nothing_from() {
        // The other half of the rule: a `lem-let` is a local everywhere else, and
        // infers as it always has.
        let files = [SourceFile::new(
            "t.lem",
            layout("<lem-let name = \"limulus.net\" /><lem.name>"),
        )];
        assert!(diagnostics_with(&shell_host(), &files).is_empty());
    }

    #[test]
    fn takes_a_capture_body_in_the_site_file_as_an_html_binding() {
        let files = [
            SourceFile::new("site.lem", "<lem-let intro><p>hi</p></lem-let>"),
            SourceFile::new("t.lem", layout("<lem.site.intro>")),
        ];
        assert!(diagnostics_with(&shell_host(), &files).is_empty());
    }

    #[test]
    fn types_site_from_a_from_bindings_head_alone() {
        // The binding's type is what tells the host what to load, and it is read from
        // the head: `site` is typed by it with nothing loaded yet.
        let files = [
            SourceFile::new("site.lem", "<lem-let posts: [string] from \"f/*.md\" />"),
            SourceFile::new(
                "t.lem",
                layout("<lem-for p in site.posts><lem.p></lem-for>"),
            ),
        ];
        assert!(diagnostics_with(&shell_host(), &files).is_empty());
    }

    #[test]
    fn does_not_warn_that_a_binding_the_host_reads_is_unused() {
        let files = [SourceFile::new(
            "site.lem",
            "<lem-let name: string = \"limulus.net\" />",
        )];
        assert!(diagnostics_with(&shell_host(), &files).is_empty());
    }

    #[test]
    fn still_warns_for_an_unread_binding_inside_the_site_file() {
        // The exemption is the top level's, where a binding is a field of the record
        // every other file reads — not every `lem-let` in the file.
        const SRC: &str = "<lem-fn f(): html><lem-let unread = 1 /><p>x</p></lem-fn>";
        assert_eq!(
            diagnostics_with(&shell_host(), &[SourceFile::new("site.lem", SRC)]),
            [format!(
                "site.lem:{}: warning: `unread` is never used",
                at(SRC, "unread")
            )]
        );
    }

    #[test]
    fn compiles_a_document_as_the_record_the_host_named() {
        let files = [
            SourceFile::new("ambient.lem", AMBIENT),
            SourceFile::new(
                "schema.lem",
                "<lem-type Post extends Page>\n  title: string\n  \
                 layout: (Post, html): html\n</lem-type>",
            ),
            SourceFile::new("post.md", "<lem.page.title>").with_record("Post"),
        ];
        assert!(diagnostics_with(&shell_host(), &files).is_empty());
    }

    #[test]
    fn compiles_a_document_the_host_gave_no_record_unknown_typed() {
        let files = [
            SourceFile::new("ambient.lem", AMBIENT),
            SourceFile::new("post.md", "<lem.page.whatever>"),
        ];
        assert!(diagnostics_with(&shell_host(), &files).is_empty());
    }

    #[test]
    fn points_a_record_diagnostic_at_the_key_the_host_named_it_at() {
        // The host parsed the front matter and knows where `type:` stands; without
        // that, the front matter as a whole is the honest span.
        let files = |file: SourceFile| [SourceFile::new("ambient.lem", AMBIENT), file];
        let text = "---\ntype: Nope\n---\nhi";
        let named = SourceFile::new("post.md", text)
            .with_record("Nope")
            .named_at(at(text, "Nope"));
        assert_eq!(
            diagnostics_with(&shell_host(), &files(named)),
            [format!("post.md:{}: unknown type `Nope`", at(text, "Nope"))]
        );
        let unplaced = SourceFile::new("post.md", text).with_record("Nope");
        assert_eq!(
            diagnostics_with(&shell_host(), &files(unplaced)),
            [format!(
                "post.md:{}: unknown type `Nope`",
                at(text, "---\ntype: Nope\n---\n")
            )]
        );
    }

    #[test]
    fn refuses_a_record_where_the_host_renders_no_documents() {
        const SRC: &str = "hi";
        let host = Host::untyped(["page"]);
        let files = [SourceFile::new("post.md", SRC).with_record("Post")];
        assert_eq!(
            diagnostics_with(&host, &files),
            [format!(
                "post.md:{}: this file is compiled as `Post`, and the host binds no \
                 document value to be one",
                at(SRC, "«»hi")
            )]
        );
    }

    #[test]
    fn takes_an_entry_field_a_page_schema_inherits_from_the_document_type() {
        // A host is free to declare the entry field on the type it renders documents
        // as; everything below inherits it, and a subtype that redeclared it could only
        // repeat the same type (D32).
        let files = [
            SourceFile::new(
                "ambient.lem",
                "<lem-type Page>\n  url: url\n  layout: (Page, html): html\n</lem-type>",
            ),
            SourceFile::new(
                "schema.lem",
                "<lem-type Post extends Page>\n  title: string\n</lem-type>",
            ),
        ];
        assert!(diagnostics_with(&shell_host(), &files).is_empty());
    }

    #[test]
    fn refuses_a_record_where_the_host_names_a_document_value_it_never_bound() {
        const POST: &str = "hello";
        // Named is not bound: there is no value for the record to type, so this is the
        // same mistake as naming no document value at all — and recording the binding
        // anyway would leave the unit looking as though the host had dropped one.
        let host = Host::untyped(["site"])
            .documents("page")
            .rendered_through("layout");
        let files = [
            SourceFile::new(
                "schema.lem",
                "<lem-type Post>\n  title: string\n</lem-type>",
            ),
            SourceFile::new("post.md", POST).with_record("Post"),
        ];
        let compiled = compile(&files, &host);
        assert_eq!(
            shown(&compiled.diagnostics),
            [format!(
                "post.md:{}: this file is compiled as `Post`, and the host binds no \
                 document value to be one",
                at(POST, "«»hello")
            )]
        );
    }

    /// The shell's host, with a generated page's neighbours named.
    fn neighboured() -> Host {
        shell_host().neighboured_by("prev", "next")
    }

    /// `schema` compiled beside the document type under `host`.
    fn schema_with(host: &Host, schema: &str) -> Vec<String> {
        diagnostics_with(
            host,
            &[
                SourceFile::new("ambient.lem", AMBIENT),
                SourceFile::new("schema.lem", schema),
            ],
        )
    }

    const SLIDE: &str = "<lem-type Slide extends Page>\n  prev: Page\n  \
         next: Slide? = null\n  layout: (Slide, html): html\n</lem-type>";

    #[test]
    fn refuses_a_neighbour_a_page_schema_declares_as_no_optional_of_itself_or_default() {
        assert_eq!(
            schema_with(&neighboured(), SLIDE),
            [
                format!(
                    "schema.lem:{}: `prev` holds a generated page's neighbour, a page of \
                     its own type: declare it `prev: Slide?`, or an optional of a type \
                     `Slide` extends",
                    at(SLIDE, "«prev»: Page")
                ),
                format!(
                    "schema.lem:{}: `next` is a page's neighbour, which the loader fills, \
                     so it takes no default",
                    at(SLIDE, "«next»: Slide?")
                ),
            ]
        );
        // The same fields are ordinary ones under a host that names no neighbours.
        assert_eq!(schema_with(&shell_host(), SLIDE), Vec::<String>::new());
    }

    #[test]
    fn leaves_fields_of_neighbour_names_alone_on_a_record_that_is_no_page() {
        // Nothing reserves the names: a record no page has is its own business, and
        // its default may read the field.
        let revision = "<lem-type Revision>\n  prev: string?\n  next: string? = null\n  \
             text: string = self.prev ?? \"first\"\n</lem-type>";
        assert_eq!(schema_with(&neighboured(), revision), Vec::<String>::new());
        // A page schema declaring one of the two, as an optional of an ancestor.
        let chapter = "<lem-type Chapter extends Page>\n  next: Page?\n  \
             layout: (Chapter, html): html\n</lem-type>";
        assert_eq!(schema_with(&neighboured(), chapter), Vec::<String>::new());
    }

    #[test]
    fn says_nothing_more_of_a_neighbour_whose_type_is_not_declared() {
        let typo = "<lem-type Slide extends Page>\n  prev: Slid?\n  \
             layout: (Slide, html): html\n</lem-type>";
        assert_eq!(
            schema_with(&neighboured(), typo),
            [format!(
                "schema.lem:{}: unknown type `Slid`",
                at(typo, "«Slid»?")
            )]
        );
    }

    #[test]
    fn refuses_a_default_that_reads_its_pages_neighbour_where_the_read_stands() {
        let reads = "<lem-type Slide extends Page>\n  title: string\n  prev: Slide?\n  \
             blurb: string = self.prev?.title ?? \"\"\n  \
             tease: [string] = [\"x\"].map(|x| self.prev?.title ?? x)\n  \
             layout: (Slide, html): html\n</lem-type>";
        let why = "is out of reach of a default: the loader fills a page's neighbours once \
                   every page of its generator is built";
        assert_eq!(
            schema_with(&neighboured(), reads),
            [
                format!(
                    "schema.lem:{}: `prev` {why}",
                    at(reads, "«self.prev»?.title ?? \"\"")
                ),
                // Inside a lambda, `self` is still the page being built.
                format!(
                    "schema.lem:{}: `prev` {why}",
                    at(reads, "«self.prev»?.title ?? x")
                ),
            ]
        );
        assert_eq!(schema_with(&shell_host(), reads), Vec::<String>::new());
    }

    #[test]
    fn says_only_that_a_page_has_no_neighbour_field_it_does_not_declare() {
        let undeclared = "<lem-type Slide extends Page>\n  title: string\n  \
             blurb: string = self.prev?.title ?? \"\"\n  \
             layout: (Slide, html): html\n</lem-type>";
        assert_eq!(
            schema_with(&neighboured(), undeclared),
            [format!(
                "schema.lem:{}: `Slide` has no field `prev`",
                at(undeclared, "self.prev")
            )]
        );
    }
}

//! What a file leaves the loader to do before any render (D37): run the function behind
//! every field default the records it declares carry (∆iCW), and index the map an
//! ambient value holds, where a field is declared `by` one (∆eag).
//!
//! [`Compiler::function_for_loader`] is the shape every such function takes — the
//! parameters the loader will hand it bound, and what its body reads and calls recorded
//! ([`uses`](super::uses)) — and the bindings file's computed bindings take it too, from
//! [`items`](super::items). Which ambient values one of them may read is
//! [`refused_at_load`]'s one rule, asked by the walk where a read stands
//! ([`expr`](super::expr)) and again by the link step when a call reaches one (∆HF0).

use std::rc::Rc;

use lemmon_syntax::ast::{Expr, Item, Spanned};
use lemmon_syntax::{Parse, Span};

use super::names::{BindingKind, Resolved};
use super::uses::Implicit;
use super::{Compiler, TokenSpan, required, rules};
use crate::bytecode::Op;
use crate::decls::{DeclKind, Decls, Shape};
use crate::types::Type;
use crate::unit::{DefaultFn, IndexDecl};

impl Compiler<'_> {
    /// Compiles the loader's function for every field default the records this file
    /// declares carry (∆iCW), and records every `by` index (∆eag). A default is
    /// checked against its field's type and runs with `self` — the record being
    /// built, as the declaring type — and, on a record that extends the host's
    /// document type, `path` and `body` as well. An index names a field of an
    /// ambient value that is a map from string to the field's record, and is no
    /// function: the loader indexes that map as loaded.
    pub(super) fn compile_defaults(&mut self, parse: &Parse) {
        for item in parse.root().items() {
            let Item::TypeDecl(decl) = &item else {
                continue;
            };
            let name = required(decl.name());
            // Only a declaration this file kept: a refused one (reserved, repeated, or
            // another file's) was reported where it was collected.
            let Some(declared) = self
                .unit
                .types
                .iter()
                .find(|d| &*d.name == name.text() && d.name_span == name.span())
                .cloned()
            else {
                continue;
            };
            if self
                .index
                .lookup_type(&declared.name)
                .is_some_and(|owner| owner.path != self.path)
            {
                continue;
            }
            let Shape::Record {
                fields: declared_fields,
                ..
            } = &declared.shape
            else {
                unreachable!("a `lem-type` declares a record");
            };
            let page_backed = self.host.document_type().is_some_and(|document| {
                *document == *declared.name || self.index.extends(&declared.name, &document)
            });
            for field in decl.fields() {
                let field_name = required(field.name());
                // A repeated field was refused with the declaration's own diagnostic;
                // whatever default it carries belongs to nothing.
                let Some(field_decl) =
                    declared_fields.iter().find(|f| f.span == field_name.span())
                else {
                    continue;
                };
                let ty = self.known(field_decl.ty.clone());
                let span = field_name.span();
                let name = format!("{}.{}", declared.name, field_decl.name);
                if let Some(default) = field.default() {
                    let (func, reads_body) =
                        self.compile_step(&name, &declared.name, page_backed, span, |c| {
                            c.check(&default, &ty);
                        });
                    self.unit.defaults.push(DefaultFn {
                        record: declared.name.clone(),
                        field: field_decl.name.clone(),
                        func,
                        span,
                        reads_body,
                    });
                }
                if let Some(index) = field.index() {
                    if let Type::Named(record) = ty.strip_optional()
                        && self.decls().kind(record) == Some(DeclKind::Record)
                    {
                        let record = record.clone();
                        if let Some((value, key)) = self.index_field(&index, &record) {
                            self.unit.indexes.push(IndexDecl {
                                record: declared.name.clone(),
                                field: field_decl.name.clone(),
                                value,
                                key,
                                span,
                            });
                        }
                    } else if ty == field_decl.ty {
                        // Declared as written — `any` included, which is spelled and
                        // resolves nothing. An unknown type reads as `any` only after
                        // the annotation reported it, and nothing more is said then.
                        self.error(
                            index.span(),
                            format!(
                                "`by` resolves a record: `{}` is declared `{ty}`",
                                field_decl.name
                            ),
                        );
                    }
                }
            }
        }
    }

    /// The ambient value and field a `by` index names — `by site.photos` — checked
    /// to be declared `{string: R}` for the field's record `R`, or nothing once what
    /// is wrong with the index has been reported: it is some other expression, the
    /// name is undefined or not an ambient value, the value is the host's document
    /// value, which no record may reach while it loads, or the field is not declared
    /// or not that map — the field read typed as `value.field` is typed anywhere.
    /// The value's binding is a fact of this unit, as any read of it is (D25); a
    /// value the host left untyped admits any field.
    fn index_field(
        &mut self,
        index: &Expr,
        record: &Rc<str>,
    ) -> Option<(Rc<str>, Rc<str>)> {
        const SHAPE: &str =
            "`by` names a field of an ambient value, as in `by site.photos`";
        let Expr::Member(member) = index else {
            self.error(index.span(), SHAPE);
            return None;
        };
        let (Expr::Name(name), false) = (required(member.object()), member.is_optional())
        else {
            self.error(index.span(), SHAPE);
            return None;
        };
        let (value, key) = (required(name.name()), required(member.name()));
        let ty = match self.names.peek(value.text()) {
            Some(Resolved::Ambient(ty)) => ty,
            Some(_) => {
                self.error(index.span(), SHAPE);
                return None;
            }
            None => {
                self.error(value.span(), self.undefined(value.text()));
                return None;
            }
        };
        if self.host.document() == Some(value.text()) {
            self.error(
                value.span(),
                format!(
                    "`{}` is bound to a record once it is loaded, so an index cannot \
                     name a field of it",
                    value.text()
                ),
            );
            return None;
        }
        let ty = self.ambient_type(value.text(), ty, value.span());
        let actual = match rules::member(&ty, key.text(), false, self.decls()) {
            Ok(actual) => self.known(actual),
            Err(message) => {
                self.error(key.span(), message);
                return None;
            }
        };
        let expected = Type::map(Type::Named(record.clone()));
        match rules::expected(actual, &expected, self.decls()) {
            Ok(_) => Some((Rc::from(value.text()), Rc::from(key.text()))),
            Err(message) => {
                self.error(index.span(), message);
                None
            }
        }
    }

    /// Compiles one function the loader runs for the record `record`: `self` bound
    /// as that record, and `path`/`body` too when `page_backed`, then whatever `lower`
    /// emits as its return value. Collects what the body reads while it runs, lambdas
    /// inside included, and returns the function's index with where it first read
    /// `body`, if it did. The three implicit names are the language's, so they take
    /// [`BindingKind::Implicit`], and they are told apart by binder at three distinct
    /// spans inside the field's name — nothing is ever reported at one.
    fn compile_step(
        &mut self,
        name: &str,
        record: &Rc<str>,
        page_backed: bool,
        span: Span,
        lower: impl FnOnce(&mut Self),
    ) -> (u32, Option<Span>) {
        let mut params: Vec<(&str, Type, Span)> =
            vec![("self", Type::Named(record.clone()), span)];
        let mut implicit = Implicit {
            self_binder: Some(span),
            body_binder: None,
        };
        if page_backed {
            let body = Span::new(span.end, span.end);
            params.push(("path", Type::String, Span::new(span.start, span.start)));
            params.push(("body", Type::Html, body));
            implicit.body_binder = Some(body);
        }
        self.function_for_loader(
            name,
            &params,
            Runs::Default { page_backed },
            implicit,
            span,
            lower,
        )
    }

    /// Compiles a function of `params` the loader calls, returning what `lower` leaves
    /// on the stack, with its reads and calls recorded in the unit — `implicit` naming
    /// the binders of the `self` and the `body` among the parameters, where there are
    /// such. Returns the function's index and where it first read `body`.
    pub(super) fn function_for_loader(
        &mut self,
        name: &str,
        params: &[(&str, Type, Span)],
        runs: Runs,
        implicit: Implicit,
        span: Span,
        lower: impl FnOnce(&mut Self),
    ) -> (u32, Option<Span>) {
        self.push_frame();
        for (param, ty, binder) in params {
            self.names
                .bind_local(param, *binder, BindingKind::Implicit, ty.clone());
        }
        let collected = self.collecting(Some(runs), implicit, Vec::new(), lower);
        self.emit(Op::Return, span);
        let (ctx, frame) = self.pop_frame();
        let func = self.reserve(name);
        self.finish(
            func,
            ctx,
            &frame,
            params
                .iter()
                .map(|(param, _, _)| Rc::from(*param))
                .collect(),
        );
        self.unit.uses.insert(func, collected.uses);
        (func, collected.body_read)
    }
}

/// The function the loader runs a read stands in: a field's default, of a record that
/// extends the host's document type or not, or a computed binding of the bindings
/// file. What decides which ambient values the function may read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Runs {
    Default { page_backed: bool },
    Binding,
}

/// Why the ambient value `name` is not there for a function the loader runs as
/// `runs`, if it is not — `document` being the host's document value: a default reads
/// only the record it builds, since a record's plan depends on its own declarations
/// alone (∆eag), and the document value is bound per page, so a computed binding,
/// which runs before any page is, may not read that one (∆iCW). One rule and one
/// sentence, said by the compiler where the read stands and by the link step when a
/// call reaches it (∆HF0).
pub(crate) fn refused_at_load(
    runs: Runs,
    document: Option<&str>,
    name: &str,
) -> Option<String> {
    match runs {
        Runs::Default { page_backed } => Some(format!(
            "`{name}` is out of scope in a default: a default reads only the record it \
             builds — `self`{}",
            if page_backed {
                ", and `path` and `body` on a page"
            } else {
                ""
            }
        )),
        Runs::Binding => (document == Some(name)).then(|| {
            format!(
                "`{name}` is bound per page, and a binding here is computed once, at load"
            )
        }),
    }
}

#[cfg(test)]
mod tests {
    use lemmon_syntax::spans::at;

    use super::*;
    use crate::SourceFile;
    use crate::testing::{AMBIENT, shell_host, shown_with, unit_with, untyped_host};

    #[test]
    fn checks_a_fields_default_against_the_fields_type() {
        const SRC: &str =
            "<lem-type A>\n  title: string = \"T\"\n  n: int = \"no\"\n</lem-type>";
        assert_eq!(
            shown_with(&untyped_host(), &[SourceFile::new("types.lem", SRC)]),
            [format!(
                "types.lem:{}: expected `int`, got `string`",
                at(SRC, "\"no\"")
            )]
        );
    }

    #[test]
    fn names_a_default_after_its_field_and_gives_it_self() {
        let unit = unit_with(
            &untyped_host(),
            "types.lem",
            &[SourceFile::new(
                "types.lem",
                "<lem-type A>\n  first: string\n  full: string = self.first\n</lem-type>",
            )],
        );
        let full = &unit.functions[unit.defaults[0].func as usize];
        assert_eq!(full.name, "A.full");
        assert_eq!(full.params, [Rc::from("self")]);
    }

    #[test]
    fn gives_a_page_backed_default_path_and_body() {
        let post = SourceFile::new(
            "schema.lem",
            "<lem-type Post extends Page>\n  slug: string = path\n  \
             excerpt: html = body\n  layout: (Post, html): html\n</lem-type>",
        );
        let files = [SourceFile::new("ambient.lem", AMBIENT), post];
        assert_eq!(shown_with(&shell_host(), &files), Vec::<String>::new());
        let unit = unit_with(&shell_host(), "schema.lem", &files);
        let slug = &unit.functions[unit.defaults[0].func as usize];
        assert_eq!(
            slug.params,
            [Rc::from("self"), Rc::from("path"), Rc::from("body")]
        );
    }

    #[test]
    fn gives_a_data_records_default_neither_path_nor_body() {
        // A record loaded from data has no document behind it.
        const SRC: &str = "<lem-type Author>\n  bio: html = body\n</lem-type>";
        let author = SourceFile::new("author.lem", SRC);
        assert_eq!(
            shown_with(
                &shell_host(),
                &[SourceFile::new("ambient.lem", AMBIENT), author]
            ),
            [format!(
                "author.lem:{}: undefined name `body`",
                at(SRC, "body")
            )]
        );
    }

    #[test]
    fn refuses_every_ambient_value_inside_a_default() {
        // A record's plan depends on its own declarations alone (∆eag): every ambient
        // value is out of scope, whole or a field of it.
        for (ty, default, name) in [
            ("url", "page.url", "page"),
            ("string", "site.name", "site"),
            ("url", "show(page)", "page"),
        ] {
            let schema = format!(
                "<lem-type Post extends Page>\n  title: {ty} = {default}\n  \
                 layout: (Post, html): html\n</lem-type>\n\
                 <lem-fn show(p: Page): url = p.url />"
            );
            let files = [
                SourceFile::new("ambient.lem", AMBIENT),
                SourceFile::new("site.lem", "<lem-let name: string = \"n\" />"),
                SourceFile::new("schema.lem", schema.clone()),
            ];
            assert_eq!(
                shown_with(&shell_host(), &files),
                [format!(
                    "schema.lem:{}: `{name}` is out of scope in a default: a default \
                     reads only the record it builds — `self`, and `path` and `body` \
                     on a page",
                    at(&schema, &format!("«{name}»"))
                )],
                "{default}"
            );
        }
    }

    #[test]
    fn tells_a_data_records_default_only_of_self() {
        // A record above the document type has no `path` or `body` to be told of.
        const SRC: &str = "<lem-type Photo>\n  caption: string = site.name\n</lem-type>";
        let files = [
            SourceFile::new("site.lem", "<lem-let name: string = \"n\" />"),
            SourceFile::new("photo.lem", SRC),
        ];
        assert_eq!(
            shown_with(&shell_host(), &files),
            [format!(
                "photo.lem:{}: `site` is out of scope in a default: a default reads \
                 only the record it builds — `self`",
                at(SRC, "«site».name")
            )]
        );
    }

    #[test]
    fn refuses_the_bindings_value_inside_a_default_declared_in_the_bindings_file() {
        // The bindings file's own records get no special reach: a default there reads
        // `site.x` as any file would, and is refused as any file would be.
        const SITE: &str = "<lem-type Tag>\n  label: string = site.name\n  \
                            bare: string = name\n</lem-type>\n\
                            <lem-let name: string = \"n\" />";
        assert_eq!(
            shown_with(&shell_host(), &[SourceFile::new("site.lem", SITE)]),
            [
                format!(
                    "site.lem:{}: `site` is out of scope in a default: a default reads \
                     only the record it builds — `self`",
                    at(SITE, "«site».name")
                ),
                format!("site.lem:{}: undefined name `name`", at(SITE, "= «name»")),
            ]
        );
    }

    #[test]
    fn records_a_by_index_as_the_ambient_field_it_names() {
        // No function: the pair is the whole of what the loader needs, and `site`'s
        // binding is a fact of the unit as any read of it would make it (∆eag).
        let site = SourceFile::new(
            "site.lem",
            "<lem-let photos: {string: Photo} from \"p.json\" />\n\
             <lem-let name: string = \"n\" />",
        );
        let photo = "<lem-type Photo>\n  alt: string\n</lem-type>\n";
        let schema =
            format!("{photo}<lem-type Post>\n  hero: Photo? by site.photos\n</lem-type>");
        let files = [site.clone(), SourceFile::new("schema.lem", schema.clone())];
        assert!(shown_with(&shell_host(), &files).is_empty());
        let unit = unit_with(&shell_host(), "schema.lem", &files);
        assert!(unit.defaults.is_empty());
        assert_eq!(
            unit.indexes,
            [IndexDecl {
                record: Rc::from("Post"),
                field: Rc::from("hero"),
                value: Rc::from("site"),
                key: Rc::from("photos"),
                span: at(&schema, "«hero»: Photo?"),
            }]
        );
        assert_eq!(unit.functions.len(), 1, "the entry alone");
        assert_eq!(
            unit.ambient_refs
                .iter()
                .map(|r| &*r.name)
                .collect::<Vec<_>>(),
            ["site"]
        );
        for (field, message) in [
            (
                "hero: Photo? by site.name",
                "expected `{string: Photo}`, got `string`",
            ),
            (
                "hero: Photo? by site.nope",
                "`site.lem` has no field `nope`; did you mean `name`?",
            ),
            (
                "hero: Photo? by lookup()",
                "`by` names a field of an ambient value, as in `by site.photos`",
            ),
            (
                "hero: Photo? by photos",
                "`by` names a field of an ambient value, as in `by site.photos`",
            ),
            (
                "hero: Photo? by page.photos",
                "`page` is bound to a record once it is loaded, so an index cannot name a \
                 field of it",
            ),
            ("hero: Photo? by ste.photos", "undefined name `ste`"),
            (
                "title: string by site.photos",
                "`by` resolves a record: `title` is declared `string`",
            ),
            // `any` is spelled, and resolves nothing.
            (
                "hero: any by site.photos",
                "`by` resolves a record: `hero` is declared `any`",
            ),
            // The unknown type was reported at the annotation; nothing more is said.
            ("x: Nope by site.photos", "unknown type `Nope`"),
            ("x: Nope? by site.photos", "unknown type `Nope`"),
        ] {
            let files = [
                site.clone(),
                SourceFile::new(
                    "schema.lem",
                    format!("{photo}<lem-type Post>\n  {field}\n</lem-type>"),
                ),
            ];
            let reported = shown_with(&shell_host(), &files);
            assert_eq!(reported.len(), 1, "{field}: {reported:?}");
            assert!(reported[0].ends_with(message), "{field}: {reported:?}");
        }
    }

    #[test]
    fn takes_any_field_of_an_untyped_value_as_a_by_index() {
        let unit = unit_with(
            &untyped_host(),
            "schema.lem",
            &[SourceFile::new(
                "schema.lem",
                "<lem-type Photo>\n  alt: string\n</lem-type>\n\
                 <lem-type Post>\n  hero: Photo? by site.whatever\n</lem-type>",
            )],
        );
        assert_eq!(unit.indexes.len(), 1);
        assert_eq!(&*unit.indexes[0].key, "whatever");
    }

    #[test]
    fn resolves_a_default_that_names_a_project_function() {
        const SCHEMA: &str = "<lem-type Post extends Page>\n  \
                              layout: (Post, html): html = other\n</lem-type>\n\
                              <lem-fn other(s: string): html><lem.s></lem-fn>";
        let files = [
            SourceFile::new("ambient.lem", AMBIENT),
            SourceFile::new(
                "schema.lem",
                "<lem-type Post extends Page>\n  layout: (Post, html): html = post\n\
                 </lem-type>\n<lem-fn post(p: Post, body: html)><lem.p.url><lem.body></lem-fn>",
            ),
        ];
        assert_eq!(shown_with(&shell_host(), &files), Vec::<String>::new());
        let files = [
            SourceFile::new("ambient.lem", AMBIENT),
            SourceFile::new("schema.lem", SCHEMA),
        ];
        assert_eq!(
            shown_with(&shell_host(), &files),
            [format!(
                "schema.lem:{}: expected `(Post, html): html`, got `(string): html`",
                at(SCHEMA, "= «other»")
            )]
        );
    }

    #[test]
    fn compiles_no_default_for_a_declaration_or_field_the_file_did_not_keep() {
        // `b.lem`'s `A` lost to `a.lem`'s, and `B`'s second `t` is a repeat: neither
        // default belongs to anything, so neither is compiled — were they, each would
        // add a diagnostic of its own to the duplicate's, since `nope` is no field.
        const A: &str = "<lem-type A>\n  t: string = \"a\"\n</lem-type>";
        const B: &str = "<lem-type A>\n  t: string = self.nope\n</lem-type>\n\
                         <lem-type B>\n  t: string = \"1\"\n  t: string = self.nope\n\
                         </lem-type>";
        let files = [SourceFile::new("a.lem", A), SourceFile::new("b.lem", B)];
        assert_eq!(
            shown_with(&untyped_host(), &files),
            [
                format!(
                    "b.lem:{}: `A` is already defined\n  a.lem:{}: first defined here",
                    at(B, "A"),
                    at(A, "A")
                ),
                format!(
                    "b.lem:{}: `t` is already defined\n  b.lem:{}: first defined here",
                    at(B, "\"1\"\n  «t»: string"),
                    at(B, "B>\n  «t»: string")
                ),
            ]
        );
    }
}

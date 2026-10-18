//! Type declarations as the checker reads them: the records `lem-type` declares and the
//! enums `lem-enum` declares, project-flat (`docs/language.md` §Schemas, §Names).
//!
//! A [`Declaration`] is one declaration with its spans — what a unit exports and the
//! [`ExportIndex`](crate::ExportIndex) holds — and a [`TypeFact`] is the same shape with
//! the spans stripped: what a unit records when it consumed another file's type (D25),
//! so that a whitespace edit in the declaring file does not make every consumer stale.
//! [`Decls`] is the table the rules ask: types are nominal, so a [`Type::Named`] is a
//! name alone and everything about it — whether it is a record or an enum, what fields
//! it has, what it extends — comes from here.
//!
//! [`no_field`], [`no_entry_field`] and [`misshaped_entry_field`] are messages rather
//! than facts, and [`entry_fault`] is the judgement the last two report; so are
//! [`misdeclared_neighbour`] and [`neighbour_default`], beside [`neighbour_fits`], the
//! judgement of a generated page's neighbour field (D43). They are here for the same
//! reason: refusing a field name, or judging a field the host names, means knowing every
//! field the record has and what it extends, so the refusals their several sites share
//! are built where that knowledge already lives.

use std::rc::Rc;

use lemmon_syntax::{Span, nearest};

use crate::types::Type;

/// Whether a declared name is a record or an enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeclKind {
    Record,
    Enum,
}

/// One `lem-type` or `lem-enum`, as declared.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declaration {
    /// The declared name.
    pub name: Rc<str>,
    /// The name token in the declaration — where a duplicate of it is reported, and what
    /// a duplicate's label points at.
    pub name_span: Span,
    pub shape: Shape,
}

/// What a declaration declares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shape {
    /// A record: its base, if it `extends` one, and its own fields in declaration order.
    Record {
        base: Option<(Rc<str>, Span)>,
        fields: Vec<FieldDecl>,
    },
    /// An enum: its variants in declaration order.
    Enum { variants: Vec<(Rc<str>, Span)> },
}

/// One field of a record: its name, its declared type, and the bytes of its name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldDecl {
    pub name: Rc<str>,
    pub ty: Type,
    pub span: Span,
}

impl Declaration {
    /// The declaration as a consumer records it: the shape, without the spans.
    #[must_use]
    pub fn fact(&self) -> TypeFact {
        TypeFact {
            name: self.name.clone(),
            shape: match &self.shape {
                Shape::Record { base, fields } => ShapeFact::Record {
                    base: base.as_ref().map(|(name, _)| name.clone()),
                    fields: fields
                        .iter()
                        .map(|field| (field.name.clone(), field.ty.clone()))
                        .collect(),
                },
                Shape::Enum { variants } => ShapeFact::Enum {
                    variants: variants.iter().map(|(name, _)| name.clone()).collect(),
                },
            },
        }
    }

    /// Whether this is a record or an enum.
    #[must_use]
    pub const fn kind(&self) -> DeclKind {
        match self.shape {
            Shape::Record { .. } => DeclKind::Record,
            Shape::Enum { .. } => DeclKind::Enum,
        }
    }
}

/// A declaration another file's unit was compiled against: the shape only, never a span.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeFact {
    pub name: Rc<str>,
    pub shape: ShapeFact,
}

impl TypeFact {
    /// The shape the way it would be declared: `Article extends Page {title: string}`,
    /// `Placement = hero | left`.
    #[must_use]
    pub fn text(&self) -> String {
        match &self.shape {
            ShapeFact::Record { base, fields } => {
                let base = base
                    .as_ref()
                    .map_or(String::new(), |base| format!(" extends {base}"));
                let fields: Vec<String> = fields
                    .iter()
                    .map(|(name, ty)| format!("{name}: {ty}"))
                    .collect();
                format!("{}{base} {{{}}}", self.name, fields.join(", "))
            }
            ShapeFact::Enum { variants } => {
                let variants: Vec<&str> = variants.iter().map(|v| &**v).collect();
                format!("{} = {}", self.name, variants.join(" | "))
            }
        }
    }
}

/// A [`Shape`] without its spans.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShapeFact {
    Record {
        base: Option<Rc<str>>,
        fields: Vec<(Rc<str>, Type)>,
    },
    Enum {
        variants: Vec<Rc<str>>,
    },
}

/// The declaration table a rule consults: one required lookup, and the walks over it
/// that every consumer needs. A base the table has no entry for ends a walk — a resident
/// subtype outlives its base's file failing to recompile — and a cycle a failing file
/// introduced never loops another file's compile.
pub trait Decls {
    /// The declaration of `name`, if the project has one.
    fn declaration(&self, name: &str) -> Option<&Declaration>;

    /// Whether `name` is a record or an enum.
    fn kind(&self, name: &str) -> Option<DeclKind> {
        self.declaration(name).map(Declaration::kind)
    }

    /// `name` and every record it extends, nearest first, stopping at a base the table
    /// does not have or has already yielded.
    fn chain(&self, name: &str) -> Vec<&Declaration> {
        let mut chain: Vec<&Declaration> = Vec::new();
        let mut next = Some(name);
        while let Some(name) = next {
            let Some(decl) = self.declaration(name) else {
                break;
            };
            if chain.iter().any(|seen| seen.name == decl.name) {
                break;
            }
            chain.push(decl);
            next = match &decl.shape {
                Shape::Record {
                    base: Some((base, _)),
                    ..
                } => Some(base),
                _ => None,
            };
        }
        chain
    }

    /// Whether the record `sub` extends `base`, directly or through other records.
    fn extends(&self, sub: &str, base: &str) -> bool {
        self.chain(sub)
            .iter()
            .skip(1)
            .any(|decl| &*decl.name == base)
    }

    /// The field `name` of the record `record` as declared, with the declaration in
    /// its chain that declares it — the nearest, since a redeclaration may change the
    /// default (D32).
    fn field_in(&self, record: &str, name: &str) -> Option<(&Declaration, &FieldDecl)> {
        self.chain(record)
            .into_iter()
            .find_map(|decl| match &decl.shape {
                Shape::Record { fields, .. } => fields
                    .iter()
                    .find(|field| &*field.name == name)
                    .map(|field| (decl, field)),
                Shape::Enum { .. } => None,
            })
    }

    /// The type of the field `name` of the record `record`: its own, or the nearest
    /// ancestor's.
    fn field(&self, record: &str, name: &str) -> Option<Type> {
        self.field_in(record, name)
            .map(|(_, field)| field.ty.clone())
    }

    /// Every field of the record `record`, its own and its ancestors', each once with
    /// the type of the nearest declaration (a redeclaration carries the same type, D32),
    /// own fields first.
    fn fields(&self, record: &str) -> Vec<(Rc<str>, Type)> {
        let mut fields: Vec<(Rc<str>, Type)> = Vec::new();
        for decl in self.chain(record) {
            if let Shape::Record { fields: own, .. } = &decl.shape {
                for field in own {
                    if !fields.iter().any(|(name, _)| *name == field.name) {
                        fields.push((field.name.clone(), field.ty.clone()));
                    }
                }
            }
        }
        fields
    }

    /// The variants of the enum `name`, if it is one.
    fn variants(&self, name: &str) -> Option<&[(Rc<str>, Span)]> {
        match &self.declaration(name)?.shape {
            Shape::Enum { variants } => Some(variants),
            Shape::Record { .. } => None,
        }
    }
}

/// The refusal a field name that names nothing gets: which record it was read on, and —
/// when one of that record's fields is close enough to be worth naming — the field it
/// was probably meant to be.
///
/// One function behind all three places a field name is refused: a `.` read, a record
/// literal's key, and a loaded document's front-matter key. A typo is the same mistake
/// wherever an author makes it, and should read the same however it was made.
///
/// Candidates come from [`Decls::fields`], which yields a record's own fields before the
/// ones it inherits, and [`nearest`] breaks a tie towards the first — so a name equally
/// close to a record's own field and to an ancestor's is answered with the record's own,
/// which is the declaration the author is looking at.
pub(crate) fn no_field(record: &str, key: &str, decls: &dyn Decls) -> String {
    let fields = decls.fields(record);
    let names = fields.iter().map(|(name, _)| &**name);
    match nearest(key, names) {
        Some(near) => format!("`{record}` has no field `{key}`; did you mean `{near}`?"),
        None => format!("`{record}` has no field `{key}`"),
    }
}

/// What keeps `record` from rendering through the field `entry` (D30): it declares
/// none, or the one it has is not a layout of it.
pub(crate) enum EntryFault<'a> {
    Undeclared,
    /// The field as declared, with the declaration in the chain that made it.
    Misshaped(&'a Declaration, &'a FieldDecl),
}

/// Whether `record` can render through `entry`, and what is wrong if not. The page the
/// field's function takes is the record or one it extends: a layout free to take the
/// base and narrow inside is what makes one layout serve a family of schemas. One
/// predicate for the compiler, which asks it of every schema it declares, and for the
/// link step, which asks it of every page's record on a relink the declaration alone
/// changed.
pub(crate) fn entry_fault<'a>(
    decls: &'a dyn Decls,
    record: &str,
    entry: &str,
) -> Option<EntryFault<'a>> {
    let Some((owner, field)) = decls.field_in(record, entry) else {
        return Some(EntryFault::Undeclared);
    };
    let takes_a_page = match &field.ty {
        Type::Fn(params, ret) => matches!(
            (params.first(), params.get(1), &**ret),
            (Some(Type::Named(page)), Some(Type::Html), Type::Html)
                if **page == *record || decls.extends(record, page)
        ),
        _ => false,
    };
    (!takes_a_page).then_some(EntryFault::Misshaped(owner, field))
}

/// How the entry field of `record` is declared: `entry: (record, html): html`.
fn entry_shape(record: &str, entry: &str) -> String {
    format!("`{entry}: ({record}, html): html`")
}

/// The refusal of a page schema that declares no entry field.
pub(crate) fn no_entry_field(record: &str, document: &str, entry: &str) -> String {
    format!(
        "`{record}` extends `{document}`, so it declares how a page renders: {}",
        entry_shape(record, entry)
    )
}

/// The refusal of an entry field that is not a layout of its record.
pub(crate) fn misshaped_entry_field(record: &str, entry: &str) -> String {
    format!(
        "`{entry}` renders a `{record}`: declare it {}",
        entry_shape(record, entry)
    )
}

/// Whether a field the host names as a neighbour may be declared `ty` on the record
/// `record` (D43): an optional of `record` itself or of a record it extends — what a
/// page of the same generator is, since every page of one generator has its type. One
/// predicate for the compiler, which asks it where the field is declared, and for the
/// link step, which asks it of every page record on a relink the declaration alone
/// changed.
pub(crate) fn neighbour_fits(decls: &dyn Decls, record: &str, ty: &Type) -> bool {
    let Type::Optional(inner) = ty else {
        return false;
    };
    match &**inner {
        Type::Named(page) => **page == *record || decls.extends(record, page),
        _ => false,
    }
}

/// The refusal of a neighbour field declared as anything but what [`neighbour_fits`]
/// takes.
pub(crate) fn misdeclared_neighbour(record: &str, field: &str) -> String {
    format!(
        "`{field}` holds a generated page's neighbour, a page of its own type: declare \
         it `{field}: {record}?`, or an optional of a type `{record}` extends"
    )
}

/// The refusal of a default on a neighbour field, which the loader fills once every
/// page of a generator is built: a default would be overwritten by the fill.
pub(crate) fn neighbour_default(field: &str) -> String {
    format!(
        "`{field}` is a page's neighbour, which the loader fills, so it takes no default"
    )
}

/// A table with nothing in it — what the rules are tested against.
impl Decls for () {
    fn declaration(&self, _name: &str) -> Option<&Declaration> {
        None
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A table of the declarations given.
    pub(crate) struct Table(pub Vec<Declaration>);

    impl Decls for Table {
        fn declaration(&self, name: &str) -> Option<&Declaration> {
            self.0.iter().find(|decl| &*decl.name == name)
        }
    }

    /// A record with the given base and fields, at no particular span.
    pub(crate) fn record(
        name: &str,
        base: Option<&str>,
        fields: &[(&str, Type)],
    ) -> Declaration {
        Declaration {
            name: Rc::from(name),
            name_span: Span::new(0, 0),
            shape: Shape::Record {
                base: base.map(|base| (Rc::from(base), Span::new(0, 0))),
                fields: fields
                    .iter()
                    .map(|(name, ty)| FieldDecl {
                        name: Rc::from(*name),
                        ty: ty.clone(),
                        span: Span::new(0, 0),
                    })
                    .collect(),
            },
        }
    }

    /// An enum of the given variants, at no particular span.
    pub(crate) fn enumeration(name: &str, variants: &[&str]) -> Declaration {
        Declaration {
            name: Rc::from(name),
            name_span: Span::new(0, 0),
            shape: Shape::Enum {
                variants: variants
                    .iter()
                    .map(|v| (Rc::from(*v), Span::new(0, 0)))
                    .collect(),
            },
        }
    }

    fn table() -> Table {
        Table(vec![
            record("Page", None, &[("url", Type::Url)]),
            record("Article", Some("Page"), &[("title", Type::String)]),
            record("LinkPost", Some("Article"), &[("link", Type::Url)]),
            enumeration("Placement", &["hero", "left"]),
        ])
    }

    #[test]
    fn fits_a_neighbour_typed_as_an_optional_of_its_record_or_an_ancestor() {
        let table = table();
        let optional = |name: &str| Type::optional(Type::Named(Rc::from(name)));
        assert!(neighbour_fits(&table, "Article", &optional("Article")));
        assert!(neighbour_fits(&table, "LinkPost", &optional("Page")));
        // Not a subtype, an unrelated record, a type that is no optional, or anything
        // else at all.
        assert!(!neighbour_fits(&table, "Article", &optional("LinkPost")));
        assert!(!neighbour_fits(&table, "Article", &optional("Placement")));
        assert!(!neighbour_fits(
            &table,
            "Article",
            &Type::Named(Rc::from("Article"))
        ));
        assert!(!neighbour_fits(
            &table,
            "Article",
            &Type::optional(Type::String)
        ));
        assert!(!neighbour_fits(&table, "Article", &Type::Any));
    }

    #[test]
    fn walks_the_chain_nearest_first() {
        let table = table();
        let names: Vec<&str> = table.chain("LinkPost").iter().map(|d| &*d.name).collect();
        assert_eq!(names, ["LinkPost", "Article", "Page"]);
        assert!(table.chain("Nope").is_empty());
    }

    #[test]
    fn extends_through_every_ancestor_but_never_itself() {
        let table = table();
        assert!(table.extends("LinkPost", "Article"));
        assert!(table.extends("LinkPost", "Page"));
        assert!(!table.extends("Article", "LinkPost"));
        assert!(!table.extends("Article", "Article"));
        assert!(!table.extends("Placement", "Page"));
    }

    #[test]
    fn reads_a_field_from_the_nearest_declaration_that_has_it() {
        let table = table();
        assert_eq!(table.field("LinkPost", "link"), Some(Type::Url));
        assert_eq!(table.field("LinkPost", "url"), Some(Type::Url));
        assert_eq!(table.field("LinkPost", "nope"), None);
        assert_eq!(table.field("Placement", "hero"), None);
        let (decl, field) = table.field_in("LinkPost", "url").unwrap();
        assert_eq!((&*decl.name, &*field.name), ("Page", "url"));
    }

    #[test]
    fn names_the_field_a_misspelling_was_close_to() {
        assert_eq!(
            no_field("LinkPost", "titel", &table()),
            "`LinkPost` has no field `titel`; did you mean `title`?"
        );
    }

    /// Own fields reach [`nearest`] before inherited ones and it breaks a tie towards
    /// the first, so `title` wins over an ancestor's equally close `titel`. Two facts
    /// that only compose here, which is why neither of their own tests catches this.
    #[test]
    fn breaks_a_tie_towards_the_records_own_field() {
        let table = Table(vec![
            record("Base", None, &[("titel", Type::String)]),
            record("Sub", Some("Base"), &[("title", Type::String)]),
        ]);
        assert_eq!(
            no_field("Sub", "titl", &table),
            "`Sub` has no field `titl`; did you mean `title`?"
        );
    }

    #[test]
    fn suggests_an_inherited_field_as_readily_as_an_own_one() {
        assert_eq!(
            no_field("LinkPost", "urls", &table()),
            "`LinkPost` has no field `urls`; did you mean `url`?"
        );
    }

    /// The other half of the rule: a key close to nothing declared is refused with no
    /// suggestion at all, rather than being answered with whatever field sits nearest.
    #[test]
    fn names_no_field_when_a_key_is_close_to_none_of_them() {
        assert_eq!(
            no_field("LinkPost", "photographer", &table()),
            "`LinkPost` has no field `photographer`"
        );
    }

    /// A record the table has no entry for has no fields to offer, which is a refusal
    /// with nothing to add rather than a walk off the end of the chain.
    #[test]
    fn names_no_field_of_a_record_the_table_does_not_have() {
        assert_eq!(
            no_field("Nope", "title", &table()),
            "`Nope` has no field `title`"
        );
    }

    #[test]
    fn lists_every_field_of_a_record_once_own_fields_first() {
        let table = Table(vec![
            record("Page", None, &[("url", Type::Url), ("title", Type::String)]),
            record(
                "Article",
                Some("Page"),
                &[("title", Type::String), ("date", Type::Datetime)],
            ),
            enumeration("Placement", &["hero"]),
        ]);
        let names: Vec<String> = table
            .fields("Article")
            .iter()
            .map(|(name, ty)| format!("{name}: {ty}"))
            .collect();
        assert_eq!(names, ["title: string", "date: datetime", "url: url"]);
        assert!(table.fields("Placement").is_empty());
        assert!(table.fields("Nope").is_empty());
    }

    #[test]
    fn stops_a_walk_at_a_missing_base_or_a_cycle() {
        let table = Table(vec![
            record("Orphan", Some("Gone"), &[]),
            record("A", Some("B"), &[("a", Type::Int)]),
            record("B", Some("A"), &[("b", Type::Int)]),
        ]);
        assert_eq!(table.chain("Orphan").len(), 1);
        assert!(!table.extends("Orphan", "Gone"));
        assert_eq!(table.chain("A").len(), 2);
        assert_eq!(table.field("A", "b"), Some(Type::Int));
        assert!(table.extends("A", "B") && table.extends("B", "A"));
    }

    #[test]
    fn tells_records_from_enums_and_lists_variants() {
        let table = table();
        assert_eq!(table.kind("Article"), Some(DeclKind::Record));
        assert_eq!(table.kind("Placement"), Some(DeclKind::Enum));
        assert_eq!(table.kind("Nope"), None);
        let variants: Vec<&str> = table
            .variants("Placement")
            .unwrap()
            .iter()
            .map(|(v, _)| &**v)
            .collect();
        assert_eq!(variants, ["hero", "left"]);
        assert_eq!(table.variants("Article"), None);
        assert_eq!(().kind("Article"), None);
    }

    #[test]
    fn writes_a_fact_the_way_it_would_be_declared() {
        let table = table();
        assert_eq!(
            table.declaration("LinkPost").unwrap().fact().text(),
            "LinkPost extends Article {link: url}"
        );
        assert_eq!(
            table.declaration("Page").unwrap().fact().text(),
            "Page {url: url}"
        );
        assert_eq!(
            table.declaration("Placement").unwrap().fact().text(),
            "Placement = hero | left"
        );
    }

    #[test]
    fn strips_spans_into_a_fact() {
        let fact = table().declaration("LinkPost").unwrap().fact();
        assert_eq!(
            fact,
            TypeFact {
                name: Rc::from("LinkPost"),
                shape: ShapeFact::Record {
                    base: Some(Rc::from("Article")),
                    fields: vec![(Rc::from("link"), Type::Url)],
                },
            }
        );
        assert_eq!(
            table().declaration("Placement").unwrap().fact().shape,
            ShapeFact::Enum {
                variants: vec![Rc::from("hero"), Rc::from("left")]
            }
        );
    }
}

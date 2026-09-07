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

use std::rc::Rc;

use lemmon_syntax::Span;

use crate::types::Type;

/// Whether a declared name is a record or an enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
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
    pub const fn kind(&self) -> Kind {
        match self.shape {
            Shape::Record { .. } => Kind::Record,
            Shape::Enum { .. } => Kind::Enum,
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
    fn kind(&self, name: &str) -> Option<Kind> {
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

    /// The type of the field `name` of the record `record`: its own, or the nearest
    /// ancestor's.
    fn field(&self, record: &str, name: &str) -> Option<Type> {
        self.chain(record)
            .iter()
            .find_map(|decl| match &decl.shape {
                Shape::Record { fields, .. } => fields
                    .iter()
                    .find(|field| &*field.name == name)
                    .map(|field| field.ty.clone()),
                Shape::Enum { .. } => None,
            })
    }

    /// The variants of the enum `name`, if it is one.
    fn variants(&self, name: &str) -> Option<&[(Rc<str>, Span)]> {
        match &self.declaration(name)?.shape {
            Shape::Enum { variants } => Some(variants),
            Shape::Record { .. } => None,
        }
    }
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
        assert_eq!(table.kind("Article"), Some(Kind::Record));
        assert_eq!(table.kind("Placement"), Some(Kind::Enum));
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

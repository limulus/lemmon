//! Types as the checker reasons about them (`docs/language.md` §Types, §Checking).
//!
//! A [`Type`] is plain data: it lives on every binding the compiler makes, in the
//! signatures units export and consume as compile-time facts (D25), and in messages. The
//! relations are [`assignable`] — whether a value of one type may stand where another is
//! expected — and [`join`], the one type two branches or two elements share.
//!
//! `any` is the spelled unknown. It is assignable to and from everything, and every
//! operation on it types as `any` again: the VM checks those at run time, as it always
//! has. Nothing else is implicit — an `int` is not a `float`, and a `T` is not a `T?`
//! until the checker sees it is one.

use std::fmt;
use std::rc::Rc;

/// A type.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    /// The spelled unknown: assignable both ways, checked at run time.
    Any,
    /// The type of the `null` literal — assignable only to an optional.
    Null,
    String,
    Int,
    Float,
    Bool,
    Datetime,
    Duration,
    Url,
    Html,
    Asset,
    /// `[T]`
    List(Rc<Type>),
    /// `{string: V}` — every map is keyed by strings.
    Map(Rc<Type>),
    /// `T?`
    Optional(Rc<Type>),
    /// `(A, B): R`
    Fn(Rc<[Type]>, Rc<Type>),
}

impl Type {
    /// `[element]`
    #[must_use]
    pub fn list(element: Type) -> Self {
        Self::List(Rc::new(element))
    }

    /// `{string: value}`
    #[must_use]
    pub fn map(value: Type) -> Self {
        Self::Map(Rc::new(value))
    }

    /// `inner?` — `any` and `null` already admit `null`, and an optional made optional
    /// again is the same optional.
    #[must_use]
    pub fn optional(inner: Type) -> Self {
        match inner {
            Self::Any | Self::Null | Self::Optional(_) => inner,
            other => Self::Optional(Rc::new(other)),
        }
    }

    /// `(params): ret`
    #[must_use]
    pub fn function(params: Vec<Type>, ret: Type) -> Self {
        Self::Fn(Rc::from(params), Rc::new(ret))
    }

    /// The type without its `?`, if it had one.
    #[must_use]
    pub fn strip_optional(&self) -> &Self {
        match self {
            Self::Optional(inner) => inner,
            other => other,
        }
    }

    /// Whether this is `int` or `float`.
    #[must_use]
    pub const fn is_number(&self) -> bool {
        matches!(self, Self::Int | Self::Float)
    }
}

/// Whether a value of type `from` may stand where `to` is expected.
#[must_use]
pub fn assignable(from: &Type, to: &Type) -> bool {
    match (from, to) {
        (Type::Any, _) | (_, Type::Any) => true,
        (Type::Null, Type::Null | Type::Optional(_)) => true,
        // `T` to `T?`, and `T?` to `U?` when `T` goes to `U`.
        (_, Type::Optional(to)) => assignable(from.strip_optional(), to),
        (Type::Optional(_) | Type::Null, _) => false,
        // Values are immutable, so a list of the narrower type is a list of the wider.
        (Type::List(from), Type::List(to)) | (Type::Map(from), Type::Map(to)) => {
            assignable(from, to)
        }
        // Parameters are contravariant: a function expected to take a `T?` must accept
        // every `T?` it is given, so one taking a bare `T` will not do.
        (Type::Fn(from_params, from_ret), Type::Fn(to_params, to_ret)) => {
            from_params.len() == to_params.len()
                && from_params
                    .iter()
                    .zip(to_params.iter())
                    .all(|(from, to)| assignable(to, from))
                && assignable(from_ret, to_ret)
        }
        _ => from == to,
    }
}

/// The one type both `a` and `b` are assignable to, if there is one: what a list of
/// both, or a conditional with one in each branch, has. `null` beside a `T` makes a
/// `T?`; `any` beside anything is `any`.
#[must_use]
pub fn join(a: &Type, b: &Type) -> Option<Type> {
    if a == b {
        return Some(a.clone());
    }
    Some(match (a, b) {
        (Type::Any, _) | (_, Type::Any) => Type::Any,
        (Type::Null, other) | (other, Type::Null) => Type::optional(other.clone()),
        _ if assignable(a, b) => b.clone(),
        _ if assignable(b, a) => a.clone(),
        _ => return None,
    })
}

/// Types print the way they are written in a template.
impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Any => f.write_str("any"),
            Self::Null => f.write_str("null"),
            Self::String => f.write_str("string"),
            Self::Int => f.write_str("int"),
            Self::Float => f.write_str("float"),
            Self::Bool => f.write_str("bool"),
            Self::Datetime => f.write_str("datetime"),
            Self::Duration => f.write_str("duration"),
            Self::Url => f.write_str("url"),
            Self::Html => f.write_str("html"),
            Self::Asset => f.write_str("asset"),
            Self::List(element) => write!(f, "[{element}]"),
            Self::Map(value) => write!(f, "{{string: {value}}}"),
            Self::Optional(inner) => write!(f, "{inner}?"),
            Self::Fn(params, ret) => {
                f.write_str("(")?;
                for (i, param) in params.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{param}")?;
                }
                write!(f, "): {ret}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opt(inner: Type) -> Type {
        Type::optional(inner)
    }

    #[test]
    fn prints_every_type_the_way_it_is_written() {
        let shown: Vec<String> = [
            Type::Any,
            Type::Null,
            Type::String,
            Type::Int,
            Type::Float,
            Type::Bool,
            Type::Datetime,
            Type::Duration,
            Type::Url,
            Type::Html,
            Type::Asset,
            Type::list(Type::Int),
            Type::map(Type::list(Type::Asset)),
            opt(Type::String),
            Type::function(vec![Type::Any, Type::Html], opt(Type::Html)),
            Type::function(vec![], Type::Int),
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        assert_eq!(
            shown,
            [
                "any",
                "null",
                "string",
                "int",
                "float",
                "bool",
                "datetime",
                "duration",
                "url",
                "html",
                "asset",
                "[int]",
                "{string: [asset]}",
                "string?",
                "(any, html): html?",
                "(): int",
            ]
        );
    }

    #[test]
    fn folds_an_optional_of_what_already_admits_null() {
        assert_eq!(opt(Type::Any), Type::Any);
        assert_eq!(opt(Type::Null), Type::Null);
        assert_eq!(opt(opt(Type::Int)), opt(Type::Int));
        assert_eq!(opt(Type::Int).strip_optional(), &Type::Int);
        assert_eq!(Type::Int.strip_optional(), &Type::Int);
    }

    #[test]
    fn assigns_any_both_ways_and_null_only_to_an_optional() {
        assert!(assignable(&Type::Any, &Type::Int));
        assert!(assignable(&Type::Int, &Type::Any));
        assert!(assignable(&Type::Null, &opt(Type::Int)));
        assert!(assignable(&Type::Null, &Type::Null));
        assert!(!assignable(&Type::Null, &Type::Int));
    }

    #[test]
    fn assigns_a_value_to_its_optional_but_never_the_reverse() {
        assert!(assignable(&Type::Int, &opt(Type::Int)));
        assert!(assignable(&opt(Type::Int), &opt(Type::Int)));
        assert!(!assignable(&opt(Type::Int), &Type::Int));
        assert!(!assignable(&Type::String, &opt(Type::Int)));
    }

    #[test]
    fn keeps_int_and_float_apart() {
        // The number model is a deferred question; the provisional reading is two types
        // that only arithmetic mixes.
        assert!(!assignable(&Type::Int, &Type::Float));
        assert!(!assignable(&Type::Float, &Type::Int));
        assert!(Type::Int.is_number());
        assert!(!Type::String.is_number());
    }

    #[test]
    fn assigns_lists_and_maps_covariantly() {
        assert!(assignable(
            &Type::list(Type::Int),
            &Type::list(opt(Type::Int))
        ));
        assert!(!assignable(
            &Type::list(opt(Type::Int)),
            &Type::list(Type::Int)
        ));
        assert!(assignable(&Type::map(Type::Int), &Type::map(Type::Any)));
        assert!(!assignable(&Type::map(Type::Int), &Type::map(Type::String)));
        assert!(!assignable(&Type::list(Type::Int), &Type::map(Type::Int)));
    }

    #[test]
    fn assigns_functions_contravariantly_in_parameters_and_covariantly_in_return() {
        let takes_opt = Type::function(vec![opt(Type::Int)], Type::Int);
        let takes_int = Type::function(vec![Type::Int], Type::Int);
        assert!(assignable(&takes_opt, &takes_int));
        assert!(!assignable(&takes_int, &takes_opt));
        let returns_int = Type::function(vec![], Type::Int);
        let returns_opt = Type::function(vec![], opt(Type::Int));
        assert!(assignable(&returns_int, &returns_opt));
        assert!(!assignable(&returns_opt, &returns_int));
        assert!(!assignable(
            &returns_int,
            &Type::function(vec![Type::Int], Type::Int)
        ));
    }

    #[test]
    fn joins_equal_types_null_and_any() {
        assert_eq!(join(&Type::Int, &Type::Int), Some(Type::Int));
        assert_eq!(join(&Type::Null, &Type::Int), Some(opt(Type::Int)));
        assert_eq!(join(&Type::Int, &Type::Null), Some(opt(Type::Int)));
        assert_eq!(join(&Type::Any, &Type::Int), Some(Type::Any));
        assert_eq!(join(&Type::Int, &Type::Any), Some(Type::Any));
    }

    #[test]
    fn joins_toward_whichever_side_admits_the_other() {
        assert_eq!(join(&Type::Int, &opt(Type::Int)), Some(opt(Type::Int)));
        assert_eq!(join(&opt(Type::Int), &Type::Int), Some(opt(Type::Int)));
        assert_eq!(join(&Type::Int, &Type::String), None);
        assert_eq!(join(&Type::Int, &Type::Float), None);
    }
}

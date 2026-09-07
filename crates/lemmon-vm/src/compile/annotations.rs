//! Types as written — annotations on parameters, bindings, and returns — resolved to
//! the [`Type`]s the checker reasons about.
//!
//! A name is one of the builtin types (`docs/language.md` §Types) or a declared one — a
//! record or an enum, project-flat, which resolves to [`Type::Named`] by name alone: the
//! index says whether it is declared, and [`compile_unit`](super::compile_unit) reports
//! the ones that are not from the uses recorded here, once every file's declarations
//! are in. A map's key type must be `string` — the parser accepts any type there, and
//! this is where the rule is applied. Every problem is reported at the type that has it,
//! and resolves to `any` so that one mistake costs one diagnostic.

use std::rc::Rc;

use lemmon_syntax::ast::{Spanned, TypeExpr};

use super::{Diagnostic, TokenSpan, required};
use crate::types::Type;
use crate::unit::ImportRef;

/// The types every template can name without declaring them.
const BUILTIN_TYPES: &[(&str, Type)] = &[
    ("any", Type::Any),
    ("string", Type::String),
    ("int", Type::Int),
    ("float", Type::Float),
    ("bool", Type::Bool),
    ("datetime", Type::Datetime),
    ("duration", Type::Duration),
    ("url", Type::Url),
    ("html", Type::Html),
    ("asset", Type::Asset),
];

/// What an author reaching for another language's spelling is told instead.
const HINTS: &[(&str, &str)] = &[
    ("map", "a map is written `{string: T}`"),
    ("list", "a list is written `[T]`"),
    ("function", "a function type is written `(A, B): R`"),
];

/// A name no declaration may take, and why.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Reserved {
    /// A builtin type's name.
    Builtin,
    /// A spelling the hints answer for, with the hint.
    Hint(&'static str),
}

/// Whether `name` is one no declaration may take, and why.
pub(super) fn reserved(name: &str) -> Option<Reserved> {
    if BUILTIN_TYPES.iter().any(|(n, _)| *n == name) {
        return Some(Reserved::Builtin);
    }
    HINTS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, hint)| Reserved::Hint(hint))
}

/// Resolves `ty` for the file at `path`, reporting into `diagnostics` and recording
/// every declared name it uses in `uses`.
pub(super) fn resolve(
    path: &str,
    ty: &TypeExpr,
    diagnostics: &mut Vec<Diagnostic>,
    uses: &mut Vec<ImportRef>,
) -> Type {
    match ty {
        TypeExpr::Named(named) => {
            let name = required(named.name());
            let text = name.text();
            if let Some((_, ty)) = BUILTIN_TYPES.iter().find(|(n, _)| *n == text) {
                return ty.clone();
            }
            if let Some((_, hint)) = HINTS.iter().find(|(n, _)| *n == text) {
                diagnostics.push(Diagnostic::error(
                    path,
                    named.span(),
                    format!("unknown type `{text}`: {hint}"),
                ));
                return Type::Any;
            }
            uses.push(ImportRef {
                name: Rc::from(text),
                span: name.span(),
            });
            Type::Named(Rc::from(text))
        }
        TypeExpr::Optional(optional) => Type::optional(resolve(
            path,
            &required(optional.inner()),
            diagnostics,
            uses,
        )),
        TypeExpr::List(list) => {
            Type::list(resolve(path, &required(list.element()), diagnostics, uses))
        }
        TypeExpr::Map(map) => {
            let key = required(map.key());
            // A hinted key type was reported as such and resolved to `any`; only a key
            // that resolved to something else is the wrong key — and a refused key's
            // names are not uses, or an undeclared one would be reported twice.
            let mut key_uses = Vec::new();
            match resolve(path, &key, diagnostics, &mut key_uses) {
                Type::String | Type::Any => uses.extend(key_uses),
                _ => diagnostics.push(Diagnostic::error(
                    path,
                    key.span(),
                    "map keys are always `string`",
                )),
            }
            Type::map(resolve(path, &required(map.value()), diagnostics, uses))
        }
        TypeExpr::Fn(function) => {
            let params = function
                .params()
                .map(|param| resolve(path, &param, diagnostics, uses))
                .collect();
            let ret = resolve(path, &required(function.ret()), diagnostics, uses);
            Type::function(params, ret)
        }
    }
}

#[cfg(test)]
mod tests {
    use lemmon_syntax::ast::Item;

    use super::*;

    /// Resolves `src` written as a parameter's annotation: the type, what was reported,
    /// and the declared names it used.
    fn resolved(src: &str) -> (Type, Vec<String>, Vec<String>) {
        let text = format!("<lem-fn f(a: {src}): int = 1 />");
        let parse = lemmon_syntax::parse(&text);
        assert!(parse.diagnostics.is_empty(), "{src} did not parse clean");
        let Some(Item::FnVoid(f)) = parse.root().items().next() else {
            panic!("expected a fn");
        };
        let ty = f
            .params()
            .and_then(|p| p.params().next())
            .and_then(|p| p.type_annotation())
            .and_then(|a| a.ty())
            .expect("an annotation");
        let mut diagnostics = Vec::new();
        let mut uses = Vec::new();
        let resolved = resolve("t.lem", &ty, &mut diagnostics, &mut uses);
        (
            resolved,
            diagnostics.iter().map(ToString::to_string).collect(),
            uses.iter()
                .map(|u| format!("{}@{}..{}", u.name, u.span.start, u.span.end))
                .collect(),
        )
    }

    #[test]
    fn resolves_every_builtin_type_name() {
        for (name, ty) in BUILTIN_TYPES {
            assert_eq!(
                resolved(name),
                (ty.clone(), Vec::new(), Vec::new()),
                "{name}"
            );
            assert_eq!(reserved(name), Some(Reserved::Builtin), "{name}");
        }
    }

    #[test]
    fn resolves_composite_types_as_written() {
        let (ty, diagnostics, uses) = resolved("{string: [(int, html?): asset]?}");
        assert_eq!(ty.to_string(), "{string: [(int, html?): asset]?}");
        assert!(diagnostics.is_empty());
        assert!(uses.is_empty());
    }

    #[test]
    fn resolves_a_declared_name_by_name_and_records_the_use() {
        // Whether `Article` is declared is the index's question, asked once every
        // file's declarations are in; here the name is only recorded.
        let (ty, diagnostics, uses) = resolved("[Article]");
        assert_eq!(ty, Type::list(Type::Named(Rc::from("Article"))));
        assert!(diagnostics.is_empty());
        assert_eq!(uses, ["Article@14..21"]);
        let (ty, _, uses) = resolved("{string: Photo?}");
        assert_eq!(ty.to_string(), "{string: Photo?}");
        assert_eq!(uses, ["Photo@22..27"]);
    }

    #[test]
    fn hints_at_the_lemmon_spelling_for_map_list_and_function() {
        assert_eq!(
            resolved("map").1,
            ["t.lem:13..16: unknown type `map`: a map is written `{string: T}`"]
        );
        assert_eq!(
            resolved("list").1,
            ["t.lem:13..17: unknown type `list`: a list is written `[T]`"]
        );
        assert_eq!(
            resolved("function").1,
            [
                "t.lem:13..21: unknown type `function`: a function type is written `(A, B): R`"
            ]
        );
        assert_eq!(
            reserved("map"),
            Some(Reserved::Hint("a map is written `{string: T}`"))
        );
        assert_eq!(reserved("Article"), None);
    }

    #[test]
    fn requires_string_map_keys_and_still_resolves_the_value() {
        let (ty, diagnostics, _) = resolved("{int: bool}");
        assert_eq!(ty, Type::map(Type::Bool));
        assert_eq!(diagnostics, ["t.lem:14..17: map keys are always `string`"]);
        // A hinted key type is one diagnostic, not two; a declared name as a key is
        // the wrong key whatever it declares.
        let (_, diagnostics, _) = resolved("{map: bool}");
        assert_eq!(
            diagnostics,
            ["t.lem:14..17: unknown type `map`: a map is written `{string: T}`"]
        );
        let (_, diagnostics, uses) = resolved("{Key: bool}");
        assert_eq!(diagnostics, ["t.lem:14..17: map keys are always `string`"]);
        assert!(uses.is_empty());
        let (_, _, uses) = resolved("{string: Key}");
        assert_eq!(uses, ["Key@22..25"]);
    }
}

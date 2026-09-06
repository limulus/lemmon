//! Types as written — annotations on parameters, bindings, and returns — resolved to
//! the [`Type`]s the checker reasons about.
//!
//! A name is one of the builtin types (`docs/language.md` §Types); records and enums
//! join them with ∆cea. A map's key type must be `string` — the parser accepts any type
//! there, and this is where the rule is applied. Every problem is reported at the type
//! that has it, and resolves to `any` so that one mistake costs one diagnostic.

use lemmon_syntax::ast::{Spanned, TypeExpr};

use super::{Diagnostic, required};
use crate::types::Type;

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

/// Resolves `ty` for the file at `path`, reporting into `diagnostics`.
pub(super) fn resolve(
    path: &str,
    ty: &TypeExpr,
    diagnostics: &mut Vec<Diagnostic>,
) -> Type {
    match ty {
        TypeExpr::Named(named) => {
            let name = required(named.name());
            let text = name.text();
            if let Some((_, ty)) = BUILTIN_TYPES.iter().find(|(n, _)| *n == text) {
                return ty.clone();
            }
            let hint = HINTS
                .iter()
                .find(|(n, _)| *n == text)
                .map_or(String::new(), |(_, hint)| format!(": {hint}"));
            diagnostics.push(Diagnostic::error(
                path,
                named.span(),
                format!("unknown type `{text}`{hint}"),
            ));
            Type::Any
        }
        TypeExpr::Optional(optional) => {
            Type::optional(resolve(path, &required(optional.inner()), diagnostics))
        }
        TypeExpr::List(list) => {
            Type::list(resolve(path, &required(list.element()), diagnostics))
        }
        TypeExpr::Map(map) => {
            let key = required(map.key());
            // An unknown key type was reported as such and resolved to `any`; only a
            // key that resolved to something else is the wrong key.
            if !matches!(resolve(path, &key, diagnostics), Type::String | Type::Any) {
                diagnostics.push(Diagnostic::error(
                    path,
                    key.span(),
                    "map keys are always `string`",
                ));
            }
            Type::map(resolve(path, &required(map.value()), diagnostics))
        }
        TypeExpr::Fn(function) => {
            let params = function
                .params()
                .map(|param| resolve(path, &param, diagnostics))
                .collect();
            let ret = resolve(path, &required(function.ret()), diagnostics);
            Type::function(params, ret)
        }
    }
}

#[cfg(test)]
mod tests {
    use lemmon_syntax::ast::Item;

    use super::*;

    /// Resolves `src` written as a parameter's annotation.
    fn resolved(src: &str) -> (Type, Vec<String>) {
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
        let resolved = resolve("t.lem", &ty, &mut diagnostics);
        (
            resolved,
            diagnostics.iter().map(ToString::to_string).collect(),
        )
    }

    #[test]
    fn resolves_every_builtin_type_name() {
        for (name, ty) in BUILTIN_TYPES {
            assert_eq!(resolved(name), (ty.clone(), Vec::new()), "{name}");
        }
    }

    #[test]
    fn resolves_composite_types_as_written() {
        let (ty, diagnostics) = resolved("{string: [(int, html?): asset]?}");
        assert_eq!(ty.to_string(), "{string: [(int, html?): asset]?}");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn reports_an_unknown_name_at_the_name_and_resolves_it_to_any() {
        let (ty, diagnostics) = resolved("[Article]");
        assert_eq!(ty, Type::list(Type::Any));
        assert_eq!(diagnostics, ["t.lem:14..21: unknown type `Article`"]);
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
    }

    #[test]
    fn requires_string_map_keys_and_still_resolves_the_value() {
        let (ty, diagnostics) = resolved("{int: bool}");
        assert_eq!(ty, Type::map(Type::Bool));
        assert_eq!(diagnostics, ["t.lem:14..17: map keys are always `string`"]);
        // An unknown key type is one diagnostic, not two.
        let (_, diagnostics) = resolved("{Key: bool}");
        assert_eq!(diagnostics, ["t.lem:14..17: unknown type `Key`"]);
    }
}

//! Views over types as written: annotations on parameters, bindings, returns, and
//! fields.

use super::{AstNode, ast_enum, ast_node, child, children, token};
use crate::{SyntaxKind, SyntaxToken};

ast_node!(
    /// `: Type`
    TypeAnnotation,
    TypeAnnotation
);

impl TypeAnnotation {
    /// The type written after the colon.
    #[must_use]
    pub fn ty(&self) -> Option<TypeExpr> {
        child(&self.syntax)
    }
}

ast_enum!(
    /// Any type as written.
    TypeExpr {
        /// A bare name: `int`, `Article`.
        Named(NamedType),
        /// `T?`
        Optional(OptionalType),
        /// `[T]`
        List(ListType),
        /// `{K: V}`
        Map(MapType),
        /// `(A, B): R`
        Fn(FnType),
    }
);

ast_node!(
    /// A bare type name.
    NamedType,
    NamedType
);

impl NamedType {
    /// The name's token.
    #[must_use]
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.syntax, SyntaxKind::Ident)
    }
}

ast_node!(
    /// `T?`
    OptionalType,
    OptionalType
);

impl OptionalType {
    /// The type made optional.
    #[must_use]
    pub fn inner(&self) -> Option<TypeExpr> {
        child(&self.syntax)
    }
}

ast_node!(
    /// `[T]`
    ListType,
    ListType
);

impl ListType {
    /// The element type.
    #[must_use]
    pub fn element(&self) -> Option<TypeExpr> {
        child(&self.syntax)
    }
}

ast_node!(
    /// `{K: V}`
    MapType,
    MapType
);

impl MapType {
    /// The key type.
    #[must_use]
    pub fn key(&self) -> Option<TypeExpr> {
        children(&self.syntax).next()
    }

    /// The value type.
    #[must_use]
    pub fn value(&self) -> Option<TypeExpr> {
        children(&self.syntax).nth(1)
    }
}

ast_node!(
    /// `(A, B): R`
    FnType,
    FnType
);

impl FnType {
    /// The parameter types, in order: every type before the `)`.
    pub fn params(&self) -> impl Iterator<Item = TypeExpr> {
        self.syntax
            .children_with_tokens()
            .take_while(|e| e.kind() != SyntaxKind::RParen)
            .filter_map(|e| e.into_node())
            .filter_map(TypeExpr::cast)
    }

    /// The return type: the type after the `)`.
    #[must_use]
    pub fn ret(&self) -> Option<TypeExpr> {
        self.syntax
            .children_with_tokens()
            .skip_while(|e| e.kind() != SyntaxKind::RParen)
            .filter_map(|e| e.into_node())
            .find_map(TypeExpr::cast)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{AstNode, Item};
    use crate::parse;

    /// The first parameter's type in `<lem-fn f(a: TYPE)></lem-fn>`.
    fn ty(src: &str) -> TypeExpr {
        let src = format!("<lem-fn f(a: {src})></lem-fn>");
        let Some(Item::FnBlock(f)) = parse(&src).root().items().next() else {
            panic!("expected a fn");
        };
        let param = f
            .params()
            .expect("params")
            .params()
            .next()
            .expect("a param");
        param
            .type_annotation()
            .expect("an annotation")
            .ty()
            .expect("a type")
    }

    fn text<N: AstNode<Language = crate::Lemmon>>(node: Option<N>) -> String {
        node.expect("a node").syntax().text().to_string()
    }

    #[test]
    fn views_a_named_type() {
        let TypeExpr::Named(named) = ty("Article") else {
            panic!("expected a named type");
        };
        assert_eq!(named.name().expect("a name").text(), "Article");
    }

    #[test]
    fn binds_the_optional_marker_to_the_type_just_written() {
        let TypeExpr::Optional(opt) = ty("[int]?") else {
            panic!("expected an optional");
        };
        assert!(matches!(opt.inner(), Some(TypeExpr::List(_))));
        let TypeExpr::List(list) = ty("[int?]") else {
            panic!("expected a list");
        };
        assert!(matches!(list.element(), Some(TypeExpr::Optional(_))));
    }

    #[test]
    fn views_the_key_and_value_of_a_map_type() {
        let TypeExpr::Map(map) = ty("{string: [asset]}") else {
            panic!("expected a map");
        };
        assert_eq!(text(map.key()), "string");
        assert_eq!(text(map.value()), "[asset]");
    }

    #[test]
    fn splits_a_function_type_at_its_close_paren() {
        let TypeExpr::Fn(f) = ty("(Page, html): html?") else {
            panic!("expected a function type");
        };
        let params: Vec<String> =
            f.params().map(|t| t.syntax().text().to_string()).collect();
        assert_eq!(params, vec!["Page", "html"]);
        assert_eq!(text(f.ret()), "html?");
        let TypeExpr::Fn(f) = ty("(): int") else {
            panic!("expected a function type");
        };
        assert_eq!(f.params().count(), 0);
        assert_eq!(text(f.ret()), "int");
    }
}

//! Typed views over the syntax tree.
//!
//! The parser's tree is a lossless [`SyntaxNode`] forest: every byte, every token, every
//! recovery `Error` node, in source order. The checker and the code generator want
//! questions answered about it — "what is this `lem-if`'s condition?", "which arguments
//! are named?" — not a child list to walk. Each type here is a zero-cost view over one
//! node kind that answers those questions through accessors, so that the stages above
//! never match on [`SyntaxKind`] or depend on the order the parser emits children.
//!
//! A view is never a copy. [`AstNode::cast`] checks the kind and wraps the node; every
//! accessor reads the tree on demand and returns `Option` (or an iterator) because the
//! parser recovers from anything — a cut head, a missing name — and the view must not
//! panic on a tree the parser was willing to build. A parse with diagnostics never
//! reaches the checker, so on well-formed input the `None`s are unreachable; on
//! malformed input they are the honest answer.

mod expr;
mod tags;

pub use expr::{
    ArgList, BinaryExpr, BinaryOp, CallExpr, Expr, IndexExpr, Lambda, LambdaParams,
    ListLit, Literal, LiteralKind, MapEntry, MapLit, MemberExpr, NameRef, NamedArg,
    ParenExpr, TernaryExpr, TypeTest, UnaryExpr, UnaryOp,
};
pub use rowan::ast::AstNode;
pub use tags::{
    AccessTag, Block, FnBlock, FnVoid, ForBinding, ForTag, IfArm, IfTag, Item, LetBlock,
    LetVoid, Param, Params, Root, TextRun, TypeAnnotation, UnknownTag, UnsupportedTag,
};

use rowan::ast::support::{child, children, token};

use crate::{Lemmon, Span, SyntaxKind, SyntaxNode, SyntaxToken};

/// The bytes a view covers. Every view has a span, so every diagnostic raised about it
/// can point at source (invariant 8).
pub trait Spanned {
    /// The bytes this node covers.
    fn span(&self) -> Span;
}

impl<N: AstNode<Language = Lemmon>> Spanned for N {
    fn span(&self) -> Span {
        self.syntax().text_range().into()
    }
}

/// Defines a view struct over exactly one [`SyntaxKind`].
macro_rules! ast_node {
    ($(#[$attr:meta])* $name:ident, $kind:ident) => {
        $(#[$attr])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        pub struct $name {
            syntax: $crate::SyntaxNode,
        }

        impl $crate::ast::AstNode for $name {
            type Language = $crate::Lemmon;

            fn can_cast(kind: $crate::SyntaxKind) -> bool {
                kind == $crate::SyntaxKind::$kind
            }

            fn cast(syntax: $crate::SyntaxNode) -> Option<Self> {
                Self::can_cast(syntax.kind()).then_some(Self { syntax })
            }

            fn syntax(&self) -> &$crate::SyntaxNode {
                &self.syntax
            }
        }
    };
}

/// Defines a view enum over a closed set of node views, dispatching by kind.
macro_rules! ast_enum {
    ($(#[$attr:meta])* $name:ident { $($(#[$vattr:meta])* $variant:ident($ty:ident)),* $(,)? }) => {
        $(#[$attr])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        pub enum $name {
            $($(#[$vattr])* $variant($ty),)*
        }

        impl $crate::ast::AstNode for $name {
            type Language = $crate::Lemmon;

            fn can_cast(kind: $crate::SyntaxKind) -> bool {
                false $(|| $ty::can_cast(kind))*
            }

            fn cast(syntax: $crate::SyntaxNode) -> Option<Self> {
                let kind = syntax.kind();
                $(if $ty::can_cast(kind) {
                    return $ty::cast(syntax).map(Self::$variant);
                })*
                None
            }

            fn syntax(&self) -> &$crate::SyntaxNode {
                match self {
                    $(Self::$variant(node) => node.syntax(),)*
                }
            }
        }
    };
}

pub(crate) use {ast_enum, ast_node};

/// The child tokens of `parent`, in source order.
fn child_tokens(parent: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> {
    parent
        .children_with_tokens()
        .filter_map(|it| it.into_token())
}

/// Every child token of `kind`, in source order.
fn tokens(parent: &SyntaxNode, kind: SyntaxKind) -> impl Iterator<Item = SyntaxToken> {
    child_tokens(parent).filter(move |t| t.kind() == kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    #[test]
    fn reports_the_span_of_the_node_it_views() {
        let root = parse("ab<lem.x>cd").root();
        let access = root.items().nth(1).expect("the access tag");
        assert_eq!(access.span(), Span::new(2, 9));
    }

    #[test]
    fn refuses_to_cast_a_node_of_another_kind() {
        let root = parse("<lem.x>").root();
        let node = root.syntax().first_child().expect("a child");
        assert!(IfTag::cast(node.clone()).is_none());
        assert!(AccessTag::cast(node).is_some());
    }
}

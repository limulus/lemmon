//! Views over expressions: the heads of tags.

use super::{ast_enum, ast_node, child, child_tokens, children, token, tokens};
use crate::{SyntaxKind, SyntaxToken};

ast_enum!(
    /// Any expression.
    Expr {
        /// `a op b`
        Binary(BinaryExpr),
        /// `-a`, `!a`
        Unary(UnaryExpr),
        /// `c ? a : b`
        Ternary(TernaryExpr),
        /// `f(args)`
        Call(CallExpr),
        /// `a.b`, `a?.b`
        Member(MemberExpr),
        /// `a[i]`
        Index(IndexExpr),
        /// `|a, b| body`
        Lambda(Lambda),
        /// `[a, b]`
        List(ListLit),
        /// `{k: v}`
        Map(MapLit),
        /// `"s"`, `1`, `1.5`, `true`, `false`, `null`
        Literal(Literal),
        /// A bare name.
        Name(NameRef),
        /// `.hero`
        Variant(VariantRef),
        /// `(a)`
        Paren(ParenExpr),
        /// `a is T`
        TypeTest(TypeTest),
    }
);

/// A binary operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[allow(missing_docs, reason = "each variant names its operator")]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    NotEq,
    Lt,
    Lte,
    Gt,
    Gte,
    And,
    Or,
    Coalesce,
    In,
}

impl BinaryOp {
    /// The operator a token of `kind` spells, if any. `is` is not one: a type test
    /// takes a type name, not an expression, and has its own view.
    #[must_use]
    pub const fn from_kind(kind: SyntaxKind) -> Option<Self> {
        Some(match kind {
            SyntaxKind::Plus => Self::Add,
            SyntaxKind::Minus => Self::Sub,
            SyntaxKind::Star => Self::Mul,
            SyntaxKind::Slash => Self::Div,
            SyntaxKind::Percent => Self::Rem,
            SyntaxKind::EqEq => Self::Eq,
            SyntaxKind::BangEq => Self::NotEq,
            SyntaxKind::Lt => Self::Lt,
            SyntaxKind::Lte => Self::Lte,
            SyntaxKind::Gt => Self::Gt,
            SyntaxKind::Gte => Self::Gte,
            SyntaxKind::AmpAmp => Self::And,
            SyntaxKind::PipePipe => Self::Or,
            SyntaxKind::QuestionQuestion => Self::Coalesce,
            SyntaxKind::In => Self::In,
            _ => return None,
        })
    }
}

ast_node!(
    /// `lhs op rhs`
    BinaryExpr,
    BinaryExpr
);

impl BinaryExpr {
    /// The left operand.
    #[must_use]
    pub fn lhs(&self) -> Option<Expr> {
        children(&self.syntax).next()
    }

    /// The right operand.
    #[must_use]
    pub fn rhs(&self) -> Option<Expr> {
        children(&self.syntax).nth(1)
    }

    /// The operator and its token.
    #[must_use]
    pub fn op(&self) -> Option<(BinaryOp, SyntaxToken)> {
        child_tokens(&self.syntax)
            .find_map(|t| BinaryOp::from_kind(t.kind()).map(|op| (op, t)))
    }
}

/// A unary operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    /// `-a`
    Neg,
    /// `!a`
    Not,
}

ast_node!(
    /// `-a`, `!a`
    UnaryExpr,
    UnaryExpr
);

impl UnaryExpr {
    /// The operator and its token.
    #[must_use]
    pub fn op(&self) -> Option<(UnaryOp, SyntaxToken)> {
        child_tokens(&self.syntax).find_map(|t| {
            let op = match t.kind() {
                SyntaxKind::Minus => UnaryOp::Neg,
                SyntaxKind::Bang => UnaryOp::Not,
                _ => return None,
            };
            Some((op, t))
        })
    }

    /// The operand.
    #[must_use]
    pub fn operand(&self) -> Option<Expr> {
        child(&self.syntax)
    }
}

ast_node!(
    /// `condition ? then : otherwise`
    TernaryExpr,
    TernaryExpr
);

impl TernaryExpr {
    /// The condition.
    #[must_use]
    pub fn condition(&self) -> Option<Expr> {
        children(&self.syntax).next()
    }

    /// The value when the condition holds.
    #[must_use]
    pub fn then(&self) -> Option<Expr> {
        children(&self.syntax).nth(1)
    }

    /// The value when it does not.
    #[must_use]
    pub fn otherwise(&self) -> Option<Expr> {
        children(&self.syntax).nth(2)
    }
}

ast_node!(
    /// `callee(args)`
    CallExpr,
    CallExpr
);

impl CallExpr {
    /// The expression being called.
    #[must_use]
    pub fn callee(&self) -> Option<Expr> {
        child(&self.syntax)
    }

    /// The argument list.
    #[must_use]
    pub fn args(&self) -> Option<ArgList> {
        child(&self.syntax)
    }
}

ast_node!(
    /// `(positional, …, name: value, …)`
    ArgList,
    ArgList
);

impl ArgList {
    /// The positional arguments, in order.
    pub fn positional(&self) -> impl Iterator<Item = Expr> {
        children(&self.syntax)
    }

    /// The named arguments, in order.
    pub fn named(&self) -> impl Iterator<Item = NamedArg> {
        children(&self.syntax)
    }
}

ast_node!(
    /// `name: value`
    NamedArg,
    NamedArg
);

impl NamedArg {
    /// The parameter being named.
    #[must_use]
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.syntax, SyntaxKind::Ident)
    }

    /// The argument's value.
    #[must_use]
    pub fn value(&self) -> Option<Expr> {
        child(&self.syntax)
    }
}

ast_node!(
    /// `object.name`, `object?.name`
    MemberExpr,
    MemberExpr
);

impl MemberExpr {
    /// The expression whose member is accessed.
    #[must_use]
    pub fn object(&self) -> Option<Expr> {
        child(&self.syntax)
    }

    /// The member's name.
    #[must_use]
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.syntax, SyntaxKind::Ident)
    }

    /// Whether the access is `?.`, propagating null.
    #[must_use]
    pub fn is_optional(&self) -> bool {
        token(&self.syntax, SyntaxKind::QuestionDot).is_some()
    }
}

ast_node!(
    /// `object[index]`
    IndexExpr,
    IndexExpr
);

impl IndexExpr {
    /// The list or map being indexed.
    #[must_use]
    pub fn object(&self) -> Option<Expr> {
        children(&self.syntax).next()
    }

    /// The index or key.
    #[must_use]
    pub fn index(&self) -> Option<Expr> {
        children(&self.syntax).nth(1)
    }
}

ast_node!(
    /// `|params| body`
    Lambda,
    Lambda
);

impl Lambda {
    /// The parameter list.
    #[must_use]
    pub fn params(&self) -> Option<LambdaParams> {
        child(&self.syntax)
    }

    /// The body.
    #[must_use]
    pub fn body(&self) -> Option<Expr> {
        child(&self.syntax)
    }
}

ast_node!(
    /// `|a, b|` — untyped names; `||` is empty.
    LambdaParams,
    LambdaParams
);

impl LambdaParams {
    /// The parameter names, in order.
    pub fn names(&self) -> impl Iterator<Item = SyntaxToken> {
        tokens(&self.syntax, SyntaxKind::Ident)
    }
}

ast_node!(
    /// `[a, b]`
    ListLit,
    ListLit
);

impl ListLit {
    /// The elements, in order.
    pub fn elements(&self) -> impl Iterator<Item = Expr> {
        children(&self.syntax)
    }
}

ast_node!(
    /// `{k: v}`
    MapLit,
    MapLit
);

impl MapLit {
    /// The entries, in order.
    pub fn entries(&self) -> impl Iterator<Item = MapEntry> {
        children(&self.syntax)
    }
}

ast_node!(
    /// `key: value`
    MapEntry,
    MapEntry
);

impl MapEntry {
    /// The key: a bare name or a string literal.
    #[must_use]
    pub fn key(&self) -> Option<SyntaxToken> {
        child_tokens(&self.syntax)
            .find(|t| matches!(t.kind(), SyntaxKind::Ident | SyntaxKind::StringLit))
    }

    /// The value.
    #[must_use]
    pub fn value(&self) -> Option<Expr> {
        child(&self.syntax)
    }
}

/// What kind of literal a [`Literal`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[allow(missing_docs, reason = "each variant names its literal")]
pub enum LiteralKind {
    String,
    Int,
    Float,
    True,
    False,
    Null,
}

ast_node!(
    /// A literal value. The token is exposed raw: string unescaping and interpolation
    /// wait on the deferred delimiter question.
    Literal,
    Literal
);

impl Literal {
    /// The literal's token.
    #[must_use]
    pub fn token(&self) -> Option<SyntaxToken> {
        self.syntax.first_token()
    }

    /// Which kind of literal this is.
    #[must_use]
    pub fn kind(&self) -> Option<LiteralKind> {
        Some(match self.token()?.kind() {
            SyntaxKind::StringLit => LiteralKind::String,
            SyntaxKind::Int => LiteralKind::Int,
            SyntaxKind::Float => LiteralKind::Float,
            SyntaxKind::True => LiteralKind::True,
            SyntaxKind::False => LiteralKind::False,
            SyntaxKind::Null => LiteralKind::Null,
            _ => return None,
        })
    }
}

ast_node!(
    /// A bare name.
    NameRef,
    NameRef
);

impl NameRef {
    /// The name's token.
    #[must_use]
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.syntax, SyntaxKind::Ident)
    }
}

ast_node!(
    /// `.hero` — a variant of whichever enum is expected here.
    VariantRef,
    VariantRef
);

impl VariantRef {
    /// The variant's token, without its dot.
    #[must_use]
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.syntax, SyntaxKind::Ident)
    }
}

ast_node!(
    /// `(inner)`
    ParenExpr,
    ParenExpr
);

impl ParenExpr {
    /// The parenthesised expression.
    #[must_use]
    pub fn inner(&self) -> Option<Expr> {
        child(&self.syntax)
    }
}

ast_node!(
    /// `expr is Type`
    TypeTest,
    TypeTest
);

impl TypeTest {
    /// The expression being tested.
    #[must_use]
    pub fn expr(&self) -> Option<Expr> {
        child(&self.syntax)
    }

    /// The type's name.
    #[must_use]
    pub fn type_name(&self) -> Option<SyntaxToken> {
        token(&self.syntax, SyntaxKind::Ident)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{AstNode, Item};
    use crate::{Lemmon, parse};

    fn head(src: &str) -> Expr {
        let Some(Item::Access(tag)) = parse(src).root().items().next() else {
            panic!("expected an access tag");
        };
        tag.expr().expect("an expression")
    }

    fn text<N: AstNode<Language = Lemmon>>(node: Option<N>) -> String {
        node.expect("a node").syntax().text().to_string()
    }

    fn token_text(token: Option<SyntaxToken>) -> String {
        token.expect("a token").text().to_owned()
    }

    #[test]
    fn exposes_the_operands_and_operator_of_a_binary_expression() {
        let Expr::Binary(expr) = head("<lem.(a + b * c)>").unwrap_paren() else {
            panic!("expected a binary expression");
        };
        assert_eq!(text(expr.lhs()), "a");
        assert_eq!(text(expr.rhs()), "b * c");
        let (op, token) = expr.op().expect("an operator");
        assert_eq!(op, BinaryOp::Add);
        assert_eq!(token.text(), "+");
        let Expr::Binary(expr) = head("<lem.(x in xs)>").unwrap_paren() else {
            panic!("expected a binary expression");
        };
        assert_eq!(expr.op().map(|(op, _)| op), Some(BinaryOp::In));
    }

    #[test]
    fn exposes_a_unary_operator_and_its_operand() {
        let Expr::Unary(expr) = head("<lem.(-a)>").unwrap_paren() else {
            panic!("expected a unary");
        };
        assert_eq!(expr.op().map(|(op, _)| op), Some(UnaryOp::Neg));
        assert_eq!(text(expr.operand()), "a");
        let Expr::Unary(expr) = head("<lem.(!a)>").unwrap_paren() else {
            panic!("expected a unary");
        };
        assert_eq!(expr.op().map(|(op, _)| op), Some(UnaryOp::Not));
    }

    #[test]
    fn exposes_the_three_parts_of_a_ternary() {
        let Expr::Ternary(expr) = head("<lem.(c ? a : b)>").unwrap_paren() else {
            panic!("expected a ternary");
        };
        assert_eq!(text(expr.condition()), "c");
        assert_eq!(text(expr.then()), "a");
        assert_eq!(text(expr.otherwise()), "b");
    }

    #[test]
    fn separates_positional_from_named_arguments() {
        let Expr::Call(call) = head("<lem.f(a, b, c: 1, d: 2)>") else {
            panic!("expected a call");
        };
        assert_eq!(text(call.callee()), "f");
        let args = call.args().expect("args");
        let positional: Vec<String> = args
            .positional()
            .map(|e| e.syntax().text().to_string())
            .collect();
        assert_eq!(positional, vec!["a", "b"]);
        let named: Vec<(String, String)> = args
            .named()
            .map(|n| (token_text(n.name()), text(n.value())))
            .collect();
        assert_eq!(
            named,
            vec![("c".into(), "1".into()), ("d".into(), "2".into())]
        );
    }

    #[test]
    fn knows_when_a_member_access_is_optional() {
        let Expr::Member(expr) = head("<lem.a?.b>") else {
            panic!("expected a member");
        };
        assert_eq!(text(expr.object()), "a");
        assert_eq!(token_text(expr.name()), "b");
        assert!(expr.is_optional());
        let Expr::Member(expr) = head("<lem.a.b>") else {
            panic!("expected a member");
        };
        assert!(!expr.is_optional());
    }

    #[test]
    fn exposes_the_object_and_index_of_an_index_expression() {
        let Expr::Index(expr) = head("<lem.xs[i + 1]>") else {
            panic!("expected an index");
        };
        assert_eq!(text(expr.object()), "xs");
        assert_eq!(text(expr.index()), "i + 1");
    }

    #[test]
    fn exposes_lambda_parameters_and_body() {
        let Expr::Lambda(lambda) = head("<lem.(|a, b| a)>").unwrap_paren() else {
            panic!("expected a lambda");
        };
        let names: Vec<String> = lambda
            .params()
            .expect("params")
            .names()
            .map(|t| t.text().to_owned())
            .collect();
        assert_eq!(names, vec!["a", "b"]);
        assert_eq!(text(lambda.body()), "a");
        let Expr::Lambda(lambda) = head("<lem.(|| 1)>").unwrap_paren() else {
            panic!("expected a lambda");
        };
        assert_eq!(lambda.params().expect("params").names().count(), 0);
    }

    #[test]
    fn exposes_list_elements_and_map_entries() {
        let Expr::List(list) = head("<lem.[1, 2]>") else {
            panic!("expected a list");
        };
        assert_eq!(list.elements().count(), 2);
        let Expr::Map(map) = head("<lem.{a: 1, \"b\": 2}>") else {
            panic!("expected a map");
        };
        let entries: Vec<(String, String)> = map
            .entries()
            .map(|e| (token_text(e.key()), text(e.value())))
            .collect();
        assert_eq!(
            entries,
            vec![("a".into(), "1".into()), ("\"b\"".into(), "2".into())]
        );
    }

    #[test]
    fn classifies_every_literal() {
        for (src, kind) in [
            ("<lem.\"s\">", LiteralKind::String),
            ("<lem.1>", LiteralKind::Int),
            ("<lem.1.5>", LiteralKind::Float),
            ("<lem.true>", LiteralKind::True),
            ("<lem.false>", LiteralKind::False),
            ("<lem.null>", LiteralKind::Null),
        ] {
            let Expr::Literal(literal) = head(src) else {
                panic!("expected a literal in {src}");
            };
            assert_eq!(literal.kind(), Some(kind), "{src}");
            assert!(literal.token().is_some());
        }
    }

    #[test]
    fn exposes_a_name_a_paren_and_a_type_test() {
        let Expr::Name(name) = head("<lem.x>") else {
            panic!("expected a name");
        };
        assert_eq!(token_text(name.name()), "x");
        let Expr::Paren(paren) = head("<lem.(x)>") else {
            panic!("expected a paren");
        };
        assert_eq!(text(paren.inner()), "x");
        let Expr::TypeTest(test) = head("<lem.(x is T)>").unwrap_paren() else {
            panic!("expected a type test");
        };
        assert_eq!(text(test.expr()), "x");
        assert_eq!(token_text(test.type_name()), "T");
    }

    impl Expr {
        fn unwrap_paren(self) -> Self {
            match self {
                Self::Paren(paren) => paren.inner().expect("an inner expression"),
                other => other,
            }
        }
    }
}

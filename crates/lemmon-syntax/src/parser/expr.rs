//! The expression level: a Pratt parser over a tag head.
//!
//! Binding powers follow JavaScript for the operators the languages share. Every infix
//! operator has a left and a right power; left-associative operators bind tighter on
//! the right, the ternary binds equally so that it nests rightward. The lambda body is
//! parsed at the lowest power, so `|a| a ? b : c` takes the whole conditional, as an
//! arrow function would.

use super::{CompletedMarker, Parser};
use crate::TokenKind;
use crate::ast::BinaryOp;
use crate::syntax_kind::SyntaxKind;

/// Binding powers, lowest first. Unary and postfix sit above every infix operator.
const TERNARY: (u8, u8) = (1, 1);
const UNARY: u8 = 17;
const POSTFIX: u8 = 19;

/// The binding powers of the infix operator `kind` spells, if any. Which tokens are
/// binary operators is [`BinaryOp`]'s knowledge; `is` is the one infix that is not one.
fn infix_bp(kind: SyntaxKind) -> Option<(u8, u8)> {
    use BinaryOp::{
        Add, And, Coalesce, Div, Eq, Gt, Gte, In, Lt, Lte, Mul, NotEq, Or, Rem, Sub,
    };
    if kind == SyntaxKind::Is {
        return Some((11, 12));
    }
    Some(match BinaryOp::from_kind(kind)? {
        Coalesce => (3, 4),
        Or => (5, 6),
        And => (7, 8),
        Eq | NotEq => (9, 10),
        Lt | Lte | Gt | Gte | In => (11, 12),
        Add | Sub => (13, 14),
        Mul | Div | Rem => (15, 16),
    })
}

/// Parses one expression; reports a missing one. Returns whether anything was parsed.
pub(super) fn expression(p: &mut Parser<'_>) -> bool {
    expr_bp(p, 0).is_some()
}

/// Whether `kind` is a reserved word: lexed as its own token, so never an identifier.
fn is_reserved(kind: Option<TokenKind>) -> bool {
    matches!(
        kind,
        Some(
            TokenKind::Lt
                | TokenKind::Lte
                | TokenKind::Gt
                | TokenKind::Gte
                | TokenKind::In
                | TokenKind::Is
                | TokenKind::As
                | TokenKind::True
                | TokenKind::False
                | TokenKind::Null
        )
    )
}

/// Takes an identifier where a reserved word cannot stand in for one — a member,
/// named-argument, field, or variant name. A reserved word there is consumed into an
/// `Error` node, so that the Pratt loop does not re-read it as the `in`/`is` operator.
pub(super) fn expect_name(p: &mut Parser<'_>, what: &str) {
    if is_reserved(p.current()) {
        let span = p.current_span();
        let message = format!(
            "`{}` is a reserved word and cannot be used as {what}",
            p.current_text()
        );
        p.error(message, span);
        let m = p.start();
        p.bump();
        m.complete(p, SyntaxKind::Error);
    } else {
        p.expect(TokenKind::Ident, what);
    }
}

fn expr_bp(p: &mut Parser<'_>, min_bp: u8) -> Option<CompletedMarker> {
    if !p.enter() {
        p.skip_to_end("expression nested too deeply");
        return None;
    }
    let result = expr_bp_in(p, min_bp);
    p.leave();
    result
}

fn expr_bp_in(p: &mut Parser<'_>, min_bp: u8) -> Option<CompletedMarker> {
    let mut lhs = prefix(p)?;
    while let Some(kind) = p.current() {
        match kind {
            TokenKind::Dot | TokenKind::QuestionDot if POSTFIX >= min_bp => {
                let m = p.precede(lhs);
                p.bump();
                expect_name(p, "a member name");
                lhs = m.complete(p, SyntaxKind::MemberExpr);
            }
            TokenKind::LBracket if POSTFIX >= min_bp => {
                let m = p.precede(lhs);
                p.bump();
                expr_bp(p, 0);
                p.expect(TokenKind::RBracket, "`]`");
                lhs = m.complete(p, SyntaxKind::IndexExpr);
            }
            TokenKind::LParen if POSTFIX >= min_bp => {
                let m = p.precede(lhs);
                arg_list(p);
                lhs = m.complete(p, SyntaxKind::CallExpr);
            }
            TokenKind::Question if TERNARY.0 >= min_bp => {
                let m = p.precede(lhs);
                p.bump();
                expr_bp(p, 0);
                p.expect(TokenKind::Colon, "`:`");
                expr_bp(p, TERNARY.1);
                lhs = m.complete(p, SyntaxKind::TernaryExpr);
            }
            TokenKind::Is if infix_bp(kind.into()).is_some_and(|(l, _)| l >= min_bp) => {
                let m = p.precede(lhs);
                p.bump();
                p.expect(TokenKind::Ident, "a type name");
                lhs = m.complete(p, SyntaxKind::TypeTest);
            }
            _ => {
                let Some((l, r)) = infix_bp(kind.into()) else {
                    break;
                };
                if l < min_bp {
                    break;
                }
                let m = p.precede(lhs);
                p.bump();
                expr_bp(p, r);
                lhs = m.complete(p, SyntaxKind::BinaryExpr);
            }
        }
    }
    Some(lhs)
}

fn prefix(p: &mut Parser<'_>) -> Option<CompletedMarker> {
    let kind = match p.current() {
        Some(TokenKind::Minus | TokenKind::Bang) => {
            let m = p.start();
            p.bump();
            expr_bp(p, UNARY);
            return Some(m.complete(p, SyntaxKind::UnaryExpr));
        }
        Some(TokenKind::Ident) => SyntaxKind::NameRef,
        Some(
            TokenKind::Int
            | TokenKind::Float
            | TokenKind::StringLit
            | TokenKind::True
            | TokenKind::False
            | TokenKind::Null,
        ) => SyntaxKind::Literal,
        // A leading `.` opens a variant: `.hero` is the variant of whichever enum is
        // expected where it stands. The parser resolves no names, so which enum that is
        // stays the compiler's question; here it is only ever `.` then a name.
        Some(TokenKind::Dot) => {
            let m = p.start();
            p.bump();
            expect_name(p, "a variant name");
            return Some(m.complete(p, SyntaxKind::VariantRef));
        }
        Some(TokenKind::LParen) => {
            let m = p.start();
            p.bump();
            expr_bp(p, 0);
            p.expect(TokenKind::RParen, "`)`");
            return Some(m.complete(p, SyntaxKind::ParenExpr));
        }
        Some(TokenKind::LBracket) => return Some(list(p)),
        Some(TokenKind::LBrace) => return Some(map(p)),
        Some(TokenKind::Pipe | TokenKind::PipePipe) => return Some(lambda(p)),
        _ => {
            p.error_expected("an expression");
            return None;
        }
    };
    let m = p.start();
    p.bump();
    Some(m.complete(p, kind))
}

/// Drives a comma-separated list up to `close`, calling `item` for each element. One
/// trailing comma is allowed (D24); an empty slot between commas is left to `item` to
/// report, one diagnostic per slot. The loop cannot stall: an item that consumes
/// nothing is followed either by a comma, which is eaten, or by anything else, which
/// ends the list.
pub(super) fn comma_list(
    p: &mut Parser<'_>,
    close: TokenKind,
    mut item: impl FnMut(&mut Parser<'_>),
) {
    loop {
        if p.at(close) || p.at_head_end() {
            break;
        }
        item(p);
        if !p.eat(TokenKind::Comma) {
            break;
        }
    }
    let close_text = match close {
        TokenKind::RParen => "`)`",
        TokenKind::RBracket => "`]`",
        TokenKind::RBrace => "`}`",
        _ => "`|`",
    };
    p.expect(close, close_text);
}

/// `(positional, …, name: value, …)`
fn arg_list(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump();
    let mut seen_named = false;
    comma_list(p, TokenKind::RParen, |p| {
        if (p.at(TokenKind::Ident) || is_reserved(p.current()))
            && p.nth(1) == Some(TokenKind::Colon)
        {
            let m = p.start();
            expect_name(p, "a named-argument name");
            p.bump();
            expr_bp(p, 0);
            m.complete(p, SyntaxKind::NamedArg);
            seen_named = true;
        } else {
            let span = p.current_span();
            // Only an argument that is actually there can be out of order; an empty
            // slot is reported as a missing argument instead.
            if expr_bp(p, 0).is_some() && seen_named {
                p.error("positional argument after a named argument", span);
            }
        }
    });
    m.complete(p, SyntaxKind::ArgList);
}

/// `[a, b]`
fn list(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump();
    comma_list(p, TokenKind::RBracket, |p| {
        expr_bp(p, 0);
    });
    m.complete(p, SyntaxKind::ListLit)
}

/// `{k: v, "k": v}`
fn map(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump();
    comma_list(p, TokenKind::RBrace, |p| {
        // An empty slot gets a diagnostic and no node: a zero-width `MapEntry` would
        // be a boundary tree-sitter has nothing to agree with.
        if p.at(TokenKind::Comma) || p.at(TokenKind::RBrace) || p.at_head_end() {
            p.error_expected("a key");
            return;
        }
        let m = p.start();
        if !p.eat(TokenKind::Ident) && !p.eat(TokenKind::StringLit) {
            p.error_expected("a key");
            // Take whatever stands in for the key, so that the `:` and value after it
            // still parse and the one mistake stays one diagnostic.
            if !p.at(TokenKind::Colon) {
                let e = p.start();
                p.bump();
                e.complete(p, SyntaxKind::Error);
            }
        }
        p.expect(TokenKind::Colon, "`:`");
        expr_bp(p, 0);
        m.complete(p, SyntaxKind::MapEntry);
    });
    m.complete(p, SyntaxKind::MapLit)
}

/// `|a, b| body` — `||` is the empty parameter list.
fn lambda(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    let params = p.start();
    if !p.eat(TokenKind::PipePipe) {
        p.bump();
        comma_list(p, TokenKind::Pipe, |p| {
            p.expect(TokenKind::Ident, "a parameter name");
        });
    }
    params.complete(p, SyntaxKind::LambdaParams);
    expr_bp(p, 0);
    m.complete(p, SyntaxKind::Lambda)
}

#[cfg(test)]
mod tests {
    use crate::parse;

    fn messages(src: &str) -> Vec<String> {
        parse(src)
            .diagnostics
            .into_iter()
            .map(|d| d.message)
            .collect()
    }

    /// The head's expression tree as nested kind names, tokens omitted.
    fn shape(src: &str) -> String {
        use crate::{SyntaxKind, SyntaxNode};
        fn go(node: &SyntaxNode, out: &mut String) {
            out.push_str(&node.kind().name());
            let children: Vec<SyntaxNode> = node.children().collect();
            if !children.is_empty() {
                out.push('(');
                for (i, child) in children.iter().enumerate() {
                    if i > 0 {
                        out.push(' ');
                    }
                    go(child, out);
                }
                out.push(')');
            }
        }
        let root = parse(src).syntax();
        let head = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::Head)
            .expect("a head");
        let mut out = String::new();
        go(&head.first_child().expect("an expression"), &mut out);
        out
    }

    #[test]
    fn binds_multiplication_tighter_than_addition() {
        assert_eq!(
            shape("<lem.(a + b * c)>"),
            "ParenExpr(BinaryExpr(NameRef BinaryExpr(NameRef NameRef)))"
        );
    }

    #[test]
    fn associates_binary_operators_to_the_left() {
        assert_eq!(
            shape("<lem.(a - b - c)>"),
            "ParenExpr(BinaryExpr(BinaryExpr(NameRef NameRef) NameRef))"
        );
    }

    #[test]
    fn associates_the_ternary_to_the_right() {
        assert_eq!(
            shape("<lem.(a ? b : c ? d : e)>"),
            "ParenExpr(TernaryExpr(NameRef NameRef TernaryExpr(NameRef NameRef NameRef)))"
        );
    }

    #[test]
    fn binds_postfix_tighter_than_unary() {
        assert_eq!(
            shape("<lem.(-a.b)>"),
            "ParenExpr(UnaryExpr(MemberExpr(NameRef)))"
        );
    }

    #[test]
    fn extends_a_lambda_body_as_far_right_as_possible() {
        assert_eq!(
            shape("<lem.f(|a| a ?? b ? c : d)>"),
            "CallExpr(NameRef ArgList(Lambda(LambdaParams TernaryExpr(BinaryExpr(NameRef \
             NameRef) NameRef NameRef))))"
        );
    }

    #[test]
    fn parses_an_empty_lambda_parameter_list() {
        assert_eq!(
            shape("<lem.f(|| 1)>"),
            "CallExpr(NameRef ArgList(Lambda(LambdaParams Literal)))"
        );
    }

    #[test]
    fn rejects_a_positional_argument_after_a_named_one() {
        assert_eq!(
            messages("<lem.f(a: 1, b)>"),
            vec!["positional argument after a named argument"]
        );
    }

    #[test]
    fn treats_as_as_unexpected() {
        assert_eq!(messages("<lem.(a as b)>"), vec!["expected `)`, found `as`"]);
    }

    #[test]
    fn takes_a_type_name_after_is() {
        assert_eq!(
            messages("<lem.(a is 1)>"),
            vec!["expected a type name, found `1`"]
        );
    }

    #[test]
    fn requires_a_key_and_colon_in_a_map_entry() {
        assert_eq!(
            messages("<lem.({1: 2})>"),
            vec!["expected a key, found `1`"]
        );
    }

    #[test]
    fn reports_a_missing_operand_once() {
        assert_eq!(
            messages("<lem.(a +>"),
            vec!["expected an expression before `>`"]
        );
    }

    #[test]
    fn reports_a_reserved_word_used_as_a_member_name_once() {
        assert_eq!(
            messages("<lem.a.in>"),
            vec!["`in` is a reserved word and cannot be used as a member name"]
        );
        assert_eq!(shape("<lem.a.in>"), "MemberExpr(NameRef Error)");
    }

    #[test]
    fn reports_a_reserved_word_used_as_a_named_argument_name() {
        assert_eq!(
            messages("<lem.f(in: 1)>"),
            vec!["`in` is a reserved word and cannot be used as a named-argument name"]
        );
    }

    #[test]
    fn reports_an_empty_slot_after_a_named_argument_as_missing() {
        assert_eq!(
            messages("<lem.f(a: 1, , b: 2)>"),
            vec!["expected an expression, found `,`"]
        );
    }

    #[test]
    fn reports_a_positional_argument_after_a_named_one() {
        assert_eq!(
            messages("<lem.f(a: 1, b)>"),
            vec!["positional argument after a named argument"]
        );
    }

    #[test]
    fn accepts_one_trailing_comma_in_every_list() {
        for src in [
            "<lem.[a, b,]>",
            "<lem.{k: v,}>",
            "<lem.f(a, b,)>",
            "<lem.f(a, b: c,)>",
            "<lem.f(|a, b,| a)>",
        ] {
            assert_eq!(messages(src), Vec::<String>::new(), "{src}");
        }
    }

    #[test]
    fn still_reports_an_empty_slot_between_commas() {
        let missing = vec!["expected an expression, found `,`"];
        assert_eq!(messages("<lem.[a,,b]>"), missing);
        assert_eq!(messages("<lem.f(,)>"), missing);
        assert_eq!(messages("<lem.[,]>"), missing);
        assert_eq!(
            messages("<lem.{a: 1,,}>"),
            vec!["expected a key, found `,`"]
        );
        assert_eq!(messages("<lem.{,}>"), vec!["expected a key, found `,`"]);
    }

    #[test]
    fn reports_every_empty_slot_separately() {
        assert_eq!(messages("<lem.[,,,]>").len(), 3);
        assert_eq!(messages("<lem.{,,}>").len(), 2);
    }

    #[test]
    fn opens_no_node_for_an_empty_map_slot() {
        assert_eq!(shape("<lem.{a: 1,,}>"), "MapLit(MapEntry(Literal))");
        assert_eq!(shape("<lem.{1: 2}>"), "MapLit(MapEntry(Error Literal))");
    }

    #[test]
    fn reports_a_list_cut_after_a_trailing_comma_once() {
        assert_eq!(messages("<lem.[a,>"), vec!["expected `]` before `>`"]);
    }

    #[test]
    fn stops_recursing_into_pathological_nesting() {
        let src = format!("<lem.{}a{}>", "(".repeat(100_000), ")".repeat(100_000));
        assert_eq!(messages(&src), vec!["expression nested too deeply"]);
        let src = format!("<lem.{}a{}>", "[".repeat(100_000), "]".repeat(100_000));
        assert_eq!(messages(&src), vec!["expression nested too deeply"]);
        let src = format!("<lem.{}a>", "-".repeat(100_000));
        assert_eq!(messages(&src), vec!["expression nested too deeply"]);
    }
}

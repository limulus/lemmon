//! The template level: text, access tags, and the keyword tags with their nesting.
//!
//! A tag's shape — void or block — is decided from its name and its head alone
//! (invariant 4): `lem-let` and `lem-fn` look for a top-level `=` among the head's
//! tokens — `lem-let` also for the contextual keyword `from` past its name — and every
//! other builtin has a fixed shape. Block bodies are parsed with a stack
//! of open blocks, so that a close tag can be matched to what is actually open, and a
//! mismatch can say what was expected instead.

use super::{Marker, Parser, expr};
use crate::syntax_kind::SyntaxKind;
use crate::{Span, TokenKind, nearest};

/// Every `lem-*` name the language reserves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Builtin {
    If,
    ElseIf,
    Else,
    For,
    Let,
    Fn,
    Match,
    Case,
    Default,
    Type,
    Enum,
    Page,
    Import,
    Style,
    Head,
    Script,
    Emit,
}

const BUILTINS: &[(&str, Builtin)] = &[
    ("if", Builtin::If),
    ("else-if", Builtin::ElseIf),
    ("else", Builtin::Else),
    ("for", Builtin::For),
    ("let", Builtin::Let),
    ("fn", Builtin::Fn),
    ("match", Builtin::Match),
    ("case", Builtin::Case),
    ("default", Builtin::Default),
    ("type", Builtin::Type),
    ("enum", Builtin::Enum),
    ("page", Builtin::Page),
    ("import", Builtin::Import),
    ("style", Builtin::Style),
    ("head", Builtin::Head),
    ("script", Builtin::Script),
    ("emit", Builtin::Emit),
];

impl Builtin {
    fn lookup(name: &str) -> Option<Self> {
        BUILTINS.iter().find(|(n, _)| *n == name).map(|(_, b)| *b)
    }

    /// The builtin closest to `name`, for a did-you-mean — none when nothing is close
    /// enough, which is what keeps `<lem-widget>` from being answered with `lem-if`.
    /// The policy is [`crate::nearest`]'s; the tag namespace only supplies the candidates.
    fn nearest(name: &str) -> Option<&'static str> {
        nearest(name, BUILTINS.iter().map(|(n, _)| *n))
    }

    /// Whether the tag is a block — for the builtins the parser keeps opaquely. The
    /// ones it parses are dispatched by name before this is consulted.
    fn is_block(self) -> bool {
        match self {
            Self::If
            | Self::ElseIf
            | Self::Else
            | Self::For
            | Self::Let
            | Self::Fn
            | Self::Match
            | Self::Case
            | Self::Default
            | Self::Type
            | Self::Enum => {
                unreachable!("parsed builtins are dispatched by name")
            }
            Self::Style | Self::Head => true,
            Self::Page | Self::Import | Self::Script | Self::Emit => false,
        }
    }

    /// Why an opaque builtin is refused: the tail of its diagnostic, naming the backlog
    /// item that gives it meaning. The phrase and the item share a row so that the
    /// placeholder gate (∆DA2) can see them together and fail the build once that item
    /// has landed and the refusal has outlived it.
    fn unsupported(self) -> &'static str {
        match self {
            Self::If
            | Self::ElseIf
            | Self::Else
            | Self::For
            | Self::Let
            | Self::Fn
            | Self::Match
            | Self::Case
            | Self::Default
            | Self::Type
            | Self::Enum => {
                unreachable!("parsed builtins are dispatched by name")
            }
            Self::Style | Self::Head | Self::Script | Self::Emit | Self::Page => {
                "is not supported yet (∆RGQ)"
            }
            Self::Import => "is not supported yet (∆PgF)",
        }
    }
}

/// What a block's body may contain besides ordinary items.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Body {
    /// `lem-else-if`s, then at most one `lem-else`.
    If {
        seen_else: bool,
    },
    /// At most one `lem-else`.
    For {
        seen_else: bool,
    },
    /// `lem-case`s, then at most one `lem-default`, and nothing else.
    Match {
        seen_default: bool,
    },
    /// The fields of a `lem-type`, which are parsed before [`items`] runs; anything
    /// `items` then finds before the close tag is a mistake.
    Fields,
    Plain,
}

impl Body {
    /// What this body refuses, when it holds only its own parts.
    const fn refuses(self) -> Option<&'static str> {
        match self {
            Self::Match { .. } => {
                Some("`lem-match` holds only `lem-case` and `lem-default`")
            }
            Self::Fields => Some("`lem-type` holds only fields, one `name: Type` per line"),
            Self::If { .. } | Self::For { .. } | Self::Plain => None,
        }
    }
}

/// A block tag whose close has not been seen yet.
struct Open<'src> {
    name: &'src str,
    body: Body,
}

/// How a block's body stopped.
enum BodyEnd {
    /// At its own close tag, not yet consumed.
    Matched,
    /// At the close tag of an outer block, left for that block to consume.
    OuterClose,
    Eof,
}

pub(super) fn root(p: &mut Parser<'_>) {
    let m = p.start();
    let mut stack = Vec::new();
    match items(p, &mut stack) {
        BodyEnd::Eof => {}
        BodyEnd::Matched | BodyEnd::OuterClose => {
            unreachable!("no block is open at the root")
        }
    }
    m.complete(p, SyntaxKind::Root);
}

/// Parses the one item at the start of the input as a `Root` of its own — what
/// document mode uses to find where a lem-block ends (`docs/language.md` §Document
/// mode). The input must start at a tag opener, so the item is never a text run.
pub(super) fn item_prefix(p: &mut Parser<'_>) {
    let m = p.start();
    let mut stack = Vec::new();
    match item(p, &mut stack) {
        None | Some(BodyEnd::Eof) => {}
        Some(BodyEnd::Matched | BodyEnd::OuterClose) => {
            unreachable!("no block is open around the prefix item")
        }
    }
    m.complete(p, SyntaxKind::Root);
}

/// Parses items until the end of input or a close tag of a block on `stack`.
fn items<'src>(p: &mut Parser<'src>, stack: &mut Vec<Open<'src>>) -> BodyEnd {
    loop {
        if let Some(end) = item(p, stack) {
            return end;
        }
    }
}

/// Parses one item, or returns how the body it stands in stopped instead.
///
/// A close tag matching nothing open is kept here as an `Error` node and parsing
/// carries on: it is never adopted as the innermost block's close, since the author's
/// real close is still to come. A close the scanner cut short adds no diagnostic of
/// its own.
fn item<'src>(p: &mut Parser<'src>, stack: &mut Vec<Open<'src>>) -> Option<BodyEnd> {
    match p.current() {
        None => return Some(BodyEnd::Eof),
        Some(TokenKind::Text) => {
            // Whitespace between a match's cases is layout, not content.
            if !p.current_text().trim().is_empty() {
                refuse_in_body(p, stack);
            }
            let m = p.start();
            p.bump();
            m.complete(p, SyntaxKind::TextRun);
        }
        Some(TokenKind::OpenAccess) => {
            refuse_in_body(p, stack);
            access_tag(p);
        }
        Some(TokenKind::OpenTag) => {
            let name = &p.current_text()["<lem-".len()..];
            let own_part = matches!(stack.last().map(|o| o.body), Some(Body::Match { .. }))
                && matches!(name, "case" | "default");
            if !own_part {
                refuse_in_body(p, stack);
            }
            open_tag(p, stack);
        }
        Some(TokenKind::CloseTag) => {
            let name = &p.current_text()["</lem-".len()..];
            if stack.last().is_some_and(|top| top.name == name) {
                return Some(BodyEnd::Matched);
            }
            let span = p.current_span();
            let cut = p.current_tag_is_cut();
            if cut {
                // The scanner has reported the cut; what the author meant to
                // close is not yet written.
            } else if Builtin::lookup(name).is_none() {
                p.error(unknown_message(name), span);
            } else if let Some(top) = stack.last() {
                p.error(
                        format!(
                            "found `</lem-{name}>` while `lem-{}` is still open: expected `</lem-{}>`",
                            top.name, top.name
                        ),
                        span,
                    );
            } else {
                p.error(
                    format!("stray `</lem-{name}>`: no `lem-{name}` is open"),
                    span,
                );
            }
            // A cut close never reaches past the innermost block: closing outer
            // blocks on its say-so would lose the author's real closes as strays.
            if !cut && stack.iter().any(|o| o.name == name) {
                return Some(BodyEnd::OuterClose);
            }
            let m = p.start();
            close_tag(p);
            m.complete(p, SyntaxKind::Error);
        }
        Some(_) => {
            // Head tokens outside a head cannot come from the scanner; handled so
            // that the parser is total over any token stream.
            p.skip_to_head_end("unexpected tokens outside a tag");
        }
    }
    None
}

/// Reports the current item when the innermost open body holds only its own parts.
fn refuse_in_body(p: &mut Parser<'_>, stack: &[Open<'_>]) {
    if let Some(message) = stack.last().and_then(|o| o.body.refuses()) {
        let span = p.current_span();
        p.error(message, span);
    }
}

/// `<lem.expr>`
fn access_tag(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump();
    head(p, |p| {
        expr::expression(p);
    });
    if p.at(TokenKind::SelfClose) {
        let span = p.current_span();
        p.error("access tags are never slashed: expected `>`", span);
    }
    void_close(p);
    m.complete(p, SyntaxKind::AccessTag);
}

/// Parses a head: `f` parses its grammar, anything left before the `>` is an error.
fn head(p: &mut Parser<'_>, f: impl FnOnce(&mut Parser<'_>)) {
    let m = p.start();
    f(p);
    let message = format!("unexpected `{}` in tag head", p.current_text());
    p.skip_to_head_end(&message);
    m.complete(p, SyntaxKind::Head);
}

/// Takes the `>` or `/>` of a void tag. A head the scanner cut short has neither.
fn void_close(p: &mut Parser<'_>) {
    if !p.eat(TokenKind::Close) {
        p.eat(TokenKind::SelfClose);
    }
}

/// Takes the `>` of a block tag. A `/>` is reported, but the name wins: the tag stays
/// the block its head says it is, and its body and close are parsed as usual.
fn block_close(p: &mut Parser<'_>, name: &str) {
    if p.at(TokenKind::SelfClose) {
        let span = p.current_span();
        p.error(
            format!("`lem-{name}` is a block tag and cannot be self-closed: expected `>`"),
            span,
        );
    }
    void_close(p);
}

/// Whether the current head contains an `=` — the void marker of `lem-let` and
/// `lem-fn`. Nothing else in a head can be an `=` (named arguments use `:`, there is
/// no assignment), so no bracket counting is needed, and a stray one in malformed
/// input still means the author wrote the void form.
fn head_has_eq(p: &Parser<'_>) -> bool {
    p.tokens[p.pos.min(p.tokens.len())..]
        .iter()
        .take_while(|t| !p.is_head_end_kind(t.kind))
        .any(|t| t.kind == TokenKind::Eq)
}

/// Whether the current head holds the contextual keyword `from` past its name — the
/// marker of the source form of `lem-let` (D30). The head's first token is the name,
/// so a capture named `from` keeps its block shape, as a field named `by` keeps its
/// type — unless a scanner error token stood in the name's place, which `bump` has
/// already skipped: then the head begins after the name. Checked after
/// [`head_has_eq`]: a head with both is the `=` form with junk before its `=`.
fn head_has_from(p: &Parser<'_>) -> bool {
    p.tokens[p.pos.min(p.tokens.len())..]
        .iter()
        .take_while(|t| !p.is_head_end_kind(t.kind))
        .skip(usize::from(!p.displaced))
        .any(|t| t.kind == TokenKind::Ident && t.text(p.src) == "from")
}

fn at_from(p: &Parser<'_>) -> bool {
    p.at(TokenKind::Ident) && p.current_text() == "from"
}

fn open_tag<'src>(p: &mut Parser<'src>, stack: &mut Vec<Open<'src>>) {
    let name = &p.current_text()["<lem-".len()..];
    let span = p.current_span();
    let m = p.start();
    p.bump();
    let Some(builtin) = Builtin::lookup(name) else {
        unknown_tag(p, m, name, span);
        return;
    };
    match builtin {
        Builtin::If => {
            head(p, |p| {
                expr::expression(p);
            });
            block(
                p,
                m,
                SyntaxKind::IfTag,
                name,
                span,
                Body::If { seen_else: false },
                stack,
            );
        }
        Builtin::For => {
            head(p, for_head);
            block(
                p,
                m,
                SyntaxKind::ForTag,
                name,
                span,
                Body::For { seen_else: false },
                stack,
            );
        }
        Builtin::ElseIf | Builtin::Else => separator(p, m, builtin, span, stack),
        Builtin::Let => {
            if head_has_eq(p) {
                head(p, let_void_head);
                void_close(p);
                m.complete(p, SyntaxKind::LetVoid);
            } else if head_has_from(p) {
                head(p, let_from_head);
                void_close(p);
                m.complete(p, SyntaxKind::LetFrom);
            } else {
                head(p, |p| {
                    p.expect(TokenKind::Ident, "a name");
                });
                block(p, m, SyntaxKind::LetBlock, name, span, Body::Plain, stack);
            }
        }
        Builtin::Fn => {
            if head_has_eq(p) {
                head(p, |p| {
                    fn_signature(p);
                    p.expect(TokenKind::Eq, "`=`");
                    expr::expression(p);
                });
                void_close(p);
                m.complete(p, SyntaxKind::FnVoid);
            } else {
                head(p, fn_signature);
                block(p, m, SyntaxKind::FnBlock, name, span, Body::Plain, stack);
            }
        }
        Builtin::Type => {
            head(p, type_head);
            type_block(p, m, name, span, stack);
        }
        Builtin::Enum => {
            head(p, enum_head);
            void_close(p);
            m.complete(p, SyntaxKind::EnumDecl);
        }
        Builtin::Match => {
            head(p, |p| {
                expr::expression(p);
            });
            block(
                p,
                m,
                SyntaxKind::MatchTag,
                name,
                span,
                Body::Match {
                    seen_default: false,
                },
                stack,
            );
        }
        Builtin::Case | Builtin::Default => {
            place_case(p, builtin, span, stack);
            let kind = match builtin {
                Builtin::Case => {
                    head(p, pattern);
                    SyntaxKind::CaseTag
                }
                _ => {
                    head(p, |p| p.skip_to_head_end("`lem-default` takes no head"));
                    SyntaxKind::DefaultTag
                }
            };
            block(p, m, kind, name, span, Body::Plain, stack);
        }
        _ => {
            p.error(format!("`lem-{name}` {}", builtin.unsupported()), span);
            let is_block = builtin.is_block();
            opaque_head(p);
            if is_block {
                block(
                    p,
                    m,
                    SyntaxKind::UnsupportedTag,
                    name,
                    span,
                    Body::Plain,
                    stack,
                );
            } else {
                void_close(p);
                m.complete(p, SyntaxKind::UnsupportedTag);
            }
        }
    }
}

/// A head that is kept but not parsed.
fn opaque_head(p: &mut Parser<'_>) {
    let m = p.start();
    while !p.at_head_end() {
        p.bump();
    }
    m.complete(p, SyntaxKind::Head);
}

/// An unrecognised `lem-*` name. Always void: the name cannot decide a shape, and a
/// block shape would need the close tag matched by name, which a context-free
/// tree-sitter grammar cannot express. Its close tag, if any, is reported the same way
/// by [`items`].
fn unknown_tag(p: &mut Parser<'_>, m: Marker, name: &str, span: Span) {
    p.error(unknown_message(name), span);
    opaque_head(p);
    void_close(p);
    m.complete(p, SyntaxKind::UnknownTag);
}

fn unknown_message(name: &str) -> String {
    match Builtin::nearest(name) {
        Some(n) => format!("unknown tag `lem-{name}`; did you mean `lem-{n}`?"),
        None => format!("unknown tag `lem-{name}`"),
    }
}

/// The `>`, body, and close tag of a block, then completes `m` as `kind`.
fn block<'src>(
    p: &mut Parser<'src>,
    m: Marker,
    kind: SyntaxKind,
    name: &'src str,
    span: Span,
    body: Body,
    stack: &mut Vec<Open<'src>>,
) {
    // A head the scanner cut short has been reported already; the block reaching the
    // end of input is a consequence of that, not a second mistake.
    let cut = p.at_cut_head();
    block_close(p, name);
    if !p.enter() {
        // Too deep to recurse: the body is left for the enclosing level to parse, where
        // this block's close tag is reported as matching nothing open — or, if it names
        // the enclosing block, taken as that block's close.
        p.error(format!("`lem-{name}` is nested too deeply"), span);
        m.complete(p, kind);
        return;
    }
    stack.push(Open { name, body });
    let body_marker = p.start();
    let end = items(p, stack);
    body_marker.complete(p, SyntaxKind::Block);
    stack.pop();
    p.leave();
    match end {
        BodyEnd::Matched => close_tag(p),
        BodyEnd::OuterClose => {}
        BodyEnd::Eof if cut => {}
        BodyEnd::Eof => p.error(
            format!("unclosed `lem-{name}`: expected `</lem-{name}>` before end of input"),
            span,
        ),
    }
    m.complete(p, kind);
}

/// `</lem-name>`
fn close_tag(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump();
    p.skip_to_head_end("unexpected tokens in close tag");
    if p.at(TokenKind::SelfClose) {
        let span = p.current_span();
        p.error("close tags are never slashed: expected `>`", span);
    }
    void_close(p);
    m.complete(p, SyntaxKind::CloseTag);
}

/// `lem-else-if` and `lem-else`, which are only meaningful inside the right body.
fn separator(
    p: &mut Parser<'_>,
    m: Marker,
    builtin: Builtin,
    span: Span,
    stack: &mut [Open<'_>],
) {
    let (tag, kind) = match builtin {
        Builtin::ElseIf => ("lem-else-if", SyntaxKind::ElseIfTag),
        _ => ("lem-else", SyntaxKind::ElseTag),
    };
    let problem = match (stack.last_mut().map(|o| &mut o.body), builtin) {
        (Some(Body::If { seen_else: true }), Builtin::ElseIf) => {
            Some(format!("`{tag}` cannot follow `lem-else`"))
        }
        (Some(Body::For { .. }), Builtin::ElseIf) => {
            Some(format!("`{tag}` is only allowed inside `lem-if`"))
        }
        (Some(Body::If { seen_else: true } | Body::For { seen_else: true }), _) => Some(
            format!("duplicate `{tag}`: a block has at most one `lem-else`"),
        ),
        (Some(Body::If { .. } | Body::For { .. }), _) => None,
        _ => Some(format!("`{tag}` outside `lem-if`")),
    };
    match problem {
        Some(message) => p.error(message, span),
        None => {
            if let Some(Body::If { seen_else } | Body::For { seen_else }) =
                stack.last_mut().map(|o| &mut o.body)
                && builtin == Builtin::Else
            {
                *seen_else = true;
            }
        }
    }
    head(p, |p| match builtin {
        Builtin::ElseIf => {
            expr::expression(p);
        }
        _ => p.skip_to_head_end("`lem-else` takes no head"),
    });
    void_close(p);
    m.complete(p, kind);
}

/// `lem-case` and `lem-default`, which are only meaningful directly inside a
/// `lem-match`, cases first. Reported like a misplaced separator; the tag is still
/// parsed as the block its name says it is.
fn place_case(p: &mut Parser<'_>, builtin: Builtin, span: Span, stack: &mut [Open<'_>]) {
    let tag = match builtin {
        Builtin::Case => "lem-case",
        _ => "lem-default",
    };
    let problem = match (stack.last_mut().map(|o| &mut o.body), builtin) {
        (Some(Body::Match { seen_default: true }), Builtin::Case) => {
            Some(format!("`{tag}` cannot follow `lem-default`"))
        }
        (Some(Body::Match { seen_default: true }), _) => Some(format!(
            "duplicate `{tag}`: a `lem-match` has at most one `lem-default`"
        )),
        (Some(Body::Match { seen_default }), Builtin::Default) => {
            *seen_default = true;
            None
        }
        (Some(Body::Match { .. }), _) => None,
        _ => Some(format!("`{tag}` outside `lem-match`")),
    };
    if let Some(message) = problem {
        p.error(message, span);
    }
}

/// The head of a `lem-case`: a literal, `null`, a variant name, or `Type as name`.
/// Nothing is opened for a missing pattern, so the node is never empty.
fn pattern(p: &mut Parser<'_>) {
    match p.current() {
        Some(
            TokenKind::Int
            | TokenKind::Float
            | TokenKind::StringLit
            | TokenKind::True
            | TokenKind::False
            | TokenKind::Null,
        ) => {
            let m = p.start();
            let literal = p.start();
            p.bump();
            literal.complete(p, SyntaxKind::Literal);
            m.complete(p, SyntaxKind::Pattern);
        }
        // `.hero` — a variant of the subject's enum. Dotted only: the bare slot below
        // is the type pattern, and nothing here resolves a name to tell them apart.
        Some(TokenKind::Dot) => {
            let m = p.start();
            p.bump();
            expr::expect_name(p, "a variant name");
            m.complete(p, SyntaxKind::Pattern);
        }
        Some(TokenKind::Ident) => {
            let m = p.start();
            p.bump();
            if p.eat(TokenKind::As) {
                p.expect(TokenKind::Ident, "a name to bind");
            }
            m.complete(p, SyntaxKind::Pattern);
        }
        _ => p.error_expected("a pattern"),
    }
}

/// `Name` or `Name extends Base`. `extends` is contextual: an identifier here, a plain
/// name anywhere else.
fn type_head(p: &mut Parser<'_>) {
    p.expect(TokenKind::Ident, "a type name");
    if p.at(TokenKind::Ident) && p.current_text() == "extends" {
        p.bump();
        p.expect(TokenKind::Ident, "a base type name");
    }
}

/// `Name = a | b | c`
fn enum_head(p: &mut Parser<'_>) {
    p.expect(TokenKind::Ident, "an enum name");
    skip_until(p, TokenKind::Eq, "unexpected tokens before `=`");
    p.expect(TokenKind::Eq, "`=`");
    loop {
        expr::expect_name(p, "a variant name");
        if !p.eat(TokenKind::Pipe) {
            break;
        }
    }
}

/// The `>`, fields, and close tag of a `lem-type`, then completes `m` as a
/// `TypeDecl`. The fields are lexed in fields mode (D29) and parsed here one per
/// line; what stands between the last field and the close tag — which the scanner
/// handed back to text mode at a `<` — is parsed as items so that its tags nest and
/// close as usual, each reported as out of place.
fn type_block<'src>(
    p: &mut Parser<'src>,
    m: Marker,
    name: &'src str,
    span: Span,
    stack: &mut Vec<Open<'src>>,
) {
    let cut = p.at_cut_head();
    block_close(p, name);
    if !p.enter() {
        p.error(format!("`lem-{name}` is nested too deeply"), span);
        m.complete(p, SyntaxKind::TypeDecl);
        return;
    }
    fields(p);
    stack.push(Open {
        name,
        body: Body::Fields,
    });
    let at_own_close =
        p.at(TokenKind::CloseTag) && &p.current_text()["</lem-".len()..] == name;
    let end = if at_own_close || p.current().is_none() {
        items(p, stack)
    } else {
        let stray = p.start();
        let end = items(p, stack);
        stray.complete(p, SyntaxKind::Block);
        end
    };
    stack.pop();
    p.leave();
    match end {
        BodyEnd::Matched => close_tag(p),
        BodyEnd::OuterClose => {}
        BodyEnd::Eof if cut => {}
        BodyEnd::Eof => p.error(
            format!("unclosed `lem-{name}`: expected `</lem-{name}>` before end of input"),
            span,
        ),
    }
    m.complete(p, SyntaxKind::TypeDecl);
}

/// The fields of a `lem-type`, up to the first text-mode token or the end of input.
/// Blank lines are allowed between fields.
fn field_list(p: &mut Parser<'_>) {
    let m = p.start();
    loop {
        while p.eat(TokenKind::Newline) {}
        if p.at_field_end() {
            break;
        }
        field(p);
    }
    m.complete(p, SyntaxKind::FieldList);
}

/// `name: Type [by index] [= default]`, ending at its line. Whatever else stands on
/// the line is skipped with one diagnostic, which is also what keeps [`field_list`]
/// moving over a line that starts with junk.
fn field(p: &mut Parser<'_>) {
    let m = p.start();
    expr::expect_name(p, "a field name");
    if p.at(TokenKind::Colon) {
        type_annotation(p);
    } else {
        p.error_expected("`: Type`");
    }
    if p.at(TokenKind::Ident) && p.current_text() == "by" {
        p.bump();
        expr::expression(p);
    }
    if p.eat(TokenKind::Eq) {
        expr::expression(p);
    }
    let message = format!("unexpected `{}` after the field", p.current_text());
    p.skip_to_field_end(&message);
    m.complete(p, SyntaxKind::Field);
}

/// The fields of a `lem-type`, parsed in fields mode.
fn fields(p: &mut Parser<'_>) {
    p.in_fields(field_list);
}

/// `x in xs`, `x, i in xs`, `k, v in m`
fn for_head(p: &mut Parser<'_>) {
    let m = p.start();
    if p.at(TokenKind::Ident) {
        p.bump();
        if p.eat(TokenKind::Comma) {
            p.expect(TokenKind::Ident, "a second loop variable");
        }
    } else {
        p.error_expected("a loop variable");
        skip_until(p, TokenKind::In, "unexpected tokens before `in`");
    }
    m.complete(p, SyntaxKind::ForBinding);
    p.expect(TokenKind::In, "`in`");
    expr::expression(p);
}

/// `name [: Type] = expr`
fn let_void_head(p: &mut Parser<'_>) {
    p.expect(TokenKind::Ident, "a name");
    if p.at(TokenKind::Colon) {
        type_annotation(p);
    }
    skip_until(p, TokenKind::Eq, "unexpected tokens before `=`");
    p.expect(TokenKind::Eq, "`=`");
    expr::expression(p);
}

/// `name: Type from source`
///
/// The type is required: it is what tells the host what to load (D30), and nothing
/// could infer it from a glob. The source is any expression — a glob or data file as
/// a string, or a call naming a host source such as `plugin("photos")` or
/// `env("SITE_URL")`; which forms mean anything is the checker's question, as whether
/// a type name resolves is.
fn let_from_head(p: &mut Parser<'_>) {
    // A scanner error token that stood in the name's place was skipped and reported
    // already; the `from` now current is the keyword, not a name.
    if !(p.displaced && at_from(p)) {
        p.expect(TokenKind::Ident, "a name");
    }
    if p.at(TokenKind::Colon) {
        let m = p.start();
        p.bump();
        // The `from` past the name is what chose this shape: it is the keyword, never
        // a type name, so a missing type is reported here rather than the source
        // after it being blamed for standing before `from`.
        if at_from(p) {
            p.error_expected("a type");
        } else {
            type_expr(p);
        }
        m.complete(p, SyntaxKind::TypeAnnotation);
    } else {
        p.error_at_current(
            "a `from` binding needs `: Type`: the type decides what the host loads",
        );
    }
    skip_until_by(p, at_from, "unexpected tokens before `from`");
    if at_from(p) {
        p.bump();
    } else {
        p.error_expected("`from`");
    }
    expr::expression(p);
}

/// `name(params) [: Type]`
fn fn_signature(p: &mut Parser<'_>) {
    p.expect(TokenKind::Ident, "a function name");
    let m = p.start();
    if p.expect(TokenKind::LParen, "`(`") {
        expr::comma_list(p, TokenKind::RParen, param);
    }
    m.complete(p, SyntaxKind::Params);
    if p.at(TokenKind::Colon) {
        type_annotation(p);
    }
}

/// `name: Type`
fn param(p: &mut Parser<'_>) {
    // An empty slot gets a diagnostic and no node, as in a map literal.
    if p.at(TokenKind::Comma) || p.at(TokenKind::RParen) || p.at_head_end() {
        p.error_expected("a parameter name");
        return;
    }
    let m = p.start();
    p.expect(TokenKind::Ident, "a parameter name");
    if p.at(TokenKind::Colon) {
        type_annotation(p);
    } else {
        p.error_expected("`: Type`");
        // Make progress on anything that is not a delimiter, so the loop cannot stall.
        if !matches!(p.current(), Some(TokenKind::Comma | TokenKind::RParen))
            && !p.at_head_end()
        {
            let e = p.start();
            p.bump();
            e.complete(p, SyntaxKind::Error);
        }
    }
    m.complete(p, SyntaxKind::Param);
}

/// `: Type`
fn type_annotation(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump();
    type_expr(p);
    m.complete(p, SyntaxKind::TypeAnnotation);
}

/// A type: a name, `[T]`, `{K: V}`, or `(A, B): R`, each optionally followed by `?`.
/// The `?` binds to the type just written, so `(A): R?` returns an optional; there is
/// no grouping form to make the function type itself optional. A missing type is
/// reported where it should have been.
fn type_expr(p: &mut Parser<'_>) {
    if !p.enter() {
        p.skip_to_end("type nested too deeply");
        return;
    }
    type_expr_in(p);
    p.leave();
}

fn type_expr_in(p: &mut Parser<'_>) {
    let kind = match p.current() {
        Some(TokenKind::Ident) => SyntaxKind::NamedType,
        Some(TokenKind::LBracket) => SyntaxKind::ListType,
        Some(TokenKind::LBrace) => SyntaxKind::MapType,
        Some(TokenKind::LParen) => SyntaxKind::FnType,
        _ => {
            p.error_expected("a type");
            return;
        }
    };
    let m = p.start();
    p.bump();
    match kind {
        SyntaxKind::ListType => {
            type_expr(p);
            p.expect(TokenKind::RBracket, "`]`");
        }
        SyntaxKind::MapType => {
            type_expr(p);
            p.expect(TokenKind::Colon, "`:`");
            type_expr(p);
            p.expect(TokenKind::RBrace, "`}`");
        }
        SyntaxKind::FnType => {
            expr::comma_list(p, TokenKind::RParen, |p| {
                type_expr(p);
            });
            p.expect(TokenKind::Colon, "`:` and a return type");
            type_expr(p);
        }
        _ => {}
    }
    let mut done = m.complete(p, kind);
    while p.at(TokenKind::Question) {
        let m = p.precede(done);
        p.bump();
        done = m.complete(p, SyntaxKind::OptionalType);
    }
}

/// Wraps everything before `kind` (or the head's end) in an `Error` node.
fn skip_until(p: &mut Parser<'_>, kind: TokenKind, message: &str) {
    skip_until_by(p, |p| p.at(kind), message);
}

/// Wraps everything before the token `at` accepts (or the head's end) in an `Error`
/// node.
fn skip_until_by(p: &mut Parser<'_>, at: impl Fn(&Parser<'_>) -> bool, message: &str) {
    p.skip_while(message, |p| !at(p) && !p.at_head_end());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only what is this function's own: that the whole tag table reaches the policy,
    /// and that the policy's refusal reaches back through it. `style` is a name the
    /// tests over [`crate::nearest`] itself do not offer, so a suggestion of it can only
    /// have come from here. How close is close enough is settled there, not again here.
    #[test]
    fn offers_every_tag_name_as_a_candidate() {
        assert_eq!(Builtin::nearest("styl"), Some("style"));
        assert_eq!(Builtin::nearest("widget"), None);
    }

    #[test]
    fn knows_the_fixed_shape_of_every_opaque_builtin() {
        assert!(Builtin::Style.is_block());
        assert!(!Builtin::Script.is_block());
    }

    #[test]
    fn names_the_item_each_opaque_builtin_waits_on() {
        assert!(Builtin::Style.unsupported().ends_with("(∆RGQ)"));
        assert!(Builtin::Import.unsupported().ends_with("(∆PgF)"));
    }
}

#[cfg(test)]
mod parse_tests {
    use crate::{Span, parse};

    fn diagnostics(src: &str) -> Vec<(Span, String)> {
        parse(src)
            .diagnostics
            .into_iter()
            .map(|d| (d.span, d.message))
            .collect()
    }

    fn messages(src: &str) -> Vec<String> {
        diagnostics(src).into_iter().map(|(_, m)| m).collect()
    }

    #[test]
    fn takes_any_equals_sign_in_a_head_as_the_void_marker() {
        assert_eq!(
            messages("<lem-let (x = 1 />"),
            vec!["expected a name, found `(`"]
        );
    }

    #[test]
    fn parses_every_from_binding_shape_clean() {
        for src in [
            "<lem-let posts: [Article] from \"feed/*/index.md\" />",
            "<lem-let photos: PhotoIndex from plugin(\"photos\") />",
            "<lem-let authors: {string: Author} from \"_data/authors.json\" />",
            "<lem-let siteUrl: string from env(\"SITE_URL\") />",
            "<lem-let from: string from env(\"SITE_URL\") />",
        ] {
            assert_eq!(messages(src), Vec::<String>::new(), "{src}");
        }
    }

    #[test]
    fn keeps_a_capture_named_from_a_block() {
        assert_eq!(messages("<lem-let from>a</lem-let>"), Vec::<String>::new());
    }

    #[test]
    fn requires_a_type_on_a_from_binding() {
        assert_eq!(
            diagnostics("<lem-let posts from \"feed/*/index.md\" />"),
            vec![(
                Span::new(15, 19),
                "a `from` binding needs `: Type`: the type decides what the host loads"
                    .into()
            )]
        );
    }

    #[test]
    fn requires_a_source_after_from() {
        assert_eq!(
            messages("<lem-let photos: PhotoIndex from />"),
            vec!["expected an expression before `/>`"]
        );
    }

    #[test]
    fn takes_the_equals_sign_over_from() {
        assert_eq!(
            messages("<lem-let tils: [Til] from \"til/*.md\" = [] />"),
            vec!["unexpected tokens before `=`"]
        );
    }

    #[test]
    fn reports_what_stands_between_the_type_and_from() {
        assert_eq!(
            messages("<lem-let posts: [Article] loaded from \"feed/*/index.md\" />"),
            vec!["unexpected tokens before `from`"]
        );
    }

    #[test]
    fn reports_a_from_that_is_only_inside_the_type() {
        assert_eq!(
            messages("<lem-let x: [from] />"),
            vec!["expected `from` before `/>`"]
        );
    }

    #[test]
    fn keeps_the_shape_of_a_from_binding_whose_name_the_scanner_displaced() {
        assert_eq!(
            messages("<lem-let @ from \"feed/*.md\" />\n<lem-if a>b</lem-if>\n"),
            vec!["unexpected character in tag"]
        );
    }

    #[test]
    fn stays_silent_about_what_a_scanner_error_displaced_in_a_from_binding() {
        assert_eq!(
            messages("<lem-let posts @ from \"feed/*.md\" />"),
            vec!["unexpected character in tag"]
        );
    }

    #[test]
    fn refuses_from_as_the_type_of_a_from_binding() {
        assert_eq!(
            diagnostics("<lem-let posts: from \"feed/*.md\" />"),
            vec![(Span::new(16, 20), "expected a type, found `from`".into())]
        );
    }

    #[test]
    fn stays_silent_about_what_a_cut_from_binding_was_missing() {
        assert_eq!(
            messages("<lem-let posts: [Article] from\n<p>"),
            vec!["unterminated tag: expected `>` before `<`"]
        );
    }

    #[test]
    fn rejects_a_slashed_access_tag() {
        assert_eq!(
            diagnostics("<lem.x />"),
            vec![(
                Span::new(7, 9),
                "access tags are never slashed: expected `>`".into()
            )]
        );
    }

    #[test]
    fn parses_an_expression_bodied_fn_clean() {
        assert_eq!(
            messages("<lem-fn f(a: int): int = a />"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn tracks_the_nesting_of_an_unsupported_block_builtin() {
        assert_eq!(
            messages("<lem-style><lem-head>a</lem-head></lem-style>"),
            vec![
                "`lem-style` is not supported yet (∆RGQ)",
                "`lem-head` is not supported yet (∆RGQ)"
            ]
        );
    }

    #[test]
    fn keeps_a_self_closed_block_tag_a_block() {
        let slashed = "`lem-if` is a block tag and cannot be self-closed: expected `>`";
        assert_eq!(messages("<lem-if x /><p>a</p></lem-if>"), vec![slashed]);
        assert_eq!(
            messages("<lem-if x /><p>a</p>"),
            vec![
                "unclosed `lem-if`: expected `</lem-if>` before end of input",
                slashed
            ]
        );
        assert_eq!(
            messages("<lem-let x />a</lem-let>"),
            vec!["`lem-let` is a block tag and cannot be self-closed: expected `>`"]
        );
    }

    #[test]
    fn never_adopts_a_wrong_close_tag() {
        assert_eq!(
            messages("<lem-if x>a</lem-for>b</lem-if>"),
            vec!["found `</lem-for>` while `lem-if` is still open: expected `</lem-if>`"]
        );
        assert_eq!(
            messages("<lem-if x>a</lem-for>"),
            vec![
                "unclosed `lem-if`: expected `</lem-if>` before end of input",
                "found `</lem-for>` while `lem-if` is still open: expected `</lem-if>`",
            ]
        );
    }

    #[test]
    fn treats_every_unknown_tag_as_void() {
        let unknown = "unknown tag `lem-iff`; did you mean `lem-if`?";
        assert_eq!(messages("<lem-iff x>a</lem-iff>"), vec![unknown, unknown]);
        assert_eq!(
            messages("<lem-for x in y><lem-iff b>a</lem-iff></lem-for>"),
            vec![unknown, unknown]
        );
        assert_eq!(
            messages("<lem-lett y>a"),
            vec!["unknown tag `lem-lett`; did you mean `lem-let`?"]
        );
    }

    #[test]
    fn reports_an_outer_close_once_at_the_innermost_block() {
        assert_eq!(
            messages("<lem-for a in b><lem-if b><lem-if c>x</lem-for>"),
            vec!["found `</lem-for>` while `lem-if` is still open: expected `</lem-if>`"]
        );
    }

    #[test]
    fn keeps_a_cut_close_naming_an_outer_block_inside_the_inner_one() {
        let cut = "unterminated tag: expected `>` before `<`";
        assert_eq!(
            messages("<lem-for a in b><lem-if c>x</lem-for\n<p>z</p></lem-if></lem-for>"),
            vec![cut]
        );
        assert_eq!(
            messages("<lem-for a in b><lem-if c>x</lem-for\n<p>z</p>"),
            vec![
                "unclosed `lem-for`: expected `</lem-for>` before end of input",
                "unclosed `lem-if`: expected `</lem-if>` before end of input",
                cut,
            ]
        );
    }

    #[test]
    fn adds_nothing_to_a_close_tag_the_scanner_cut() {
        let cut = "unterminated tag: expected `>` before end of input";
        assert_eq!(messages("<lem-if x>a</lem-if"), vec![cut]);
        assert_eq!(
            messages("<lem-if x>a</lem-"),
            vec![
                "unclosed `lem-if`: expected `</lem-if>` before end of input",
                cut
            ]
        );
    }

    #[test]
    fn reports_an_unclosed_list_type_at_the_void_marker() {
        assert_eq!(
            messages("<lem-let x: [int = 1 />"),
            vec!["expected `]`, found `=`"]
        );
    }

    #[test]
    fn does_not_blame_a_cut_head_for_an_unclosed_block() {
        assert_eq!(
            messages("<lem-if x lt\n<p>hi</p>\n"),
            vec!["unterminated tag: expected `>` before `<`"]
        );
        assert_eq!(
            messages("<lem-if x lt\n<p>hi</p></lem-if>"),
            vec!["unterminated tag: expected `>` before `<`"]
        );
        assert_eq!(
            messages("<lem-if x lt\n<p>hi</p></lem-for>"),
            vec![
                "unterminated tag: expected `>` before `<`",
                "found `</lem-for>` while `lem-if` is still open: expected `</lem-if>`",
            ]
        );
    }

    #[test]
    fn allows_one_else_in_a_for_body() {
        assert_eq!(
            messages("<lem-for x in xs><lem-else /></lem-for>"),
            Vec::<String>::new()
        );
        assert_eq!(
            messages("<lem-for x in xs><lem-else /><lem-else /></lem-for>"),
            vec!["duplicate `lem-else`: a block has at most one `lem-else`"]
        );
    }

    #[test]
    fn rejects_a_head_on_else() {
        assert_eq!(
            messages("<lem-if x><lem-else y /></lem-if>"),
            vec!["`lem-else` takes no head"]
        );
    }

    #[test]
    fn names_the_open_block_when_an_inner_close_is_missing() {
        assert_eq!(
            messages("<lem-if a><lem-for x in xs>b</lem-if>"),
            vec!["found `</lem-if>` while `lem-for` is still open: expected `</lem-for>`"]
        );
    }

    #[test]
    fn reports_a_nameless_tag_without_a_suggestion() {
        assert_eq!(messages("<lem->"), vec!["unknown tag `lem-`"]);
    }

    #[test]
    fn requires_a_second_loop_variable_after_a_comma() {
        assert_eq!(
            messages("<lem-for x, in xs></lem-for>"),
            vec!["expected a second loop variable, found `in`"]
        );
    }

    #[test]
    fn accepts_a_trailing_comma_in_a_parameter_list() {
        assert_eq!(
            messages("<lem-fn f(a: int, b: string,)></lem-fn>"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn opens_no_node_for_an_empty_parameter_slot() {
        assert_eq!(
            messages("<lem-fn f(,)></lem-fn>"),
            vec!["expected a parameter name, found `,`"]
        );
        let parse = crate::parse("<lem-fn f(,)></lem-fn>");
        let params = parse
            .syntax()
            .descendants()
            .find(|n| n.kind() == crate::SyntaxKind::Params)
            .expect("params");
        assert_eq!(params.children().count(), 0);
    }

    #[test]
    fn requires_a_type_on_every_parameter() {
        assert_eq!(
            messages("<lem-fn f(a, b: int)></lem-fn>"),
            vec!["expected `: Type`, found `,`"]
        );
    }

    #[test]
    fn reports_junk_inside_a_close_tag() {
        assert_eq!(
            messages("<lem-if x></lem-if y>"),
            vec!["unexpected tokens in close tag"]
        );
    }

    #[test]
    fn stays_silent_about_what_a_cut_head_was_missing() {
        assert_eq!(
            messages("<lem.(a +\n<lem.b>"),
            vec!["unterminated tag: expected `>` before `<`"]
        );
    }

    #[test]
    fn rejects_a_slashed_close_tag() {
        assert_eq!(
            messages("<lem-if x></lem-if />"),
            vec!["close tags are never slashed: expected `>`"]
        );
    }

    #[test]
    fn still_reports_an_unclosed_block_after_an_error_at_its_opener() {
        assert_eq!(
            messages("<lem-style>hello"),
            vec![
                "`lem-style` is not supported yet (∆RGQ)",
                "unclosed `lem-style`: expected `</lem-style>` before end of input",
            ]
        );
    }

    #[test]
    fn tells_an_else_if_inside_a_for_from_a_duplicate_else() {
        assert_eq!(
            messages("<lem-for x in xs><lem-else /><lem-else-if y /></lem-for>"),
            vec!["`lem-else-if` is only allowed inside `lem-if`"]
        );
        assert_eq!(
            messages("<lem-for x in xs><lem-else /><lem-else /></lem-for>"),
            vec!["duplicate `lem-else`: a block has at most one `lem-else`"]
        );
    }

    #[test]
    fn parses_composite_types_clean() {
        for src in [
            "<lem-fn f(xs: [Article])></lem-fn>",
            "<lem-let m: {string: int} = {} />",
            "<lem-fn f(a: int): (int): html></lem-fn>",
            "<lem-fn f(g: (Page, html): html?, h: (): [int]?)></lem-fn>",
        ] {
            assert_eq!(messages(src), Vec::<String>::new(), "{src}");
        }
    }

    #[test]
    fn requires_a_return_type_on_a_function_type() {
        assert_eq!(
            messages("<lem-fn f(g: (int))></lem-fn>"),
            vec!["expected `:` and a return type, found `)`"]
        );
    }

    #[test]
    fn reports_a_missing_type_after_a_bracket_once() {
        assert_eq!(
            messages("<lem-let x: [] = 1 />"),
            vec!["expected a type, found `]`"]
        );
        assert_eq!(
            messages("<lem-let x: {string} = 1 />"),
            vec!["expected `:`, found `}`"]
        );
    }

    #[test]
    fn stops_recursing_into_pathologically_nested_types() {
        let src = format!(
            "<lem-let x: {}int{} = 1 />",
            "[".repeat(1_000),
            "]".repeat(1_000)
        );
        assert_eq!(messages(&src)[0], "type nested too deeply");
    }

    #[test]
    fn stops_recursing_into_pathologically_nested_blocks() {
        let src = format!(
            "{}a{}",
            "<lem-if x>".repeat(50_000),
            "</lem-if>".repeat(50_000)
        );
        let found = messages(&src);
        assert_eq!(found[0], "`lem-if` is nested too deeply");
        assert!(found.iter().all(|m| !m.contains("unclosed")));
    }
}

#[cfg(test)]
mod declaration_tests {
    use crate::parse;

    fn messages(src: &str) -> Vec<String> {
        parse(src)
            .diagnostics
            .into_iter()
            .map(|d| d.message)
            .collect()
    }

    fn clean(src: &str) {
        assert_eq!(messages(src), Vec::<String>::new(), "{src}");
    }

    #[test]
    fn parses_a_type_with_every_field_shape_clean() {
        clean(
            "<lem-type A extends B>\n  a: int\n  b: Photo? by site.photos\n  \
             c: [asset] = []\n  d: int by idx = 1\n\n  e: {string: any}\n</lem-type>",
        );
        clean("<lem-type Empty></lem-type>");
        clean("<lem-type A>\n</lem-type>");
    }

    #[test]
    fn accepts_a_multi_line_default_in_a_field() {
        clean("<lem-type A>\n  xs: [int] = [\n    1,\n    2,\n  ]\n  b: int\n</lem-type>");
    }

    #[test]
    fn reports_one_diagnostic_for_an_unclosed_default() {
        assert_eq!(
            messages("<lem-type A>\n  xs: [int] = [\n  b: int\n</lem-type>"),
            vec!["expected `]`, found `:`"]
        );
    }

    #[test]
    fn requires_a_type_on_every_field() {
        assert_eq!(
            messages("<lem-type A>\n  a\n  b: int\n</lem-type>"),
            vec!["expected `: Type` before the end of the line"]
        );
        assert_eq!(
            messages("<lem-type A>\n  a = 1\n</lem-type>"),
            vec!["expected `: Type`, found `=`"]
        );
    }

    #[test]
    fn reports_a_field_the_close_tag_cuts_short() {
        // Fields mode reaches `</lem-type>` as a matter of course, so a field left
        // incomplete on the close tag's line is the parser's to report — the scanner
        // said nothing about it (invariant 10).
        for (src, expected) in [
            (
                "<lem-type A>x</lem-type>",
                "expected `: Type` before the end of the line",
            ),
            (
                "<lem-type A>\n  a: </lem-type>",
                "expected a type before the end of the line",
            ),
            (
                "<lem-type A>\n  a: [int</lem-type>",
                "expected `]` before the end of the line",
            ),
            (
                "<lem-type A>\n  a: int by</lem-type>",
                "expected an expression before the end of the line",
            ),
            (
                "<lem-type A>\n  a: int =</lem-type>",
                "expected an expression before the end of the line",
            ),
        ] {
            assert_eq!(messages(src), vec![expected.to_owned()], "{src}");
        }
    }

    #[test]
    fn still_says_nothing_about_what_a_cut_type_head_displaced() {
        // The other half: the scanner cut this head and reported it, so the fields
        // that follow are not blamed for what the cut took with it.
        assert_eq!(
            messages("<lem-type A\n  a: int\n</lem-type>"),
            vec![
                "unterminated tag: expected `>` before `<`",
                "unexpected `a` in tag head",
            ]
        );
    }

    #[test]
    fn reports_a_stray_close_angle_inside_a_field() {
        assert_eq!(
            messages("<lem-type A>\n  a: int >\n  b: int\n</lem-type>"),
            vec!["unexpected `>` after the field"]
        );
    }

    #[test]
    fn reports_what_stands_in_a_type_body_besides_fields() {
        let only_fields = "`lem-type` holds only fields, one `name: Type` per line";
        assert_eq!(
            messages("<lem-type A>\n  a: int\n  <lem.x>\n  b: int\n</lem-type>"),
            vec![only_fields, only_fields]
        );
        // The tags after a stray one still nest and close: a wrong close inside
        // the body names the type, not the file.
        assert_eq!(
            messages("<lem-type A>\n<lem-if x>y</lem-type>"),
            vec![
                only_fields,
                "found `</lem-type>` while `lem-if` is still open: expected `</lem-if>`"
            ]
        );
    }

    #[test]
    fn keeps_a_self_closed_type_a_block() {
        assert_eq!(
            messages("<lem-type A />\n  a: int\n</lem-type>"),
            vec!["`lem-type` is a block tag and cannot be self-closed: expected `>`"]
        );
    }

    #[test]
    fn reports_an_unclosed_type_at_its_opener() {
        assert_eq!(
            messages("<lem-type A>\n  a: int\n"),
            vec!["unclosed `lem-type`: expected `</lem-type>` before end of input"]
        );
    }

    #[test]
    fn treats_extends_and_by_as_names_elsewhere() {
        clean("<lem-let by = extends />");
        clean("<lem-let from = 1 />");
        clean("<lem-fn f(by: int, extends: int, from: int): int = by + extends + from />");
    }

    #[test]
    fn requires_a_base_after_extends() {
        assert_eq!(
            messages("<lem-type A extends></lem-type>"),
            vec!["expected a base type name before `>`"]
        );
    }

    #[test]
    fn parses_an_enum_and_reports_its_missing_parts() {
        clean("<lem-enum Size = small | medium | large />");
        assert_eq!(
            messages("<lem-enum Size = small | null />"),
            vec!["`null` is a reserved word and cannot be used as a variant name"]
        );
        assert_eq!(
            messages("<lem-enum = a />"),
            vec!["expected an enum name, found `=`"]
        );
        // The skipped run enters the error state, so the missing `=` goes unsaid.
        assert_eq!(
            messages("<lem-enum Size a | b />"),
            vec!["unexpected tokens before `=`"]
        );
    }

    #[test]
    fn parses_every_pattern_shape_clean() {
        clean(
            "<lem-match x><lem-case Til as t>a</lem-case><lem-case hero>b</lem-case>\
             <lem-case 1>c</lem-case><lem-case \"s\">d</lem-case><lem-case null>e</lem-case>\
             <lem-case true>f</lem-case><lem-default>g</lem-default></lem-match>",
        );
    }

    #[test]
    fn places_cases_and_defaults() {
        assert_eq!(
            messages("<lem-case 1>a</lem-case>"),
            vec!["`lem-case` outside `lem-match`"]
        );
        assert_eq!(
            messages("<lem-default>a</lem-default>"),
            vec!["`lem-default` outside `lem-match`"]
        );
        assert_eq!(
            messages(
                "<lem-match x><lem-default>a</lem-default><lem-case 1>b</lem-case></lem-match>"
            ),
            vec!["`lem-case` cannot follow `lem-default`"]
        );
        assert_eq!(
            messages(
                "<lem-match x><lem-default>a</lem-default><lem-default>b</lem-default></lem-match>"
            ),
            vec!["duplicate `lem-default`: a `lem-match` has at most one `lem-default`"]
        );
    }

    #[test]
    fn rejects_a_head_on_default_and_a_missing_pattern() {
        assert_eq!(
            messages("<lem-match x><lem-default y>a</lem-default></lem-match>"),
            vec!["`lem-default` takes no head"]
        );
        assert_eq!(
            messages("<lem-match x><lem-case>a</lem-case></lem-match>"),
            vec!["expected a pattern before `>`"]
        );
        assert_eq!(
            messages("<lem-match x><lem-case -1>a</lem-case></lem-match>"),
            vec!["expected a pattern, found `-`"]
        );
    }

    #[test]
    fn refuses_content_between_cases_but_not_layout() {
        clean("<lem-match x>\n  <lem-case 1>a</lem-case>\n  \n</lem-match>");
        let only_parts = "`lem-match` holds only `lem-case` and `lem-default`";
        assert_eq!(
            messages("<lem-match x>text<lem-case 1>a</lem-case></lem-match>"),
            vec![only_parts]
        );
        assert_eq!(
            messages("<lem-match x><lem-let y = 1 /></lem-match>"),
            vec![only_parts]
        );
        assert_eq!(
            messages(
                "<lem-match x><lem-if y><lem-case 1>a</lem-case></lem-if></lem-match>"
            ),
            vec![only_parts, "`lem-case` outside `lem-match`"]
        );
    }
}

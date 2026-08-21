use logos::Logos;

use crate::Span;
use crate::scanner::ScanError;

/// What a run of source bytes is.
///
/// One flat enumeration covers both scanner modes, and it doubles as the tag-mode lexer:
/// the `#[token]`/`#[regex]` patterns here are what the scanner runs between a tag's
/// opener and its closing `>`. The text-mode kinds and [`Error`](Self::Error) carry no
/// pattern because the text-mode lexer and the driver produce them instead.
///
/// Kinds carry no payload — a token is its span, and the lexeme (a tag's name, a
/// literal's digits) is recovered by slicing the source, so a [`Token`] stays `Copy` and
/// the CST can reuse it as-is.
#[derive(Logos, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[logos(error = ScanError)]
pub enum TokenKind {
    // --- Text mode ---
    /// A run of opaque text: HTML, prose, anything that is not a Lemmon tag.
    Text,
    /// `<lem.` — an access tag opens; the head follows in tag mode.
    OpenAccess,
    /// `<lem-name` — a keyword tag opens. The lexeme after `<lem-` is the name: any run
    /// of `[A-Za-z0-9_-]`, possibly empty. Whether it names a builtin is the parser's
    /// call, so that it can say which builtin was probably meant.
    OpenTag,
    /// `</lem-name` — a block closes. Named like [`OpenTag`](Self::OpenTag).
    CloseTag,

    // --- Tag mode ---
    /// Whitespace inside a head. Kept so that every byte belongs to a token.
    #[regex(r"[ \t\r\n]+")]
    Trivia,
    /// `>` — the head ends.
    #[token(">")]
    Close,
    /// `/>` — the head ends; the canonical spelling of a void tag.
    #[token("/>")]
    SelfClose,
    /// `[A-Za-z_][A-Za-z0-9_]*`, once keywords are excluded.
    #[regex(r"[A-Za-z_][A-Za-z0-9_]*")]
    Ident,
    /// A `"…"` literal, quotes included, escapes and interpolation unprocessed.
    #[token("\"", crate::scanner::string_literal)]
    StringLit,
    /// `[0-9]+`
    #[regex(r"[0-9]+")]
    Int,
    /// `[0-9]+\.[0-9]+`
    #[regex(r"[0-9]+\.[0-9]+")]
    Float,
    /// `true`
    #[token("true")]
    True,
    /// `false`
    #[token("false")]
    False,
    /// `null`
    #[token("null")]
    Null,
    /// `lt`
    #[token("lt")]
    Lt,
    /// `lte`
    #[token("lte")]
    Lte,
    /// `gt`
    #[token("gt")]
    Gt,
    /// `gte`
    #[token("gte")]
    Gte,
    /// `in`
    #[token("in")]
    In,
    /// `is`
    #[token("is")]
    Is,
    /// `as`
    #[token("as")]
    As,
    /// `(`
    #[token("(")]
    LParen,
    /// `)`
    #[token(")")]
    RParen,
    /// `[`
    #[token("[")]
    LBracket,
    /// `]`
    #[token("]")]
    RBracket,
    /// `{`
    #[token("{")]
    LBrace,
    /// `}`
    #[token("}")]
    RBrace,
    /// `,`
    #[token(",")]
    Comma,
    /// `:`
    #[token(":")]
    Colon,
    /// `.`
    #[token(".")]
    Dot,
    /// `?.`
    #[token("?.")]
    QuestionDot,
    /// `?`
    #[token("?")]
    Question,
    /// `??`
    #[token("??")]
    QuestionQuestion,
    /// `=`
    #[token("=")]
    Eq,
    /// `==`
    #[token("==")]
    EqEq,
    /// `!=`
    #[token("!=")]
    BangEq,
    /// `!`
    #[token("!")]
    Bang,
    /// `+`
    #[token("+")]
    Plus,
    /// `-`
    #[token("-")]
    Minus,
    /// `*`
    #[token("*")]
    Star,
    /// `/`
    #[token("/")]
    Slash,
    /// `%`
    #[token("%")]
    Percent,
    /// `&&`
    #[token("&&")]
    AmpAmp,
    /// `||`
    #[token("||")]
    PipePipe,
    /// `|`
    #[token("|")]
    Pipe,
    /// A `<` inside a head. Never emitted: the scanner treats it as the head having been
    /// left open, reports that, and rescans from the `<` in text mode.
    #[token("<")]
    OpenInHead,

    // --- Either mode ---
    /// Bytes the scanner could not lex. A [`Diagnostic`](crate::Diagnostic) with the same
    /// span says why; the token exists so that the stream stays lossless.
    Error,
}

/// A classified run of source bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    /// What the run is.
    pub kind: TokenKind,
    /// Where it is.
    pub span: Span,
}

impl Token {
    /// Creates a token of `kind` covering `span`.
    #[must_use]
    pub const fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }

    /// The token's bytes, sliced from the source it was scanned from.
    ///
    /// # Panics
    ///
    /// If `src` is not the source this token was scanned from and the span falls
    /// outside it or off a character boundary.
    #[must_use]
    pub fn text<'src>(&self, src: &'src str) -> &'src str {
        &src[self.span.start as usize..self.span.end as usize]
    }

    /// The name of an [`OpenTag`](TokenKind::OpenTag) or [`CloseTag`](TokenKind::CloseTag)
    /// — the part after `<lem-` or `</lem-`, possibly empty. `None` for every other kind.
    #[must_use]
    pub fn tag_name<'src>(&self, src: &'src str) -> Option<&'src str> {
        let prefix = match self.kind {
            TokenKind::OpenTag => "<lem-".len(),
            TokenKind::CloseTag => "</lem-".len(),
            _ => return None,
        };
        Some(&self.text(src)[prefix..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slices_its_text_out_of_the_source() {
        let token = Token::new(TokenKind::Ident, Span::new(5, 8));
        assert_eq!(token.text("<lem.foo>"), "foo");
    }

    #[test]
    fn names_an_open_tag_without_its_prefix() {
        let token = Token::new(TokenKind::OpenTag, Span::new(0, 7));
        assert_eq!(token.tag_name("<lem-if x>"), Some("if"));
    }

    #[test]
    fn names_a_close_tag_without_its_prefix() {
        let token = Token::new(TokenKind::CloseTag, Span::new(0, 8));
        assert_eq!(token.tag_name("</lem-if>"), Some("if"));
    }

    #[test]
    fn names_a_nameless_tag_with_the_empty_string() {
        let token = Token::new(TokenKind::OpenTag, Span::new(0, 5));
        assert_eq!(token.tag_name("<lem->"), Some(""));
    }

    #[test]
    fn has_no_name_for_other_kinds() {
        let token = Token::new(TokenKind::Text, Span::new(0, 3));
        assert_eq!(token.tag_name("abc"), None);
    }
}

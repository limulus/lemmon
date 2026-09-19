//! The scanner: one buffer, two modes — and a third for one tag's body.
//!
//! Text mode treats everything as opaque text until it sees `<lem.`, `<lem-`, or `</lem-`.
//! Tag mode then lexes the head — the expression language — up to the first `>` or `/>`
//! outside a string literal, and hands back to text mode. The switch is a logos
//! [`morph`](Lexer::morph): both modes are lexers over the same source, and the driver
//! below moves one cursor between them. The tag-mode patterns live on [`TokenKind`]
//! itself; the text-mode lexer is the private [`TextMode`] here.
//!
//! One body is not text: a `<lem-type …>` block holds field declarations, one per line,
//! written in the expression language. Its opener's name alone decides that (invariant
//! 4, D29), so after a `lem-type` head closes the scanner stays in the tag-mode
//! vocabulary — *fields mode* — with two differences: a line ending is a
//! [`Newline`](TokenKind::Newline) token rather than trivia, because it ends a field,
//! and a `<` ends the mode without a diagnostic and rescans as text, so that the block's
//! `</lem-type>` is lexed as any close tag is and anything else standing there is the
//! parser's to report. A line ending inside open brackets stays trivia, so a default
//! can be laid out one element per line like any literal (D24).
//!
//! The output is lossless: every byte of the source belongs to exactly one token —
//! whitespace inside a head is [`Trivia`](TokenKind::Trivia), bytes that cannot be lexed
//! are [`Error`](TokenKind::Error) tokens with a matching [`Diagnostic`]. Concatenating
//! the tokens' text reproduces the source, and the property tests at the bottom hold the
//! scanner to it.
//!
//! Recovery is local by construction: a `<` inside a head ends the head (it is reported
//! as unterminated and the `<` is rescanned as text), and a string literal ends at the
//! end of its line at the latest. So one missing `>` or `"` costs one diagnostic, not
//! the rest of the file.

use logos::{Lexer, Logos};

use crate::{Diagnostic, Span, Token, TokenKind, index_u32};

/// Why a run of bytes became an [`Error`](TokenKind::Error) token.
///
/// Public only because it is the logos error type of the public [`TokenKind`]; it is
/// not re-exported, and the scanner turns every value into a [`Diagnostic`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScanError {
    /// Nothing in tag mode starts with this character.
    #[default]
    UnexpectedCharacter,
    /// `"` with no closing `"` before the end of the line.
    UnterminatedString,
}

impl ScanError {
    const fn message(self) -> &'static str {
        match self {
            Self::UnexpectedCharacter => "unexpected character in tag",
            Self::UnterminatedString => "unterminated string literal: expected `\"`",
        }
    }
}

#[derive(Logos, Clone, Copy, Debug, PartialEq, Eq)]
#[logos(error = ScanError)]
enum TextMode {
    // A lone `<` is text too; the driver coalesces adjacent runs, so `<lemon>` comes out
    // as one token even though it lexes as `<` + `lemon>`.
    #[regex(r"[^<]+")]
    #[token("<")]
    Text,
    #[token("<lem.")]
    OpenAccess,
    // The name is deliberately loose — `<lem-If`, `<lem-1`, `<lem--`, even a bare `<lem-`
    // all open a tag here — so that the parser, which owns the builtin table, can reject
    // a bad name with a did-you-mean instead of the scanner losing it as text.
    #[regex(r"<lem-[A-Za-z0-9_-]*")]
    OpenTag,
    #[regex(r"</lem-[A-Za-z0-9_-]*")]
    CloseTag,
}

impl TextMode {
    const fn kind(self) -> TokenKind {
        match self {
            Self::Text => TokenKind::Text,
            Self::OpenAccess => TokenKind::OpenAccess,
            Self::OpenTag => TokenKind::OpenTag,
            Self::CloseTag => TokenKind::CloseTag,
        }
    }
}

/// Consumes a string literal after its opening quote: up to the closing quote, or to
/// the end of the line if there is none, so that a missing quote cannot swallow the
/// rest of the file. A backslash escapes the next character, whatever it is; decoding
/// escapes and splitting interpolation are the parser's job, since the interpolation
/// delimiter is still a deferred question.
pub(crate) fn string_literal(lex: &mut Lexer<TokenKind>) -> Result<(), ScanError> {
    let rest = lex.remainder();
    let mut chars = rest.char_indices();
    while let Some((at, c)) = chars.next() {
        match c {
            '\\' if !matches!(chars.clone().next(), Some((_, '\n' | '\r'))) => {
                chars.next();
            }
            '"' => {
                lex.bump(at + 1);
                return Ok(());
            }
            // A bare `\r` counts as a line end too, so that CRLF sources never get
            // the `\r` inside an unterminated string's span.
            '\n' | '\r' => {
                lex.bump(at);
                return Err(ScanError::UnterminatedString);
            }
            _ => {}
        }
    }
    lex.bump(rest.len());
    Err(ScanError::UnterminatedString)
}

/// Everything the scanner produced for one source file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScanResult {
    /// The tokens, in source order, covering every byte of the source exactly once.
    pub tokens: Vec<Token>,
    /// Problems found on the way, sorted by span. Empty means the source lexed cleanly.
    pub diagnostics: Vec<Diagnostic>,
}

impl ScanResult {
    fn push(&mut self, kind: TokenKind, span: Span) {
        self.tokens.push(Token::new(kind, span));
    }

    fn push_text(&mut self, span: Span) {
        match self.tokens.last_mut() {
            Some(last) if last.kind == TokenKind::Text && last.span.end == span.start => {
                last.span.end = span.end;
            }
            _ => self.push(TokenKind::Text, span),
        }
    }

    fn push_error(&mut self, error: ScanError, span: Span) {
        self.push(TokenKind::Error, span);
        self.diagnostics
            .push(Diagnostic::new(error.message(), span));
    }

    /// Pushes a run of head whitespace as fields mode sees it: each `\n` or `\r\n` is
    /// its own [`Newline`](TokenKind::Newline), whatever lies between them is trivia.
    fn push_line_trivia(&mut self, text: &str, span: Span) {
        // Head whitespace is ASCII, so byte offsets are character offsets here.
        let bytes = text.as_bytes();
        let sub = |from: usize, to: usize| {
            Span::new(span.start + index_u32(from), span.start + index_u32(to))
        };
        let (mut run, mut at) = (0, 0);
        while at < bytes.len() {
            let newline = match bytes[at] {
                b'\n' => 1,
                b'\r' if bytes.get(at + 1) == Some(&b'\n') => 2,
                // A lone `\r` is whitespace like a space; only `\n` ends a line.
                _ => {
                    at += 1;
                    continue;
                }
            };
            if run < at {
                self.push(TokenKind::Trivia, sub(run, at));
            }
            self.push(TokenKind::Newline, sub(at, at + newline));
            at += newline;
            run = at;
        }
        if run < bytes.len() {
            self.push(TokenKind::Trivia, sub(run, bytes.len()));
        }
    }
}

/// How a `lem-type` body stopped.
enum FieldsEnd {
    /// At a `<`, at this absolute byte offset, which belongs to text mode — the
    /// block's own close tag, or something the parser will report.
    Reopened(u32),
    Eof,
}

/// How a tag head stopped.
enum HeadEnd {
    /// At a `>` or `/>`, as it should.
    Closed,
    /// At a `<`, which starts at this absolute byte offset and belongs to text mode.
    Reopened(u32),
    /// At the end of the input.
    Eof,
}

/// Scans `src` into tokens.
///
/// Never fails: anything that cannot be lexed becomes an [`Error`](TokenKind::Error)
/// token plus a diagnostic, and scanning continues, so a half-typed template still yields
/// a complete token stream for the parser to recover over.
///
/// # Panics
///
/// If `src` is longer than a [`Span`] can address (4 GiB).
#[must_use]
pub fn scan(src: &str) -> ScanResult {
    assert!(
        u32::try_from(src.len()).is_ok(),
        "source exceeds the 4 GiB a Span can address"
    );
    // Checked above, so the narrowing casts below cannot truncate. `base` is the absolute
    // offset of the slice the current lexers run over: zero, until a `<` inside a head
    // forces a fresh text-mode lexer on the remainder.
    let span_of = |base: usize, range: std::ops::Range<usize>| {
        Span::new((base + range.start) as u32, (base + range.end) as u32)
    };
    let eof = src.len() as u32;

    let mut out = ScanResult::default();
    let mut base = 0;
    let mut text = TextMode::lexer(src);
    while let Some(result) = text.next() {
        let span = span_of(base, text.span());
        let fields = match result {
            Ok(TextMode::Text) => {
                out.push_text(span);
                continue;
            }
            Ok(opener) => {
                out.push(opener.kind(), span);
                // The one body that is not text: decided by the opener's name alone.
                opener == TextMode::OpenTag && &text.slice()["<lem-".len()..] == "type"
            }
            Err(error) => {
                out.push_error(error, span);
                continue;
            }
        };

        let mut tag = text.morph::<TokenKind>();
        let end = loop {
            let Some(result) = tag.next() else {
                break HeadEnd::Eof;
            };
            let span = span_of(base, tag.span());
            match result {
                Ok(kind @ (TokenKind::Close | TokenKind::SelfClose)) => {
                    out.push(kind, span);
                    break HeadEnd::Closed;
                }
                Ok(TokenKind::OpenInHead) => break HeadEnd::Reopened(span.start),
                Ok(kind) => out.push(kind, span),
                Err(error) => out.push_error(error, span),
            }
        };
        match end {
            HeadEnd::Closed if fields => {
                // A line ending inside open brackets is whitespace, so that a default
                // can be a multi-line literal (D24); only a line ending at depth zero
                // ends a field.
                let mut depth = 0usize;
                let end = loop {
                    let Some(result) = tag.next() else {
                        break FieldsEnd::Eof;
                    };
                    let span = span_of(base, tag.span());
                    match result {
                        Ok(TokenKind::Trivia) if depth == 0 => {
                            out.push_line_trivia(tag.slice(), span);
                        }
                        Ok(TokenKind::OpenInHead) => break FieldsEnd::Reopened(span.start),
                        Ok(kind) => {
                            match kind {
                                TokenKind::LParen
                                | TokenKind::LBracket
                                | TokenKind::LBrace => depth += 1,
                                TokenKind::RParen
                                | TokenKind::RBracket
                                | TokenKind::RBrace => depth = depth.saturating_sub(1),
                                _ => {}
                            }
                            out.push(kind, span);
                        }
                        Err(error) => out.push_error(error, span),
                    }
                };
                match end {
                    FieldsEnd::Reopened(at) => {
                        base = at as usize;
                        text = TextMode::lexer(&src[base..]);
                    }
                    FieldsEnd::Eof => break,
                }
            }
            HeadEnd::Closed => text = tag.morph(),
            HeadEnd::Reopened(at) => {
                out.diagnostics.push(Diagnostic::new(
                    "unterminated tag: expected `>` before `<`",
                    Span::new(span.start, at),
                ));
                base = at as usize;
                text = TextMode::lexer(&src[base..]);
            }
            HeadEnd::Eof => {
                out.diagnostics.push(Diagnostic::new(
                    "unterminated tag: expected `>` before end of input",
                    Span::new(span.start, eof),
                ));
                break;
            }
        }
    }
    // A head's own diagnostic is known only once the head has ended, after any errors
    // inside it; sorting restores the documented span order.
    out.diagnostics.sort_by_key(|d| (d.span.start, d.span.end));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spans::{all_of, at};
    use TokenKind::*;

    fn lex(src: &str) -> Vec<(TokenKind, &str)> {
        scan(src)
            .tokens
            .iter()
            .map(|t| (t.kind, t.text(src)))
            .collect()
    }

    fn kinds(src: &str) -> Vec<TokenKind> {
        scan(src)
            .tokens
            .iter()
            .map(|t| t.kind)
            .filter(|k| *k != Trivia)
            .collect()
    }

    fn diagnostics(src: &str) -> Vec<Diagnostic> {
        scan(src).diagnostics
    }

    fn assert_clean(src: &str) {
        assert_eq!(
            diagnostics(src),
            vec![],
            "expected no diagnostics for {src:?}"
        );
    }

    #[test]
    fn treats_html_as_one_text_run() {
        let src = "<p class=\"x\">Hello, <em>world</em>!</p>\n";
        assert_eq!(lex(src), vec![(Text, src)]);
    }

    #[test]
    fn treats_a_bare_lem_element_as_text() {
        assert_eq!(lex("<lem>hi</lem>"), vec![(Text, "<lem>hi</lem>")]);
    }

    #[test]
    fn treats_elements_that_merely_start_with_lem_as_text() {
        assert_eq!(lex("<lemon><lemma>"), vec![(Text, "<lemon><lemma>")]);
    }

    #[test]
    fn treats_html_comments_as_text() {
        assert_eq!(lex("<!-- <lem> -->"), vec![(Text, "<!-- <lem> -->")]);
    }

    #[test]
    fn treats_an_entity_escaped_trigger_as_text() {
        assert_eq!(lex("&lt;lem.x>"), vec![(Text, "&lt;lem.x>")]);
    }

    #[test]
    fn scans_nothing_for_empty_input() {
        assert_eq!(scan(""), ScanResult::default());
    }

    #[test]
    fn opens_an_access_tag_and_lexes_its_head() {
        assert_eq!(
            lex("<lem.article.title>"),
            vec![
                (OpenAccess, "<lem."),
                (Ident, "article"),
                (Dot, "."),
                (Ident, "title"),
                (Close, ">"),
            ]
        );
        assert_clean("<lem.article.title>");
    }

    #[test]
    fn opens_a_keyword_tag_with_its_name() {
        let result = scan("<lem-else-if x>");
        assert_eq!(result.tokens[0].kind, OpenTag);
        assert_eq!(
            result.tokens[0].tag_name("<lem-else-if x>"),
            Some("else-if")
        );
        assert_eq!(
            lex("<lem-else-if x>"),
            vec![
                (OpenTag, "<lem-else-if"),
                (Trivia, " "),
                (Ident, "x"),
                (Close, ">")
            ]
        );
    }

    #[test]
    fn closes_a_block_tag_bare() {
        assert_eq!(lex("</lem-fn>"), vec![(CloseTag, "</lem-fn"), (Close, ">")]);
        assert_clean("</lem-fn>");
    }

    #[test]
    fn lexes_a_void_tag_with_a_self_close() {
        assert_eq!(
            lex("<lem-let x = 1 />"),
            vec![
                (OpenTag, "<lem-let"),
                (Trivia, " "),
                (Ident, "x"),
                (Trivia, " "),
                (Eq, "="),
                (Trivia, " "),
                (Int, "1"),
                (Trivia, " "),
                (SelfClose, "/>"),
            ]
        );
    }

    #[test]
    fn returns_to_text_mode_after_a_head() {
        assert_eq!(
            lex("a<lem.x>b<lem.y>c"),
            vec![
                (Text, "a"),
                (OpenAccess, "<lem."),
                (Ident, "x"),
                (Close, ">"),
                (Text, "b"),
                (OpenAccess, "<lem."),
                (Ident, "y"),
                (Close, ">"),
                (Text, "c"),
            ]
        );
    }

    #[test]
    fn preserves_text_around_tags_as_written() {
        assert_eq!(
            lex("  <lem-if x>\n  <lem-else />\n</lem-if>\n"),
            vec![
                (Text, "  "),
                (OpenTag, "<lem-if"),
                (Trivia, " "),
                (Ident, "x"),
                (Close, ">"),
                (Text, "\n  "),
                (OpenTag, "<lem-else"),
                (Trivia, " "),
                (SelfClose, "/>"),
                (Text, "\n"),
                (CloseTag, "</lem-if"),
                (Close, ">"),
                (Text, "\n"),
            ]
        );
    }

    #[test]
    fn leaves_tag_name_validity_to_the_parser() {
        let src = "<lem-If x><lem-if2><lem-if-><lem-_>";
        assert_eq!(
            scan(src)
                .tokens
                .iter()
                .filter_map(|t| t.tag_name(src))
                .collect::<Vec<_>>(),
            vec!["If", "if2", "if-", "_"]
        );
        assert_clean(src);
    }

    #[test]
    fn opens_a_nameless_tag_for_the_parser_to_reject() {
        assert_eq!(lex("<lem->"), vec![(OpenTag, "<lem-"), (Close, ">")]);
        assert_eq!(
            lex("</lem- >"),
            vec![(CloseTag, "</lem-"), (Trivia, " "), (Close, ">")]
        );
        assert_clean("<lem-></lem- >");
    }

    #[test]
    fn scans_a_would_be_comment_as_a_tag_named_dash() {
        let src = "<lem-- x -->";
        assert_eq!(scan(src).tokens[0].tag_name(src), Some("-"));
        assert_eq!(
            lex(src),
            vec![
                (OpenTag, "<lem--"),
                (Trivia, " "),
                (Ident, "x"),
                (Trivia, " "),
                (Minus, "-"),
                (Minus, "-"),
                (Close, ">"),
            ]
        );
    }

    #[test]
    fn ends_a_head_at_the_first_close_outside_a_string() {
        assert_eq!(
            lex(r#"<lem.f("a > b", x)>"#),
            vec![
                (OpenAccess, "<lem."),
                (Ident, "f"),
                (LParen, "("),
                (StringLit, "\"a > b\""),
                (Comma, ","),
                (Trivia, " "),
                (Ident, "x"),
                (RParen, ")"),
                (Close, ">"),
            ]
        );
    }

    #[test]
    fn keeps_escaped_quotes_inside_a_string() {
        assert_eq!(
            lex(r#"<lem.("say \"hi\" \\")>"#),
            vec![
                (OpenAccess, "<lem."),
                (LParen, "("),
                (StringLit, r#""say \"hi\" \\""#),
                (RParen, ")"),
                (Close, ">"),
            ]
        );
    }

    #[test]
    fn keeps_multibyte_characters_inside_a_string() {
        assert_eq!(
            lex("<lem.(\"héllo \\é\")>"),
            vec![
                (OpenAccess, "<lem."),
                (LParen, "("),
                (StringLit, "\"héllo \\é\""),
                (RParen, ")"),
                (Close, ">"),
            ]
        );
    }

    #[test]
    fn ends_an_unterminated_string_at_the_end_of_its_line() {
        let src = "<lem-let t = \"Hello\n<p>x</p>\n<lem.y>\n";
        assert_eq!(
            lex(src),
            vec![
                (OpenTag, "<lem-let"),
                (Trivia, " "),
                (Ident, "t"),
                (Trivia, " "),
                (Eq, "="),
                (Trivia, " "),
                (Error, "\"Hello"),
                (Trivia, "\n"),
                (Text, "<p>x</p>\n"),
                (OpenAccess, "<lem."),
                (Ident, "y"),
                (Close, ">"),
                (Text, "\n"),
            ]
        );
        assert_eq!(
            diagnostics(src),
            vec![
                Diagnostic::new(
                    "unterminated tag: expected `>` before `<`",
                    at(src, "<lem-let t = \"Hello\n")
                ),
                Diagnostic::new(
                    "unterminated string literal: expected `\"`",
                    at(src, "\"Hello")
                ),
            ]
        );
    }

    #[test]
    fn ends_an_unterminated_string_before_a_carriage_return() {
        assert_eq!(
            lex("<lem.(\"abc\r\n)>"),
            vec![
                (TokenKind::OpenAccess, "<lem."),
                (TokenKind::LParen, "("),
                (TokenKind::Error, "\"abc"),
                (TokenKind::Trivia, "\r\n"),
                (TokenKind::RParen, ")"),
                (TokenKind::Close, ">"),
            ]
        );
    }

    #[test]
    fn does_not_let_a_backslash_escape_a_carriage_return() {
        assert_eq!(lex("<lem.(\"a\\\r\n)>")[2], (TokenKind::Error, "\"a\\"));
    }

    #[test]
    fn does_not_let_a_backslash_escape_the_end_of_the_line() {
        assert_eq!(lex("<lem.(\"a\\\n")[2], (Error, "\"a\\"));
    }

    #[test]
    fn reports_an_unterminated_string_at_the_end_of_input() {
        let src = "<lem.f(\"abc>";
        assert_eq!(
            lex(src),
            vec![
                (OpenAccess, "<lem."),
                (Ident, "f"),
                (LParen, "("),
                (Error, "\"abc>")
            ]
        );
        assert_eq!(
            diagnostics(src),
            vec![
                Diagnostic::new(
                    "unterminated tag: expected `>` before end of input",
                    all_of(src)
                ),
                Diagnostic::new(
                    "unterminated string literal: expected `\"`",
                    at(src, "\"abc>")
                ),
            ]
        );
    }

    #[test]
    fn distinguishes_word_operators_from_identifiers() {
        assert_eq!(
            kinds("<lem.(a lt b lte c gt d gte e in f is G as h)>")
                .into_iter()
                .filter(|kind| *kind != Ident)
                .collect::<Vec<_>>(),
            vec![
                OpenAccess, LParen, Lt, Lte, Gt, Gte, In, Is, As, RParen, Close
            ]
        );
        assert_eq!(
            lex("<lem.ltx>"),
            vec![(OpenAccess, "<lem."), (Ident, "ltx"), (Close, ">")]
        );
    }

    #[test]
    fn lexes_literal_keywords() {
        assert_eq!(
            lex("<lem.(true||false??null)>"),
            vec![
                (OpenAccess, "<lem."),
                (LParen, "("),
                (True, "true"),
                (PipePipe, "||"),
                (False, "false"),
                (QuestionQuestion, "??"),
                (Null, "null"),
                (RParen, ")"),
                (Close, ">"),
            ]
        );
    }

    #[test]
    fn lexes_ints_and_floats_separately() {
        assert_eq!(
            lex("<lem.(42+1.5)>"),
            vec![
                (OpenAccess, "<lem."),
                (LParen, "("),
                (Int, "42"),
                (Plus, "+"),
                (Float, "1.5"),
                (RParen, ")"),
                (Close, ">"),
            ]
        );
    }

    #[test]
    fn has_no_datetime_literal() {
        assert_eq!(
            kinds("<lem.(2024-11-24)>"),
            vec![
                OpenAccess, LParen, Int, Minus, Int, Minus, Int, RParen, Close
            ]
        );
    }

    #[test]
    fn lexes_a_lambda_head() {
        assert_eq!(
            kinds("<lem.(xs.sortBy(|a| a.date))>"),
            vec![
                OpenAccess, LParen, Ident, Dot, Ident, LParen, Pipe, Ident, Pipe, Ident,
                Dot, Ident, RParen, RParen, Close,
            ]
        );
    }

    #[test]
    fn lexes_compound_punctuation_greedily() {
        assert_eq!(
            kinds("<lem.(a?.b?c==d:e!=!f&&g%h*i/j-k)>")
                .into_iter()
                .filter(|kind| *kind != Ident)
                .collect::<Vec<_>>(),
            vec![
                OpenAccess,
                LParen,
                QuestionDot,
                Question,
                EqEq,
                Colon,
                BangEq,
                Bang,
                AmpAmp,
                Percent,
                Star,
                Slash,
                Minus,
                RParen,
                Close,
            ]
        );
    }

    #[test]
    fn lexes_a_slash_that_is_not_a_self_close() {
        assert_eq!(
            kinds("<lem.(a / b)>"),
            vec![OpenAccess, LParen, Ident, Slash, Ident, RParen, Close]
        );
    }

    #[test]
    fn lexes_list_and_map_literals() {
        assert_eq!(
            kinds("<lem.f([a, b], {k: v})>"),
            vec![
                OpenAccess, Ident, LParen, LBracket, Ident, Comma, Ident, RBracket, Comma,
                LBrace, Ident, Colon, Ident, RBrace, RParen, Close,
            ]
        );
    }

    #[test]
    fn treats_newlines_inside_a_head_as_trivia() {
        assert_eq!(
            lex("<lem-fn f(\n  a: A,\n  b: B\n)>")
                .into_iter()
                .filter(|(kind, _)| *kind == Trivia)
                .map(|(_, text)| text)
                .collect::<Vec<_>>(),
            vec![" ", "\n  ", " ", "\n  ", " ", "\n"]
        );
    }

    #[test]
    fn reports_an_unexpected_character_and_carries_on() {
        let src = "<lem.(a @ b)>";
        assert_eq!(
            lex(src)
                .into_iter()
                .filter(|(kind, _)| *kind != Trivia)
                .collect::<Vec<_>>(),
            vec![
                (OpenAccess, "<lem."),
                (LParen, "("),
                (Ident, "a"),
                (Error, "@"),
                (Ident, "b"),
                (RParen, ")"),
                (Close, ">"),
            ]
        );
        assert_eq!(
            diagnostics(src),
            vec![Diagnostic::new("unexpected character in tag", at(src, "@"))]
        );
    }

    #[test]
    fn reports_an_unexpected_multibyte_character_as_one_error() {
        assert_eq!(lex("<lem.é>")[1], (Error, "é"));
    }

    #[test]
    fn ends_a_head_at_a_nested_open_and_rescans_it() {
        let src = "<lem.<lem.x>";
        assert_eq!(
            lex(src),
            vec![
                (OpenAccess, "<lem."),
                (OpenAccess, "<lem."),
                (Ident, "x"),
                (Close, ">")
            ]
        );
        assert_eq!(
            diagnostics(src),
            vec![Diagnostic::new(
                "unterminated tag: expected `>` before `<`",
                at(src, "«<lem.»<lem.x>")
            )]
        );
    }

    #[test]
    fn does_not_let_a_missing_close_swallow_the_following_html() {
        let src = "<lem-if a.b\n<p>hello</p>";
        assert_eq!(
            lex(src),
            vec![
                (OpenTag, "<lem-if"),
                (Trivia, " "),
                (Ident, "a"),
                (Dot, "."),
                (Ident, "b"),
                (Trivia, "\n"),
                (Text, "<p>hello</p>"),
            ]
        );
        assert_eq!(
            diagnostics(src),
            vec![Diagnostic::new(
                "unterminated tag: expected `>` before `<`",
                at(src, "<lem-if a.b\n")
            )]
        );
    }

    #[test]
    fn reports_a_head_left_open_at_end_of_input() {
        let src = "text <lem-if a.b";
        assert_eq!(
            lex(src),
            vec![
                (Text, "text "),
                (OpenTag, "<lem-if"),
                (Trivia, " "),
                (Ident, "a"),
                (Dot, "."),
                (Ident, "b"),
            ]
        );
        assert_eq!(
            diagnostics(src),
            vec![Diagnostic::new(
                "unterminated tag: expected `>` before end of input",
                at(src, "<lem-if a.b")
            )]
        );
    }

    #[test]
    fn scans_the_canonical_example_cleanly() {
        let src = r#"<lem-fn articleCard(article: Article)>
  <article class="article-card">
    <a href="<lem.article.url>">
      <lem-let preview = article.image ?? article.hero />
      <lem-if preview>
        <lem.picture(preview, [md], class: "preview")>
      <lem-else-if article.vod />
        <img src="<lem.article.vod.poster>" alt="<lem.article.vod.alt>" class="preview">
      </lem-if>
      <div class="text-content">
        <header>
          <h1><lem.article.title></h1>
          <lem-let chips>
            <li><lem.authorChip(article, nolink: true)></li>
            <li><lem.publishedChip(article)></li>
            <lem-match article>
              <lem-case LinkPost as l><li><lem.linkChip(l, nolink: true)></li></lem-case>
              <lem-case Til as t><li><lem.tilChip(t.topic, nolink: true)></li></lem-case>
              <lem-default><li><lem.timeChip(article)></li></lem-default>
            </lem-match>
          </lem-let>
          <lem.chipList(size: small, slot: chips)>
        </header>
        <p><lem.(article.subhead ?? article.teaser ?? "")></p>
      </div>
    </a>
  </article>
  <lem-style>
    .article-card {
      > a { display: block; overflow: hidden; }
      img.preview { display: block; width: 100%; height: auto; }
    }
  </lem-style>
</lem-fn>
"#;
        let result = scan(src);
        assert_eq!(result.diagnostics, vec![]);
        assert!(!result.tokens.iter().any(|t| t.kind == Error));
        let rebuilt: String = result.tokens.iter().map(|t| t.text(src)).collect();
        assert_eq!(rebuilt, src);
        let opens = result
            .tokens
            .iter()
            .filter(|t| t.kind == OpenTag)
            .filter_map(|t| t.tag_name(src))
            .collect::<Vec<_>>();
        assert_eq!(
            opens,
            vec![
                "fn", "let", "if", "else-if", "let", "match", "case", "case", "default",
                "style",
            ]
        );
    }

    #[test]
    fn lexes_a_type_body_as_fields_with_newlines_ending_them() {
        let src =
            "<lem-type A>\n  title: string\n  hero: Photo? by site.photos\n</lem-type>";
        assert_eq!(
            lex(src),
            vec![
                (OpenTag, "<lem-type"),
                (Trivia, " "),
                (Ident, "A"),
                (Close, ">"),
                (Newline, "\n"),
                (Trivia, "  "),
                (Ident, "title"),
                (Colon, ":"),
                (Trivia, " "),
                (Ident, "string"),
                (Newline, "\n"),
                (Trivia, "  "),
                (Ident, "hero"),
                (Colon, ":"),
                (Trivia, " "),
                (Ident, "Photo"),
                (Question, "?"),
                (Trivia, " "),
                (Ident, "by"),
                (Trivia, " "),
                (Ident, "site"),
                (Dot, "."),
                (Ident, "photos"),
                (Newline, "\n"),
                (CloseTag, "</lem-type"),
                (Close, ">"),
            ]
        );
        assert_clean(src);
    }

    #[test]
    fn splits_blank_lines_and_crlf_endings_into_one_newline_each() {
        assert_eq!(
            lex("<lem-type A>\r\n\r\n  a: int \r\n</lem-type>"),
            vec![
                (OpenTag, "<lem-type"),
                (Trivia, " "),
                (Ident, "A"),
                (Close, ">"),
                (Newline, "\r\n"),
                (Newline, "\r\n"),
                (Trivia, "  "),
                (Ident, "a"),
                (Colon, ":"),
                (Trivia, " "),
                (Ident, "int"),
                (Trivia, " "),
                (Newline, "\r\n"),
                (CloseTag, "</lem-type"),
                (Close, ">"),
            ]
        );
    }

    #[test]
    fn treats_a_lone_carriage_return_in_a_type_body_as_trivia() {
        assert_eq!(
            kinds("<lem-type A>\r a: int\n"),
            vec![OpenTag, Ident, Close, Ident, Colon, Ident, Newline]
        );
        assert_eq!(lex("<lem-type A>\r a: int\n")[4], (Trivia, "\r "));
    }

    #[test]
    fn leaves_fields_mode_at_any_open_angle_without_a_diagnostic() {
        let src = "<lem-type A>\n  a: int\n<p>x</p>\n</lem-type>";
        assert_eq!(
            kinds(src),
            vec![
                OpenTag, Ident, Close, Newline, Ident, Colon, Ident, Newline, Text,
                CloseTag, Close
            ]
        );
        assert_clean(src);
        assert_eq!(
            kinds("<lem-type A>\n<lem.x>"),
            vec![OpenTag, Ident, Close, Newline, OpenAccess, Ident, Close]
        );
    }

    #[test]
    fn stays_in_fields_mode_across_a_stray_close_angle() {
        // `>` never ends a field — it is the parser's to report — so a mistaken one
        // does not turn the rest of the body into text.
        assert_eq!(
            kinds("<lem-type A>\n  a: int >\n  b: int\n</lem-type>"),
            vec![
                OpenTag, Ident, Close, Newline, Ident, Colon, Ident, Close, Newline, Ident,
                Colon, Ident, Newline, CloseTag, Close
            ]
        );
    }

    #[test]
    fn reports_a_type_body_reaching_the_end_of_input_only_through_the_parser() {
        assert_eq!(
            kinds("<lem-type A>\n  a: int"),
            vec![OpenTag, Ident, Close, Newline, Ident, Colon, Ident]
        );
        assert_clean("<lem-type A>\n  a: int");
    }

    #[test]
    fn enters_fields_mode_only_for_the_exact_tag_name() {
        // `lem-Type` is no builtin: the parser rejects it, and its body is text.
        assert_eq!(
            kinds("<lem-Type A>\n  a: int\n"),
            vec![OpenTag, Ident, Close, Text]
        );
        assert_eq!(
            kinds("<lem-type A\n  a: int\n"),
            vec![OpenTag, Ident, Ident, Colon, Ident]
        );
    }

    #[test]
    fn keeps_a_self_closed_type_head_in_fields_mode() {
        // The name decides the shape (D21): the parser reports the `/>`, and the body
        // is still fields.
        assert_eq!(
            kinds("<lem-type A />\n  a: int\n</lem-type>"),
            vec![
                OpenTag, Ident, SelfClose, Newline, Ident, Colon, Ident, Newline, CloseTag,
                Close
            ]
        );
    }

    #[test]
    fn keeps_line_endings_inside_brackets_as_trivia_in_a_type_body() {
        let src = "<lem-type A>\n  xs: [int] = [\n    1,\n  ]\n  b: int\n</lem-type>";
        assert_eq!(
            kinds(src),
            vec![
                OpenTag, Ident, Close, Newline, Ident, Colon, LBracket, Ident, RBracket,
                Eq, LBracket, Int, Comma, RBracket, Newline, Ident, Colon, Ident, Newline,
                CloseTag, Close
            ]
        );
        assert_eq!(lex(src)[16], (Trivia, "\n    "));
    }

    #[test]
    fn lets_an_unclosed_bracket_swallow_the_lines_after_it() {
        // Depth never goes negative, so a stray closer cannot end a line early either.
        assert_eq!(
            kinds("<lem-type A>\n  a: int = [\n  b: int\n</lem-type>"),
            vec![
                OpenTag, Ident, Close, Newline, Ident, Colon, Ident, Eq, LBracket, Ident,
                Colon, Ident, CloseTag, Close
            ]
        );
        assert_eq!(
            kinds("<lem-type A>\n  a: int = ]\n  b: int\n</lem-type>"),
            vec![
                OpenTag, Ident, Close, Newline, Ident, Colon, Ident, Eq, RBracket, Newline,
                Ident, Colon, Ident, Newline, CloseTag, Close
            ]
        );
    }

    #[test]
    fn reports_scanner_errors_inside_a_type_body() {
        let src = "<lem-type A>\n  a: @\n</lem-type>";
        assert_eq!(
            diagnostics(src),
            vec![Diagnostic::new("unexpected character in tag", at(src, "@"))]
        );
    }

    mod properties {
        use super::*;
        use proptest::prelude::*;

        /// Source text biased towards the scanner's decision points: the triggers, their
        /// near misses, delimiters, and escapes, plus arbitrary characters.
        fn source() -> impl Strategy<Value = String> {
            let fragment = prop_oneof![
                Just("<lem.".to_string()),
                Just("<lem-".to_string()),
                Just("<lem-type".to_string()),
                Just("</lem-".to_string()),
                Just("<lem".to_string()),
                Just("<".to_string()),
                Just(">".to_string()),
                Just("/>".to_string()),
                Just("\"".to_string()),
                Just("\\".to_string()),
                Just(" ".to_string()),
                Just("\n".to_string()),
                "[a-z]{1,4}",
                "[0-9]{1,4}",
                "[-:T.?=!&|(){}\\[\\],+*/%@]",
                any::<char>().prop_map(String::from),
            ];
            prop::collection::vec(fragment, 0..24).prop_map(|fragments| fragments.concat())
        }

        proptest! {
            #[test]
            fn covers_every_byte_exactly_once_in_order(src in source()) {
                let result = scan(&src);
                let mut cursor = 0;
                for token in &result.tokens {
                    prop_assert_eq!(token.span.start, cursor, "gap or overlap before {:?}", token);
                    prop_assert!(token.span.end > token.span.start, "empty token {:?}", token);
                    cursor = token.span.end;
                }
                prop_assert_eq!(cursor as usize, src.len());
                let rebuilt: String = result.tokens.iter().map(|t| t.text(&src)).collect();
                prop_assert_eq!(rebuilt, src);
            }

            #[test]
            fn pairs_every_error_token_with_a_diagnostic_at_its_span(src in source()) {
                let result = scan(&src);
                for token in result.tokens.iter().filter(|t| t.kind == Error) {
                    prop_assert!(
                        result.diagnostics.iter().any(|d| d.span == token.span),
                        "no diagnostic for {:?}", token
                    );
                }
            }

            #[test]
            fn sorts_diagnostics_by_span(src in source()) {
                let spans: Vec<(u32, u32)> = scan(&src)
                    .diagnostics
                    .iter()
                    .map(|d| (d.span.start, d.span.end))
                    .collect();
                let mut sorted = spans.clone();
                sorted.sort_unstable();
                prop_assert_eq!(spans, sorted);
            }

            #[test]
            fn never_emits_a_placeholder_kind(src in source()) {
                let result = scan(&src);
                prop_assert!(!result.tokens.iter().any(|t| t.kind == OpenInHead));
            }

            #[test]
            fn emits_a_newline_only_as_one_line_ending(src in source()) {
                let result = scan(&src);
                for token in result.tokens.iter().filter(|t| t.kind == Newline) {
                    prop_assert!(matches!(token.text(&src), "\n" | "\r\n"), "{:?}", token);
                }
            }

            #[test]
            fn emits_a_newline_only_inside_a_type_body(
                src in source().prop_filter("no type tag", |s| !s.contains("<lem-type"))
            ) {
                let result = scan(&src);
                prop_assert!(!result.tokens.iter().any(|t| t.kind == Newline));
            }

            #[test]
            fn passes_text_without_a_trigger_through_as_one_run(
                src in source().prop_filter("no trigger", |s| {
                    !s.contains("<lem.") && !s.contains("<lem-") && !s.contains("</lem-")
                })
            ) {
                let result = scan(&src);
                prop_assert_eq!(&result.diagnostics, &vec![]);
                if src.is_empty() {
                    prop_assert!(result.tokens.is_empty());
                } else {
                    prop_assert_eq!(result.tokens.len(), 1);
                    prop_assert_eq!(result.tokens[0].kind, Text);
                    prop_assert_eq!(result.tokens[0].text(&src), src.as_str());
                }
            }
        }
    }
}

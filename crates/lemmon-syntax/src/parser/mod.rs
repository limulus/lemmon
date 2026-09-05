//! The parser: recursive descent over tags, Pratt over expressions, on a rowan tree.
//!
//! The parser consumes the scanner's tokens — it never re-lexes — and records what it
//! finds as a flat list of [`Event`]s: start a node, take a token, finish a node. A
//! final pass folds the events into a rowan green tree, emitting the trivia and
//! scanner-error tokens that the parser itself skips over in between, so that the tree
//! owns every byte of the source. Recording events rather than building the tree
//! directly is what lets a Pratt loop wrap an already-parsed operand in a `BinaryExpr`
//! after the fact ([`Parser::precede`]).
//!
//! Recovery is deliberately shallow and never guesses: an unexpected token sequence is
//! wrapped in an `Error` node with one diagnostic, a missing token is reported where it
//! should have been and parsing carries on as if it were there. After a diagnostic
//! about the current token the parser is in an *error state* until it consumes a
//! token, and further diagnostics about that same token are dropped as restatements —
//! which keeps one mistake at one diagnostic even when several nested constructs are
//! all cut short by the same `>`. A diagnostic anchored elsewhere (an unclosed block,
//! reported at its opener) is a separate mistake: it is always kept and never enters
//! the error state. The policies for what a tag's shape is when its terminator or
//! close contradicts it are listed in `docs/language.md` (§Scanner, parser rules) and
//! implemented in [`template`].

mod expr;
mod template;

use rowan::{GreenNode, GreenNodeBuilder};

use crate::ast::{AstNode, Root};
use crate::syntax_kind::{SyntaxKind, SyntaxNode};
use crate::{Diagnostic, Span, Token, TokenKind, scan};

/// The result of parsing one source file: a lossless tree plus every diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parse {
    /// The green (position-free, shareable) tree. [`syntax`](Self::syntax) wraps it.
    pub green: GreenNode,
    /// Scanner and parser diagnostics merged, sorted by span.
    pub diagnostics: Vec<Diagnostic>,
}

impl Parse {
    /// The root of the red tree, positioned at offset zero.
    #[must_use]
    pub fn syntax(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green.clone())
    }

    /// The typed view of the whole file.
    #[must_use]
    pub fn root(&self) -> Root {
        Root::cast(self.syntax()).expect("the parser always emits a `Root`")
    }
}

/// Parses `src` into a lossless syntax tree.
///
/// Never fails and never panics on any input: malformed source yields a tree that still
/// covers every byte, plus diagnostics that each point at the bytes at fault.
///
/// # Panics
///
/// If `src` is longer than a [`Span`] can address (4 GiB), as [`scan`] does.
#[must_use]
pub fn parse(src: &str) -> Parse {
    let scanned = scan(src);
    let mut parser = Parser::new(src, &scanned.tokens);
    template::root(&mut parser);
    let (green, mut diagnostics) = parser.finish();
    diagnostics.extend(scanned.diagnostics);
    diagnostics.sort_by_key(|d| (d.span.start, d.span.end));
    Parse { green, diagnostics }
}

/// Parses only the item at the start of `src` — a tag, since document mode calls this
/// at a `<lem` it found — into a `Root` holding that one item and nothing after it.
///
/// This is how document mode answers "where does this lem-block end?". The segmenter
/// finds where a block *starts* by looking at lines; how far it runs is a question only
/// the grammar can answer, and the root's text length is that answer. Delegating it
/// here is what keeps the segmenter from growing a tag matcher of its own — a second
/// thing that would have to know void shapes from block ones and how closes match, and
/// the first place document mode and template mode would drift apart. Asking for one
/// item is well defined because a tag's shape is decidable from its head alone
/// (invariant 4), so nothing past the item can change what the item is.
///
/// Diagnostics are limited to that range: the scanner lexes all of `src`, but what it
/// found past the item is prose that was never meant to lex as Lemmon.
pub(crate) fn parse_item_prefix(src: &str) -> Parse {
    let scanned = scan(src);
    let mut parser = Parser::new(src, &scanned.tokens);
    template::item_prefix(&mut parser);
    let (green, mut diagnostics) = parser.finish();
    let end = green.text_len().into();
    diagnostics.extend(
        scanned
            .diagnostics
            .into_iter()
            .filter(|d| d.span.start < end),
    );
    diagnostics.sort_by_key(|d| (d.span.start, d.span.end));
    Parse { green, diagnostics }
}

/// Renders a tree and its diagnostics in the bake-off's dump format.
///
/// A test and debugging aid, not an API: one node or token per line, indented two
/// spaces per depth, then a blank line, `--- diagnostics`, and one diagnostic per line.
/// Takes a [`Parse`] rather than source so that a document's tree — assembled by
/// [`parse_document`](crate::parse_document), not parsed — dumps the same way.
#[must_use]
pub fn dump(parse: &Parse) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    for event in parse.syntax().preorder_with_tokens() {
        let element = match event {
            rowan::WalkEvent::Enter(element) => element,
            rowan::WalkEvent::Leave(_) => {
                depth -= 1;
                continue;
            }
        };
        let range = element.text_range();
        out.push_str(&"  ".repeat(depth));
        out.push_str(&element.kind().name());
        out.push_str(&format!(
            "@{}..{}",
            u32::from(range.start()),
            u32::from(range.end())
        ));
        if let rowan::NodeOrToken::Token(token) = element {
            out.push_str(&format!(" {}", quoted(token.text())));
        }
        out.push('\n');
        depth += 1;
    }
    out.push_str("\n--- diagnostics\n");
    for diagnostic in &parse.diagnostics {
        let Span { start, end } = diagnostic.span;
        out.push_str(&format!("{start}..{end}: {}\n", diagnostic.message));
    }
    out
}

/// Renders the tag boundaries of a tree: every tag interior elided.
///
/// This is the agreement target invariant 9 names — what the future `tree-sitter-lemmon`
/// grammar has to reproduce for every corpus case. A context-free grammar can say where
/// a tag begins and ends; it cannot be held to the shape the Pratt parser gave a head.
/// So heads, expressions, and the `Block` wrapper are dropped and their tag descendants
/// lifted, while text runs stay.
///
/// `Error` is kept, as a node or a token, but only where items stand — directly under a
/// `Root` or a `Block`. There it marks bytes the parser could place in neither a tag nor
/// text, which a grammar does have to account for; inside a head it is tag interior like
/// any expression, and elided with the rest.
///
/// The projection is not a partition of the file: trivia between items belongs to no
/// boundary, and a void tag whose head the scanner cut ends at the last token its head
/// consumed rather than at the cut. Those gaps are visible in `tests/corpus/**/*.tags`
/// and are the parser's shape, not the projection's.
#[must_use]
pub fn tag_dump(parse: &Parse) -> String {
    let mut out = String::new();
    project(&parse.syntax(), 0, false, &mut out);
    out
}

/// Whether an element of `kind` is a tag boundary rather than tag interior — the
/// elements [`tag_dump`] keeps.
///
/// `in_items` says whether the element stands where items run, directly under a `Root`
/// or a `Block`. That is the only place an `Error` is a boundary, and it is why this
/// takes more than a kind: the same `Error`, node or token, is tag interior inside a
/// head. No other boundary is position-dependent.
fn is_boundary(kind: SyntaxKind, in_items: bool) -> bool {
    matches!(
        kind,
        SyntaxKind::Root
            | SyntaxKind::TextRun
            | SyntaxKind::AccessTag
            | SyntaxKind::IfTag
            | SyntaxKind::ElseIfTag
            | SyntaxKind::ElseTag
            | SyntaxKind::ForTag
            | SyntaxKind::LetVoid
            | SyntaxKind::LetBlock
            | SyntaxKind::LetFrom
            | SyntaxKind::FnBlock
            | SyntaxKind::FnVoid
            | SyntaxKind::TypeDecl
            | SyntaxKind::EnumDecl
            | SyntaxKind::MatchTag
            | SyntaxKind::CaseTag
            | SyntaxKind::DefaultTag
            | SyntaxKind::UnsupportedTag
            | SyntaxKind::UnknownTag
            | SyntaxKind::CloseTag
    ) || (in_items && kind == SyntaxKind::Error)
}

fn project(node: &SyntaxNode, depth: usize, in_items: bool, out: &mut String) {
    let depth = if is_boundary(node.kind(), in_items) {
        line(out, depth, node.kind(), node.text_range());
        depth + 1
    } else {
        depth
    };
    let items = matches!(node.kind(), SyntaxKind::Root | SyntaxKind::Block);
    for child in node.children_with_tokens() {
        match child {
            rowan::NodeOrToken::Node(child) => project(&child, depth, items, out),
            rowan::NodeOrToken::Token(token) => {
                if is_boundary(token.kind(), items) {
                    line(out, depth, token.kind(), token.text_range());
                }
            }
        }
    }
}

fn line(out: &mut String, depth: usize, kind: SyntaxKind, range: rowan::TextRange) {
    out.push_str(&"  ".repeat(depth));
    out.push_str(&kind.name());
    out.push_str(&format!(
        "@{}..{}\n",
        u32::from(range.start()),
        u32::from(range.end())
    ));
}

/// JSON-escapes `text` for the dump, truncating at 40 bytes (on a character boundary)
/// with an ellipsis.
fn quoted(text: &str) -> String {
    const LIMIT: usize = 40;
    let mut cut = LIMIT.min(text.len());
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    let mut out = String::from('"');
    for c in text[..cut].chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
    }
    out.push('"');
    if cut < text.len() {
        out.push('…');
    }
    out
}

/// What the parser recorded, folded into a tree by [`Parser::finish`].
#[derive(Debug)]
enum Event {
    /// Begin a node. `forward_parent`, when set, is the index of a later `Start` event
    /// whose node must open *before* this one — how a completed operand gets wrapped
    /// by the operator that follows it.
    Start {
        kind: SyntaxKind,
        forward_parent: Option<usize>,
    },
    /// Take the next significant token from the stream.
    Token,
    /// Close the innermost open node.
    Finish,
    /// A `Start` that was abandoned, or consumed as a forward parent.
    Placeholder,
}

/// An open node, returned by [`Parser::start`]; must be completed or abandoned.
#[must_use]
struct Marker {
    pos: usize,
}

/// A finished node, which a later construct may still [`precede`](Parser::precede).
#[derive(Clone, Copy)]
struct CompletedMarker {
    pos: usize,
}

/// The parser state: a cursor over the tokens and the events recorded so far.
struct Parser<'src> {
    src: &'src str,
    tokens: &'src [Token],
    /// Index of the next significant token; trivia and scanner errors are skipped.
    pos: usize,
    events: Vec<Event>,
    diagnostics: Vec<Diagnostic>,
    /// Token position of the last diagnostic about the current token. Until a token is
    /// consumed the parser is in an error state, and further diagnostics about that
    /// token would only restate the first: one `>` cutting several nested constructs
    /// short is one mistake.
    error_pos: Option<usize>,
    /// Whether a scanner error token was skipped since the last consumed token. The
    /// scanner reported it; whatever the grammar expected in its place goes unsaid,
    /// while diagnostics about the tokens that are present are still reported.
    displaced: bool,
    /// Whether a `lem-type` field is being parsed, so that the constructs a field is
    /// made of end at its line rather than at a head's `>` (D29).
    in_fields: bool,
    /// Current nesting of blocks and bracketed expressions, capped at [`MAX_DEPTH`].
    depth: u32,
}

/// How deeply blocks and expressions may nest before the parser stops recursing. Well
/// inside the default wasm32 stack for both debug and release frames; real templates
/// nest a few dozen levels at most.
const MAX_DEPTH: u32 = 200;

impl<'src> Parser<'src> {
    fn new(src: &'src str, tokens: &'src [Token]) -> Self {
        let mut parser = Self {
            src,
            tokens,
            pos: 0,
            events: Vec::new(),
            diagnostics: Vec::new(),
            error_pos: None,
            displaced: false,
            in_fields: false,
            depth: 0,
        };
        parser.skip_insignificant();
        parser
    }

    /// Trivia and scanner-error tokens are invisible to the grammar; the sink emits
    /// them in place. An error token carries no extra diagnostic — the scanner reported
    /// it — but it marks the parser as [`displaced`](Self::displaced).
    fn skip_insignificant(&mut self) {
        while let Some(token) = self.tokens.get(self.pos)
            && matches!(token.kind, TokenKind::Trivia | TokenKind::Error)
        {
            self.displaced |= token.kind == TokenKind::Error;
            self.pos += 1;
        }
    }

    /// The kind of the current significant token, or `None` at the end of input.
    fn current(&self) -> Option<TokenKind> {
        self.tokens.get(self.pos).map(|t| t.kind)
    }

    /// The kind of the `n`th significant token after the current one.
    fn nth(&self, n: usize) -> Option<TokenKind> {
        self.tokens[self.pos.min(self.tokens.len())..]
            .iter()
            .filter(|t| !matches!(t.kind, TokenKind::Trivia | TokenKind::Error))
            .nth(n)
            .map(|t| t.kind)
    }

    fn at(&self, kind: TokenKind) -> bool {
        self.current() == Some(kind)
    }

    /// The current token's text, or the empty string at the end of input.
    fn current_text(&self) -> &'src str {
        self.tokens.get(self.pos).map_or("", |t| t.text(self.src))
    }

    /// The span of the current token, or a zero-width span at the end of input.
    fn current_span(&self) -> Span {
        self.tokens.get(self.pos).map_or_else(
            || {
                let end = self.tokens.last().map_or(0, |t| t.span.end);
                Span::new(end, end)
            },
            |t| t.span,
        )
    }

    /// Whether the current token ends a head: its `>`/`/>`, or — when the scanner cut
    /// the head at a `<` or the end of input — the text-mode token or EOF after it.
    fn at_head_end(&self) -> bool {
        self.current()
            .is_none_or(|kind| self.is_head_end_kind(kind))
    }

    /// Whether a token of `kind` ends a head (see [`at_head_end`](Self::at_head_end)).
    fn is_head_end_kind(&self, kind: TokenKind) -> bool {
        matches!(kind, TokenKind::Close | TokenKind::SelfClose)
            || SyntaxKind::from(kind).is_text_mode()
    }

    /// Whether the head was cut short by the scanner rather than closed. The scanner
    /// already reported that, so anything the parser was still expecting goes unsaid.
    ///
    /// Inside a field this is never true: fields mode leaves at a `<` as a matter of
    /// course — that is how `</lem-type>` is reached — and the scanner reports nothing,
    /// so a field cut short by the close tag is the parser's to report like any other.
    fn at_cut_head(&self) -> bool {
        !self.in_fields
            && self.at_head_end()
            && !matches!(
                self.current(),
                Some(TokenKind::Close | TokenKind::SelfClose)
            )
    }

    /// Takes the current token into the tree.
    fn bump(&mut self) {
        assert!(self.pos < self.tokens.len(), "bump at end of input");
        self.events.push(Event::Token);
        self.pos += 1;
        self.displaced = false;
        self.skip_insignificant();
    }

    /// Takes the current token if it is `kind`.
    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.at(kind) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Takes the current token if it is `kind`, else reports it missing.
    fn expect(&mut self, kind: TokenKind, what: &str) -> bool {
        if self.eat(kind) {
            return true;
        }
        self.error_expected(what);
        false
    }

    /// Reports that `what` was expected at the current token. Silent when the head was
    /// cut by the scanner or a scanner error token was just skipped: the scanner has
    /// already said so, and this would only name what its mistake displaced.
    fn error_expected(&mut self, what: &str) {
        // A field ends at its line, whether that line ends with a newline or with the
        // `</lem-type>` the body ends at; either way the field is where the mistake is.
        let ends_the_field = self.in_fields && self.at_field_end();
        let message = match self.current() {
            _ if ends_the_field && self.current().is_some() => {
                format!("expected {what} before the end of the line")
            }
            Some(TokenKind::Close | TokenKind::SelfClose) => {
                format!("expected {what} before `{}`", self.current_text())
            }
            Some(_) => format!("expected {what}, found `{}`", self.current_text()),
            None => format!("expected {what} before end of input"),
        };
        self.error_at_current(message);
    }

    /// Reports `message` about the current token. Silent when the head was cut by the
    /// scanner or a scanner error token was just skipped: the scanner has already
    /// said so, and this would only name what its mistake displaced.
    fn error_at_current(&mut self, message: impl Into<String>) {
        if self.at_cut_head() || self.displaced {
            return;
        }
        let span = self.current_span();
        self.error(message, span);
    }

    /// Records a diagnostic. One about the current token is dropped while in the error
    /// state — it would only restate the last one — and otherwise enters it. A
    /// diagnostic anchored elsewhere (an unclosed block, reported at its opener) is a
    /// separate mistake: always kept, and it says nothing about the current token, so
    /// it does not enter the error state either.
    fn error(&mut self, message: impl Into<String>, span: Span) {
        if span.start == self.current_span().start {
            if self.in_error_state() {
                return;
            }
            self.error_pos = Some(self.pos);
        }
        self.diagnostics.push(Diagnostic::new(message, span));
    }

    /// Records a diagnostic about a run of tokens just skipped, up to the current
    /// token. What the grammar wanted next is fallout of that run, so the current
    /// token enters the error state as if the diagnostic were about it.
    fn error_skipped(&mut self, message: impl Into<String>, span: Span) {
        self.diagnostics.push(Diagnostic::new(message, span));
        self.error_pos = Some(self.pos);
    }

    /// Enters one nesting level; `false` when [`MAX_DEPTH`] is reached, in which case
    /// the caller must not recurse and nothing was entered.
    fn enter(&mut self) -> bool {
        if self.depth >= MAX_DEPTH {
            return false;
        }
        self.depth += 1;
        true
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }

    /// Whether a diagnostic was already reported at the current position, so that the
    /// tokens about to be skipped are fallout of that mistake rather than a new one.
    fn in_error_state(&self) -> bool {
        self.error_pos == Some(self.pos)
    }

    /// Whether the tokens about to be skipped are fallout of something already
    /// reported: a diagnostic at the current position, or a scanner error token.
    fn stale(&self) -> bool {
        self.in_error_state() || self.displaced
    }

    /// Whether the tag starting at the current token ends in a scanner cut rather than
    /// a `>` or `/>`.
    fn current_tag_is_cut(&self) -> bool {
        self.tokens[(self.pos + 1).min(self.tokens.len())..]
            .iter()
            .find(|t| self.is_head_end_kind(t.kind))
            .is_none_or(|t| !matches!(t.kind, TokenKind::Close | TokenKind::SelfClose))
    }

    /// Wraps every token up to the end of the head in an `Error` node, with one
    /// diagnostic covering them. Nothing happens when already at the head's end.
    fn skip_to_head_end(&mut self, message: &str) {
        self.skip_while(message, |p| !p.at_head_end());
    }

    /// Whether the current token ends a field: a line ending, or what ends a head the
    /// scanner cut — a text-mode token or the end of input. A `>` does not: fields
    /// mode runs to the block's close tag, so a stray one is a mistake inside the
    /// field, not its end.
    fn at_field_end(&self) -> bool {
        self.current().is_none_or(|kind| {
            kind == TokenKind::Newline || SyntaxKind::from(kind).is_text_mode()
        })
    }

    /// Wraps every token up to the end of the line in an `Error` node, with one
    /// diagnostic covering them. Nothing happens when already at the line's end.
    fn skip_to_field_end(&mut self, message: &str) {
        self.skip_while(message, |p| !p.at_field_end());
    }

    /// Skips to the end of whatever the current construct sits in: a field's line, or
    /// a tag's head. What recovery from inside an expression or a type wants, since
    /// either can stand in both places.
    fn skip_to_end(&mut self, message: &str) {
        if self.in_fields {
            self.skip_to_field_end(message);
        } else {
            self.skip_to_head_end(message);
        }
    }

    /// Parses `f` as part of a `lem-type` field, restoring the previous mode after —
    /// so that a `lem-type` nested in another's body is parsed like any other.
    fn in_fields<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let previous = std::mem::replace(&mut self.in_fields, true);
        let result = f(self);
        self.in_fields = previous;
        result
    }

    fn skip_while(&mut self, message: &str, more: impl Fn(&Self) -> bool) {
        if !more(self) {
            return;
        }
        let stale = self.stale();
        let start = self.current_span().start;
        let marker = self.start();
        let mut end = start;
        while more(self) {
            end = self.current_span().end;
            self.bump();
        }
        marker.complete(self, SyntaxKind::Error);
        if !stale {
            self.error_skipped(message, Span::new(start, end));
        }
    }

    fn start(&mut self) -> Marker {
        let pos = self.events.len();
        self.events.push(Event::Placeholder);
        Marker { pos }
    }

    /// Opens a new node that will wrap the completed one: the sink splices its `Start`
    /// in ahead of `completed`'s. The caller parses the rest of the new node's children
    /// and completes it as usual.
    fn precede(&mut self, completed: CompletedMarker) -> Marker {
        let marker = self.start();
        match &mut self.events[completed.pos] {
            Event::Start { forward_parent, .. } => *forward_parent = Some(marker.pos),
            _ => unreachable!("a completed marker always points at a Start event"),
        }
        marker
    }

    /// Folds the events into a green tree, interleaving the skipped trivia and error
    /// tokens, and returns it with the parser's diagnostics.
    fn finish(self) -> (GreenNode, Vec<Diagnostic>) {
        let mut sink = Sink {
            builder: GreenNodeBuilder::new(),
            src: self.src,
            tokens: self.tokens,
            pos: 0,
        };
        let mut events = self.events;
        let last = events.len() - 1;
        for i in 0..events.len() {
            match std::mem::replace(&mut events[i], Event::Placeholder) {
                Event::Start {
                    kind,
                    forward_parent,
                } => {
                    // Collect the chain of forward parents, outermost last, then open
                    // them outermost first.
                    let mut kinds = vec![kind];
                    let mut next = forward_parent;
                    while let Some(idx) = next {
                        match std::mem::replace(&mut events[idx], Event::Placeholder) {
                            Event::Start {
                                kind,
                                forward_parent,
                            } => {
                                kinds.push(kind);
                                next = forward_parent;
                            }
                            _ => {
                                unreachable!("forward parents always point at Start events")
                            }
                        }
                    }
                    for kind in kinds.into_iter().rev() {
                        if kind != SyntaxKind::Root {
                            sink.flush_insignificant();
                        }
                        sink.builder.start_node(kind.into());
                    }
                }
                Event::Token => sink.token(),
                Event::Finish => {
                    // Trailing trivia belongs to the root, the one node that closes
                    // after the last token.
                    if i == last {
                        sink.flush_insignificant();
                    }
                    sink.builder.finish_node();
                }
                Event::Placeholder => {}
            }
        }
        (sink.builder.finish(), self.diagnostics)
    }
}

impl Marker {
    /// Closes the node as `kind`.
    fn complete(self, parser: &mut Parser<'_>, kind: SyntaxKind) -> CompletedMarker {
        parser.events[self.pos] = Event::Start {
            kind,
            forward_parent: None,
        };
        parser.events.push(Event::Finish);
        CompletedMarker { pos: self.pos }
    }
}

/// Writes events into a green tree, restoring the tokens the grammar skipped.
struct Sink<'src> {
    builder: GreenNodeBuilder<'static>,
    src: &'src str,
    tokens: &'src [Token],
    pos: usize,
}

impl Sink<'_> {
    /// Emits pending trivia and scanner-error tokens at the current depth.
    fn flush_insignificant(&mut self) {
        while let Some(token) = self.tokens.get(self.pos)
            && matches!(token.kind, TokenKind::Trivia | TokenKind::Error)
        {
            self.push(*token);
        }
    }

    fn token(&mut self) {
        self.flush_insignificant();
        let token = self.tokens[self.pos];
        self.push(token);
    }

    fn push(&mut self, token: Token) {
        self.builder
            .token(SyntaxKind::from(token.kind).into(), token.text(self.src));
        self.pos += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_the_source_through_the_tree() {
        let src = "<p><lem.a.b ?? \"x\"></p><lem-if x> y </lem-if>";
        assert_eq!(parse(src).syntax().text().to_string(), src);
    }

    #[test]
    fn merges_scanner_diagnostics_in_span_order() {
        let parse = parse("<lem.(a + )><lem.x @>");
        let starts: Vec<u32> = parse.diagnostics.iter().map(|d| d.span.start).collect();
        let mut sorted = starts.clone();
        sorted.sort_unstable();
        assert_eq!(starts, sorted);
        assert_eq!(parse.diagnostics.len(), 2);
    }

    #[test]
    fn dumps_tokens_with_escaped_and_truncated_text() {
        assert_eq!(quoted("a\"b\\c\nd"), "\"a\\\"b\\\\c\\nd\"");
        let long = "x".repeat(50);
        assert_eq!(quoted(&long), format!("\"{}\"…", "x".repeat(40)));
    }

    #[test]
    fn truncates_on_a_character_boundary() {
        let text = format!("{}é", "x".repeat(39));
        assert_eq!(quoted(&text), format!("\"{}\"…", "x".repeat(39)));
    }

    fn messages(src: &str) -> Vec<String> {
        parse(src)
            .diagnostics
            .into_iter()
            .map(|d| d.message)
            .collect()
    }

    #[test]
    fn stays_silent_about_what_a_scanner_error_displaced() {
        assert_eq!(messages("<lem.x @ y>"), vec!["unexpected character in tag"]);
        assert_eq!(messages("<lem.@>"), vec!["unexpected character in tag"]);
        assert_eq!(
            messages("<lem-for @ in xs></lem-for>"),
            vec!["unexpected character in tag"]
        );
    }

    #[test]
    fn still_reports_the_tokens_present_after_a_scanner_error() {
        assert_eq!(
            messages("<lem-if x @ />a</lem-if>"),
            vec![
                "unexpected character in tag",
                "`lem-if` is a block tag and cannot be self-closed: expected `>`",
            ]
        );
        assert_eq!(
            messages("<lem.x @\n</lem-if>"),
            vec![
                "unterminated tag: expected `>` before `<`",
                "unexpected character in tag",
                "stray `</lem-if>`: no `lem-if` is open",
            ]
        );
        assert_eq!(
            messages("<lem.x.@in>"),
            vec![
                "unexpected character in tag",
                "`in` is a reserved word and cannot be used as a member name",
            ]
        );
        assert_eq!(
            messages("<lem-let x: @[int = 1 />"),
            vec!["unexpected character in tag", "expected `]`, found `=`"]
        );
    }

    #[test]
    fn enters_the_error_state_only_for_a_diagnostic_about_the_current_token() {
        assert_eq!(
            messages("<lem-style /></lem-style>"),
            vec![
                "`lem-style` is not supported yet",
                "`lem-style` is a block tag and cannot be self-closed: expected `>`",
            ]
        );
    }

    #[test]
    fn dumps_an_empty_source_as_a_bare_root() {
        assert_eq!(dump(&parse("")), "Root@0..0\n\n--- diagnostics\n");
    }
}

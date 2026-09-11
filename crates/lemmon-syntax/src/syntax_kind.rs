use crate::TokenKind;

/// Defines [`SyntaxKind`] and, from the same list, the table that maps a raw `u16` back
/// to it — so the two cannot drift, and the reverse mapping needs no `unsafe`.
macro_rules! syntax_kinds {
    ($($(#[$attr:meta])* $name:ident),* $(,)?) => {
        /// Every kind of token and node in a Lemmon syntax tree.
        ///
        /// The token kinds come first and mirror [`TokenKind`] one for one, so that the
        /// scanner's output can be written into the tree unchanged; the node kinds
        /// follow. One flat enumeration is what rowan wants — it stores kinds as a bare
        /// `u16` and asks the [`Language`](rowan::Language) impl below to convert.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[allow(missing_docs, reason = "the token kinds are documented on `TokenKind`")]
        #[repr(u16)]
        pub enum SyntaxKind {
            $($(#[$attr])* $name,)*
        }

        /// Every kind, indexed by its discriminant.
        const ALL_KINDS: &[SyntaxKind] = &[$(SyntaxKind::$name),*];
    };
}

syntax_kinds! {
    // --- Tokens (in `TokenKind` order) ---
    Text,
    OpenAccess,
    OpenTag,
    CloseTagToken,
    Trivia,
    Close,
    SelfClose,
    Ident,
    StringLit,
    Int,
    Float,
    True,
    False,
    Null,
    Lt,
    Lte,
    Gt,
    Gte,
    In,
    Is,
    As,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Colon,
    Dot,
    QuestionDot,
    Question,
    QuestionQuestion,
    Eq,
    EqEq,
    BangEq,
    Bang,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    AmpAmp,
    PipePipe,
    Pipe,
    OpenInHead,
    Newline,
    /// Bytes the scanner could not lex — and, as a node, a run of tokens the parser
    /// skipped while recovering. Nothing is ever dropped from the tree.
    Error,

    // --- Nodes ---
    /// The whole file.
    Root,
    /// An opaque text run.
    TextRun,
    /// `<lem.expr>`
    AccessTag,
    /// `<lem-if cond> … </lem-if>`
    IfTag,
    /// `<lem-else-if cond />` — a separator inside an `IfTag` body.
    ElseIfTag,
    /// `<lem-else />` — a separator inside an `IfTag` or `ForTag` body.
    ElseTag,
    /// `<lem-for x in xs> … </lem-for>`
    ForTag,
    /// `<lem-let name = expr />`
    LetVoid,
    /// `<lem-let name> … </lem-let>`
    LetBlock,
    /// `<lem-let name: Type from source />`
    LetFrom,
    /// `<lem-fn name(params)> … </lem-fn>`
    FnBlock,
    /// `<lem-fn name(params): T = expr />`
    FnVoid,
    /// `<lem-type Name> fields </lem-type>`
    TypeDecl,
    /// `<lem-enum Name = a | b />`
    EnumDecl,
    /// `<lem-match subject> cases </lem-match>`
    MatchTag,
    /// `<lem-case pattern> … </lem-case>` — a block only meaningful inside a `MatchTag`.
    CaseTag,
    /// `<lem-default> … </lem-default>` — likewise.
    DefaultTag,
    /// A builtin outside the parsed subset, skipped opaquely.
    UnsupportedTag,
    /// A `lem-*` name that is no builtin.
    UnknownTag,
    /// Everything between a tag's opener and its `>`.
    Head,
    /// The loop variables of a `lem-for`: `x`, `x, i`, or `k, v`.
    ForBinding,
    /// A parenthesised parameter list.
    Params,
    /// One `name: Type` parameter.
    Param,
    /// `: Type`
    TypeAnnotation,
    /// A bare type name: `int`, `Article`.
    NamedType,
    /// `Type?`
    OptionalType,
    /// `[Type]`
    ListType,
    /// `{Key: Value}`
    MapType,
    /// `(A, B): R`
    FnType,
    /// The body of a `lem-type`: its fields, one per line.
    FieldList,
    /// One `name: Type [by value.field] [= default]` field.
    Field,
    /// The head of a `lem-case`: a literal, `null`, a variant name, or `Type as name`.
    Pattern,
    /// The body of a block tag.
    Block,
    /// `</lem-name>`, wrapping the close token and its `>`.
    CloseTag,
    BinaryExpr,
    UnaryExpr,
    TernaryExpr,
    CallExpr,
    ArgList,
    NamedArg,
    MemberExpr,
    IndexExpr,
    Lambda,
    LambdaParams,
    ListLit,
    MapLit,
    MapEntry,
    Literal,
    NameRef,
    /// `.hero` — an enum variant, resolved against the type expected here.
    VariantRef,
    ParenExpr,
    /// `expr is Type`
    TypeTest,
}

impl SyntaxKind {
    /// Every kind, in discriminant order.
    ///
    /// Exposed for the corpus's coverage gate, which is how invariant 9 ("any change to
    /// syntax lands with corpus coverage") stops being a convention: a kind no corpus
    /// case reaches fails the build.
    pub const ALL: &'static [Self] = ALL_KINDS;

    /// The name the dump format prints for this kind.
    ///
    /// The two kinds that collide with a token's name in the shared vocabulary — the
    /// `Text` node and the `CloseTag` node — keep their agreed names here and carry
    /// distinct identifiers only inside Rust.
    #[must_use]
    pub fn name(self) -> String {
        match self {
            Self::TextRun => "Text".to_owned(),
            Self::CloseTagToken => "CloseTag".to_owned(),
            other => format!("{other:?}"),
        }
    }

    /// Whether this kind is one of the text-mode tokens — the kinds at which a head
    /// that the scanner cut short ends.
    #[must_use]
    pub const fn is_text_mode(self) -> bool {
        matches!(
            self,
            Self::Text | Self::OpenAccess | Self::OpenTag | Self::CloseTagToken
        )
    }
}

impl From<TokenKind> for SyntaxKind {
    fn from(kind: TokenKind) -> Self {
        match kind {
            TokenKind::Text => Self::Text,
            TokenKind::OpenAccess => Self::OpenAccess,
            TokenKind::OpenTag => Self::OpenTag,
            TokenKind::CloseTag => Self::CloseTagToken,
            TokenKind::Trivia => Self::Trivia,
            TokenKind::Close => Self::Close,
            TokenKind::SelfClose => Self::SelfClose,
            TokenKind::Ident => Self::Ident,
            TokenKind::StringLit => Self::StringLit,
            TokenKind::Int => Self::Int,
            TokenKind::Float => Self::Float,
            TokenKind::True => Self::True,
            TokenKind::False => Self::False,
            TokenKind::Null => Self::Null,
            TokenKind::Lt => Self::Lt,
            TokenKind::Lte => Self::Lte,
            TokenKind::Gt => Self::Gt,
            TokenKind::Gte => Self::Gte,
            TokenKind::In => Self::In,
            TokenKind::Is => Self::Is,
            TokenKind::As => Self::As,
            TokenKind::LParen => Self::LParen,
            TokenKind::RParen => Self::RParen,
            TokenKind::LBracket => Self::LBracket,
            TokenKind::RBracket => Self::RBracket,
            TokenKind::LBrace => Self::LBrace,
            TokenKind::RBrace => Self::RBrace,
            TokenKind::Comma => Self::Comma,
            TokenKind::Colon => Self::Colon,
            TokenKind::Dot => Self::Dot,
            TokenKind::QuestionDot => Self::QuestionDot,
            TokenKind::Question => Self::Question,
            TokenKind::QuestionQuestion => Self::QuestionQuestion,
            TokenKind::Eq => Self::Eq,
            TokenKind::EqEq => Self::EqEq,
            TokenKind::BangEq => Self::BangEq,
            TokenKind::Bang => Self::Bang,
            TokenKind::Plus => Self::Plus,
            TokenKind::Minus => Self::Minus,
            TokenKind::Star => Self::Star,
            TokenKind::Slash => Self::Slash,
            TokenKind::Percent => Self::Percent,
            TokenKind::AmpAmp => Self::AmpAmp,
            TokenKind::PipePipe => Self::PipePipe,
            TokenKind::Pipe => Self::Pipe,
            TokenKind::OpenInHead => Self::OpenInHead,
            TokenKind::Newline => Self::Newline,
            TokenKind::Error => Self::Error,
        }
    }
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        Self(kind as u16)
    }
}

/// The rowan language marker for Lemmon syntax trees.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Lemmon {}

impl rowan::Language for Lemmon {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        ALL_KINDS[usize::from(raw.0)]
    }

    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        kind.into()
    }
}

/// A node of the red (positioned, parent-linked) tree.
pub type SyntaxNode = rowan::SyntaxNode<Lemmon>;
/// A token of the red tree.
pub type SyntaxToken = rowan::SyntaxToken<Lemmon>;
/// Either a node or a token of the red tree.
pub type SyntaxElement = rowan::SyntaxElement<Lemmon>;

#[cfg(test)]
mod tests {
    use super::*;
    use rowan::Language;

    #[test]
    fn round_trips_every_kind_through_the_raw_representation() {
        for raw in 0..u16::try_from(ALL_KINDS.len()).expect("fits") {
            let kind = Lemmon::kind_from_raw(rowan::SyntaxKind(raw));
            assert_eq!(Lemmon::kind_to_raw(kind).0, raw);
        }
    }

    #[test]
    fn maps_every_token_kind_onto_a_kind_of_the_same_name() {
        assert_eq!(SyntaxKind::from(TokenKind::Ident), SyntaxKind::Ident);
        assert_eq!(SyntaxKind::from(TokenKind::Error), SyntaxKind::Error);
    }

    #[test]
    fn prints_the_shared_vocabulary_names() {
        assert_eq!(SyntaxKind::TextRun.name(), "Text");
        assert_eq!(SyntaxKind::CloseTagToken.name(), "CloseTag");
        assert_eq!(SyntaxKind::BinaryExpr.name(), "BinaryExpr");
    }
}

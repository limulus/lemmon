//! Views over the template side of the tree: the file, its tags, and their bodies.

use super::{
    AstNode, Expr, Literal, Spanned, TypeAnnotation, ast_enum, ast_node, child, children,
    token, tokens,
};
use crate::{Lemmon, Span, SyntaxKind, SyntaxNode, SyntaxToken};

ast_node!(
    /// The whole file.
    Root,
    Root
);

impl Root {
    /// The file's top-level items, in source order.
    pub fn items(&self) -> impl Iterator<Item = Item> {
        children(&self.syntax)
    }
}

ast_enum!(
    /// One thing that can stand in a body: a run of text or a tag.
    ///
    /// The separators `lem-else-if` and `lem-else` are not items — they surface as the
    /// boundaries of [`IfTag::arms`] and [`ForTag::else_items`] — and recovery `Error`
    /// nodes are skipped: a parse with diagnostics never reaches the stages that walk
    /// items.
    Item {
        /// An opaque text run.
        Text(TextRun),
        /// `<lem.expr>`
        Access(AccessTag),
        /// `<lem-if cond> … </lem-if>`
        If(IfTag),
        /// `<lem-for x in xs> … </lem-for>`
        For(ForTag),
        /// `<lem-let name = expr />`
        LetVoid(LetVoid),
        /// `<lem-let name> … </lem-let>`
        LetBlock(LetBlock),
        /// `<lem-let name: Type from source />`
        LetFrom(LetFrom),
        /// `<lem-fn name(params)> … </lem-fn>`
        FnBlock(FnBlock),
        /// `<lem-fn name(params) = expr />`
        FnVoid(FnVoid),
        /// `<lem-type Name> fields </lem-type>`
        TypeDecl(TypeDecl),
        /// `<lem-enum Name = a | b />`
        EnumDecl(EnumDecl),
        /// `<lem-match subject> cases </lem-match>`
        Match(MatchTag),
        /// A builtin outside the parsed subset.
        Unsupported(UnsupportedTag),
        /// A `lem-*` name that is no builtin.
        Unknown(UnknownTag),
    }
);

ast_node!(
    /// An opaque text run.
    TextRun,
    TextRun
);

impl TextRun {
    /// The text token itself.
    #[must_use]
    pub fn text(&self) -> Option<SyntaxToken> {
        token(&self.syntax, SyntaxKind::Text)
    }
}

ast_node!(
    /// `<lem.expr>`
    AccessTag,
    AccessTag
);

impl AccessTag {
    /// The expression to print or call.
    #[must_use]
    pub fn expr(&self) -> Option<Expr> {
        head_child(&self.syntax)
    }
}

ast_node!(
    /// `<lem-if cond> … </lem-if>`
    IfTag,
    IfTag
);

/// One arm of a `lem-if`: its condition and the items rendered when it holds.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct IfArm {
    /// The arm's condition; `None` for the `lem-else` arm.
    pub condition: Option<Expr>,
    /// The items between this arm's opener and the next separator or the close.
    pub items: Vec<Item>,
    /// From the arm's opener (the `lem-if` head, or the separator) to the end of its
    /// last item.
    pub span: Span,
}

impl IfTag {
    /// The first arm's condition.
    #[must_use]
    pub fn condition(&self) -> Option<Expr> {
        head_child(&self.syntax)
    }

    /// The raw body, separators included.
    #[must_use]
    pub fn block(&self) -> Option<Block> {
        child(&self.syntax)
    }

    /// The arms in source order: the `lem-if` arm, one per `lem-else-if`, then the
    /// `lem-else` arm if any.
    #[must_use]
    pub fn arms(&self) -> Vec<IfArm> {
        let span = self.span();
        let block = self.block();
        let opener_end = block.as_ref().map_or(span.end, |b| b.span().start);
        let mut arms = vec![IfArm {
            condition: self.condition(),
            items: Vec::new(),
            span: Span::new(span.start, opener_end),
        }];
        for node in block.iter().flat_map(|b| b.syntax.children()) {
            if Block::is_separator(node.kind()) {
                arms.push(IfArm {
                    condition: head_child(&node),
                    items: Vec::new(),
                    span: node.text_range().into(),
                });
            } else if let Some(item) = Item::cast(node) {
                let arm = arms.last_mut().expect("at least one arm");
                arm.span.end = item.span().end;
                arm.items.push(item);
            }
        }
        arms
    }
}

ast_node!(
    /// `<lem-for x in xs> … </lem-for>`
    ForTag,
    ForTag
);

impl ForTag {
    /// The loop variables.
    #[must_use]
    pub fn binding(&self) -> Option<ForBinding> {
        head_child(&self.syntax)
    }

    /// The list or map being iterated.
    #[must_use]
    pub fn iterable(&self) -> Option<Expr> {
        head_child(&self.syntax)
    }

    /// The raw body, the `lem-else` separator included.
    #[must_use]
    pub fn block(&self) -> Option<Block> {
        child(&self.syntax)
    }

    /// The items rendered once per element: those before the `lem-else`, if any.
    pub fn items(&self) -> impl Iterator<Item = Item> {
        self.block()
            .into_iter()
            .flat_map(|b| b.syntax.children())
            .take_while(|n| !Block::is_separator(n.kind()))
            .filter_map(Item::cast)
    }

    /// The items rendered when the iterable is empty, if a `lem-else` is present.
    #[must_use]
    pub fn else_items(&self) -> Option<impl Iterator<Item = Item>> {
        let mut nodes = self.block()?.syntax.children();
        nodes.find(|n| Block::is_separator(n.kind()))?;
        Some(nodes.filter_map(Item::cast))
    }
}

ast_node!(
    /// The loop variables of a `lem-for`: `x`, `x, i`, or `k, v`.
    ForBinding,
    ForBinding
);

impl ForBinding {
    /// The first variable: the element, or the key of a map.
    #[must_use]
    pub fn first(&self) -> Option<SyntaxToken> {
        tokens(&self.syntax, SyntaxKind::Ident).next()
    }

    /// The second variable, if written: the index, or the value of a map. Which it is
    /// depends on the iterable's type, so that is the checker's call.
    #[must_use]
    pub fn second(&self) -> Option<SyntaxToken> {
        tokens(&self.syntax, SyntaxKind::Ident).nth(1)
    }
}

ast_node!(
    /// `<lem-let name = expr />`
    LetVoid,
    LetVoid
);

impl LetVoid {
    /// The name being bound.
    #[must_use]
    pub fn name(&self) -> Option<SyntaxToken> {
        head_token(&self.syntax, SyntaxKind::Ident)
    }

    /// The declared type, if written.
    #[must_use]
    pub fn type_annotation(&self) -> Option<TypeAnnotation> {
        head_child(&self.syntax)
    }

    /// The bound expression.
    #[must_use]
    pub fn value(&self) -> Option<Expr> {
        head_child(&self.syntax)
    }
}

ast_node!(
    /// `<lem-let name> … </lem-let>` — binds the rendered body.
    LetBlock,
    LetBlock
);

impl LetBlock {
    /// The name being bound.
    #[must_use]
    pub fn name(&self) -> Option<SyntaxToken> {
        head_token(&self.syntax, SyntaxKind::Ident)
    }

    /// The body whose rendering is bound.
    #[must_use]
    pub fn body(&self) -> Option<Block> {
        child(&self.syntax)
    }
}

ast_node!(
    /// `<lem-let name: Type from source />` — binds a source the host fulfils at load
    /// (D30): a glob, a data file, a plugin's data, an environment value.
    LetFrom,
    LetFrom
);

impl LetFrom {
    /// The name being bound.
    #[must_use]
    pub fn name(&self) -> Option<SyntaxToken> {
        opening_name(&child::<Head>(&self.syntax)?.syntax)
    }

    /// The declared type, which decides what the host loads.
    #[must_use]
    pub fn type_annotation(&self) -> Option<TypeAnnotation> {
        head_child(&self.syntax)
    }

    /// The source after `from`: a string naming a glob or data file, or a call naming
    /// a host source (`plugin("photos")`, `env("SITE_URL")`). The parser reads one
    /// expression in this head, after the keyword, so the head's first expression is
    /// the source whatever the name and type are.
    #[must_use]
    pub fn source(&self) -> Option<Expr> {
        head_child(&self.syntax)
    }
}

ast_node!(
    /// `<lem-fn name(params)> … </lem-fn>`
    FnBlock,
    FnBlock
);

impl FnBlock {
    /// The function's name.
    #[must_use]
    pub fn name(&self) -> Option<SyntaxToken> {
        head_token(&self.syntax, SyntaxKind::Ident)
    }

    /// The parameter list.
    #[must_use]
    pub fn params(&self) -> Option<Params> {
        head_child(&self.syntax)
    }

    /// The declared return type, if written.
    #[must_use]
    pub fn return_type(&self) -> Option<TypeAnnotation> {
        head_child(&self.syntax)
    }

    /// The function's body.
    #[must_use]
    pub fn body(&self) -> Option<Block> {
        child(&self.syntax)
    }
}

ast_node!(
    /// `<lem-fn name(params): T = expr />` — the expression-bodied form.
    FnVoid,
    FnVoid
);

impl FnVoid {
    /// The function's name.
    #[must_use]
    pub fn name(&self) -> Option<SyntaxToken> {
        head_token(&self.syntax, SyntaxKind::Ident)
    }

    /// The parameter list.
    #[must_use]
    pub fn params(&self) -> Option<Params> {
        head_child(&self.syntax)
    }

    /// The declared return type, if written.
    #[must_use]
    pub fn return_type(&self) -> Option<TypeAnnotation> {
        head_child(&self.syntax)
    }

    /// The expression the function returns.
    #[must_use]
    pub fn value(&self) -> Option<Expr> {
        head_child(&self.syntax)
    }
}

ast_node!(
    /// `<lem-type Name> fields </lem-type>` — a record schema.
    TypeDecl,
    TypeDecl
);

impl TypeDecl {
    /// The type's name.
    #[must_use]
    pub fn name(&self) -> Option<SyntaxToken> {
        head_token(&self.syntax, SyntaxKind::Ident)
    }

    /// The base type named after `extends`, if any.
    #[must_use]
    pub fn base(&self) -> Option<SyntaxToken> {
        let head = child::<Head>(&self.syntax)?;
        let mut idents = tokens(&head.syntax, SyntaxKind::Ident);
        idents.find(|t| t.text() == "extends")?;
        idents.next()
    }

    /// The fields, in declaration order.
    pub fn fields(&self) -> impl Iterator<Item = Field> {
        child::<FieldList>(&self.syntax)
            .into_iter()
            .flat_map(|list| children(&list.syntax))
    }
}

ast_node!(FieldList, FieldList);

ast_node!(
    /// One `name: Type [by index] [= default]` field of a `lem-type`.
    Field,
    Field
);

impl Field {
    /// The field's name.
    #[must_use]
    pub fn name(&self) -> Option<SyntaxToken> {
        opening_name(&self.syntax)
    }

    /// The field's type.
    #[must_use]
    pub fn type_annotation(&self) -> Option<TypeAnnotation> {
        child(&self.syntax)
    }

    /// The reference index after `by`, if the field is a reference field.
    #[must_use]
    pub fn index(&self) -> Option<Expr> {
        self.expr_after(|t| t.kind() == SyntaxKind::Ident && t.text() == "by")
    }

    /// The default after `=`, if any: a literal, or a computed default.
    #[must_use]
    pub fn default(&self) -> Option<Expr> {
        self.expr_after(|t| t.kind() == SyntaxKind::Eq)
    }

    /// The first expression after the token `marker` accepts, searching only what
    /// follows the field's type. The name comes before it, so a field *called* `by`
    /// cannot be mistaken for the `by` keyword (`by: string = "Eric"` is a plain
    /// field with a default and no index).
    fn expr_after(&self, marker: impl Fn(&SyntaxToken) -> bool) -> Option<Expr> {
        self.syntax
            .children_with_tokens()
            .skip_while(|e| e.kind() != SyntaxKind::TypeAnnotation)
            .skip_while(|e| !e.as_token().is_some_and(&marker))
            .filter_map(|e| e.into_node())
            .find_map(Expr::cast)
    }
}

ast_node!(
    /// `<lem-enum Name = a | b | c />`
    EnumDecl,
    EnumDecl
);

impl EnumDecl {
    /// The enum's name.
    #[must_use]
    pub fn name(&self) -> Option<SyntaxToken> {
        head_token(&self.syntax, SyntaxKind::Ident)
    }

    /// The variant names, in order: every name after the `=`.
    pub fn variants(&self) -> impl Iterator<Item = SyntaxToken> {
        child::<Head>(&self.syntax)
            .into_iter()
            .flat_map(|head| head.syntax.children_with_tokens())
            .filter_map(|e| e.into_token())
            .skip_while(|t| t.kind() != SyntaxKind::Eq)
            .filter(|t| t.kind() == SyntaxKind::Ident)
    }
}

ast_node!(
    /// `<lem-match subject> cases </lem-match>`
    MatchTag,
    MatchTag
);

impl MatchTag {
    /// The value being matched.
    #[must_use]
    pub fn subject(&self) -> Option<Expr> {
        head_child(&self.syntax)
    }

    /// The raw body.
    #[must_use]
    pub fn block(&self) -> Option<Block> {
        child(&self.syntax)
    }

    /// The cases, in source order.
    pub fn cases(&self) -> impl Iterator<Item = CaseTag> {
        self.block().into_iter().flat_map(|b| children(&b.syntax))
    }

    /// The `lem-default`, if any.
    #[must_use]
    pub fn default(&self) -> Option<DefaultTag> {
        self.block().and_then(|b| child(&b.syntax))
    }
}

ast_node!(
    /// `<lem-case pattern> … </lem-case>`
    CaseTag,
    CaseTag
);

impl CaseTag {
    /// The pattern the subject is matched against.
    #[must_use]
    pub fn pattern(&self) -> Option<Pattern> {
        head_child(&self.syntax)
    }

    /// The items rendered when the pattern matches.
    #[must_use]
    pub fn body(&self) -> Option<Block> {
        child(&self.syntax)
    }
}

ast_node!(
    /// `<lem-default> … </lem-default>`
    DefaultTag,
    DefaultTag
);

impl DefaultTag {
    /// The items rendered when no case matches.
    #[must_use]
    pub fn body(&self) -> Option<Block> {
        child(&self.syntax)
    }
}

ast_node!(
    /// A `lem-case` pattern: a literal (`null` included), a bare variant name, or
    /// `Type as name`.
    Pattern,
    Pattern
);

impl Pattern {
    /// The literal, when the pattern is one.
    #[must_use]
    pub fn literal(&self) -> Option<Literal> {
        child(&self.syntax)
    }

    /// The name, when the pattern is a variant name or a type: its first identifier.
    #[must_use]
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.syntax, SyntaxKind::Ident)
    }

    /// The name bound after `as`, when the pattern is a type test.
    #[must_use]
    pub fn binding(&self) -> Option<SyntaxToken> {
        let mut idents = tokens(&self.syntax, SyntaxKind::Ident);
        idents.next()?;
        token(&self.syntax, SyntaxKind::As)?;
        idents.next()
    }
}

ast_node!(
    /// A builtin outside the parsed subset, kept opaquely.
    UnsupportedTag,
    UnsupportedTag
);

impl UnsupportedTag {
    /// The opener; the tag's name is its text after `<lem-`.
    #[must_use]
    pub fn open_tag(&self) -> Option<SyntaxToken> {
        token(&self.syntax, SyntaxKind::OpenTag)
    }

    /// The body, when the builtin is a block.
    #[must_use]
    pub fn body(&self) -> Option<Block> {
        child(&self.syntax)
    }
}

ast_node!(
    /// A `lem-*` name that is no builtin; always void.
    UnknownTag,
    UnknownTag
);

impl UnknownTag {
    /// The opener; the tag's name is its text after `<lem-`.
    #[must_use]
    pub fn open_tag(&self) -> Option<SyntaxToken> {
        token(&self.syntax, SyntaxKind::OpenTag)
    }
}

ast_node!(
    /// A parenthesised parameter list.
    Params,
    Params
);

impl Params {
    /// The parameters, in order.
    pub fn params(&self) -> impl Iterator<Item = Param> {
        children(&self.syntax)
    }
}

ast_node!(
    /// One `name: Type` parameter.
    Param,
    Param
);

impl Param {
    /// The parameter's name.
    #[must_use]
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.syntax, SyntaxKind::Ident)
    }

    /// The parameter's type.
    #[must_use]
    pub fn type_annotation(&self) -> Option<TypeAnnotation> {
        child(&self.syntax)
    }
}

ast_node!(
    /// The body of a block tag.
    Block,
    Block
);

impl Block {
    /// The body's items, separators excluded.
    pub fn items(&self) -> impl Iterator<Item = Item> {
        children(&self.syntax)
    }

    /// Whether `kind` is a separator inside a block: `lem-else-if` or `lem-else`. The
    /// parser keeps a misplaced separator's kind, so one test serves a `lem-for` body
    /// as well as a `lem-if` body.
    const fn is_separator(kind: SyntaxKind) -> bool {
        matches!(kind, SyntaxKind::ElseIfTag | SyntaxKind::ElseTag)
    }
}

ast_node!(Head, Head);

/// The first child of the tag's head that can be viewed as `N`.
fn head_child<N: AstNode<Language = Lemmon>>(tag: &SyntaxNode) -> Option<N> {
    child::<Head>(tag).and_then(|head| child(&head.syntax))
}

/// The first token of `kind` directly inside the tag's head.
fn head_token(tag: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
    child::<Head>(tag).and_then(|head| token(&head.syntax, kind))
}

/// The name `parent` opens with: its first significant element, when that is an
/// identifier. Where a bare contextual keyword can follow the name — `from` in a
/// source binding's head, `by` in a field — a head that lost its name must not yield
/// the keyword as if it were one.
fn opening_name(parent: &SyntaxNode) -> Option<SyntaxToken> {
    parent
        .children_with_tokens()
        .find(|e| !matches!(e.kind(), SyntaxKind::Trivia | SyntaxKind::Error))?
        .into_token()
        .filter(|t| t.kind() == SyntaxKind::Ident)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::LiteralKind;
    use crate::parse;

    fn first_item(src: &str) -> Item {
        parse(src).root().items().next().expect("an item")
    }

    fn text(token: Option<SyntaxToken>) -> String {
        token.expect("a token").text().to_owned()
    }

    /// The type an annotation names, as written.
    fn annotated(annotation: Option<TypeAnnotation>) -> String {
        let ty = annotation.expect("an annotation").ty().expect("a type");
        ty.syntax().text().to_string()
    }

    fn expr_text(expr: Option<Expr>) -> Option<String> {
        expr.map(|e| e.syntax().text().to_string())
    }

    fn item_texts(items: &[Item]) -> Vec<String> {
        items
            .iter()
            .map(|i| i.syntax().text().to_string())
            .collect()
    }

    fn condition_text(arm: &IfArm) -> Option<String> {
        arm.condition
            .as_ref()
            .map(|e| e.syntax().text().to_string())
    }

    #[test]
    fn lists_the_top_level_items_skipping_error_nodes() {
        let root = parse("a<lem.x></lem-if>b").root();
        let kinds: Vec<SyntaxKind> = root.items().map(|i| i.syntax().kind()).collect();
        assert_eq!(
            kinds,
            vec![
                SyntaxKind::TextRun,
                SyntaxKind::AccessTag,
                SyntaxKind::TextRun
            ]
        );
    }

    #[test]
    fn exposes_a_text_run_as_its_token() {
        let Item::Text(run) = first_item("hello") else {
            panic!("expected text");
        };
        assert_eq!(text(run.text()), "hello");
    }

    #[test]
    fn exposes_the_expression_of_an_access_tag() {
        let Item::Access(tag) = first_item("<lem.a.b>") else {
            panic!("expected an access tag");
        };
        assert!(matches!(tag.expr(), Some(Expr::Member(_))));
    }

    #[test]
    fn splits_an_if_into_arms_at_each_separator() {
        let src = "<lem-if a>1<lem-else-if b />2<lem-else />3</lem-if>";
        let Item::If(tag) = first_item(src) else {
            panic!("expected an if");
        };
        let arms = tag.arms();
        assert_eq!(arms.len(), 3);
        assert_eq!(condition_text(&arms[0]), Some("a".into()));
        assert_eq!(item_texts(&arms[0].items), vec!["1"]);
        assert_eq!(arms[0].span, Span::new(0, 11));
        assert_eq!(condition_text(&arms[1]), Some("b".into()));
        assert_eq!(item_texts(&arms[1].items), vec!["2"]);
        assert_eq!(arms[1].span, Span::new(11, 29));
        assert_eq!(arms[2].condition, None);
        assert_eq!(item_texts(&arms[2].items), vec!["3"]);
        assert_eq!(arms[2].span, Span::new(29, 42));
    }

    #[test]
    fn gives_an_if_without_separators_one_arm_spanning_its_head() {
        let Item::If(tag) = first_item("<lem-if a></lem-if>") else {
            panic!("expected an if");
        };
        let arms = tag.arms();
        assert_eq!(arms.len(), 1);
        assert!(arms[0].items.is_empty());
        assert_eq!(arms[0].span, Span::new(0, 10));
        assert_eq!(tag.block().expect("a block").items().count(), 0);
    }

    #[test]
    fn keeps_an_empty_else_arm_at_its_separator() {
        let Item::If(tag) = first_item("<lem-if a>x<lem-else /></lem-if>") else {
            panic!("expected an if");
        };
        let arms = tag.arms();
        assert_eq!(arms[1].span, Span::new(11, 23));
    }

    #[test]
    fn gives_an_if_with_a_cut_head_one_empty_arm() {
        let Item::If(tag) = first_item("<lem-if a") else {
            panic!("expected an if");
        };
        let arms = tag.arms();
        assert_eq!(arms.len(), 1);
        assert!(arms[0].items.is_empty());
        assert_eq!(arms[0].span, Span::new(0, 9));
    }

    #[test]
    fn separates_a_for_body_from_its_else_body() {
        let src = "<lem-for x, i in xs>a<lem-else />b</lem-for>";
        let Item::For(tag) = first_item(src) else {
            panic!("expected a for");
        };
        let binding = tag.binding().expect("a binding");
        assert_eq!(text(binding.first()), "x");
        assert_eq!(text(binding.second()), "i");
        assert!(matches!(tag.iterable(), Some(Expr::Name(_))));
        assert_eq!(item_texts(&tag.items().collect::<Vec<_>>()), vec!["a"]);
        let else_items: Vec<Item> = tag.else_items().expect("an else").collect();
        assert_eq!(item_texts(&else_items), vec!["b"]);
        assert_eq!(tag.block().expect("a block").items().count(), 2);
    }

    #[test]
    fn reports_no_else_body_when_a_for_has_no_else() {
        let Item::For(tag) = first_item("<lem-for x in xs>a</lem-for>") else {
            panic!("expected a for");
        };
        assert!(tag.binding().expect("a binding").second().is_none());
        assert!(tag.else_items().is_none());
        assert_eq!(tag.items().count(), 1);
    }

    #[test]
    fn treats_a_misplaced_else_if_as_a_separator_in_a_for_body() {
        let src = "<lem-for x in xs>a<lem-else-if y />b</lem-for>";
        let Item::For(tag) = first_item(src) else {
            panic!("expected a for");
        };
        assert_eq!(item_texts(&tag.items().collect::<Vec<_>>()), vec!["a"]);
        let else_items: Vec<Item> = tag.else_items().expect("a separator").collect();
        assert_eq!(item_texts(&else_items), vec!["b"]);
    }

    #[test]
    fn exposes_the_parts_of_a_void_let() {
        let Item::LetVoid(tag) = first_item("<lem-let n: int? = 1 />") else {
            panic!("expected a void let");
        };
        assert_eq!(text(tag.name()), "n");
        assert_eq!(annotated(tag.type_annotation()), "int?");
        assert!(matches!(tag.value(), Some(Expr::Literal(_))));
    }

    #[test]
    fn exposes_the_parts_of_a_from_binding() {
        let src = "<lem-let posts: [Article] from \"feed/*/index.md\" />";
        let Item::LetFrom(tag) = first_item(src) else {
            panic!("expected a from binding");
        };
        assert_eq!(text(tag.name()), "posts");
        assert_eq!(annotated(tag.type_annotation()), "[Article]");
        assert!(matches!(tag.source(), Some(Expr::Literal(_))));
        let Item::LetFrom(tag) = first_item("<lem-let p: P from plugin(\"photos\") />")
        else {
            panic!("expected a from binding");
        };
        assert_eq!(
            expr_text(tag.source()).as_deref(),
            Some("plugin(\"photos\")")
        );
    }

    #[test]
    fn tells_a_binding_named_from_from_the_from_keyword() {
        let Item::LetFrom(tag) = first_item("<lem-let from: string from env(\"X\") />")
        else {
            panic!("expected a from binding");
        };
        assert_eq!(text(tag.name()), "from");
        assert!(matches!(tag.source(), Some(Expr::Call(_))));
    }

    #[test]
    fn leaves_the_name_absent_when_a_from_binding_lost_it() {
        let Item::LetFrom(tag) = first_item("<lem-let : string from env(\"X\") />") else {
            panic!("expected a from binding");
        };
        assert!(tag.name().is_none());
        assert_eq!(annotated(tag.type_annotation()), "string");
        assert!(matches!(tag.source(), Some(Expr::Call(_))));
    }

    #[test]
    fn leaves_the_source_absent_when_nothing_follows_from() {
        let Item::LetFrom(tag) = first_item("<lem-let posts: [Article] from />") else {
            panic!("expected a from binding");
        };
        assert!(tag.source().is_none());
    }

    #[test]
    fn exposes_the_parts_of_a_block_let() {
        let Item::LetBlock(tag) = first_item("<lem-let n>body</lem-let>") else {
            panic!("expected a block let");
        };
        assert_eq!(text(tag.name()), "n");
        assert_eq!(tag.body().expect("a body").items().count(), 1);
    }

    #[test]
    fn exposes_the_signature_of_a_block_fn() {
        let src = "<lem-fn f(a: A, b: B?): html>x</lem-fn>";
        let Item::FnBlock(tag) = first_item(src) else {
            panic!("expected a block fn");
        };
        assert_eq!(text(tag.name()), "f");
        let params: Vec<Param> = tag.params().expect("params").params().collect();
        assert_eq!(params.len(), 2);
        assert_eq!(text(params[1].name()), "b");
        assert_eq!(annotated(params[1].type_annotation()), "B?");
        assert_eq!(annotated(params[0].type_annotation()), "A");
        assert_eq!(annotated(tag.return_type()), "html");
        assert_eq!(tag.body().expect("a body").items().count(), 1);
    }

    #[test]
    fn leaves_the_return_type_absent_when_not_written() {
        let Item::FnBlock(tag) = first_item("<lem-fn f()></lem-fn>") else {
            panic!("expected a block fn");
        };
        assert!(tag.return_type().is_none());
        assert_eq!(tag.params().expect("params").params().count(), 0);
    }

    #[test]
    fn exposes_the_signature_and_value_of_a_void_fn() {
        let Item::FnVoid(tag) = first_item("<lem-fn f(a: int): int = a />") else {
            panic!("expected a void fn");
        };
        assert_eq!(text(tag.name()), "f");
        assert_eq!(tag.params().expect("params").params().count(), 1);
        assert_eq!(annotated(tag.return_type()), "int");
        assert!(matches!(tag.value(), Some(Expr::Name(_))));
    }

    #[test]
    fn exposes_the_parts_of_a_type_declaration() {
        let src = "<lem-type LinkPost extends Article>\n  \
                   hero: Photo? by site.photos\n  \
                   title: string = \"Link: {linkTitle}\"\n  \
                   tags: [string] by site.tags = []\n</lem-type>";
        let Item::TypeDecl(tag) = first_item(src) else {
            panic!("expected a type declaration");
        };
        assert_eq!(text(tag.name()), "LinkPost");
        assert_eq!(text(tag.base()), "Article");
        let fields: Vec<Field> = tag.fields().collect();
        assert_eq!(fields.len(), 3);
        assert_eq!(text(fields[0].name()), "hero");
        assert_eq!(annotated(fields[0].type_annotation()), "Photo?");
        assert_eq!(expr_text(fields[0].index()), Some("site.photos".into()));
        assert_eq!(expr_text(fields[0].default()), None);
        assert_eq!(expr_text(fields[1].index()), None);
        assert_eq!(
            expr_text(fields[1].default()),
            Some("\"Link: {linkTitle}\"".into())
        );
        assert_eq!(expr_text(fields[2].index()), Some("site.tags".into()));
        assert_eq!(expr_text(fields[2].default()), Some("[]".into()));
    }

    #[test]
    fn tells_a_field_named_by_from_the_by_keyword() {
        // `by` is contextual: before the type it is the field's own name.
        let src = "<lem-type A>\n  by: string = \"Eric\"\n</lem-type>";
        let Item::TypeDecl(tag) = first_item(src) else {
            panic!("expected a type declaration");
        };
        let field = tag.fields().next().expect("a field");
        assert_eq!(text(field.name()), "by");
        assert_eq!(field.index(), None);
        assert_eq!(expr_text(field.default()), Some("\"Eric\"".into()));
    }

    #[test]
    fn leaves_a_field_name_absent_when_the_line_lost_it() {
        let src = "<lem-type A>\n  : string by site.tags\n</lem-type>";
        let Item::TypeDecl(tag) = first_item(src) else {
            panic!("expected a type");
        };
        let field = tag.fields().next().expect("a field");
        assert!(field.name().is_none());
        assert_eq!(annotated(field.type_annotation()), "string");
        assert!(field.index().is_some());
    }

    #[test]
    fn leaves_the_base_absent_when_a_type_extends_nothing() {
        let Item::TypeDecl(tag) = first_item("<lem-type A>\n</lem-type>") else {
            panic!("expected a type declaration");
        };
        assert!(tag.base().is_none());
        assert_eq!(tag.fields().count(), 0);
    }

    #[test]
    fn exposes_the_name_and_variants_of_an_enum() {
        let Item::EnumDecl(tag) = first_item("<lem-enum Placement = hero | left />") else {
            panic!("expected an enum");
        };
        assert_eq!(text(tag.name()), "Placement");
        let variants: Vec<String> = tag.variants().map(|t| t.text().to_owned()).collect();
        assert_eq!(variants, vec!["hero", "left"]);
    }

    #[test]
    fn exposes_the_cases_and_default_of_a_match() {
        let src = "<lem-match a>\n  <lem-case Til as t>1</lem-case>\n  \
                   <lem-case hero>2</lem-case>\n  <lem-case null>3</lem-case>\n  \
                   <lem-default>4</lem-default>\n</lem-match>";
        let Item::Match(tag) = first_item(src) else {
            panic!("expected a match");
        };
        assert_eq!(expr_text(tag.subject()), Some("a".into()));
        let cases: Vec<CaseTag> = tag.cases().collect();
        assert_eq!(cases.len(), 3);
        let pattern = cases[0].pattern().expect("a pattern");
        assert_eq!(text(pattern.name()), "Til");
        assert_eq!(text(pattern.binding()), "t");
        assert!(pattern.literal().is_none());
        let pattern = cases[1].pattern().expect("a pattern");
        assert_eq!(text(pattern.name()), "hero");
        assert!(pattern.binding().is_none());
        let pattern = cases[2].pattern().expect("a pattern");
        assert!(pattern.name().is_none());
        assert!(matches!(
            pattern.literal().and_then(|l| l.kind()),
            Some(LiteralKind::Null)
        ));
        assert_eq!(
            item_texts(&cases[2].body().expect("a body").items().collect::<Vec<_>>()),
            vec!["3"]
        );
        let default = tag.default().expect("a default");
        assert_eq!(
            item_texts(&default.body().expect("a body").items().collect::<Vec<_>>()),
            vec!["4"]
        );
        // The cases are not items of the match's block: only its layout text is.
        assert!(
            tag.block()
                .expect("a block")
                .items()
                .all(|i| matches!(i, Item::Text(_)))
        );
    }

    #[test]
    fn leaves_the_default_absent_when_a_match_has_none() {
        let Item::Match(tag) =
            first_item("<lem-match a><lem-case 1>x</lem-case></lem-match>")
        else {
            panic!("expected a match");
        };
        assert!(tag.default().is_none());
        assert_eq!(tag.cases().count(), 1);
    }

    #[test]
    fn exposes_the_opener_of_unsupported_and_unknown_tags() {
        let Item::Unsupported(tag) = first_item("<lem-style>a</lem-style>") else {
            panic!("expected an unsupported tag");
        };
        assert_eq!(text(tag.open_tag()), "<lem-style");
        assert_eq!(tag.body().expect("a body").items().count(), 1);
        let Item::Unsupported(tag) = first_item("<lem-emit styles />") else {
            panic!("expected an unsupported tag");
        };
        assert!(tag.body().is_none());
        let Item::Unknown(tag) = first_item("<lem-iff x>") else {
            panic!("expected an unknown tag");
        };
        assert_eq!(text(tag.open_tag()), "<lem-iff");
    }
}

//! Views over the template side of the tree: the file, its tags, and their bodies.

use super::{AstNode, Expr, Spanned, ast_enum, ast_node, child, children, token, tokens};
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
        /// `<lem-fn name(params)> … </lem-fn>`
        FnBlock(FnBlock),
        /// `<lem-fn name(params) = expr />`
        FnVoid(FnVoid),
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
    /// `<lem-fn name(params) = expr />` — recognised by the parser, not yet supported.
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
    /// `: Type` or `: Type?`
    TypeAnnotation,
    TypeAnnotation
);

impl TypeAnnotation {
    /// The type's name. `None` when the annotation is a composite type, which the
    /// parser keeps as an `Error` node until composite types are supported.
    #[must_use]
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.syntax, SyntaxKind::Ident)
    }

    /// Whether the type carries the `?` optional marker.
    #[must_use]
    pub fn is_optional(&self) -> bool {
        token(&self.syntax, SyntaxKind::Question).is_some()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    fn first_item(src: &str) -> Item {
        parse(src).root().items().next().expect("an item")
    }

    fn text(token: Option<SyntaxToken>) -> String {
        token.expect("a token").text().to_owned()
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
        let ty = tag.type_annotation().expect("a type");
        assert_eq!(text(ty.name()), "int");
        assert!(ty.is_optional());
        assert!(matches!(tag.value(), Some(Expr::Literal(_))));
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
        let ty = params[1].type_annotation().expect("a type");
        assert_eq!(text(ty.name()), "B");
        assert!(ty.is_optional());
        assert!(!params[0].type_annotation().expect("a type").is_optional());
        assert_eq!(
            text(tag.return_type().expect("a return type").name()),
            "html"
        );
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
        assert_eq!(
            text(tag.return_type().expect("a return type").name()),
            "int"
        );
        assert!(matches!(tag.value(), Some(Expr::Name(_))));
    }

    #[test]
    fn names_a_composite_type_as_none() {
        let Item::LetVoid(tag) = first_item("<lem-let xs: [int] = [] />") else {
            panic!("expected a void let");
        };
        assert!(tag.type_annotation().expect("a type").name().is_none());
    }

    #[test]
    fn exposes_the_opener_of_unsupported_and_unknown_tags() {
        let Item::Unsupported(tag) = first_item("<lem-match x>a</lem-match>") else {
            panic!("expected an unsupported tag");
        };
        assert_eq!(text(tag.open_tag()), "<lem-match");
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

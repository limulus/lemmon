//! Lowering of what stands in a body: text runs and tags.
//!
//! What the literal text between them *is* — the line rule and the body rule — belongs
//! to [`whitespace`](super::whitespace), which has run by the time a [`Body`] gets here.

use std::rc::Rc;

use lemmon_syntax::Span;
use lemmon_syntax::ast::{
    BinaryOp, ForTag, Item, LetBlock, LetVoid, LiteralKind, MatchTag, Pattern, Spanned,
};

use super::expr::decode_string;
use super::names::BindingKind;
use super::whitespace::{Arm, Body, Case, Kind, Piece};
use super::{Compiler, Diagnostic, TokenSpan, required, rules};
use crate::bytecode::Op;
use crate::types::Type;

/// A document chunk as emitted: without the line ending a silent tag's sentinel left at
/// its head, when one stands before it.
fn chunk(chunk: &str, after_silent: bool) -> &str {
    if after_silent {
        chunk.strip_prefix('\n').unwrap_or(chunk)
    } else {
        chunk
    }
}

/// Where a piece stands, which decides what may stand there.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Level {
    /// A template's top level, where definitions and `from` bindings stand.
    File,
    /// A document's top level: definitions stand there, `from` bindings never do — a
    /// document is not the site file.
    Document,
    /// Inside a block body.
    Nested,
}

impl Compiler<'_> {
    /// Lowers a body in its own scope.
    pub(super) fn lower_block(&mut self, items: Vec<Item>, kind: Kind) {
        self.push_scope();
        self.lower_body(Body::new(items, kind));
        self.pop_scope();
    }

    fn lower_body(&mut self, body: Body) {
        let level = if body.kind == Kind::File {
            Level::File
        } else {
            Level::Nested
        };
        for piece in body.pieces {
            self.lower_piece(piece, level);
        }
    }

    /// Lowers a document's top level: its prose, rendered to HTML already, around its
    /// lem-blocks. The text runs in the tree are the prose as written and are not
    /// emitted; what stands in their place is the chunk rendered for that gap — which
    /// may exist where no text run does, as an empty last gap still collects the
    /// document's footnotes. Of the line rule only one half applies — a block is alone
    /// on its line by construction, so the only line a silent tag can take with it is
    /// the line ending its sentinel left at the head of the next chunk.
    pub(super) fn lower_document(
        &mut self,
        items: Vec<Item>,
        prose: &[Span],
        chunks: &[String],
    ) {
        self.push_scope();
        let mut block = 0;
        let mut after_silent = false;
        for item in items {
            if let Item::Text(_) = item {
                continue;
            }
            let piece = Piece::build(item);
            let silent = piece.renders_nothing();
            self.emit_chunk(prose[block], chunk(&chunks[block], after_silent));
            self.lower_piece(piece, Level::Document);
            after_silent = silent;
            block += 1;
        }
        self.emit_chunk(prose[block], chunk(&chunks[block], after_silent));
        self.pop_scope();
    }

    fn emit_chunk(&mut self, span: Span, chunk: &str) {
        if !chunk.is_empty() {
            let index = self.intern(chunk);
            self.emit(Op::EmitText { index }, span);
        }
    }

    /// Lowers one piece of a body.
    fn lower_piece(&mut self, piece: Piece, level: Level) {
        match piece {
            Piece::Text { text, span } => {
                if !text.is_empty() {
                    let index = self.intern(&text);
                    self.emit(Op::EmitText { index }, span);
                }
            }
            Piece::Tag { item, .. } => self.lower_item(&item, level),
            Piece::If { arms } => self.lower_if(arms),
            Piece::For {
                tag,
                body,
                otherwise,
            } => self.lower_for(&tag, body, otherwise),
            Piece::Match {
                tag,
                cases,
                default,
            } => self.lower_match(&tag, cases, default),
        }
    }

    /// Lowers one item that is not a text run or a flow construct.
    fn lower_item(&mut self, item: &Item, level: Level) {
        match item {
            Item::Text(_) | Item::If(_) | Item::For(_) | Item::Match(_) => {
                unreachable!("these are pieces of their own")
            }
            Item::Access(tag) => {
                let ty = self.expr(&required(tag.expr()));
                self.require(tag.span(), rules::printable(&ty, self.decls()));
                self.emit(Op::Emit, tag.span());
            }
            Item::LetVoid(tag) => self.lower_let_void(tag, level),
            Item::LetBlock(tag) => self.lower_let_block(tag, level),
            // Parsed by `lemmon-syntax` (∆1vS); the loader (∆iCW) fulfils it, and
            // requiring the file to be the host's site file is the host's to check —
            // the compiler cannot tell. It can tell a nested binding and a document
            // from a template's top level, so those two placements are reported here.
            Item::LetFrom(_) if level == Level::File => {
                self.error(
                    item.span(),
                    "`from` bindings need the loader, which is not here yet (∆iCW)",
                );
            }
            Item::LetFrom(_) => {
                self.error(
                    item.span(),
                    "a `from` binding must be at the top level of a template",
                );
            }
            // Top-level definitions were compiled into their own functions already;
            // they stand in the piece list only so the line rule sees them.
            Item::FnBlock(_) | Item::FnVoid(_) if level != Level::Nested => {}
            Item::FnBlock(_) | Item::FnVoid(_) => {
                self.error(item.span(), "`lem-fn` must be at the top level of a file");
            }
            // Declarations were collected into the index before anything was lowered;
            // like definitions they stand in the piece list only for the line rule.
            Item::TypeDecl(_) | Item::EnumDecl(_) if level != Level::Nested => {}
            Item::TypeDecl(_) => {
                self.error(item.span(), "`lem-type` must be at the top level of a file");
            }
            Item::EnumDecl(_) => {
                self.error(item.span(), "`lem-enum` must be at the top level of a file");
            }
            Item::Unsupported(_) | Item::Unknown(_) => {
                unreachable!(
                    "the parser reports these, and a parse with diagnostics is never compiled"
                )
            }
        }
    }

    fn lower_if(&mut self, arms: Vec<Arm>) {
        let mut ends = Vec::new();
        for arm in arms {
            // A condition on an optional local narrows it for the arm it guards.
            let depth = self.narrowings.len();
            let next = arm.condition.as_ref().map(|condition| {
                let (_, narrowing) = self.lower_condition(condition);
                self.narrowings.extend(narrowing);
                self.emit_jump(Op::JumpIfFalse { to: 0 }, condition.span())
            });
            self.push_scope();
            self.lower_body(arm.body);
            self.pop_scope();
            self.narrowings.truncate(depth);
            match next {
                Some(next) => {
                    ends.push(self.emit_jump(Op::Jump { to: 0 }, arm.span));
                    self.patch(next);
                }
                // The `lem-else` arm: nothing to skip past.
                None => break,
            }
        }
        for end in ends {
            self.patch(end);
        }
    }

    /// Lowers a `lem-match` (D35): the subject once, into a slot no name reaches, then
    /// each case as a test over it and a jump past its body; the `lem-default` last.
    /// A case's body sees the subject narrowed to a type pattern's type — where the
    /// subject is a path — and the `as` binding, if the pattern has one. The rules
    /// after the cases: a pattern spelled twice is refused at the repeat, and a match
    /// with no default must cover its subject (the provisional exhaustiveness reading).
    fn lower_match(&mut self, tag: &MatchTag, cases: Vec<Case>, default: Option<Body>) {
        let subject = required(tag.subject());
        let subject_ty = self.expr(&subject);
        let subject_path = self.path_of(&subject).map(|(path, _)| path);
        let tmp = self.names.temp();
        self.emit(Op::StoreLocal { slot: tmp }, subject.span());
        let head = Span::new(
            tag.span().start,
            tag.block()
                .map_or(tag.span().end, |block| block.span().start),
        );
        let mut ends = Vec::new();
        let mut seen: Vec<(String, Span)> = Vec::new();
        let mut covered: Vec<Rc<str>> = Vec::new();
        let mut null_covered = false;
        for case in cases {
            let pattern = required(case.tag.pattern());
            let span = pattern.span();
            self.emit(Op::Local { slot: tmp }, span);
            let (spelling, narrow) =
                self.pattern(&pattern, &subject_ty, &mut covered, &mut null_covered);
            match seen.iter().find(|(seen, _)| *seen == spelling) {
                Some((_, first)) => self.diagnostics.push(
                    Diagnostic::error(
                        self.path,
                        span,
                        format!("`{spelling}` is already matched"),
                    )
                    .with_label(
                        self.path,
                        *first,
                        "first matched here",
                    ),
                ),
                None => seen.push((spelling, span)),
            }
            let next = self.emit_jump(Op::JumpIfFalse { to: 0 }, span);
            self.push_scope();
            let depth = self.narrowings.len();
            if let (Some(path), Some(narrow)) = (&subject_path, &narrow) {
                self.narrowings.push((path.clone(), narrow.clone()));
            }
            if let Some(name) = pattern.binding() {
                self.emit(Op::Local { slot: tmp }, name.span());
                let ty = narrow.unwrap_or(Type::Any);
                let slot = self.bind(&name, BindingKind::Definition, ty);
                self.emit(Op::StoreLocal { slot }, name.span());
            }
            self.lower_body(case.body);
            self.narrowings.truncate(depth);
            self.pop_scope();
            ends.push(self.emit_jump(Op::Jump { to: 0 }, case.tag.span()));
            self.patch(next);
        }
        self.require(
            head,
            rules::exhaustive(
                &subject_ty,
                &covered,
                null_covered,
                default.is_some(),
                self.decls(),
            ),
        );
        if let Some(default) = default {
            self.push_scope();
            self.lower_body(default);
            self.pop_scope();
        }
        for end in ends {
            self.patch(end);
        }
    }

    /// Lowers one case's test over the subject on the stack, leaving a bool, and
    /// reports what the pattern rules refuse. Returns the pattern as spelled, for the
    /// repeat rule, and what a type pattern narrows the subject to. A variant or a
    /// `null` is noted in `covered`/`null_covered` for the exhaustiveness rule.
    fn pattern(
        &mut self,
        pattern: &Pattern,
        subject: &Type,
        covered: &mut Vec<Rc<str>>,
        null_covered: &mut bool,
    ) -> (String, Option<Type>) {
        let span = pattern.span();
        if let Some(literal) = pattern.literal() {
            // Keyed on the value, not the spelling: `"x"` and `"\x"` are one string,
            // so the second is a dead case and not a second pattern.
            let token = required(literal.token());
            let spelling = if required(literal.kind()) == LiteralKind::String {
                format!("\"{}\"", decode_string(token.text()))
            } else {
                token.text().to_owned()
            };
            let ty = self.literal(&literal);
            if ty == Type::Null {
                *null_covered = true;
                self.require(span, rules::null_pattern(subject));
            } else {
                let verdict =
                    rules::operator(BinaryOp::Eq, "==", subject, &ty, self.decls());
                self.require(span, verdict.map(|_| ()));
            }
            self.emit(Op::Eq, span);
            return (spelling, None);
        }
        if let Some(variant) = pattern.variant() {
            let name = variant.text();
            self.require(span, rules::variant_pattern(subject, name, self.decls()));
            covered.push(Rc::from(name));
            self.emit_variant(name, span);
            self.emit(Op::Eq, span);
            return (format!(".{name}"), None);
        }
        // A type pattern. The rule knows what a `lem-case` may name — an enum's name
        // here is an author reaching for its variants, and is told so (D32).
        let name = required(pattern.name());
        let narrow = self.type_test(subject, &name, span, rules::TestForm::Case);
        (name.text().to_owned(), narrow)
    }

    fn lower_for(&mut self, tag: &ForTag, body: Body, otherwise: Option<Body>) {
        let binding = required(tag.binding());
        let first = required(binding.first());
        let second = binding.second();
        let iterable = required(tag.iterable());
        let span = tag.span();

        let ty = self.expr(&iterable);
        let (first_ty, second_ty) = self.rule_or(
            iterable.span(),
            rules::iterable(&ty),
            (Type::Any, Type::Any),
        );
        let init = self.emit_jump(Op::IterInit { exit: 0 }, iterable.span());
        let top = self.here();
        let next = self.emit_jump(
            Op::IterNext {
                exit: 0,
                pair: second.is_some(),
            },
            binding.span(),
        );
        self.push_scope();
        let first_slot = self.bind(&first, BindingKind::Loop, first_ty);
        if let Some(second) = &second {
            let second_slot = self.bind(second, BindingKind::Loop, second_ty);
            self.emit(Op::StoreLocal { slot: second_slot }, second.span());
        }
        self.emit(Op::StoreLocal { slot: first_slot }, first.span());
        self.lower_body(body);
        self.pop_scope();
        self.emit(Op::Jump { to: top }, span);
        self.patch(init);
        if let Some(otherwise) = otherwise {
            self.push_scope();
            self.lower_body(otherwise);
            self.pop_scope();
        }
        self.patch(next);
    }

    fn lower_let_void(&mut self, tag: &LetVoid, level: Level) {
        let name = required(tag.name());
        let value = required(tag.value());
        // The value is compiled before the name is bound, so `x = x + 1` reads the
        // outer `x` rather than its own uninitialised slot. An annotation is what the
        // value is checked against and what the name then has; without one the name
        // has what the value has.
        let ty = match tag.type_annotation() {
            Some(annotation) => {
                let declared = self.annotation(&required(annotation.ty()));
                self.check(&value, &declared);
                declared
            }
            None => self.expr(&value),
        };
        let slot = self.bind(&name, self.definition(level), ty);
        self.emit(Op::StoreLocal { slot }, name.span());
    }

    fn lower_let_block(&mut self, tag: &LetBlock, level: Level) {
        let name = required(tag.name());
        let body = required(tag.body());
        self.emit(Op::Capture, tag.span());
        self.lower_block(body.items().collect(), Kind::Value);
        self.emit(Op::EndCapture, tag.span());
        let slot = self.bind(&name, self.definition(level), Type::Html);
        self.emit(Op::StoreLocal { slot }, name.span());
    }

    /// What a `lem-let` at `level` is: a definition, or — at the top level of the file
    /// an ambient value takes its bindings from — a field of that value's record, which
    /// every other file may read and this one cannot tell whether any does (D30).
    ///
    /// Both top levels, since which file a host reads its bindings from is the host's
    /// to say and the collection that makes the record reads either the same way.
    const fn definition(&self, level: Level) -> BindingKind {
        match level {
            Level::File | Level::Document if self.exports_bindings => BindingKind::Exported,
            _ => BindingKind::Definition,
        }
    }
}

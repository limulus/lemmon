//! Lowering of what stands in a body: text runs and tags.
//!
//! What the literal text between them *is* — the line rule and the body rule — belongs
//! to [`whitespace`](super::whitespace), which has run by the time a [`Body`] gets here.

use lemmon_syntax::Span;
use lemmon_syntax::ast::{ForTag, Item, LetBlock, LetVoid, Spanned};

use super::whitespace::{Arm, Body, Kind, Piece};
use super::{Compiler, TokenSpan, required};
use crate::bytecode::Op;

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
        }
    }

    /// Lowers one item that is not a text run or a flow construct.
    fn lower_item(&mut self, item: &Item, level: Level) {
        match item {
            Item::Text(_) | Item::If(_) | Item::For(_) => {
                unreachable!("these are pieces of their own")
            }
            Item::Access(tag) => {
                self.expr(&required(tag.expr()));
                self.emit(Op::Emit, tag.span());
            }
            Item::LetVoid(tag) => self.lower_let_void(tag),
            Item::LetBlock(tag) => self.lower_let_block(tag),
            // Parsed by `lemmon-syntax` (∆1vS); the loader (∆iCW) fulfils it, and the
            // checker (∆C8d) will require the file to be the host's site file, which
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
            // Parsed by `lemmon-syntax` (∆up2) for the checker to consume; until it
            // exists nothing can be lowered from them. Each names the item it waits on,
            // so the placeholder gate (∆DA2) notices when that item has landed.
            Item::TypeDecl(_) => self.error(
                item.span(),
                "`lem-type` needs the type checker, which is not here yet (∆C8d)",
            ),
            Item::EnumDecl(_) => self.error(
                item.span(),
                "`lem-enum` needs the type checker, which is not here yet (∆C8d)",
            ),
            Item::Match(_) => self.error(
                item.span(),
                "`lem-match` needs the type checker, which is not here yet (∆PCK)",
            ),
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
            let next = arm.condition.as_ref().map(|condition| {
                self.expr(condition);
                self.emit_jump(Op::JumpIfFalse { to: 0 }, condition.span())
            });
            self.push_scope();
            self.lower_body(arm.body);
            self.pop_scope();
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

    fn lower_for(&mut self, tag: &ForTag, body: Body, otherwise: Option<Body>) {
        let binding = required(tag.binding());
        let first = required(binding.first());
        let second = binding.second();
        let iterable = required(tag.iterable());
        let span = tag.span();

        self.expr(&iterable);
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
        let first_slot = self.declare(&first);
        if let Some(second) = &second {
            let second_slot = self.declare(second);
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

    fn lower_let_void(&mut self, tag: &LetVoid) {
        let name = required(tag.name());
        // The value is compiled before the name is bound, so `x = x + 1` reads the
        // outer `x` rather than its own uninitialised slot.
        self.expr(&required(tag.value()));
        let slot = self.declare(&name);
        self.emit(Op::StoreLocal { slot }, name.span());
    }

    fn lower_let_block(&mut self, tag: &LetBlock) {
        let name = required(tag.name());
        let body = required(tag.body());
        self.emit(Op::Capture, tag.span());
        self.lower_block(body.items().collect(), Kind::Value);
        self.emit(Op::EndCapture, tag.span());
        let slot = self.declare(&name);
        self.emit(Op::StoreLocal { slot }, name.span());
    }
}

//! Lowering of what stands in a body: text runs and tags.
//!
//! Text around block tags is preserved as written; a keyword void tag alone on its line
//! (`<lem-let x = 1 />`, `<lem-else />`, …) takes that line with it, so that a template
//! laid out one statement per line does not leak a blank line per statement into the
//! output. `<lem.…>` is not a keyword and keeps its line: `<h1>\n<lem.title>\n</h1>`
//! renders the newlines it shows.

use lemmon_syntax::Span;
use lemmon_syntax::ast::{ForTag, IfTag, Item, LetBlock, LetVoid, Spanned};

use super::{Compiler, TokenSpan, required};
use crate::bytecode::Op;

/// A body's items with the text between them, after the void-line rule has been
/// applied within it. `None` stands where a tag does.
struct Body {
    items: Vec<Item>,
    texts: Vec<Option<String>>,
    /// Whether this is a file's top level, where `lem-fn` definitions stand.
    file_level: bool,
}

impl Body {
    /// `file_level` treats the start and end of the item list as line boundaries, the
    /// way the start and end of a file are; inside a block the opener and close tag
    /// share those lines with whatever follows or precedes them.
    fn new(items: Vec<Item>, file_level: bool) -> Self {
        let mut texts: Vec<Option<String>> = items
            .iter()
            .map(|item| match item {
                Item::Text(run) => Some(required(run.text()).text().to_owned()),
                _ => None,
            })
            .collect();
        let last = items.len().saturating_sub(1);
        for (i, item) in items.iter().enumerate() {
            if is_void(item) {
                // What stands beside the tag decides whether its side of the line is
                // clear: text is judged by its own blankness, another void tag is
                // transparent (a line of nothing but void tags is still an empty line),
                // and the ends of a file are boundaries as much as newlines are. Any
                // other tag is not: it renders something onto this line.
                let (before, after) = texts.split_at_mut(i);
                trim_void_line(
                    before.last_mut().and_then(Option::as_mut),
                    after.get_mut(1).and_then(Option::as_mut),
                    if i == 0 {
                        file_level
                    } else {
                        is_void(&items[i - 1])
                    },
                    if i == last {
                        file_level
                    } else {
                        is_void(&items[i + 1])
                    },
                );
            }
        }
        Self {
            items,
            texts,
            file_level,
        }
    }

    fn first_text(&mut self) -> Option<&mut String> {
        self.texts.first_mut().and_then(Option::as_mut)
    }

    fn last_text(&mut self) -> Option<&mut String> {
        self.texts.last_mut().and_then(Option::as_mut)
    }
}

/// A document chunk as emitted: without the line ending a void keyword tag's sentinel
/// left at its head, when one stands before it.
fn chunk(chunk: &str, after_void: bool) -> &str {
    if after_void {
        chunk.strip_prefix('\n').unwrap_or(chunk)
    } else {
        chunk
    }
}

/// Whether `item` is a void keyword tag — the tags the void-line rule is about.
/// `<lem.…>` is not one: it renders in place, so it keeps the line it is written on.
const fn is_void(item: &Item) -> bool {
    matches!(item, Item::LetVoid(_) | Item::FnVoid(_))
}

/// Applies the void-line rule around one void tag, given the text on either side of it
/// (`None` when a tag stands there instead). `*_missing_ok` says whether a missing side
/// counts as a line boundary.
fn trim_void_line(
    prev: Option<&mut String>,
    next: Option<&mut String>,
    prev_missing_ok: bool,
    next_missing_ok: bool,
) {
    let blank = |s: &str| s.bytes().all(|b| matches!(b, b' ' | b'\t' | b'\r'));
    let prev_cut = match &prev {
        None => prev_missing_ok.then_some(0),
        Some(text) => {
            let cut = text.rfind('\n').map_or(0, |at| at + 1);
            blank(&text[cut..]).then_some(cut)
        }
    };
    let next_cut = match &next {
        None => next_missing_ok.then_some(0),
        Some(text) => match text.find('\n') {
            Some(at) => blank(&text[..at]).then_some(at + 1),
            None => blank(text).then_some(text.len()),
        },
    };
    if let (Some(prev_cut), Some(next_cut)) = (prev_cut, next_cut) {
        if let Some(prev) = prev {
            prev.truncate(prev_cut);
        }
        if let Some(next) = next {
            next.drain(..next_cut);
        }
    }
}

impl Compiler<'_> {
    /// Lowers a body in its own scope.
    pub(super) fn lower_block(&mut self, items: Vec<Item>, file_level: bool) {
        self.push_scope();
        self.lower_body(Body::new(items, file_level));
        self.pop_scope();
    }

    fn lower_body(&mut self, body: Body) {
        let file_level = body.file_level;
        for (item, text) in body.items.into_iter().zip(body.texts) {
            if let Item::Text(_) = &item {
                let text = text.expect("a text run keeps its text");
                if !text.is_empty() {
                    let index = self.intern(&text);
                    self.emit(Op::EmitText { index }, item.span());
                }
            } else {
                self.lower_item(&item, file_level);
            }
        }
    }

    /// Lowers a document's top level: its prose, rendered to HTML already, around its
    /// lem-blocks. The text runs in the tree are the prose as written and are not
    /// emitted; what stands in their place is the chunk rendered for that gap — which
    /// may exist where no text run does, as an empty last gap still collects the
    /// document's footnotes. Of the void-line rule only one half applies — a block is
    /// alone on its line by construction, so the only line a void keyword tag can take
    /// with it is the line ending its sentinel left at the head of the next chunk.
    pub(super) fn lower_document(
        &mut self,
        items: Vec<Item>,
        prose: &[Span],
        chunks: &[String],
    ) {
        self.push_scope();
        let mut block = 0;
        let mut after_void = false;
        for item in items {
            if let Item::Text(_) = item {
                continue;
            }
            self.emit_chunk(prose[block], chunk(&chunks[block], after_void));
            self.lower_item(&item, true);
            after_void = is_void(&item);
            block += 1;
        }
        self.emit_chunk(prose[block], chunk(&chunks[block], after_void));
        self.pop_scope();
    }

    fn emit_chunk(&mut self, span: Span, chunk: &str) {
        if !chunk.is_empty() {
            let index = self.intern(chunk);
            self.emit(Op::EmitText { index }, span);
        }
    }

    /// Lowers one item that is not a text run.
    fn lower_item(&mut self, item: &Item, file_level: bool) {
        match item {
            Item::Text(_) => unreachable!("text runs are lowered by their body"),
            Item::Access(tag) => {
                self.expr(&required(tag.expr()));
                self.emit(Op::Emit, tag.span());
            }
            Item::If(tag) => self.lower_if(tag),
            Item::For(tag) => self.lower_for(tag),
            Item::LetVoid(tag) => self.lower_let_void(tag),
            Item::LetBlock(tag) => self.lower_let_block(tag),
            // Top-level definitions were compiled into their own functions already;
            // they stand in the item list only so the void-line rule sees them.
            Item::FnBlock(_) | Item::FnVoid(_) if file_level => {}
            Item::FnBlock(_) | Item::FnVoid(_) => {
                self.error(item.span(), "`lem-fn` must be at the top level of a file");
            }
            Item::Unsupported(_) | Item::Unknown(_) => {
                unreachable!(
                    "the parser reports these, and a parse with diagnostics is never compiled"
                )
            }
        }
    }

    fn lower_if(&mut self, tag: &IfTag) {
        let arms = tag.arms();
        let mut bodies: Vec<Body> = arms
            .iter()
            .map(|arm| Body::new(arm.items.clone(), false))
            .collect();
        for i in 1..bodies.len() {
            let (before, after) = bodies.split_at_mut(i);
            let prev = before.last_mut().expect("an arm before");
            trim_void_line(prev.last_text(), after[0].first_text(), false, false);
        }
        let mut ends = Vec::new();
        for (arm, body) in arms.iter().zip(bodies) {
            let next = arm.condition.as_ref().map(|condition| {
                self.expr(condition);
                self.emit_jump(Op::JumpIfFalse { to: 0 }, condition.span())
            });
            self.push_scope();
            self.lower_body(body);
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

    fn lower_for(&mut self, tag: &ForTag) {
        let binding = required(tag.binding());
        let first = required(binding.first());
        let second = binding.second();
        let iterable = required(tag.iterable());
        let span = tag.span();

        let mut body = Body::new(tag.items().collect(), false);
        let mut otherwise = tag
            .else_items()
            .map(|items| Body::new(items.collect(), false));
        if let Some(otherwise) = &mut otherwise {
            trim_void_line(body.last_text(), otherwise.first_text(), false, false);
        }

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
        self.lower_block(body.items().collect(), false);
        self.emit(Op::EndCapture, tag.span());
        let slot = self.declare(&name);
        self.emit(Op::StoreLocal { slot }, name.span());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trimmed(prev: &str, next: &str) -> (String, String) {
        let (mut prev, mut next) = (prev.to_owned(), next.to_owned());
        trim_void_line(Some(&mut prev), Some(&mut next), false, false);
        (prev, next)
    }

    #[test]
    fn removes_the_line_a_void_tag_stands_alone_on() {
        assert_eq!(
            trimmed("<ul>\n  ", "\n  <li>"),
            ("<ul>\n".to_owned(), "  <li>".to_owned())
        );
    }

    #[test]
    fn leaves_a_void_tag_that_shares_its_line_alone() {
        assert_eq!(
            trimmed("<ul> ", "\n<li>"),
            ("<ul> ".to_owned(), "\n<li>".to_owned())
        );
        assert_eq!(
            trimmed("<ul>\n", " x\n"),
            ("<ul>\n".to_owned(), " x\n".to_owned())
        );
    }

    #[test]
    fn treats_the_whole_text_as_one_line_when_it_has_no_newline() {
        assert_eq!(trimmed("  ", "  "), (String::new(), String::new()));
    }

    #[test]
    fn treats_a_crlf_line_end_as_blank_space() {
        assert_eq!(
            trimmed("a\r\n", "\r\nb"),
            ("a\r\n".to_owned(), "b".to_owned())
        );
    }

    #[test]
    fn treats_an_adjacent_void_tag_as_a_line_boundary() {
        // Both sides of the pair report "missing", so this only trims if the caller
        // says a missing side is a boundary — which `Body::new` does for a void tag.
        let mut next = "\n  <li>y</li>".to_owned();
        trim_void_line(None, Some(&mut next), true, false);
        assert_eq!(next, "  <li>y</li>");
    }

    #[test]
    fn treats_a_missing_side_as_a_boundary_only_when_told_to() {
        let mut next = "\nx".to_owned();
        trim_void_line(None, Some(&mut next), false, false);
        assert_eq!(next, "\nx");
        trim_void_line(None, Some(&mut next), true, false);
        assert_eq!(next, "x");
        let mut prev = "a\n  ".to_owned();
        trim_void_line(Some(&mut prev), None, false, false);
        assert_eq!(prev, "a\n  ");
        trim_void_line(Some(&mut prev), None, false, true);
        assert_eq!(prev, "a\n");
    }
}

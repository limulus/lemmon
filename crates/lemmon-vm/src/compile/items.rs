//! Lowering of what stands in a body: text runs and tags.
//!
//! What the literal text between them *is* — the line rule and the body rule — belongs
//! to [`whitespace`](super::whitespace), which has run by the time a [`Body`] gets here.

use std::rc::Rc;

use lemmon_syntax::ast::{
    ArgList, BinaryOp, Expr, ForTag, Item, LetBlock, LetFrom, LetVoid, LiteralKind,
    MatchTag, Pattern, Spanned,
};
use lemmon_syntax::{Span, SyntaxToken};

use super::expr::{decode_string, unparen};
use super::names::{BindingKind, already_bound};
use super::whitespace::{Arm, Body, Case, Kind, Piece};
use super::{Compiler, Diagnostic, TokenSpan, required, rules};
use crate::bytecode::Op;
use crate::decls::Decls;
use crate::plan::{Fulfilled, SourceKind};
use crate::types::Type;
use crate::unit::BindingFn;

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
    /// A document's top level: definitions stand there, and `from` bindings only
    /// when the host reads its bindings from this document — which file that is being
    /// the host's to say.
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
            // At the top level of the file an ambient value reads its bindings from, a
            // binding is a field of that value (D36): compiled for the loader, never a
            // local. Both top levels, since which file that is is the host's to say.
            Item::LetVoid(tag) if self.exports_bindings && level != Level::Nested => {
                self.lower_exported_void(tag);
            }
            Item::LetBlock(tag) if self.exports_bindings && level != Level::Nested => {
                self.lower_exported_block(tag);
            }
            Item::LetVoid(tag) => self.lower_let_void(tag),
            Item::LetBlock(tag) => self.lower_let_block(tag),
            Item::LetFrom(tag) => self.lower_let_from(tag, level),
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

    fn lower_let_void(&mut self, tag: &LetVoid) {
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
        let slot = self.bind(&name, BindingKind::Definition, ty);
        self.emit(Op::StoreLocal { slot }, name.span());
    }

    fn lower_let_block(&mut self, tag: &LetBlock) {
        let name = required(tag.name());
        let body = required(tag.body());
        self.emit(Op::Capture, tag.span());
        self.lower_block(body.items().collect(), Kind::Value);
        self.emit(Op::EndCapture, tag.span());
        let slot = self.bind(&name, BindingKind::Definition, Type::Html);
        self.emit(Op::StoreLocal { slot }, name.span());
    }

    // --- The bindings file's top level (D36, ∆iCW) --------------------------------

    /// Whether `name` may be one of the bindings file's top-level bindings: not a
    /// repeat of a sibling — they are fields, in no scope, so the repeat rule is kept
    /// here — and admitted by the rules a binding answers to.
    fn admit_exported(&mut self, name: &SyntaxToken) -> bool {
        if let Some(&(_, first)) = self
            .top_bindings
            .iter()
            .find(|(bound, _)| &**bound == name.text())
        {
            self.diagnostics.push(already_bound(
                self.path,
                name.span(),
                name.text(),
                first,
            ));
            return false;
        }
        self.top_bindings.push((Rc::from(name.text()), name.span()));
        self.names.check_exported(name.text(), name.span())
    }

    /// The type the synthesized record declares for the binding `name` — read from its
    /// head when the file was prepared (D36), `any` where it had none.
    fn exported_type(&self, name: &str) -> Type {
        self.known(self.index.field(self.path, name).unwrap_or(Type::Any))
    }

    /// One top-level binding of the bindings file: its value lowered into a function
    /// of nothing — for its own diagnostics even when the binding is refused, which
    /// `lower` is told — and recorded as computed when it was admitted.
    fn lower_exported(
        &mut self,
        name: &SyntaxToken,
        span: Span,
        lower: impl FnOnce(&mut Self, bool),
    ) {
        let admitted = self.admit_exported(name);
        let (func, reads) =
            self.function_for_loader(name.text(), &[], None, span, |c| lower(c, admitted));
        if admitted {
            self.unit.bindings.push(BindingFn {
                name: Rc::from(name.text()),
                fulfilled: Fulfilled::Computed { func, reads },
                span: name.span(),
            });
        }
    }

    /// A computed binding, checked against the type its head declared. A repeat is
    /// held to no type — the first binding's is not its.
    fn lower_exported_void(&mut self, tag: &LetVoid) {
        let name = required(tag.name());
        let value = required(tag.value());
        let ty = self.exported_type(name.text());
        self.lower_exported(&name, tag.span(), |c, admitted| {
            if admitted {
                c.check(&value, &ty);
            } else {
                c.expr(&value);
            }
        });
    }

    /// A capture binding at the bindings file's top level: `html`, computed the same
    /// way.
    fn lower_exported_block(&mut self, tag: &LetBlock) {
        let name = required(tag.name());
        let body = required(tag.body());
        self.lower_exported(&name, tag.span(), |c, _| {
            c.emit(Op::Capture, tag.span());
            c.lower_block(body.items().collect(), Kind::Value);
            c.emit(Op::EndCapture, tag.span());
        });
    }

    /// A `from` binding: the host fulfils it (`docs/language.md` §Ambient types), so
    /// nothing is emitted — what is recorded is the source, by its form, for the
    /// loader and the shell to read. It stands only at the top level of a file the
    /// host reads bindings from; anywhere else nothing would ever load it, and the
    /// error names where it belongs.
    fn lower_let_from(&mut self, tag: &LetFrom, level: Level) {
        if !self.exports_bindings || level == Level::Nested {
            let files: Vec<String> = self
                .host
                .bindings_files()
                .map(|(_, file)| format!("`{file}`"))
                .collect();
            let message = match files.len() {
                0 => "a `from` binding needs a host that reads bindings from a file, and \
                      this one reads none"
                    .to_owned(),
                1 => format!(
                    "a `from` binding belongs at the top level of {}, the file the host \
                     reads bindings from",
                    files[0]
                ),
                _ => format!(
                    "a `from` binding belongs at the top level of {}, the files the host \
                     reads bindings from",
                    files.join(" or ")
                ),
            };
            self.error(tag.span(), message);
            return;
        }
        let name = required(tag.name());
        let admitted = self.admit_exported(&name);
        let Some(kind) = self.source_kind(&required(tag.source())) else {
            return;
        };
        if admitted {
            self.unit.bindings.push(BindingFn {
                name: Rc::from(name.text()),
                fulfilled: Fulfilled::ByHost(kind),
                span: name.span(),
            });
        }
    }

    /// What a `from` source names, by its form: a string literal — a glob or a data
    /// file, which the host tells apart by the binding's type — or `plugin("…")` or
    /// `env("…")` with one string literal argument. Anything else is reported at the
    /// source; the parser read it as any expression, and this is where the forms that
    /// mean something are decided.
    fn source_kind(&mut self, source: &Expr) -> Option<SourceKind> {
        const FORMS: &str = "a `from` source is a string naming a glob or a data file, \
                             `plugin(\"…\")`, or `env(\"…\")`";
        let string_literal = |expr: &Expr| match unparen(expr.clone()) {
            Expr::Literal(literal) if required(literal.kind()) == LiteralKind::String => {
                Some(decode_string(required(literal.token()).text()))
            }
            _ => None,
        };
        if let Some(text) = string_literal(source) {
            return Some(SourceKind::File(Rc::from(text)));
        }
        let Expr::Call(call) = unparen(source.clone()) else {
            self.error(source.span(), FORMS);
            return None;
        };
        let callee = match required(call.callee()) {
            Expr::Name(name) => required(name.name()),
            _ => {
                self.error(source.span(), FORMS);
                return None;
            }
        };
        let make: fn(Rc<str>) -> SourceKind = match callee.text() {
            "plugin" => SourceKind::Plugin,
            "env" => SourceKind::Env,
            _ => {
                self.error(source.span(), FORMS);
                return None;
            }
        };
        let args = call.args();
        let positional: Vec<Expr> = args.iter().flat_map(ArgList::positional).collect();
        let named = args.iter().flat_map(ArgList::named).count();
        let argument = match (positional.as_slice(), named) {
            ([only], 0) => string_literal(only),
            _ => None,
        };
        match argument {
            Some(text) => Some(make(Rc::from(text))),
            None => {
                self.error(
                    call.span(),
                    format!(
                        "`{}` names its source with one string literal: `{}(\"…\")`",
                        callee.text(),
                        callee.text()
                    ),
                );
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use crate::compile::{compile, compile_unit, prepare};
    use crate::exports::ExportIndex;
    use crate::host::{Ambient, Host};
    use crate::plan::{Fulfilled, Read, SourceKind};
    use crate::testing::{all_of, at};
    use crate::{SourceFile, Unit, Value, render};

    /// A host shaped like the standard shell's, reading `site` from `site.lem`.
    fn shell_host() -> Host {
        Host::new()
            .bind("page", Ambient::named("Page"))
            .bind("site", Ambient::bindings("site.lem"))
            .documents("page")
            .rendered_through("layout")
    }

    const PAGE: &str = "<lem-type Page>\n  url: url\n</lem-type>";

    fn shown(host: &Host, files: &[SourceFile]) -> Vec<String> {
        compile(files, host)
            .diagnostics
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    /// The unit of `path`, compiled against `host` and an index of every file given.
    fn unit_of(host: &Host, path: &str, files: &[SourceFile]) -> Unit {
        let prepared: Vec<_> = files
            .iter()
            .map(|file| prepare(file, host).expect("the fixture parses"))
            .collect();
        let mut index = ExportIndex::new();
        for p in &prepared {
            index.offer(p.path(), p.exports());
            index.offer_types(p.path(), p.types());
        }
        let this = prepared
            .iter()
            .find(|p| p.path() == path)
            .expect("the file is given");
        compile_unit(this, &index, host).expect_output()
    }

    fn site_unit(text: &str) -> Unit {
        unit_of(
            &shell_host(),
            "site.lem",
            &[SourceFile::new("site.lem", text)],
        )
    }

    #[test]
    fn records_each_from_source_by_its_form() {
        let unit = site_unit(
            "<lem-let posts: [string] from \"feed/*.md\" />\n\
             <lem-let photos: {string: int} from plugin(\"photos\") />\n\
             <lem-let base: string from (env(\"SITE_URL\")) />",
        );
        let rows: Vec<(&str, &Fulfilled)> = unit
            .bindings
            .iter()
            .map(|b| (&*b.name, &b.fulfilled))
            .collect();
        assert_eq!(
            rows,
            [
                (
                    "posts",
                    &Fulfilled::ByHost(SourceKind::File(Rc::from("feed/*.md")))
                ),
                (
                    "photos",
                    &Fulfilled::ByHost(SourceKind::Plugin(Rc::from("photos")))
                ),
                (
                    "base",
                    &Fulfilled::ByHost(SourceKind::Env(Rc::from("SITE_URL")))
                ),
            ]
        );
        // The host fulfils them: nothing to run, so nothing beyond the entry.
        assert_eq!(unit.functions.len(), 1);
    }

    #[test]
    fn refuses_a_from_source_of_another_form() {
        const FORMS: &str = "a `from` source is a string naming a glob or a data file, \
                             `plugin(\"…\")`, or `env(\"…\")`";
        let literal = |callee: &str| {
            format!(
                "`{callee}` names its source with one string literal: `{callee}(\"…\")`"
            )
        };
        for (source, message) in [
            ("42", FORMS.to_owned()),
            ("photos(\"x\")", FORMS.to_owned()),
            ("f()(\"x\")", FORMS.to_owned()),
            ("plugin(1)", literal("plugin")),
            ("plugin(\"a\", \"b\")", literal("plugin")),
            ("env(name: \"a\")", literal("env")),
            ("env()", literal("env")),
        ] {
            let text = format!("<lem-let x: [string] from {source} />");
            let reported = shown(&shell_host(), &[SourceFile::new("site.lem", &text)]);
            assert_eq!(reported.len(), 1, "{source}: {reported:?}");
            assert!(reported[0].ends_with(&message), "{source}: {reported:?}");
        }
    }

    #[test]
    fn refuses_a_from_binding_outside_the_bindings_file() {
        const SRC: &str = "<lem-let posts: [string] from \"f/*.md\" />";
        let file = SourceFile::new("other.lem", SRC);
        assert_eq!(
            shown(&shell_host(), std::slice::from_ref(&file)),
            [format!(
                "other.lem:{}: a `from` binding belongs at the top level of `site.lem`, \
                 the file the host reads bindings from",
                all_of(SRC)
            )]
        );
        assert_eq!(
            shown(&Host::untyped(["site"]), &[file]),
            [format!(
                "other.lem:{}: a `from` binding needs a host that reads bindings from a \
                 file, and this one reads none",
                all_of(SRC)
            )]
        );
    }

    #[test]
    fn compiles_a_computed_binding_into_a_function_that_reads_its_siblings() {
        // Both spellings of a sibling read — the bare name and `site.name` — are one
        // read of one field, and the function takes nothing: `site` is ambient.
        let unit = site_unit(
            "<lem-let name: string = \"limulus.net\" />\n\
             <lem-let tagline: string = \"Notes from \" + name + site.name />",
        );
        let Fulfilled::Computed { func, reads } = &unit.bindings[1].fulfilled else {
            panic!("computed: {:?}", unit.bindings);
        };
        assert_eq!(
            *reads,
            [Read::Ambient {
                value: Rc::from("site"),
                field: Rc::from("name")
            }]
        );
        let function = &unit.functions[*func as usize];
        assert_eq!(function.name, "tagline");
        assert!(function.params.is_empty());
        assert_eq!(&*unit.bindings[0].name, "name");
    }

    #[test]
    fn reads_a_sibling_by_its_bare_name_from_the_ambient_value() {
        // Rendering the bindings file itself proves the read reaches `site`, not a
        // local: the value bound at render is what comes out.
        let files = [SourceFile::new(
            "site.lem",
            "<lem-let name: string = \"x\" />\n<lem.name>",
        )];
        let module = compile(&files, &shell_host()).expect_output();
        let site = Value::map(
            [(Rc::from("name"), Value::str("from-render"))]
                .into_iter()
                .collect(),
        );
        assert_eq!(
            render(&module, "site.lem", &[("site", site)]).unwrap(),
            "from-render"
        );
    }

    #[test]
    fn narrows_a_sibling_read_by_its_bare_name() {
        let files = [SourceFile::new(
            "site.lem",
            "<lem-let sub: string? = null />\n<lem-if sub><lem.sub></lem-if>",
        )];
        assert!(shown(&shell_host(), &files).is_empty());
    }

    #[test]
    fn keeps_bare_sibling_names_out_of_the_files_functions() {
        // A `lem-fn` in the bindings file reads the value as every other file does.
        const SRC: &str = "<lem-let name: string = \"x\" />\n\
                           <lem-fn f(): string = name />";
        assert_eq!(
            shown(&shell_host(), &[SourceFile::new("site.lem", SRC)]),
            [format!(
                "site.lem:{}: undefined name `name`",
                at(SRC, "= «name» />")
            )]
        );
    }

    #[test]
    fn reports_a_repeated_top_level_binding_once() {
        // The repeat is not held to the first binding's type: one mistake, one
        // diagnostic.
        const SRC: &str = "<lem-let a: int = 1 />\n<lem-let a: string = \"x\" />";
        assert_eq!(
            shown(&shell_host(), &[SourceFile::new("site.lem", SRC)]),
            [format!(
                "site.lem:{}: `a` is already bound here\n  site.lem:{}: first bound here",
                at(SRC, "«a»: string"),
                at(SRC, "«a»: int")
            )]
        );
    }

    #[test]
    fn refuses_a_top_level_binding_that_takes_a_host_value_s_name() {
        const SRC: &str = "<lem-let page: int = 1 />";
        assert_eq!(
            shown(&shell_host(), &[SourceFile::new("site.lem", SRC)]),
            [format!(
                "site.lem:{}: `page` is bound by the host and cannot be redefined",
                at(SRC, "page")
            )]
        );
    }

    #[test]
    fn warns_when_a_top_level_binding_shadows_the_stdlib() {
        const SRC: &str = "<lem-let raw: string = \"x\" />";
        assert_eq!(
            shown(&shell_host(), &[SourceFile::new("site.lem", SRC)]),
            [format!(
                "site.lem:{}: warning: `raw` shadows the stdlib function of the same \
                 name",
                at(SRC, "raw")
            )]
        );
    }

    #[test]
    fn checks_a_computed_binding_against_the_type_its_head_declares() {
        // The other half of the repeat rule above: an admitted binding is held to its
        // declared type.
        const SRC: &str = "<lem-let n: int = \"x\" />";
        assert_eq!(
            shown(&shell_host(), &[SourceFile::new("site.lem", SRC)]),
            [format!(
                "site.lem:{}: expected `int`, got `string`",
                at(SRC, "\"x\"")
            )]
        );
    }

    #[test]
    fn refuses_a_from_binding_nested_in_the_bindings_file() {
        const SRC: &str = "<lem-if true><lem-let x: [string] from \"f/*.md\" /></lem-if>";
        assert_eq!(
            shown(&shell_host(), &[SourceFile::new("site.lem", SRC)]),
            [format!(
                "site.lem:{}: a `from` binding belongs at the top level of `site.lem`, \
                 the file the host reads bindings from",
                at(SRC, "<lem-let x: [string] from \"f/*.md\" />")
            )]
        );
    }

    #[test]
    fn names_every_bindings_file_when_a_host_reads_several() {
        let host = Host::new()
            .bind("site", Ambient::bindings("site.lem"))
            .bind("data", Ambient::bindings("data.lem"));
        const SRC: &str = "<lem-let x: [string] from \"f/*.md\" />";
        assert_eq!(
            shown(&host, &[SourceFile::new("other.lem", SRC)]),
            [format!(
                "other.lem:{}: a `from` binding belongs at the top level of `site.lem` \
                 or `data.lem`, the files the host reads bindings from",
                all_of(SRC)
            )]
        );
    }

    #[test]
    fn refuses_the_document_value_in_a_computed_binding() {
        const HOME: &str = "<lem-let home: url = page.url />";
        let files = [
            SourceFile::new("ambient.lem", PAGE),
            SourceFile::new("site.lem", HOME),
        ];
        assert_eq!(
            shown(&shell_host(), &files),
            [format!(
                "site.lem:{}: `page` is bound per page, and a binding here is computed \
                 once, at load",
                at(HOME, "page")
            )]
        );
    }

    #[test]
    fn compiles_a_capture_binding_for_the_loader() {
        let unit = site_unit("<lem-let intro><p>hi</p></lem-let>");
        let Fulfilled::Computed { func, reads } = &unit.bindings[0].fulfilled else {
            panic!("computed: {:?}", unit.bindings);
        };
        assert!(reads.is_empty());
        assert_eq!(unit.functions[*func as usize].name, "intro");
    }

    #[test]
    fn reads_a_binding_that_declared_no_type_without_a_second_diagnostic() {
        let files = [SourceFile::new(
            "site.lem",
            "<lem-let name = \"x\" />\n<lem.name>",
        )];
        let reported = shown(&shell_host(), &files);
        assert_eq!(reported.len(), 1, "{reported:?}");
        assert!(reported[0].contains("declares its type"), "{reported:?}");
    }
}

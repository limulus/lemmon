//! What a body's literal text is, before any of it is lowered.
//!
//! Two rules, both lexical: they read the template's own bytes and nothing else, so an
//! interpolated value — which arrives at run time — is never touched by either.
//!
//! **The line rule.** A tag that renders nothing where it stands takes its line with it
//! when nothing else stands on that line. Those are the definitions and bindings:
//! `lem-let` and `lem-fn`, in their void form and in their block form alike. So a file
//! laid out one statement per line does not leak a blank line per statement, and a file
//! of component definitions does not leak one per component. `<lem.…>` is not one of
//! them — it renders in place, so `<h1>\n<lem.title>\n</h1>` renders the newlines it
//! shows — and neither are `lem-if`/`lem-for`, whose bodies are output.
//!
//! **The body rule (D26).** A value-producing body — a `lem-fn` block body, a `lem-let`
//! capture body — loses the newline-and-indent at each edge and is dedented by the
//! common indentation of the lines that remain, so a definition's own indentation never
//! reaches the output of the pages that call it. It runs after the line rule, and a line
//! that rule deleted must not set the margin: [`trim_statement_line`] reports whether it
//! fired, and a tag whose line is gone is transparent to the measurement.
//!
//! The measurement is over the body's whole lexical extent, flow constructs included: a
//! `lem-if` arm inside a definition is dedented with the definition around it, and its
//! lines set the margin like any others. A nested capture body is not — it is its own
//! root, normalized on its own, and stands here as one opaque mark. Which is what keeps
//! its relative indentation from being eaten a second time.
//!
//! No element awareness (invariant 2): `<pre>` dedents like everything else. Only the
//! shared margin goes; relative indentation survives.

use lemmon_syntax::Span;
use lemmon_syntax::ast::{CaseTag, Expr, ForTag, Item, MatchTag, Spanned};

use super::required;

/// Where a body stands, which decides which rules reach it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    /// A file's top level, where `lem-fn` definitions stand. Its start and end are line
    /// boundaries the way the start and end of a file are; nothing is dedented.
    File,
    /// A `lem-fn` block body or a `lem-let` capture body: the body rule normalizes it.
    Value,
    /// A flow construct's body — a `lem-if` arm, a `lem-for` body or its `lem-else`.
    /// Verbatim: it takes no edge trim and no dedent of its own, and its text is
    /// dedented with the definition enclosing it, if there is one.
    Flow,
}

/// A body's pieces: its text as it will be emitted, and the tags standing between the
/// runs of it, with the flow constructs' own bodies built in place.
pub(super) struct Body {
    pub(super) pieces: Vec<Piece>,
    pub(super) kind: Kind,
}

/// One thing standing in a body.
pub(super) enum Piece {
    /// A text run's text, after both rules, and the run it came from — a span the
    /// emitted text still points at, however much whitespace the rules took off it.
    Text { text: String, span: Span },
    /// A tag lowered where it stands. `took_its_line` records that the line rule
    /// removed the line it stood on, which makes it transparent to the body rule.
    Tag { item: Item, took_its_line: bool },
    /// `<lem-if …>`, with one body per arm.
    If { arms: Vec<Arm> },
    /// `<lem-for …>`, with its body and its `lem-else`.
    For {
        tag: ForTag,
        body: Body,
        otherwise: Option<Body>,
    },
    /// `<lem-match …>`, with one body per case and one for its `lem-default`. The
    /// whitespace between the cases belongs to no body and is never emitted.
    Match {
        tag: MatchTag,
        cases: Vec<Case>,
        default: Option<Body>,
    },
}

/// One arm of a `lem-if`: what it is conditional on, and what it renders.
pub(super) struct Arm {
    /// The arm's condition; `None` for the `lem-else` arm.
    pub(super) condition: Option<Expr>,
    /// From the arm's opener to the end of its last item.
    pub(super) span: Span,
    pub(super) body: Body,
}

/// One `lem-case` of a `lem-match`: its tag, for the pattern, and what it renders.
pub(super) struct Case {
    pub(super) tag: CaseTag,
    pub(super) body: Body,
}

impl Body {
    /// Builds a body: its pieces, then the line rule over them, then — for a value
    /// body — the body rule.
    pub(super) fn new(items: Vec<Item>, kind: Kind) -> Self {
        let mut pieces: Vec<Piece> = items.into_iter().map(Piece::build).collect();
        line_rule(&mut pieces, kind == Kind::File);
        let mut body = Self { pieces, kind };
        if kind == Kind::Value {
            body.normalize();
        }
        body
    }

    fn first_text(&mut self) -> Option<&mut String> {
        self.pieces.first_mut().and_then(Piece::text_mut)
    }

    fn last_text(&mut self) -> Option<&mut String> {
        self.pieces.last_mut().and_then(Piece::text_mut)
    }

    /// The body rule: trim each edge, measure the margin over what remains, strip it.
    fn normalize(&mut self) {
        let at_line_start = self.trim_edges();
        let mut measure = Measure::new(at_line_start);
        measure.body(self);
        let margin = measure.margin.unwrap_or_default();
        if !margin.is_empty() {
            Strip::new(&margin, at_line_start).body(self);
        }
    }

    /// Drops the newline-and-indent at each edge — one newline per edge, so that a
    /// deliberate blank line survives as one. Reports whether the leading edge went,
    /// which is what makes the body's first byte a line start.
    fn trim_edges(&mut self) -> bool {
        if let Some(text) = self.last_text()
            && let Some(at) = text.rfind('\n')
            && is_blank(&text[at + 1..])
        {
            let cr = usize::from(text[..at].ends_with('\r'));
            text.truncate(at - cr);
        }
        let Some(text) = self.first_text() else {
            return false;
        };
        let Some(at) = text.find('\n') else {
            return false;
        };
        if !is_blank(&text[..at]) {
            return false;
        }
        text.drain(..=at);
        true
    }
}

impl Piece {
    /// Builds one piece, and with it the bodies of a flow construct's arms — which the
    /// line rule reaches across, a `lem-else` being alone on its line as often as any
    /// other tag is.
    pub(super) fn build(item: Item) -> Self {
        match item {
            Item::Text(ref run) => Self::Text {
                text: required(run.text()).text().to_owned(),
                span: run.span(),
            },
            Item::If(ref tag) => {
                let mut arms: Vec<Arm> = tag
                    .arms()
                    .into_iter()
                    .map(|arm| Arm {
                        condition: arm.condition,
                        span: arm.span,
                        body: Body::new(arm.items, Kind::Flow),
                    })
                    .collect();
                for i in 1..arms.len() {
                    let (before, after) = arms.split_at_mut(i);
                    let prev = before.last_mut().expect("an arm before");
                    trim_statement_line(
                        prev.body.last_text(),
                        after[0].body.first_text(),
                        false,
                        false,
                    );
                }
                Self::If { arms }
            }
            Item::For(ref tag) => {
                let mut body = Body::new(tag.items().collect(), Kind::Flow);
                let mut otherwise = tag
                    .else_items()
                    .map(|items| Body::new(items.collect(), Kind::Flow));
                if let Some(otherwise) = &mut otherwise {
                    trim_statement_line(
                        body.last_text(),
                        otherwise.first_text(),
                        false,
                        false,
                    );
                }
                Self::For {
                    tag: tag.clone(),
                    body,
                    otherwise,
                }
            }
            // A case body runs from its opener to its close tag, so no statement line
            // straddles two of them: nothing to trim between cases.
            Item::Match(ref tag) => Self::Match {
                tag: tag.clone(),
                cases: tag
                    .cases()
                    .map(|case| Case {
                        body: Body::new(
                            required(case.body()).items().collect(),
                            Kind::Flow,
                        ),
                        tag: case,
                    })
                    .collect(),
                default: tag.default().map(|default| {
                    Body::new(required(default.body()).items().collect(), Kind::Flow)
                }),
            },
            _ => Self::Tag {
                item,
                took_its_line: false,
            },
        }
    }

    fn text_mut(&mut self) -> Option<&mut String> {
        match self {
            Self::Text { text, .. } => Some(text),
            _ => None,
        }
    }

    /// Whether this piece renders nothing where it stands — the tags the line rule is
    /// about.
    pub(super) const fn renders_nothing(&self) -> bool {
        match self {
            // A declaration is a statement like a definition: it renders nothing and
            // takes its line (D27).
            Self::Tag { item, .. } => matches!(
                item,
                Item::LetVoid(_)
                    | Item::LetBlock(_)
                    | Item::LetFrom(_)
                    | Item::FnVoid(_)
                    | Item::FnBlock(_)
                    | Item::TypeDecl(_)
                    | Item::EnumDecl(_)
            ),
            _ => false,
        }
    }
}

/// Applies the line rule across a body's pieces. `ends_are_boundaries` says whether the
/// start and end of the piece list count as line boundaries, the way the start and end
/// of a file do; inside a block the opener and close tag share those lines with whatever
/// follows or precedes them.
fn line_rule(pieces: &mut [Piece], ends_are_boundaries: bool) {
    let silent: Vec<bool> = pieces.iter().map(Piece::renders_nothing).collect();
    let last = pieces.len().saturating_sub(1);
    for i in 0..pieces.len() {
        if !silent[i] {
            continue;
        }
        // What stands beside the tag decides whether its side of the line is clear:
        // text is judged by its own blankness, another silent tag is transparent (a
        // line of nothing but definitions is still an empty line), and the ends of a
        // file are boundaries as much as newlines are. Any other tag is not: it renders
        // something onto this line.
        let (before, after) = pieces.split_at_mut(i);
        let took = trim_statement_line(
            before.last_mut().and_then(Piece::text_mut),
            after.get_mut(1).and_then(Piece::text_mut),
            if i == 0 {
                ends_are_boundaries
            } else {
                silent[i - 1]
            },
            if i == last {
                ends_are_boundaries
            } else {
                silent[i + 1]
            },
        );
        if let Piece::Tag { took_its_line, .. } = &mut pieces[i] {
            *took_its_line = took;
        }
    }
}

/// Applies the line rule around one silent tag, given the text on either side of it
/// (`None` when a tag stands there instead). `*_missing_ok` says whether a missing side
/// counts as a line boundary. Reports whether the line went.
fn trim_statement_line(
    prev: Option<&mut String>,
    next: Option<&mut String>,
    prev_missing_ok: bool,
    next_missing_ok: bool,
) -> bool {
    let prev_cut = match &prev {
        None => prev_missing_ok.then_some(0),
        Some(text) => {
            let cut = text.rfind('\n').map_or(0, |at| at + 1);
            is_blank(&text[cut..]).then_some(cut)
        }
    };
    let next_cut = match &next {
        None => next_missing_ok.then_some(0),
        Some(text) => match text.find('\n') {
            Some(at) => is_blank(&text[..at]).then_some(at + 1),
            None => is_blank(text).then_some(text.len()),
        },
    };
    let (Some(prev_cut), Some(next_cut)) = (prev_cut, next_cut) else {
        return false;
    };
    if let Some(prev) = prev {
        prev.truncate(prev_cut);
    }
    if let Some(next) = next {
        next.drain(..next_cut);
    }
    true
}

/// Whether a stretch of one line holds nothing that renders.
fn is_blank(text: &str) -> bool {
    text.bytes().all(|b| matches!(b, b' ' | b'\t' | b'\r'))
}

/// The indentation a line opens with: its leading run of spaces and tabs.
fn indent_of(line: &str) -> &str {
    let rest = line.trim_start_matches([' ', '\t']);
    &line[..line.len() - rest.len()]
}

/// The first pass of the body rule: the common indentation of the lines that carry
/// something. A line that begins where the body does, rather than after a newline, has
/// no indentation to speak of — which is what makes a body hugging its opening tag
/// (`<lem-fn f()><svg>` …) verbatim, the way to emit a margin of one's own.
struct Measure {
    margin: Option<String>,
    /// Whether the line being read began at a newline.
    from_a_newline: bool,
    /// Its indentation so far, while nothing has rendered on it yet.
    indent: String,
    /// Whether nothing has rendered on it yet.
    blank: bool,
}

impl Measure {
    fn new(at_line_start: bool) -> Self {
        Self {
            margin: None,
            from_a_newline: at_line_start,
            indent: String::new(),
            blank: true,
        }
    }

    fn body(&mut self, body: &Body) {
        for piece in &body.pieces {
            match piece {
                Piece::Text { text, .. } => self.text(text),
                Piece::Tag { took_its_line, .. } => {
                    if !took_its_line {
                        self.renders();
                    }
                }
                // The opening tag renders onto the line it is written on, and so does
                // the close tag onto its own; between them stand the arms, in the order
                // they are written, since the rule is over the source and not over any
                // one run of the program.
                Piece::If { arms } => {
                    self.renders();
                    for arm in arms {
                        self.body(&arm.body);
                    }
                    self.renders();
                }
                Piece::For {
                    body, otherwise, ..
                } => {
                    self.renders();
                    self.body(body);
                    if let Some(otherwise) = otherwise {
                        self.body(otherwise);
                    }
                    self.renders();
                }
                Piece::Match { cases, default, .. } => {
                    self.renders();
                    for case in cases {
                        self.body(&case.body);
                    }
                    if let Some(default) = default {
                        self.body(default);
                    }
                    self.renders();
                }
            }
        }
    }

    fn text(&mut self, text: &str) {
        for (i, line) in text.split('\n').enumerate() {
            if i > 0 {
                self.from_a_newline = true;
                self.indent.clear();
                self.blank = true;
            }
            if !self.blank {
                continue;
            }
            let indent = indent_of(line);
            if self.from_a_newline {
                self.indent.push_str(indent);
            }
            if !is_blank(&line[indent.len()..]) {
                self.renders();
            }
        }
    }

    /// Something renders on this line: its indentation is the last word on what the
    /// margin can be.
    fn renders(&mut self) {
        if !self.blank {
            return;
        }
        self.blank = false;
        let indent = if self.from_a_newline {
            self.indent.as_str()
        } else {
            ""
        };
        match &mut self.margin {
            None => self.margin = Some(indent.to_owned()),
            Some(margin) => {
                let shared = margin
                    .bytes()
                    .zip(indent.bytes())
                    .take_while(|(a, b)| a == b)
                    .count();
                margin.truncate(shared);
            }
        }
    }
}

/// The second pass: the margin off the head of every line, however many pieces its
/// indentation is spread across. A blank line gives up what it has of the margin and no
/// more.
struct Strip<'a> {
    margin: &'a str,
    /// What is left of the margin to take off the line being written.
    owing: usize,
}

impl<'a> Strip<'a> {
    fn new(margin: &'a str, at_line_start: bool) -> Self {
        Self {
            margin,
            owing: if at_line_start { margin.len() } else { 0 },
        }
    }

    fn body(&mut self, body: &mut Body) {
        for piece in &mut body.pieces {
            match piece {
                Piece::Text { text, .. } => *text = self.text(text),
                Piece::Tag { took_its_line, .. } => {
                    if !*took_its_line {
                        self.owing = 0;
                    }
                }
                Piece::If { arms } => {
                    self.owing = 0;
                    for arm in arms {
                        self.body(&mut arm.body);
                    }
                    self.owing = 0;
                }
                Piece::For {
                    body, otherwise, ..
                } => {
                    self.owing = 0;
                    self.body(body);
                    if let Some(otherwise) = otherwise {
                        self.body(otherwise);
                    }
                    self.owing = 0;
                }
                Piece::Match { cases, default, .. } => {
                    self.owing = 0;
                    for case in cases {
                        self.body(&mut case.body);
                    }
                    if let Some(default) = default {
                        self.body(default);
                    }
                    self.owing = 0;
                }
            }
        }
    }

    fn text(&mut self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        for (i, line) in text.split('\n').enumerate() {
            if i > 0 {
                out.push('\n');
                self.owing = self.margin.len();
            }
            if self.owing == 0 {
                out.push_str(line);
                continue;
            }
            let indent = indent_of(line);
            let cut = indent.len().min(self.owing);
            self.owing -= cut;
            if indent.len() < line.len() {
                self.owing = 0;
            }
            out.push_str(&line[cut..]);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use lemmon_syntax::parse;

    use super::*;

    fn trimmed(prev: &str, next: &str) -> (String, String) {
        let (mut prev, mut next) = (prev.to_owned(), next.to_owned());
        trim_statement_line(Some(&mut prev), Some(&mut next), false, false);
        (prev, next)
    }

    /// The literal text a body would emit, in order, with `{}` standing where a tag is
    /// lowered. Built from the first item of `src`, which must be the body's tag.
    fn body_of(src: &str) -> String {
        let parse = parse(src);
        assert_eq!(parse.diagnostics, &[], "the case must parse clean");
        let items: Vec<Item> = parse.root().items().collect();
        let body = match items.first().expect("a tag") {
            Item::FnBlock(tag) => tag.body().expect("a body").items().collect(),
            Item::LetBlock(tag) => tag.body().expect("a body").items().collect(),
            _ => panic!("the first item must be a block definition"),
        };
        let mut out = String::new();
        write(&mut out, &Body::new(body, Kind::Value));
        out
    }

    fn write(out: &mut String, body: &Body) {
        for piece in &body.pieces {
            match piece {
                Piece::Text { text, .. } => out.push_str(text),
                Piece::Tag { took_its_line, .. } => {
                    if !took_its_line {
                        out.push_str("{}");
                    }
                }
                Piece::If { arms } => {
                    out.push_str("{if}");
                    for arm in arms {
                        write(out, &arm.body);
                    }
                    out.push_str("{/if}");
                }
                Piece::For {
                    body, otherwise, ..
                } => {
                    out.push_str("{for}");
                    write(out, body);
                    if let Some(otherwise) = otherwise {
                        write(out, otherwise);
                    }
                    out.push_str("{/for}");
                }
                Piece::Match { cases, default, .. } => {
                    out.push_str("{match}");
                    for case in cases {
                        out.push_str("{case}");
                        write(out, &case.body);
                    }
                    if let Some(default) = default {
                        out.push_str("{default}");
                        write(out, default);
                    }
                    out.push_str("{/match}");
                }
            }
        }
    }

    #[test]
    fn dedents_a_match_with_the_definition_around_it_and_drops_what_is_between_cases() {
        assert_eq!(
            body_of(concat!(
                "<lem-fn f(p: any)>\n",
                "  <lem-match p>\n",
                "    <lem-case .hero>\n",
                "      <p>hero</p>\n",
                "    </lem-case>\n",
                "\n",
                "    <lem-default>\n",
                "      <p>other</p>\n",
                "    </lem-default>\n",
                "  </lem-match>\n",
                "</lem-fn>",
            )),
            "{match}{case}\n    <p>hero</p>\n  {default}\n    <p>other</p>\n  {/match}"
        );
    }

    #[test]
    fn removes_the_line_a_silent_tag_stands_alone_on() {
        assert_eq!(
            trimmed("<ul>\n  ", "\n  <li>"),
            ("<ul>\n".to_owned(), "  <li>".to_owned())
        );
    }

    #[test]
    fn leaves_a_silent_tag_that_shares_its_line_alone() {
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
    fn treats_an_adjacent_silent_tag_as_a_line_boundary() {
        // Both sides of the pair report "missing", so this only trims if the caller
        // says a missing side is a boundary — which `line_rule` does for a silent tag.
        let mut next = "\n  <li>y</li>".to_owned();
        trim_statement_line(None, Some(&mut next), true, false);
        assert_eq!(next, "  <li>y</li>");
    }

    #[test]
    fn treats_a_missing_side_as_a_boundary_only_when_told_to() {
        let mut next = "\nx".to_owned();
        assert!(!trim_statement_line(None, Some(&mut next), false, false));
        assert_eq!(next, "\nx");
        assert!(trim_statement_line(None, Some(&mut next), true, false));
        assert_eq!(next, "x");
        let mut prev = "a\n  ".to_owned();
        assert!(!trim_statement_line(Some(&mut prev), None, false, false));
        assert_eq!(prev, "a\n  ");
        assert!(trim_statement_line(Some(&mut prev), None, false, true));
        assert_eq!(prev, "a\n");
    }

    #[test]
    fn dedents_a_body_laid_out_on_its_own_lines() {
        assert_eq!(
            body_of("<lem-fn f()>\n  <p>\n    hi\n  </p>\n</lem-fn>"),
            "<p>\n  hi\n</p>"
        );
    }

    #[test]
    fn leaves_a_body_hugging_its_opening_tag_verbatim() {
        // The lines below the first are indented alike, and that indentation is the
        // point: hugging is how a definition emits a margin of its own.
        assert_eq!(
            body_of("<lem-fn f()><svg>\n      <desc>d</desc>\n      </svg></lem-fn>"),
            "<svg>\n      <desc>d</desc>\n      </svg>"
        );
        // Whatever stands between the opening tag and the first line's content is
        // content too, not indentation, so it sets no margin and is not taken off.
        assert_eq!(
            body_of("<lem-fn f()>  <p>a</p>\n  <p>b</p>\n</lem-fn>"),
            "  <p>a</p>\n  <p>b</p>"
        );
    }

    #[test]
    fn leaves_a_single_line_body_verbatim() {
        assert_eq!(body_of("<lem-fn f()>  hi  </lem-fn>"), "  hi  ");
    }

    #[test]
    fn trims_one_newline_at_each_edge_and_keeps_a_deliberate_blank_line() {
        assert_eq!(
            body_of("<lem-fn f()>\n\n  <p>hi</p>\n\n</lem-fn>"),
            "\n<p>hi</p>\n"
        );
    }

    #[test]
    fn drops_a_carriage_return_with_the_newline_it_belongs_to() {
        assert_eq!(
            body_of("<lem-fn f()>\r\n  <p>hi</p>\r\n</lem-fn>"),
            "<p>hi</p>"
        );
    }

    #[test]
    fn ignores_blank_lines_when_it_measures_the_margin() {
        assert_eq!(
            body_of("<lem-fn f()>\n    <p>a</p>\n\n    <p>b</p>\n</lem-fn>"),
            "<p>a</p>\n\n<p>b</p>"
        );
    }

    #[test]
    fn takes_the_shared_prefix_rather_than_the_shortest_count() {
        // Two spaces and a tab share only the first space: taking two characters from
        // each would eat a tab's worth of indentation from the second line.
        assert_eq!(
            body_of("<lem-fn f()>\n  <p>a</p>\n \t<p>b</p>\n</lem-fn>"),
            " <p>a</p>\n\t<p>b</p>"
        );
    }

    #[test]
    fn keeps_relative_indentation_so_that_pre_survives() {
        assert_eq!(
            body_of("<lem-fn f()>\n  <pre>\n      indented\n  </pre>\n</lem-fn>"),
            "<pre>\n    indented\n</pre>"
        );
    }

    #[test]
    fn does_not_let_a_line_the_line_rule_deleted_set_the_margin() {
        assert_eq!(
            body_of("<lem-fn f()>\n  <lem-let x = 1 />\n  <p><lem.x></p>\n</lem-fn>"),
            "<p>{}</p>"
        );
    }

    #[test]
    fn dedents_a_flow_construct_with_the_definition_around_it() {
        assert_eq!(
            body_of("<lem-fn f()>\n  <lem-if c>\n    <p>hi</p>\n  </lem-if>\n</lem-fn>"),
            "{if}\n  <p>hi</p>\n{/if}"
        );
    }

    #[test]
    fn leaves_a_body_that_opens_on_a_tag_verbatim() {
        // Nothing was trimmed off the head, so the first line began where the opening
        // tag left off and has no indentation to give.
        assert_eq!(
            body_of("<lem-fn f()><lem.x>\n  <p>hi</p>\n</lem-fn>"),
            "{}\n  <p>hi</p>"
        );
    }

    #[test]
    fn dedents_a_loop_and_its_else_with_the_definition_around_them() {
        assert_eq!(
            body_of(concat!(
                "<lem-fn f(xs: list)>\n",
                "  <ul>\n",
                "    <lem-for x in xs>\n",
                "      <li><lem.x></li>\n",
                "    <lem-else />\n",
                "      <li>none</li>\n",
                "    </lem-for>\n",
                "  </ul>\n",
                "</lem-fn>",
            )),
            "<ul>\n  {for}\n    <li>{}</li>\n    <li>none</li>\n  {/for}\n</ul>"
        );
    }

    #[test]
    fn measures_the_lines_a_flow_construct_stands_on() {
        // The `lem-for` opener and its close both render onto lines of their own, and
        // those lines are the shallowest here: nothing may be taken off them.
        assert_eq!(
            body_of("<lem-fn f()>\n<lem-for x in xs>\n  <li>a</li>\n</lem-for>\n</lem-fn>"),
            "{for}\n  <li>a</li>\n{/for}"
        );
    }

    #[test]
    fn leaves_a_nested_capture_body_to_normalize_itself() {
        // The inner body is a root of its own: its `<p>b</p>` keeps the two spaces that
        // set it apart from `<p>a</p>`, and the outer margin never reaches inside it.
        assert_eq!(
            body_of(
                "<lem-fn f()>\n  <lem-let x>\n    <p>a</p>\n      <p>b</p>\n  </lem-let>\n  <lem.x>\n</lem-fn>"
            ),
            "{}"
        );
        assert_eq!(
            body_of("<lem-let x>\n    <p>a</p>\n      <p>b</p>\n  </lem-let>"),
            "<p>a</p>\n  <p>b</p>"
        );
    }

    #[test]
    fn takes_the_lines_of_the_definitions_a_file_is_made_of() {
        let parse = parse("<lem-fn f()>x</lem-fn>\n<lem-fn g()>y</lem-fn>\n<lem.f()>\n");
        assert_eq!(parse.diagnostics, &[]);
        let body = Body::new(parse.root().items().collect(), Kind::File);
        let mut out = String::new();
        write(&mut out, &body);
        assert_eq!(out, "{}\n");
    }

    #[test]
    fn takes_the_line_of_a_from_binding_as_of_any_declaration() {
        let parse =
            parse("<lem-let posts: [Article] from \"feed/*/index.md\" />\n<lem.posts>\n");
        assert_eq!(parse.diagnostics, &[]);
        let body = Body::new(parse.root().items().collect(), Kind::File);
        let mut out = String::new();
        write(&mut out, &body);
        assert_eq!(out, "{}\n");
    }
}

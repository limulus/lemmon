//! What a render returns: the page's text, the named ranges the host fills in it, and
//! what the page's collect blocks gathered beside it (D42).
//!
//! A mark is a value, not an instruction to the printer: `mark()` makes markup carrying
//! one range over its default content, and printing that markup anywhere copies its
//! marks in, shifted to where the text now stands. So a captured value places its mark
//! as often as it is printed, and never if it is not.

use std::collections::BTreeMap;
use std::rc::Rc;

use lemmon_syntax::Span;

use crate::Value;

/// A named range of the page, with its default content already in it, for the host's
/// handler to replace after the render (D42).
///
/// `start` and `end` are byte offsets into the text it stands in — the buffer while a
/// render is building one, the page when the render returns.
#[derive(Clone, Debug)]
pub struct Mark {
    /// The name the host's handlers are dispatched by.
    pub name: Rc<str>,
    /// What the handlers for that name are dispatched on, where the claim keys them: a
    /// code block's language word, or the payload field a `mark()` call filled (D42).
    pub key: Option<Rc<str>>,
    /// What the handler is given beside the range, where there is anything to give.
    pub payload: Option<Payload>,
    pub start: u32,
    pub end: u32,
    /// The file the `mark()` call stands in: an index into
    /// [`Module::files`](crate::Module::files). A mark made in a layout points there,
    /// not at the page being rendered.
    pub file: u32,
    /// The call, so that a handler's diagnostic can point at source (invariant 8).
    pub span: Span,
}

/// What a mark carries for its handler (D42).
#[derive(Clone, Debug, PartialEq)]
pub enum Payload {
    /// A record the `mark()` call built, carried with the mark.
    Value(Value),
    /// A payload the document reported when it compiled — a code block's — named by its
    /// id, so that nothing about the code crosses per render.
    Reported(Rc<str>),
}

/// Equality is what `==` on `html` asks: two marks that would place the same content in
/// the same place are equal, wherever the `mark()` that made them was written. One rule,
/// so the derived equality of a [`Value`] cannot say something else. The key and the
/// payload are part of it because they are what the handler is dispatched on and given,
/// so two marks that differ in either place different content.
impl PartialEq for Mark {
    fn eq(&self, other: &Self) -> bool {
        (&self.name, &self.key, &self.payload, self.start, self.end)
            == (
                &other.name,
                &other.key,
                &other.payload,
                other.start,
                other.end,
            )
    }
}

/// Markup carrying marks: what `mark()` makes, and what a capture whose output holds one
/// becomes. Markup with no mark stays a plain [`Value::Html`], so nothing pays for this
/// but the pages that use it.
#[derive(Clone, Debug, PartialEq)]
pub struct Marked {
    pub text: Rc<str>,
    pub marks: Box<[Mark]>,
}

/// One output buffer: the text rendered into it, and the marks that came with the values
/// printed there.
#[derive(Debug, Default)]
pub(crate) struct Buffer {
    text: String,
    marks: Vec<Mark>,
}

impl Buffer {
    /// Appends text with no marks of its own.
    pub(crate) fn push_str(&mut self, text: &str) {
        self.text.push_str(text);
    }

    /// Appends marked markup, each of its marks shifted to where its text now stands.
    ///
    /// # Errors
    ///
    /// If the buffer has grown past what a `u32` offset can name. A `usize` is thirty-two
    /// bits in the `.wasm`, so this is an error rather than an `as` that would wrap or a
    /// panic that would poison the instance (invariant 8).
    pub(crate) fn push_marked(&mut self, marked: &Marked) -> Result<(), String> {
        let by = self.len()?;
        for mark in &marked.marks {
            self.marks.push(Mark {
                name: mark.name.clone(),
                key: mark.key.clone(),
                payload: mark.payload.clone(),
                start: shift(mark.start, by)?,
                end: shift(mark.end, by)?,
                file: mark.file,
                span: mark.span,
            });
        }
        self.text.push_str(&marked.text);
        Ok(())
    }

    /// The text's length as an offset.
    ///
    /// # Errors
    ///
    /// As [`push_marked`](Self::push_marked).
    pub(crate) fn len(&self) -> Result<u32, String> {
        u32::try_from(self.text.len()).map_err(|_| TOO_LONG.to_owned())
    }

    /// What the buffer is as a value once its capture ends: plain markup, or marked
    /// markup where a mark was printed into it.
    pub(crate) fn into_value(self) -> Value {
        if self.marks.is_empty() {
            return Value::html(self.text);
        }
        Value::Marked(Rc::new(Marked {
            text: Rc::from(self.text),
            marks: self.marks.into_boxed_slice(),
        }))
    }

    /// The buffer taken apart: its text and the marks standing in it.
    pub(crate) fn into_parts(self) -> (String, Vec<Mark>) {
        (self.text, self.marks)
    }
}

/// So that a value prints into a buffer as it printed into a `String`.
impl std::fmt::Write for Buffer {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        self.text.push_str(text);
        Ok(())
    }
}

const TOO_LONG: &str = "this page is too long to mark: over four gigabytes of output";

fn shift(offset: u32, by: u32) -> Result<u32, String> {
    offset.checked_add(by).ok_or_else(|| TOO_LONG.to_owned())
}

/// One collect block's rendered body — a `lem-style`'s CSS, a `lem-head`'s markup, a
/// `lem-script`'s URL — kept once under its name however often the block ran.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fragment {
    /// The text, as the block rendered it.
    pub text: Rc<str>,
    /// The file of the block that first gave this text: an index into
    /// [`Module::files`](crate::Module::files).
    pub file: u32,
    /// That block's tag, so that a host's handler can point at it (invariant 8).
    pub span: Span,
}

/// A render's fragments, by name: names in sorted order, and each name's fragments in
/// the order a render first reached them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fragments(BTreeMap<Rc<str>, Vec<Fragment>>);

impl Fragments {
    /// Adds `fragment` under `name`, unless its text is empty or equal to one already
    /// there. Dedup is by content, not by block: a component called three times gives
    /// its styles once, and one whose styles print a parameter gives one per distinct
    /// value. An empty body has nothing for a host to bundle.
    pub(crate) fn push(&mut self, name: &str, fragment: Fragment) {
        if fragment.text.is_empty() {
            return;
        }
        if !self.0.contains_key(name) {
            self.0.insert(Rc::from(name), Vec::new());
        }
        let fragments = self.0.get_mut(name).expect("inserted above");
        if fragments.iter().all(|kept| kept.text != fragment.text) {
            fragments.push(fragment);
        }
    }

    /// The fragments under `name`, in first-reached order.
    #[must_use]
    pub fn get(&self, name: &str) -> &[Fragment] {
        self.0.get(name).map_or(&[], Vec::as_slice)
    }

    /// Every name with fragments, in sorted order, with its fragments.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &[Fragment])> {
        self.0
            .iter()
            .map(|(name, fragments)| (&**name, fragments.as_slice()))
    }
}

/// A page as [`render`](crate::render) returns it.
#[derive(Clone, Debug, PartialEq)]
pub struct Rendered {
    /// The page's text.
    pub html: String,
    /// The ranges the host's handlers fill, in non-decreasing order of `start`: marks
    /// are recorded as the text is printed, and two at one offset stand in print order.
    pub marks: Vec<Mark>,
    /// What its collect blocks gathered.
    pub fragments: Fragments,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fragment(text: &str, start: u32) -> Fragment {
        Fragment {
            text: Rc::from(text),
            file: 0,
            span: Span::new(start, start + 1),
        }
    }

    fn texts(fragments: &Fragments, name: &str) -> Vec<String> {
        fragments
            .get(name)
            .iter()
            .map(|f| f.text.to_string())
            .collect()
    }

    #[test]
    fn keeps_equal_text_once_with_the_first_span() {
        let mut fragments = Fragments::default();
        fragments.push("styles", fragment("a {}", 1));
        fragments.push("styles", fragment("b {}", 2));
        fragments.push("styles", fragment("a {}", 3));
        assert_eq!(texts(&fragments, "styles"), ["a {}", "b {}"]);
        assert_eq!(fragments.get("styles")[0].span, Span::new(1, 2));
    }

    #[test]
    fn keeps_names_apart() {
        let mut fragments = Fragments::default();
        fragments.push("styles", fragment("x", 1));
        fragments.push("head", fragment("x", 2));
        let names: Vec<&str> = fragments.iter().map(|(name, _)| name).collect();
        assert_eq!(names, ["head", "styles"]);
        assert_eq!(texts(&fragments, "head"), ["x"]);
    }

    #[test]
    fn collects_nothing_for_an_empty_body() {
        let mut fragments = Fragments::default();
        fragments.push("styles", fragment("", 1));
        assert_eq!(fragments, Fragments::default());
        assert!(fragments.get("styles").is_empty());
    }
}

//! Runtime values.
//!
//! Values are immutable and reference-counted, so passing one around — into a call, into
//! a closure's captures, out of a `lem-let` — is a pointer copy. The VM never mutates a
//! value after it is built; `MakeList`/`MakeMap` build it in one go.
//!
//! The VM is the runtime for `any`: what the checker could not decide statically — an
//! operation on a value typed `any`, which the compiler lets through unchecked — it
//! checks here, and the errors below are raised with a span. On a fully typed program
//! none of them can fire. The escaping rule is type-directed by variant either way:
//! [`Value::Str`] is untrusted text and is escaped on emit; [`Value::Html`] is trusted
//! markup and is emitted verbatim (invariant 7).
//!
//! A [`Value::Record`] carries the name of its declared type (D35): that tag is what a
//! type test reads at run time, and the one thing that tells a record from a map, whose
//! entries are never fields to the checker. A record has no printed form.

use std::borrow::Cow;
use std::cmp::Ordering;
use std::fmt::{self, Write as _};
use std::rc::Rc;

use crate::datetime::Datetime;
use crate::rendered::{Buffer, Marked};

/// An insertion-ordered string-keyed map — the shape front-matter records and map
/// literals take. Lookup is linear; records in the spike have a handful of fields.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Map {
    entries: Vec<(Rc<str>, Value)>,
}

impl Map {
    /// The value under `key`, if any.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.entries
            .iter()
            .find(|(k, _)| &**k == key)
            .map(|(_, v)| v)
    }

    /// The entries in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = (&Rc<str>, &Value)> {
        self.entries.iter().map(|(k, v)| (k, v))
    }

    /// The `index`th entry in insertion order. What `lem-for` walks a map by: reaching
    /// the nth entry through [`iter`](Self::iter) would make one loop quadratic.
    #[must_use]
    pub fn entry(&self, index: usize) -> Option<(&Rc<str>, &Value)> {
        self.entries.get(index).map(|(k, v)| (k, v))
    }

    /// How many entries the map holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the map holds no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Map {
    /// A map from `entries` whose keys are unique by construction — a JSON object, a
    /// record's declared fields, a map rebuilt entry by entry. Skips the scan for an
    /// earlier occurrence of each key that [`FromIterator`] makes, which would make a
    /// wide map quadratic to build.
    #[must_use]
    pub fn from_unique(entries: impl IntoIterator<Item = (Rc<str>, Value)>) -> Self {
        let entries: Vec<(Rc<str>, Value)> = entries.into_iter().collect();
        debug_assert!(
            {
                let mut keys: Vec<&Rc<str>> = entries.iter().map(|(key, _)| key).collect();
                keys.sort();
                keys.windows(2).all(|pair| pair[0] != pair[1])
            },
            "a key given twice to `Map::from_unique`"
        );
        Self { entries }
    }
}

impl FromIterator<(Rc<str>, Value)> for Map {
    /// A later entry under an existing key replaces the earlier one in place, so a map
    /// never holds a key twice and the first occurrence fixes its position.
    fn from_iter<I: IntoIterator<Item = (Rc<str>, Value)>>(iter: I) -> Self {
        let mut map = Self::default();
        for (key, value) in iter {
            match map.entries.iter_mut().find(|(k, _)| *k == key) {
                Some(slot) => slot.1 = value,
                None => map.entries.push((key, value)),
            }
        }
        map
    }
}

/// A function value: a compiled function plus the enclosing locals it closed over.
///
/// Captures are copied, not referenced — values are immutable, so the two are
/// indistinguishable, and copying keeps closures free of any shared environment.
#[derive(Clone, Debug, PartialEq)]
pub struct Closure {
    /// Index into the module's functions.
    pub func: u32,
    /// Captured values, in the order the function's `captures` table names their slots.
    pub captures: Vec<Value>,
}

/// A record value: its declared type's name and its fields, by name. Fields are read
/// by name rather than by declaration index — the index is an optimisation nothing
/// reads yet (D35).
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    /// The declared type's name — the most specific type the value was made as.
    pub name: Rc<str>,
    /// Every field the declaration and its ancestors have, an omitted optional as null.
    pub fields: Map,
}

/// A runtime value. See the module docs for the `Str`/`Html` distinction.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// The absent value of an optional.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// A 64-bit integer.
    Int(i64),
    /// A 64-bit float.
    Float(f64),
    /// Untrusted text; escaped on emit.
    Str(Rc<str>),
    /// Trusted markup; emitted verbatim.
    Html(Rc<str>),
    /// Trusted markup carrying marks — named ranges the host fills after the render
    /// (D42). Behind an `Rc` so that a `Value` stays 24 bytes (D38).
    Marked(Rc<Marked>),
    /// A moment, and the offset it was written in; equal and ordered by the moment.
    Datetime(Datetime),
    /// A span of time, in whole seconds. Has no printed form: a site says how it wants
    /// one read, `readingTime.minutes()`.
    Duration(i64),
    /// A URL, percent-encoded when it was made (`url.rs`); escaped on emit.
    Url(Rc<str>),
    /// An asset, holding the hashed URL the manifest gave its logical path; escaped on
    /// emit. What an author wrote is resolved before a value exists (D38).
    Asset(Rc<str>),
    /// An immutable list.
    List(Rc<[Value]>),
    /// An immutable insertion-ordered map.
    Map(Rc<Map>),
    /// A function value — a `lem-fn` named without parentheses, or a lambda.
    Fn(Rc<Closure>),
    /// A record, tagged with its declared type.
    Record(Rc<Record>),
}

impl Value {
    /// An untrusted string.
    pub fn str(text: impl AsRef<str>) -> Self {
        Self::Str(Rc::from(text.as_ref()))
    }

    /// Trusted markup.
    pub fn html(text: impl AsRef<str>) -> Self {
        Self::Html(Rc::from(text.as_ref()))
    }

    /// A URL, already percent-encoded.
    pub fn url(text: impl AsRef<str>) -> Self {
        Self::Url(Rc::from(text.as_ref()))
    }

    /// An asset, holding the URL the manifest resolved it to.
    pub fn asset(url: impl AsRef<str>) -> Self {
        Self::Asset(Rc::from(url.as_ref()))
    }

    /// A list.
    #[must_use]
    pub fn list(items: Vec<Self>) -> Self {
        Self::List(Rc::from(items))
    }

    /// A map.
    #[must_use]
    pub fn map(map: Map) -> Self {
        Self::Map(Rc::new(map))
    }

    /// A record of the declared type `name`.
    #[must_use]
    pub fn record(name: impl AsRef<str>, fields: Map) -> Self {
        Self::Record(Rc::new(Record {
            name: Rc::from(name.as_ref()),
            fields,
        }))
    }

    /// Whether this value holds marked markup anywhere inside it.
    ///
    /// The one thing a mark's payload may not carry (D42): a mark names a range of the
    /// page it was printed into, and a payload is never printed, so a mark inside one
    /// would stand for a range of nothing. Checked where the payload is made rather than
    /// where a handler receives it, so the error points at the call (invariant 8).
    #[must_use]
    pub fn carries_a_mark(&self) -> bool {
        match self {
            Self::Marked(_) => true,
            Self::List(items) => items.iter().any(Self::carries_a_mark),
            Self::Map(map) => map.iter().any(|(_, value)| value.carries_a_mark()),
            Self::Record(record) => record
                .fields
                .iter()
                .any(|(_, value)| value.carries_a_mark()),
            // A lambda's captures are copied values like any other, and a captured
            // capture can hold a mark.
            Self::Fn(closure) => closure.captures.iter().any(Self::carries_a_mark),
            Self::Null
            | Self::Bool(_)
            | Self::Int(_)
            | Self::Float(_)
            | Self::Str(_)
            | Self::Html(_)
            | Self::Datetime(_)
            | Self::Duration(_)
            | Self::Url(_)
            | Self::Asset(_) => false,
        }
    }

    /// The name the language uses for this value's type, for messages: a record names
    /// its declared type.
    #[must_use]
    pub fn type_name(&self) -> Cow<'static, str> {
        Cow::Borrowed(match self {
            Self::Null => "null",
            Self::Bool(_) => "bool",
            Self::Int(_) => "int",
            Self::Float(_) => "float",
            Self::Str(_) => "string",
            Self::Html(_) | Self::Marked(_) => "html",
            Self::Datetime(_) => "datetime",
            Self::Duration(_) => "duration",
            Self::Url(_) => "url",
            Self::Asset(_) => "asset",
            Self::List(_) => "list",
            Self::Map(_) => "map",
            Self::Fn(_) => "function",
            Self::Record(record) => return Cow::Owned(format!("`{}` record", record.name)),
        })
    }

    /// How a `lem-if` reads this value: a `bool` is itself; an optional is its
    /// non-null test. The checker rejects a condition that is neither, so anything else
    /// reaching here came through `any`, and counts as present when it is not null.
    #[must_use]
    pub const fn as_condition(&self) -> bool {
        match self {
            Self::Null => false,
            Self::Bool(b) => *b,
            _ => true,
        }
    }

    /// Appends this value to `out` the way `<lem.…>` prints it — marked markup with its
    /// marks, shifted to where its text now stands.
    ///
    /// # Errors
    ///
    /// For a value that has no printed form — `null`, a `bool`, a list, a map, or a
    /// function. The checker reports these at compile time; here they come through
    /// `any`. And for output too long to carry a mark's offsets.
    pub(crate) fn emit(&self, out: &mut Buffer) -> Result<(), String> {
        match self {
            Self::Str(s) => escape_html(s, out),
            Self::Html(h) => out.push_str(h),
            Self::Marked(marked) => out.push_marked(marked)?,
            Self::Int(i) => {
                let _ = write!(out, "{i}");
            }
            Self::Float(f) => {
                let _ = write!(out, "{f}");
            }
            Self::Datetime(d) => {
                let _ = write!(out, "{d}");
            }
            // Percent-encoding is not HTML escaping: `&` and `'` survive it, and both
            // have to be escaped to stand in an attribute.
            Self::Url(u) | Self::Asset(u) => escape_html(u, out),
            other => return Err(format!("cannot print a {}", other.type_name())),
        }
        Ok(())
    }

    /// Structural equality as `==` sees it. Two records are equal when they are of one
    /// declared type and their fields are; a record beside a map is never equal, as
    /// the checker would never let the two be compared.
    ///
    /// # Errors
    ///
    /// If either side is a function: functions have no equality.
    pub fn equals(&self, other: &Self) -> Result<bool, String> {
        match (self, other) {
            (Self::Fn(_), _) | (_, Self::Fn(_)) => {
                Err("functions cannot be compared".to_owned())
            }
            (Self::List(a), Self::List(b)) => {
                if a.len() != b.len() {
                    return Ok(false);
                }
                for (x, y) in a.iter().zip(b.iter()) {
                    if !x.equals(y)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            (Self::Map(a), Self::Map(b)) => maps_equal(a, b),
            (Self::Record(a), Self::Record(b)) => {
                if a.name != b.name {
                    return Ok(false);
                }
                maps_equal(&a.fields, &b.fields)
            }
            (Self::Int(a), Self::Float(b)) | (Self::Float(b), Self::Int(a)) =>
            {
                #[allow(clippy::cast_precision_loss, reason = "numeric comparison")]
                Ok((*a as f64) == *b)
            }
            // A datetime among them: `Datetime`'s own equality is by instant, so two
            // offsets naming one moment are one value here as they are to `lt`.
            _ => Ok(self == other),
        }
    }

    /// The order `lt`, `lte`, `gt`, and `gte` read, and the one a sort reads: one rule
    /// for what is ordered, so the two cannot drift apart.
    ///
    /// Numbers order numerically whatever their type, strings by code point, datetimes
    /// by their instant. `Ok(None)` is a pair with no order between them, which the
    /// caller words — an operator names itself, a sort names the keys it takes.
    ///
    /// # Errors
    ///
    /// For a NaN, which is a number with an order to nothing, itself included.
    pub fn order(&self, other: &Self) -> Result<Option<Ordering>, String> {
        let float = |a: f64, b: f64| {
            a.partial_cmp(&b)
                .map(Some)
                .ok_or_else(|| "cannot order a NaN".to_owned())
        };
        #[allow(
            clippy::cast_precision_loss,
            reason = "int/float promotion is the rule"
        )]
        match (self, other) {
            (Self::Str(a), Self::Str(b)) => Ok(Some(a.cmp(b))),
            (Self::Datetime(a), Self::Datetime(b)) => Ok(Some(a.cmp(b))),
            (Self::Int(a), Self::Int(b)) => Ok(Some(a.cmp(b))),
            (Self::Int(a), Self::Float(b)) => float(*a as f64, *b),
            (Self::Float(a), Self::Int(b)) => float(*a, *b as f64),
            (Self::Float(a), Self::Float(b)) => float(*a, *b),
            _ => Ok(None),
        }
    }
}

/// Whether two maps hold the same keys with equal values, as [`Value::equals`] sees it.
fn maps_equal(a: &Map, b: &Map) -> Result<bool, String> {
    if a.len() != b.len() {
        return Ok(false);
    }
    for (key, x) in a.iter() {
        match b.get(key) {
            Some(y) if x.equals(y)? => {}
            _ => return Ok(false),
        }
    }
    Ok(true)
}

/// Appends `text` to `out` with `& < > " '` replaced by entities — safe in text content
/// and in quoted attribute values alike.
pub fn escape_html(text: &str, out: &mut impl fmt::Write) {
    for c in text.chars() {
        let _ = match c {
            '&' => out.write_str("&amp;"),
            '<' => out.write_str("&lt;"),
            '>' => out.write_str("&gt;"),
            '"' => out.write_str("&quot;"),
            '\'' => out.write_str("&#39;"),
            other => out.write_char(other),
        };
    }
}

#[cfg(test)]
mod tests {
    use lemmon_syntax::Span;

    use super::*;
    use crate::rendered::Mark;

    /// Two values that would put the same content in the same place are one value,
    /// wherever the `mark()` calls that made them were written: `==` on `html` asks
    /// about the page, not about the source.
    #[test]
    fn compares_marked_markup_by_what_its_marks_place() {
        let mark = |name: &str, end: u32, at: u32| Mark {
            name: Rc::from(name),
            key: None,
            payload: None,
            start: 0,
            end,
            file: 0,
            span: Span::new(at, at + 1),
        };
        let marked = |mark: Mark| {
            Value::Marked(Rc::new(Marked {
                text: Rc::from("x"),
                marks: Box::from([mark]),
            }))
        };
        let styles = marked(mark("styles", 1, 4));
        assert_eq!(styles.equals(&marked(mark("styles", 1, 40))), Ok(true));
        assert_eq!(styles.equals(&marked(mark("head", 1, 4))), Ok(false));
        assert_eq!(styles.equals(&marked(mark("styles", 0, 4))), Ok(false));
        // The key and the payload are what the handler is dispatched on and given, so
        // two marks differing in either place different content and are not equal.
        let keyed = |key: Option<&str>, payload: Option<Value>| {
            marked(Mark {
                key: key.map(Rc::from),
                payload,
                ..mark("code", 1, 4)
            })
        };
        assert_eq!(
            keyed(Some("ts"), None).equals(&keyed(Some("ts"), None)),
            Ok(true)
        );
        assert_eq!(
            keyed(Some("ts"), None).equals(&keyed(Some("rs"), None)),
            Ok(false)
        );
        assert_eq!(
            keyed(Some("ts"), None).equals(&keyed(None, None)),
            Ok(false)
        );
        assert_eq!(
            keyed(None, Some(Value::str("a"))).equals(&keyed(None, Some(Value::str("b")))),
            Ok(false)
        );
        // Markup of the same text with no mark in it places nothing, so it is neither.
        assert_eq!(styles.equals(&Value::html("x")), Ok(false));
        assert_eq!(Value::html("x").equals(&styles), Ok(false));
    }

    /// A value is copied on every push, pop, and capture, so its size is a budget (D38).
    /// `Marked` rides behind an `Rc` to stay inside it.
    #[test]
    fn stays_within_its_budget_of_twenty_four_bytes() {
        assert_eq!(size_of::<Value>(), 24);
    }

    #[test]
    fn builds_a_map_from_unique_entries_in_their_order() {
        let map = Map::from_unique([
            (Rc::from("z"), Value::Int(1)),
            (Rc::from("a"), Value::Int(2)),
        ]);
        let keys: Vec<&str> = map.iter().map(|(key, _)| &**key).collect();
        assert_eq!(keys, ["z", "a"]);
        assert_eq!(map.get("a"), Some(&Value::Int(2)));
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "a key given twice to `Map::from_unique`")]
    fn refuses_a_repeated_key_given_as_unique() {
        let _ = Map::from_unique([
            (Rc::from("a"), Value::Int(1)),
            (Rc::from("a"), Value::Int(2)),
        ]);
    }

    fn emitted(value: &Value) -> Result<String, String> {
        let mut buffer = Buffer::default();
        value.emit(&mut buffer)?;
        Ok(buffer.into_parts().0)
    }

    #[test]
    fn escapes_every_character_that_matters_in_text_and_attributes() {
        assert_eq!(
            emitted(&Value::str("a & b < c > \"d\" 'e'")).unwrap(),
            "a &amp; b &lt; c &gt; &quot;d&quot; &#39;e&#39;"
        );
    }

    #[test]
    fn emits_html_verbatim() {
        assert_eq!(emitted(&Value::html("<b>&</b>")).unwrap(), "<b>&</b>");
    }

    #[test]
    fn prints_numbers_in_decimal() {
        assert_eq!(emitted(&Value::Int(-42)).unwrap(), "-42");
        assert_eq!(emitted(&Value::Float(1.5)).unwrap(), "1.5");
    }

    fn when(text: &str) -> Datetime {
        Datetime::parse(text).expect("a datetime")
    }

    #[test]
    fn prints_a_datetime_as_rfc_3339_in_the_offset_it_was_written_in() {
        let written = "2026-04-04T10:30:00-07:00";
        assert_eq!(emitted(&Value::Datetime(when(written))).unwrap(), written);
    }

    #[test]
    fn escapes_a_url_and_an_asset_as_html_though_both_are_already_encoded() {
        // Percent-encoding leaves `&` and `'` alone and an attribute cannot.
        let composed = "/tils/?a=1&b='2'";
        let escaped = "/tils/?a=1&amp;b=&#39;2&#39;";
        assert_eq!(emitted(&Value::url(composed)).unwrap(), escaped);
        assert_eq!(emitted(&Value::asset(composed)).unwrap(), escaped);
    }

    #[test]
    fn refuses_to_print_a_value_with_no_printed_form() {
        for value in [
            Value::Null,
            Value::Bool(true),
            // A duration has no one spelling a site would want: `.minutes()` says.
            Value::Duration(90),
            Value::list(vec![]),
            Value::map(Map::default()),
            Value::Fn(Rc::new(Closure {
                func: 0,
                captures: vec![],
            })),
            Value::record("Article", Map::default()),
        ] {
            let message = emitted(&value).unwrap_err();
            assert_eq!(message, format!("cannot print a {}", value.type_name()));
        }
        assert_eq!(
            emitted(&Value::record("Article", Map::default())).unwrap_err(),
            "cannot print a `Article` record"
        );
    }

    fn article(title: &str) -> Value {
        Value::record(
            "Article",
            [(Rc::from("title"), Value::str(title))]
                .into_iter()
                .collect(),
        )
    }

    #[test]
    fn compares_records_by_type_and_fields() {
        assert!(article("a").equals(&article("a")).unwrap());
        assert!(!article("a").equals(&article("b")).unwrap());
        let link_post = Value::record(
            "LinkPost",
            [(Rc::from("title"), Value::str("a"))].into_iter().collect(),
        );
        assert!(!article("a").equals(&link_post).unwrap());
        let map = Value::map([(Rc::from("title"), Value::str("a"))].into_iter().collect());
        assert!(!article("a").equals(&map).unwrap());
        assert!(!map.equals(&article("a")).unwrap());
        assert!(article("a").as_condition());
    }

    #[test]
    fn reads_a_condition_as_bool_or_as_a_null_test() {
        assert!(!Value::Null.as_condition());
        assert!(!Value::Bool(false).as_condition());
        assert!(Value::Bool(true).as_condition());
        assert!(Value::Int(0).as_condition());
        assert!(Value::str("").as_condition());
    }

    #[test]
    fn compares_lists_and_maps_structurally() {
        let a = Value::list(vec![Value::Int(1), Value::str("x")]);
        let b = Value::list(vec![Value::Int(1), Value::str("x")]);
        assert!(a.equals(&b).unwrap());
        assert!(!a.equals(&Value::list(vec![Value::Int(1)])).unwrap());
        assert!(
            !a.equals(&Value::list(vec![Value::Int(1), Value::str("y")]))
                .unwrap()
        );
        let m1: Map = [(Rc::from("k"), Value::Int(1))].into_iter().collect();
        let m2: Map = [(Rc::from("k"), Value::Int(1))].into_iter().collect();
        let m3: Map = [(Rc::from("k"), Value::Int(2))].into_iter().collect();
        assert!(Value::map(m1.clone()).equals(&Value::map(m2)).unwrap());
        assert!(!Value::map(m1.clone()).equals(&Value::map(m3)).unwrap());
        assert!(!Value::map(m1).equals(&Value::map(Map::default())).unwrap());
    }

    #[test]
    fn names_every_type_the_way_the_language_does() {
        let names: Vec<String> = [
            Value::Null,
            Value::Bool(true),
            Value::Int(1),
            Value::Float(1.0),
            Value::str(""),
            Value::html(""),
            Value::Datetime(when("2024-11-24")),
            Value::Duration(0),
            Value::url(""),
            Value::asset(""),
            Value::list(vec![]),
            Value::map(Map::default()),
            Value::Fn(Rc::new(Closure {
                func: 0,
                captures: vec![],
            })),
            Value::record("Photo", Map::default()),
        ]
        .iter()
        .map(|value| value.type_name().into_owned())
        .collect();
        assert_eq!(
            names,
            [
                "null",
                "bool",
                "int",
                "float",
                "string",
                "html",
                "datetime",
                "duration",
                "url",
                "asset",
                "list",
                "map",
                "function",
                "`Photo` record"
            ]
        );
    }

    #[test]
    fn treats_maps_with_different_keys_as_unequal() {
        let a: Map = [(Rc::from("k"), Value::Int(1))].into_iter().collect();
        let b: Map = [(Rc::from("j"), Value::Int(1))].into_iter().collect();
        assert!(!Value::map(a).equals(&Value::map(b)).unwrap());
    }

    #[test]
    fn orders_strings_numbers_and_datetimes_and_nothing_else() {
        let ordered = |a: Value, b: Value| a.order(&b).unwrap();
        assert_eq!(
            ordered(Value::str("a"), Value::str("b")),
            Some(Ordering::Less)
        );
        assert_eq!(
            ordered(Value::Int(2), Value::Float(2.0)),
            Some(Ordering::Equal)
        );
        assert_eq!(
            ordered(Value::Float(3.0), Value::Int(2)),
            Some(Ordering::Greater)
        );
        assert_eq!(ordered(Value::Int(1), Value::Int(1)), Some(Ordering::Equal));
        assert_eq!(
            ordered(
                Value::Datetime(when("2026-04-04T10:30:00-07:00")),
                Value::Datetime(when("2026-04-04T17:30:00Z"))
            ),
            Some(Ordering::Equal)
        );
        for (a, b) in [
            (Value::str("a"), Value::Int(1)),
            (Value::Bool(true), Value::Bool(false)),
            (Value::url("/a/"), Value::url("/b/")),
            (Value::Duration(1), Value::Duration(2)),
            (Value::Null, Value::Null),
        ] {
            assert_eq!(a.order(&b).unwrap(), None, "{a:?} {b:?}");
        }
    }

    #[test]
    fn refuses_to_order_a_nan_with_anything_itself_included() {
        let nan = Value::Float(f64::NAN);
        for other in [Value::Float(1.0), Value::Int(1), Value::Float(f64::NAN)] {
            assert_eq!(nan.order(&other), Err("cannot order a NaN".to_owned()));
            assert_eq!(other.order(&nan), Err("cannot order a NaN".to_owned()));
        }
    }

    #[test]
    fn compares_a_datetime_by_its_instant_and_never_by_its_offset() {
        let here = Value::Datetime(when("2026-04-04T10:30:00-07:00"));
        let there = Value::Datetime(when("2026-04-04T17:30:00Z"));
        assert!(here.equals(&there).unwrap());
        assert!(!here.equals(&Value::Datetime(when("2024-11-24"))).unwrap());
    }

    #[test]
    fn compares_an_int_with_a_float_numerically() {
        assert!(Value::Int(2).equals(&Value::Float(2.0)).unwrap());
        assert!(!Value::Float(2.5).equals(&Value::Int(2)).unwrap());
    }

    #[test]
    fn refuses_to_compare_functions() {
        let f = Value::Fn(Rc::new(Closure {
            func: 0,
            captures: vec![],
        }));
        assert_eq!(f.equals(&f).unwrap_err(), "functions cannot be compared");
        assert!(Value::Null.equals(&f).is_err());
    }

    #[test]
    fn treats_a_list_and_a_map_as_unequal_without_error() {
        assert!(
            !Value::list(vec![])
                .equals(&Value::map(Map::default()))
                .unwrap()
        );
    }

    #[test]
    fn keeps_the_first_position_of_a_key_written_twice() {
        let map: Map = [
            (Rc::from("a"), Value::Int(1)),
            (Rc::from("b"), Value::Int(2)),
            (Rc::from("a"), Value::Int(3)),
        ]
        .into_iter()
        .collect();
        let keys: Vec<&str> = map.iter().map(|(k, _)| &**k).collect();
        assert_eq!(keys, ["a", "b"]);
        assert_eq!(map.get("a"), Some(&Value::Int(3)));
        assert_eq!(map.len(), 2);
        assert!(!map.is_empty());
        assert!(map.get("z").is_none());
    }
}

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

use std::fmt::Write as _;
use std::rc::Rc;

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
    /// An immutable list.
    List(Rc<[Value]>),
    /// An immutable insertion-ordered map.
    Map(Rc<Map>),
    /// A function value — a `lem-fn` named without parentheses, or a lambda.
    Fn(Rc<Closure>),
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

    /// The name the language uses for this value's type, for messages.
    #[must_use]
    pub const fn type_name(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool(_) => "bool",
            Self::Int(_) => "int",
            Self::Float(_) => "float",
            Self::Str(_) => "string",
            Self::Html(_) => "html",
            Self::List(_) => "list",
            Self::Map(_) => "map",
            Self::Fn(_) => "function",
        }
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

    /// Appends this value to `out` the way `<lem.…>` prints it.
    ///
    /// # Errors
    ///
    /// For a value that has no printed form — `null`, a `bool`, a list, a map, or a
    /// function. The checker reports these at compile time; here they come through
    /// `any`.
    pub fn emit(&self, out: &mut String) -> Result<(), String> {
        match self {
            Self::Str(s) => escape_html(s, out),
            Self::Html(h) => out.push_str(h),
            Self::Int(i) => {
                let _ = write!(out, "{i}");
            }
            Self::Float(f) => {
                let _ = write!(out, "{f}");
            }
            other => return Err(format!("cannot print a {}", other.type_name())),
        }
        Ok(())
    }

    /// Structural equality as `==` sees it.
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
            (Self::Map(a), Self::Map(b)) => {
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
            (Self::Int(a), Self::Float(b)) | (Self::Float(b), Self::Int(a)) =>
            {
                #[allow(clippy::cast_precision_loss, reason = "numeric comparison")]
                Ok((*a as f64) == *b)
            }
            _ => Ok(self == other),
        }
    }
}

/// Appends `text` to `out` with `& < > " '` replaced by entities — safe in text content
/// and in quoted attribute values alike.
pub fn escape_html(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn emitted(value: &Value) -> Result<String, String> {
        let mut out = String::new();
        value.emit(&mut out).map(|()| out)
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

    #[test]
    fn refuses_to_print_a_value_with_no_printed_form() {
        for value in [
            Value::Null,
            Value::Bool(true),
            Value::list(vec![]),
            Value::map(Map::default()),
            Value::Fn(Rc::new(Closure {
                func: 0,
                captures: vec![],
            })),
        ] {
            let message = emitted(&value).unwrap_err();
            assert_eq!(message, format!("cannot print a {}", value.type_name()));
        }
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
        let names: Vec<&str> = [
            Value::Null,
            Value::Bool(true),
            Value::Int(1),
            Value::Float(1.0),
            Value::str(""),
            Value::html(""),
            Value::list(vec![]),
            Value::map(Map::default()),
            Value::Fn(Rc::new(Closure {
                func: 0,
                captures: vec![],
            })),
        ]
        .iter()
        .map(Value::type_name)
        .collect();
        assert_eq!(
            names,
            [
                "null", "bool", "int", "float", "string", "html", "list", "map", "function"
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

//! The functions the core ships: the ambient ones every template can call by name
//! (D23), and the methods a builtin type carries (D34).
//!
//! One table holds both, so a call — however it was written — is one `CallBuiltin` with
//! one id. What differs is how the compiler finds the entry: an ambient function by its
//! bare name through the value namespace, a method by the receiver's type through
//! [`method`], which reads no namespace at all. A method's receiver is its first
//! argument, so `s.slugify()` and a one-argument ambient function push the same stack.
//!
//! The stdlib proper arrives with `∆Q6L` and extends this table; the entries here are
//! the ones ∆495 needed to prove the two ways in, and `raw` — the one `string` → `html`
//! cast (invariant 7), which stays an ambient function so the cast stays conspicuous.

use crate::Value;
use crate::types::Type;

/// A builtin type as a method's receiver: [`Type`] with its element types dropped,
/// which is all dispatch needs — every `[T]` carries the same methods, and what `T` is
/// belongs to the entry's [typing rule](Builtin::ty).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    String,
    Int,
    Float,
    Bool,
    Datetime,
    Duration,
    Url,
    Html,
    Asset,
    List,
    Map,
}

impl Kind {
    /// The receiver kind of `ty`, when it has one. `any` has no type to dispatch on; an
    /// optional is unwrapped by the caller of `?.` before it asks; and a function, a
    /// declared type, and `null` carry no methods at all.
    #[must_use]
    pub const fn of(ty: &Type) -> Option<Self> {
        match ty {
            Type::String => Some(Self::String),
            Type::Int => Some(Self::Int),
            Type::Float => Some(Self::Float),
            Type::Bool => Some(Self::Bool),
            Type::Datetime => Some(Self::Datetime),
            Type::Duration => Some(Self::Duration),
            Type::Url => Some(Self::Url),
            Type::Html => Some(Self::Html),
            Type::Asset => Some(Self::Asset),
            Type::List(_) => Some(Self::List),
            Type::Map(_) => Some(Self::Map),
            Type::Any | Type::Null | Type::Optional(_) | Type::Fn(..) | Type::Named(_) => {
                None
            }
        }
    }

    /// The receiver kind of a value the VM holds, for a call the checker could not
    /// resolve because the receiver was `any`. A `datetime`, `duration`, `url`, or
    /// `asset` is a [`Value::Str`] at run time, so its methods are not here yet (∆Q6L,
    /// which decides how such a value is represented); until then one reached through
    /// `any` is looked for on `string` and reported as missing there.
    #[must_use]
    pub const fn of_value(value: &Value) -> Option<Self> {
        match value {
            Value::Str(_) => Some(Self::String),
            Value::Int(_) => Some(Self::Int),
            Value::Float(_) => Some(Self::Float),
            Value::Bool(_) => Some(Self::Bool),
            Value::Html(_) => Some(Self::Html),
            Value::List(_) => Some(Self::List),
            Value::Map(_) => Some(Self::Map),
            Value::Null | Value::Fn(_) => None,
        }
    }
}

/// How a builtin is reached.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reach {
    /// By its bare name, resolved in the value namespace: `raw(s)`.
    Ambient,
    /// As a method on a receiver of this kind: `s.slugify()`. The name is the type's,
    /// not the namespace's, so nothing an author binds can shadow or collide with it.
    Method(Kind),
}

/// One builtin function.
pub struct Builtin {
    /// The name templates call it by.
    pub name: &'static str,
    /// How the compiler finds it.
    pub reach: Reach,
    /// How many arguments it takes, a method's receiver among them.
    pub arity: usize,
    /// The typing rule: the result type for the given argument types, or the compile
    /// error's message. A rule rather than a signature, because the stdlib's list
    /// functions are generic over their element type (∆Q6L). A method's rule is asked
    /// only about receivers dispatch has already matched, so it types the result and
    /// does not re-check what kind the receiver was.
    pub ty: fn(&[Type]) -> Result<Type, String>,
    /// The implementation. An `Err` is the runtime error's message; the VM adds the span.
    pub call: fn(&[Value]) -> Result<Value, String>,
}

impl Builtin {
    /// How many arguments an author writes at the call — one fewer than a method takes,
    /// since the receiver is written to the left of the dot.
    #[must_use]
    pub const fn written_arity(&self) -> usize {
        match self.reach {
            Reach::Ambient => self.arity,
            Reach::Method(_) => self.arity - 1,
        }
    }

    /// How a diagnostic spells a call of this: a method by the dot that reaches it.
    #[must_use]
    pub fn written(&self) -> String {
        match self.reach {
            Reach::Ambient => format!("`{}`", self.name),
            Reach::Method(_) => format!("`.{}`", self.name),
        }
    }
}

/// Every builtin, indexed by the `id` a `CallBuiltin` carries.
pub const BUILTINS: &[Builtin] = &[
    Builtin {
        name: "raw",
        reach: Reach::Ambient,
        arity: 1,
        ty: raw_type,
        call: raw,
    },
    Builtin {
        name: "slugify",
        reach: Reach::Method(Kind::String),
        arity: 1,
        ty: slugify_type,
        call: slugify,
    },
    Builtin {
        name: "first",
        reach: Reach::Method(Kind::List),
        arity: 1,
        ty: first_type,
        call: first,
    },
    Builtin {
        name: "keys",
        reach: Reach::Method(Kind::Map),
        arity: 1,
        ty: keys_type,
        call: keys,
    },
];

/// The id of the ambient function called `name`, if there is one.
#[must_use]
pub fn lookup(name: &str) -> Option<u32> {
    id_of(|b| b.reach == Reach::Ambient && b.name == name)
}

/// The id of the method `kind` carries under `name`, if it carries one.
#[must_use]
pub fn method(kind: Kind, name: &str) -> Option<u32> {
    id_of(|b| b.reach == Reach::Method(kind) && b.name == name)
}

/// Every method name `kind` carries, for a diagnostic that lists what was available.
pub fn methods_of(kind: Kind) -> impl Iterator<Item = &'static str> {
    BUILTINS
        .iter()
        .filter(move |b| b.reach == Reach::Method(kind))
        .map(|b| b.name)
}

fn id_of(wanted: impl FnMut(&Builtin) -> bool) -> Option<u32> {
    BUILTINS
        .iter()
        .position(wanted)
        .map(|i| u32::try_from(i).expect("fewer than 2^32 builtins"))
}

/// `raw(s: string): html` — `html` in is `html` out, and `any` is the VM's to check.
fn raw_type(args: &[Type]) -> Result<Type, String> {
    match &args[0] {
        Type::String | Type::Html | Type::Any => Ok(Type::Html),
        other => Err(format!("raw() takes a string, got `{other}`")),
    }
}

/// `raw(s)`: vouches for a string as trusted markup. Already-trusted markup passes
/// through unchanged.
fn raw(args: &[Value]) -> Result<Value, String> {
    match &args[0] {
        Value::Str(s) => Ok(Value::Html(s.clone())),
        html @ Value::Html(_) => Ok(html.clone()),
        other => Err(format!("raw() takes a string, got {}", other.type_name())),
    }
}

/// `string.slugify(): string`.
fn slugify_type(_args: &[Type]) -> Result<Type, String> {
    Ok(Type::String)
}

/// `s.slugify()`: lower-cased, with every run of characters that are neither letters
/// nor digits a single `-`, and none at either end.
///
/// Letters are Unicode's, not ASCII's, so two titles that differ only outside ASCII
/// slugify differently — dropping `é` would give "Café Life" and "Caf Life" one slug
/// between them, and a page the other's URL. Folding a letter to its ASCII neighbour is
/// not here yet (∆Q6L); `url()` percent-encodes what is left.
fn slugify(args: &[Value]) -> Result<Value, String> {
    let text = match &args[0] {
        Value::Str(s) => s.to_lowercase(),
        other => return Err(method_receiver("slugify", "a string", other)),
    };
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            out.push(ch);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    Ok(Value::str(out.trim_matches('-')))
}

/// `[T].first(): T?` — the element type the receiver was built over, made optional.
fn first_type(args: &[Type]) -> Result<Type, String> {
    match &args[0] {
        Type::List(element) => Ok(Type::optional((**element).clone())),
        other => unreachable!("`first` is dispatched on a list, not a `{other}`"),
    }
}

/// `xs.first()`: the first element, or null when there is none. An empty list is not an
/// error here, unlike an out-of-range index — that is what the optional is for.
fn first(args: &[Value]) -> Result<Value, String> {
    match &args[0] {
        Value::List(items) => Ok(items.first().cloned().unwrap_or(Value::Null)),
        other => Err(method_receiver("first", "a list", other)),
    }
}

/// `{string: V}.keys(): [string]`.
fn keys_type(_args: &[Type]) -> Result<Type, String> {
    Ok(Type::list(Type::String))
}

/// `m.keys()`: the map's keys in insertion order.
fn keys(args: &[Value]) -> Result<Value, String> {
    match &args[0] {
        Value::Map(map) => Ok(Value::list(
            map.iter().map(|(k, _)| Value::Str(k.clone())).collect(),
        )),
        other => Err(method_receiver("keys", "a map", other)),
    }
}

/// The runtime refusal of a method reached on the wrong receiver. Only an `any` receiver
/// can get here — the checker matched every other one before the call was emitted — and
/// the VM checks it again rather than trusting the value (`value.rs`, module docs).
fn method_receiver(name: &str, wanted: &str, got: &Value) -> String {
    format!("`.{name}()` needs {wanted}, got {}", got.type_name())
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::*;
    use crate::{Closure, Map};

    fn call(id: u32, args: &[Value]) -> Result<Value, String> {
        (BUILTINS[id as usize].call)(args)
    }

    #[test]
    fn finds_an_ambient_function_by_name_and_never_a_method() {
        assert_eq!(lookup("raw"), Some(0));
        assert_eq!(lookup("slugify"), None);
        assert_eq!(lookup("nope"), None);
    }

    #[test]
    fn finds_a_method_by_its_receivers_kind() {
        assert_eq!(method(Kind::String, "slugify"), Some(1));
        assert_eq!(method(Kind::List, "first"), Some(2));
        assert_eq!(method(Kind::Map, "keys"), Some(3));
    }

    #[test]
    fn offers_a_kind_only_its_own_methods_and_never_an_ambient_function() {
        assert_eq!(method(Kind::List, "slugify"), None);
        assert_eq!(method(Kind::Int, "first"), None);
        assert_eq!(method(Kind::String, "raw"), None);
    }

    #[test]
    fn lists_the_methods_a_kind_carries() {
        assert_eq!(methods_of(Kind::String).collect::<Vec<_>>(), ["slugify"]);
        assert_eq!(methods_of(Kind::Bool).count(), 0);
    }

    #[test]
    fn reads_a_receiver_kind_off_a_type_and_off_nothing_that_dispatches_elsewhere() {
        assert_eq!(Kind::of(&Type::String), Some(Kind::String));
        assert_eq!(Kind::of(&Type::list(Type::Int)), Some(Kind::List));
        assert_eq!(Kind::of(&Type::map(Type::Int)), Some(Kind::Map));
        assert_eq!(Kind::of(&Type::Datetime), Some(Kind::Datetime));
        assert_eq!(Kind::of(&Type::Duration), Some(Kind::Duration));
        assert_eq!(Kind::of(&Type::Url), Some(Kind::Url));
        assert_eq!(Kind::of(&Type::Html), Some(Kind::Html));
        assert_eq!(Kind::of(&Type::Asset), Some(Kind::Asset));
        assert_eq!(Kind::of(&Type::Float), Some(Kind::Float));
        assert_eq!(Kind::of(&Type::Bool), Some(Kind::Bool));
        assert_eq!(Kind::of(&Type::Any), None);
        assert_eq!(Kind::of(&Type::Null), None);
        assert_eq!(Kind::of(&Type::optional(Type::String)), None);
        assert_eq!(Kind::of(&Type::function(vec![], Type::Int)), None);
        assert_eq!(Kind::of(&Type::Named(Rc::from("Article"))), None);
    }

    #[test]
    fn reads_a_receiver_kind_off_a_runtime_value() {
        assert_eq!(Kind::of_value(&Value::str("s")), Some(Kind::String));
        assert_eq!(Kind::of_value(&Value::html("<b>")), Some(Kind::Html));
        assert_eq!(Kind::of_value(&Value::Int(1)), Some(Kind::Int));
        assert_eq!(Kind::of_value(&Value::Float(1.0)), Some(Kind::Float));
        assert_eq!(Kind::of_value(&Value::Bool(true)), Some(Kind::Bool));
        assert_eq!(Kind::of_value(&Value::list(vec![])), Some(Kind::List));
        assert_eq!(Kind::of_value(&Value::map(Map::default())), Some(Kind::Map));
        assert_eq!(Kind::of_value(&Value::Null), None);
        let closure = Closure {
            func: 0,
            captures: vec![],
        };
        assert_eq!(Kind::of_value(&Value::Fn(Rc::new(closure))), None);
    }

    #[test]
    fn counts_a_methods_receiver_out_of_the_arity_an_author_writes() {
        assert_eq!(BUILTINS[0].written_arity(), 1);
        assert_eq!(BUILTINS[1].written_arity(), 0);
        assert_eq!(BUILTINS[0].written(), "`raw`");
        assert_eq!(BUILTINS[1].written(), "`.slugify`");
    }

    #[test]
    fn raw_turns_a_string_into_html_unchanged() {
        assert_eq!(raw(&[Value::str("<b>")]).unwrap(), Value::html("<b>"));
    }

    #[test]
    fn raw_passes_html_through() {
        assert_eq!(raw(&[Value::html("<b>")]).unwrap(), Value::html("<b>"));
    }

    #[test]
    fn raw_types_a_string_or_html_as_html_and_refuses_the_rest() {
        assert_eq!(raw_type(&[Type::String]), Ok(Type::Html));
        assert_eq!(raw_type(&[Type::Html]), Ok(Type::Html));
        assert_eq!(raw_type(&[Type::Any]), Ok(Type::Html));
        assert_eq!(
            raw_type(&[Type::Int]),
            Err("raw() takes a string, got `int`".to_owned())
        );
    }

    #[test]
    fn raw_refuses_anything_else() {
        assert_eq!(
            raw(&[Value::Int(1)]).unwrap_err(),
            "raw() takes a string, got int"
        );
    }

    #[test]
    fn slugify_lower_cases_and_joins_words_with_one_dash() {
        assert_eq!(
            call(1, &[Value::str("Hello,  World! -- Again")]).unwrap(),
            Value::str("hello-world-again")
        );
        assert_eq!(call(1, &[Value::str("  ")]).unwrap(), Value::str(""));
        // Distinct titles keep distinct slugs: a letter outside ASCII is a letter.
        assert_eq!(
            call(1, &[Value::str("Café Life")]).unwrap(),
            Value::str("café-life")
        );
        assert_ne!(
            call(1, &[Value::str("Café")]).unwrap(),
            call(1, &[Value::str("Caf")]).unwrap()
        );
        assert_eq!(slugify_type(&[Type::String]), Ok(Type::String));
    }

    #[test]
    fn first_yields_the_head_of_a_list_and_null_when_it_is_empty() {
        let xs = Value::list(vec![Value::Int(1), Value::Int(2)]);
        assert_eq!(call(2, &[xs]).unwrap(), Value::Int(1));
        assert_eq!(call(2, &[Value::list(vec![])]).unwrap(), Value::Null);
    }

    #[test]
    fn first_is_typed_by_the_element_type_it_was_dispatched_on() {
        assert_eq!(
            first_type(&[Type::list(Type::Int)]),
            Ok(Type::optional(Type::Int))
        );
    }

    #[test]
    fn keys_yields_the_maps_keys_in_insertion_order() {
        let m = Value::map(
            [
                (Rc::from("b"), Value::Int(1)),
                (Rc::from("a"), Value::Int(2)),
            ]
            .into_iter()
            .collect(),
        );
        assert_eq!(
            call(3, &[m]).unwrap(),
            Value::list(vec![Value::str("b"), Value::str("a")])
        );
        assert_eq!(keys_type(&[]), Ok(Type::list(Type::String)));
    }

    #[test]
    fn a_method_refuses_a_receiver_only_an_any_call_could_have_handed_it() {
        assert_eq!(
            call(1, &[Value::Int(1)]).unwrap_err(),
            "`.slugify()` needs a string, got int"
        );
        assert_eq!(
            call(2, &[Value::str("s")]).unwrap_err(),
            "`.first()` needs a list, got string"
        );
        assert_eq!(
            call(3, &[Value::Bool(true)]).unwrap_err(),
            "`.keys()` needs a map, got bool"
        );
    }
}

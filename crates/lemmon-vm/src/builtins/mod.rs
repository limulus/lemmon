//! The functions the core ships: the ambient ones every template can call by name
//! (D23), and the methods a builtin type carries (D34).
//!
//! One table holds both, so a call — however it was written — is one `CallBuiltin` with
//! one id. What differs is how the compiler finds the entry: an ambient function by its
//! bare name through the value namespace, a method by the receiver's type through
//! [`method`], which reads no namespace at all. A method's receiver is its first
//! argument, so `s.slugify()` and a one-argument ambient function push the same stack.
//!
//! A builtin runs against a [`Machine`] — the VM — so that one taking a function, `map` or
//! `filter`, can invoke it and get the value back mid-instruction; and its typing rule
//! has a companion, [`Builtin::expect`], that shapes each argument from the ones before
//! it, which is what types `xs.map(|a| …)`'s parameter from the receiver's element type.
//!
//! The table below is the whole inventory, and reading it top to bottom is the shortest
//! description of what the stdlib offers. Each entry's rules and implementation live in
//! the module for the type it belongs to; only the ambient functions — the ones that
//! make a value or vouch for one (D34) — share a module of their own.

mod lists;
mod maps;
mod numbers;
mod strings;
mod text;
mod time;
mod values;

use crate::types::Type;
use crate::{RuntimeError, Value};

/// A builtin type as a method's receiver: [`Type`] with its element types dropped,
/// which is all dispatch needs — every `[T]` carries the same methods, and what `T` is
/// belongs to the entry's [typing rule](Builtin::rule).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReceiverKind {
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

impl ReceiverKind {
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
    /// resolve because the receiver was `any`. Every variant that carries methods
    /// answers here, so that the `any` path reaches the same entry the checker would
    /// have found. A record has no methods: it is data, and takes `lem-fn`s (D34);
    /// neither has `null`, which is refused before a method is looked for at all.
    #[must_use]
    pub const fn of_value(value: &Value) -> Option<Self> {
        match value {
            Value::Str(_) => Some(Self::String),
            Value::Int(_) => Some(Self::Int),
            Value::Float(_) => Some(Self::Float),
            Value::Bool(_) => Some(Self::Bool),
            Value::Html(_) | Value::Marked(_) => Some(Self::Html),
            Value::Datetime(_) => Some(Self::Datetime),
            Value::Duration(_) => Some(Self::Duration),
            Value::Url(_) => Some(Self::Url),
            Value::Asset(_) => Some(Self::Asset),
            Value::List(_) => Some(Self::List),
            Value::Map(_) => Some(Self::Map),
            Value::Null | Value::Fn(_) | Value::Record(_) => None,
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
    Method(ReceiverKind),
}

/// What a builtin may ask of the VM running it.
pub trait Machine {
    /// Calls `callee` with `args`, runs it to completion, and hands back its value.
    ///
    /// # Errors
    ///
    /// A [`Fault::Message`] when `callee` cannot be called with `args` — not a function,
    /// the wrong arity, the depth cap — and a [`Fault::Placed`] when the call ran and
    /// failed inside, already pointing at the source that failed.
    fn invoke(&mut self, callee: &Value, args: Vec<Value>) -> Result<Value, Fault>;

    /// The next number for `prefix`, counting from zero and separately per prefix —
    /// what `uniqueId` is built on. The only state a builtin may ask the VM to keep,
    /// and it starts again at every render so that two renders of a page agree
    /// (invariant 1).
    fn next_id(&mut self, prefix: &str) -> u64;
}

/// How a builtin fails.
#[derive(Debug, PartialEq, Eq)]
pub enum Fault {
    /// A message the VM places at the instruction that called the builtin.
    Message(String),
    /// An error raised inside a function the builtin invoked, placed where it happened:
    /// a lambda body's own span must survive the builtin that ran it (invariant 8).
    Placed(RuntimeError),
}

impl From<String> for Fault {
    fn from(message: String) -> Self {
        Self::Message(message)
    }
}

impl From<&str> for Fault {
    fn from(message: &str) -> Self {
        Self::Message(message.to_owned())
    }
}

/// What a builtin expects of an argument, given the ones before it.
#[derive(Clone, Debug, PartialEq)]
pub enum Expected {
    /// A value of this type.
    Type(Type),
    /// A function of these parameter types whose return is the argument's to decide: a
    /// lambda takes them as its parameters and its body is inferred, and a function
    /// value fits when its parameters accept them. `map` asks this of its one argument.
    /// Spelling it `(T): any` instead would make an expected `any` mean "infer" here
    /// and "opt out" everywhere else (§Checking, Literals).
    Function(Vec<Type>),
}

/// One builtin function.
pub struct Builtin {
    /// The name templates call it by.
    pub name: &'static str,
    /// How the compiler finds it.
    pub reach: Reach,
    /// How many arguments it takes, a method's receiver among them.
    pub arity: usize,
    /// What the next argument is expected to be, given the types of those before it — a
    /// method's receiver first. `None` lowers the argument with nothing expected. This
    /// is what types a lambda's parameters from the receiver's element type (D34):
    /// `xs.filter(|a| …)` expects `(T): bool` of its one argument.
    pub expect: fn(&[Type]) -> Option<Expected>,
    /// The typing rule: the result type for the given argument types, or the compile
    /// error's message. A rule rather than a signature, because the list entries are
    /// generic over their element type — `first` on a `[T]` gives a `T?`, and no
    /// signature says that. A method's rule is asked only about receivers dispatch has
    /// already matched, so it types the result and does not re-check what kind the
    /// receiver was.
    pub rule: fn(&[Type]) -> Result<Type, String>,
    /// The implementation, with the VM to invoke a function argument through.
    pub call: fn(&mut dyn Machine, &[Value]) -> Result<Value, Fault>,
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
///
/// Order is the id space and nothing more, so entries are grouped by the receiver they
/// belong to and left in the order an author reaches for them — which is also the order
/// a diagnostic lists them in. `arity` counts a method's receiver, so a method an author
/// writes with one argument has an arity of two.
pub const BUILTINS: &[Builtin] = &[
    // The ambient functions: each makes a value or vouches for one (D34).
    ambient("raw", 1, values::raw_type, values::raw),
    ambient("datetime", 1, values::datetime_type, values::datetime),
    ambient("url", 1, values::url_type, values::url),
    ambient("asset", 1, values::asset_type, values::asset),
    ambient("uniqueId", 1, values::unique_id_type, values::unique_id),
    // Written `mark("styles")`, `mark("code", payload)`, or either with
    // `default: fallback`: the one builtin with a named argument and the one with an
    // optional positional, so `rules::mark_arity` counts its call rather than the
    // `arity` here, and the compiler reads every part (D42).
    ambient("mark", 1, values::mark_type, values::mark),
    // `string`
    method(
        ReceiverKind::String,
        "trim",
        1,
        unshaped,
        yields_string,
        strings::trim,
    ),
    method(
        ReceiverKind::String,
        "lower",
        1,
        unshaped,
        yields_string,
        strings::lower,
    ),
    method(
        ReceiverKind::String,
        "upper",
        1,
        unshaped,
        yields_string,
        strings::upper,
    ),
    method(
        ReceiverKind::String,
        "slugify",
        1,
        unshaped,
        yields_string,
        strings::slugify,
    ),
    method(
        ReceiverKind::String,
        "length",
        1,
        unshaped,
        yields_int,
        strings::length,
    ),
    method(
        ReceiverKind::String,
        "split",
        2,
        wants_string,
        strings::split_type,
        strings::split,
    ),
    method(
        ReceiverKind::String,
        "replace",
        3,
        wants_string,
        yields_string,
        strings::replace,
    ),
    method(
        ReceiverKind::String,
        "startsWith",
        2,
        wants_string,
        yields_bool,
        strings::starts_with,
    ),
    method(
        ReceiverKind::String,
        "endsWith",
        2,
        wants_string,
        yields_bool,
        strings::ends_with,
    ),
    method(
        ReceiverKind::String,
        "contains",
        2,
        wants_string,
        yields_bool,
        strings::contains,
    ),
    // `[T]`
    method(
        ReceiverKind::List,
        "first",
        1,
        unshaped,
        lists::element_or_null,
        lists::first,
    ),
    method(
        ReceiverKind::List,
        "last",
        1,
        unshaped,
        lists::element_or_null,
        lists::last,
    ),
    method(
        ReceiverKind::List,
        "length",
        1,
        unshaped,
        yields_int,
        lists::length,
    ),
    method(
        ReceiverKind::List,
        "reverse",
        1,
        unshaped,
        yields_receiver,
        lists::reverse,
    ),
    method(
        ReceiverKind::List,
        "contains",
        2,
        lists::wants_element,
        yields_bool,
        lists::contains,
    ),
    method(
        ReceiverKind::List,
        "take",
        2,
        wants_count,
        yields_receiver,
        lists::take,
    ),
    method(
        ReceiverKind::List,
        "drop",
        2,
        wants_count,
        yields_receiver,
        lists::drop,
    ),
    method(
        ReceiverKind::List,
        "join",
        2,
        wants_string,
        lists::join_type,
        lists::join,
    ),
    method(
        ReceiverKind::List,
        "map",
        2,
        lists::wants_mapper,
        lists::map_type,
        lists::map,
    ),
    method(
        ReceiverKind::List,
        "filter",
        2,
        lists::wants_predicate,
        yields_receiver,
        lists::filter,
    ),
    method(
        ReceiverKind::List,
        "find",
        2,
        lists::wants_predicate,
        lists::element_or_null,
        lists::find,
    ),
    method(
        ReceiverKind::List,
        "any",
        2,
        lists::wants_predicate,
        yields_bool,
        lists::any,
    ),
    method(
        ReceiverKind::List,
        "sortBy",
        2,
        lists::wants_mapper,
        lists::sort_by_type,
        lists::sort_by,
    ),
    method(
        ReceiverKind::List,
        "groupBy",
        2,
        lists::wants_key,
        lists::group_by_type,
        lists::group_by,
    ),
    // `{string: V}`
    method(
        ReceiverKind::Map,
        "keys",
        1,
        unshaped,
        maps::keys_type,
        maps::keys,
    ),
    method(
        ReceiverKind::Map,
        "values",
        1,
        unshaped,
        maps::values_type,
        maps::values,
    ),
    method(
        ReceiverKind::Map,
        "has",
        2,
        wants_string,
        yields_bool,
        maps::has,
    ),
    // `int` and `float`
    method(
        ReceiverKind::Int,
        "abs",
        1,
        unshaped,
        yields_receiver,
        numbers::int_abs,
    ),
    method(
        ReceiverKind::Float,
        "abs",
        1,
        unshaped,
        yields_receiver,
        numbers::float_abs,
    ),
    method(
        ReceiverKind::Float,
        "round",
        1,
        unshaped,
        yields_int,
        numbers::round,
    ),
    method(
        ReceiverKind::Float,
        "floor",
        1,
        unshaped,
        yields_int,
        numbers::floor,
    ),
    method(
        ReceiverKind::Float,
        "fixed",
        2,
        wants_count,
        yields_string,
        numbers::fixed,
    ),
    // `datetime`
    method(
        ReceiverKind::Datetime,
        "rfc3339",
        1,
        unshaped,
        yields_string,
        time::rfc3339,
    ),
    method(
        ReceiverKind::Datetime,
        "rfc822",
        1,
        unshaped,
        yields_string,
        time::rfc822,
    ),
    method(
        ReceiverKind::Datetime,
        "format",
        2,
        wants_string,
        yields_string,
        time::format,
    ),
    method(
        ReceiverKind::Datetime,
        "year",
        1,
        unshaped,
        yields_int,
        time::year,
    ),
    method(
        ReceiverKind::Datetime,
        "month",
        1,
        unshaped,
        yields_int,
        time::month,
    ),
    method(
        ReceiverKind::Datetime,
        "day",
        1,
        unshaped,
        yields_int,
        time::day,
    ),
    // `duration`
    method(
        ReceiverKind::Duration,
        "seconds",
        1,
        unshaped,
        yields_int,
        time::seconds,
    ),
    method(
        ReceiverKind::Duration,
        "minutes",
        1,
        unshaped,
        yields_int,
        time::minutes,
    ),
    // `url`
    method(
        ReceiverKind::Url,
        "text",
        1,
        unshaped,
        yields_string,
        text::url_text,
    ),
    // `html`
    method(
        ReceiverKind::Html,
        "readingTime",
        1,
        unshaped,
        yields_duration,
        text::reading_time,
    ),
];

/// The typing rule of an entry whose result type is fixed, whatever it was called on.
fn yields_string(_args: &[Type]) -> Result<Type, String> {
    Ok(Type::String)
}

fn yields_int(_args: &[Type]) -> Result<Type, String> {
    Ok(Type::Int)
}

fn yields_bool(_args: &[Type]) -> Result<Type, String> {
    Ok(Type::Bool)
}

fn yields_duration(_args: &[Type]) -> Result<Type, String> {
    Ok(Type::Duration)
}

/// The typing rule of an entry that gives back a value of the receiver's own type:
/// `[T].reverse()` is a `[T]`, and an `int`'s `abs` is an `int`.
fn yields_receiver(args: &[Type]) -> Result<Type, String> {
    Ok(args[0].clone())
}

/// The [`Builtin::expect`] of an entry whose next argument is a `string`.
fn wants_string(_so_far: &[Type]) -> Option<Expected> {
    Some(Expected::Type(Type::String))
}

/// The [`Builtin::expect`] of an entry whose next argument counts something.
fn wants_count(_so_far: &[Type]) -> Option<Expected> {
    Some(Expected::Type(Type::Int))
}

/// An entry reached by its bare name, shaping none of its arguments: every ambient
/// function takes one value and makes another of it.
const fn ambient(
    name: &'static str,
    arity: usize,
    rule: fn(&[Type]) -> Result<Type, String>,
    call: fn(&mut dyn Machine, &[Value]) -> Result<Value, Fault>,
) -> Builtin {
    Builtin {
        name,
        reach: Reach::Ambient,
        arity,
        expect: unshaped,
        rule,
        call,
    }
}

/// An entry `kind` carries, whose first argument is the receiver.
const fn method(
    kind: ReceiverKind,
    name: &'static str,
    arity: usize,
    expect: fn(&[Type]) -> Option<Expected>,
    rule: fn(&[Type]) -> Result<Type, String>,
    call: fn(&mut dyn Machine, &[Value]) -> Result<Value, Fault>,
) -> Builtin {
    Builtin {
        name,
        reach: Reach::Method(kind),
        arity,
        expect,
        rule,
        call,
    }
}

/// The id of the ambient function called `name`, if there is one.
#[must_use]
pub fn lookup(name: &str) -> Option<u32> {
    id_of(|b| b.reach == Reach::Ambient && b.name == name)
}

/// The id of the method `kind` carries under `name`, if it carries one.
#[must_use]
pub fn method_id(kind: ReceiverKind, name: &str) -> Option<u32> {
    id_of(|b| b.reach == Reach::Method(kind) && b.name == name)
}

/// Every method name `kind` carries, for a diagnostic that lists what was available.
pub fn methods_of(kind: ReceiverKind) -> impl Iterator<Item = &'static str> {
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

/// The [`Builtin::expect`] of an entry that shapes none of its arguments.
fn unshaped(_so_far: &[Type]) -> Option<Expected> {
    None
}

/// The runtime refusal of a method reached on the wrong receiver. Only an `any` receiver
/// can get here — the checker matched every other one before the call was emitted — and
/// the VM checks it again rather than trusting the value (`value.rs`, module docs).
fn method_receiver(name: &str, wanted: &str, got: &Value) -> Fault {
    format!("`.{name}()` needs {wanted}, got {}", got.type_name()).into()
}

#[cfg(test)]
pub(crate) mod tests {
    use std::rc::Rc;

    use super::*;
    use crate::Closure;

    /// A machine that stands in for the VM: `invoke` ignores the callee and applies a Rust
    /// function to the one argument, so an entry taking a function can be tested alone.
    /// It counts what it was asked, which is how `sortBy` is held to one key per
    /// element rather than one per comparison.
    pub(crate) struct Apply {
        apply: fn(&Value) -> Result<Value, Fault>,
        pub(crate) calls: usize,
        ids: std::collections::BTreeMap<String, u64>,
    }

    impl Apply {
        pub(crate) fn new(apply: fn(&Value) -> Result<Value, Fault>) -> Self {
            Self {
                apply,
                calls: 0,
                ids: std::collections::BTreeMap::new(),
            }
        }
    }

    impl Machine for Apply {
        fn invoke(&mut self, _callee: &Value, args: Vec<Value>) -> Result<Value, Fault> {
            self.calls += 1;
            (self.apply)(&args[0])
        }

        /// Counts as the VM's does, so that `uniqueId` can be tested without one.
        fn next_id(&mut self, prefix: &str) -> u64 {
            let next = self.ids.entry(prefix.to_owned()).or_default();
            *next += 1;
            *next - 1
        }
    }

    pub(crate) fn double(value: &Value) -> Result<Value, Fault> {
        match value {
            Value::Int(i) => Ok(Value::Int(i * 2)),
            other => Err(format!("cannot double a {}", other.type_name()).into()),
        }
    }

    /// The function argument of an entry whose key or predicate is the element itself.
    pub(crate) fn itself(value: &Value) -> Result<Value, Fault> {
        Ok(value.clone())
    }

    pub(crate) fn is_positive(value: &Value) -> Result<Value, Fault> {
        match value {
            Value::Int(i) => Ok(Value::Bool(*i > 0)),
            other => Ok(other.clone()),
        }
    }

    /// The entry `raw`, `datetime`, … is reached by, so that a test names what it
    /// exercises and no test has to be edited when the table grows.
    pub(crate) fn ambient(name: &str) -> &'static Builtin {
        &BUILTINS[lookup(name).expect("an ambient function") as usize]
    }

    /// The entry `kind` carries under `name`.
    pub(crate) fn of(kind: ReceiverKind, name: &str) -> &'static Builtin {
        &BUILTINS[method_id(kind, name).expect("a method") as usize]
    }

    pub(crate) fn call(builtin: &Builtin, args: &[Value]) -> Result<Value, Fault> {
        call_with(builtin, double, args)
    }

    pub(crate) fn call_with(
        builtin: &Builtin,
        apply: fn(&Value) -> Result<Value, Fault>,
        args: &[Value],
    ) -> Result<Value, Fault> {
        (builtin.call)(&mut Apply::new(apply), args)
    }

    pub(crate) fn func(value: Value) -> Value {
        Value::Fn(Rc::new(Closure {
            func: 0,
            captures: vec![value],
        }))
    }

    #[test]
    fn finds_an_ambient_function_by_name_and_never_a_method() {
        for name in ["raw", "datetime", "url"] {
            assert_eq!(ambient(name).name, name);
        }
        assert_eq!(lookup("slugify"), None);
        assert_eq!(lookup("nope"), None);
    }

    #[test]
    fn finds_a_method_by_its_receivers_kind() {
        for (kind, name) in [
            (ReceiverKind::String, "slugify"),
            (ReceiverKind::List, "first"),
            (ReceiverKind::Map, "keys"),
            (ReceiverKind::List, "map"),
            (ReceiverKind::Float, "round"),
        ] {
            assert_eq!(of(kind, name).reach, Reach::Method(kind), "{name}");
        }
    }

    #[test]
    fn offers_a_kind_only_its_own_methods_and_never_an_ambient_function() {
        assert_eq!(method_id(ReceiverKind::List, "slugify"), None);
        assert_eq!(method_id(ReceiverKind::Int, "first"), None);
        assert_eq!(method_id(ReceiverKind::String, "raw"), None);
        // `round` is a `float`'s question: an `int` is already round.
        assert_eq!(method_id(ReceiverKind::Int, "round"), None);
    }

    #[test]
    fn lists_the_methods_a_kind_carries_in_the_tables_order() {
        assert_eq!(
            methods_of(ReceiverKind::String).collect::<Vec<_>>(),
            [
                "trim",
                "lower",
                "upper",
                "slugify",
                "length",
                "split",
                "replace",
                "startsWith",
                "endsWith",
                "contains"
            ]
        );
        assert_eq!(
            methods_of(ReceiverKind::Map).collect::<Vec<_>>(),
            ["keys", "values", "has"]
        );
        assert_eq!(methods_of(ReceiverKind::Bool).count(), 0);
    }

    #[test]
    fn gives_one_name_to_one_entry_per_receiver() {
        // Two entries a dispatch cannot tell apart would make the later one dead.
        let mut seen: Vec<(Reach, &str)> = Vec::new();
        for builtin in BUILTINS {
            let key = (builtin.reach, builtin.name);
            assert!(
                !seen.contains(&key),
                "{} is in the table twice",
                builtin.name
            );
            seen.push(key);
        }
    }

    #[test]
    fn reads_a_receiver_kind_off_a_type_and_off_nothing_that_dispatches_elsewhere() {
        assert_eq!(ReceiverKind::of(&Type::String), Some(ReceiverKind::String));
        assert_eq!(
            ReceiverKind::of(&Type::list(Type::Int)),
            Some(ReceiverKind::List)
        );
        assert_eq!(
            ReceiverKind::of(&Type::map(Type::Int)),
            Some(ReceiverKind::Map)
        );
        assert_eq!(
            ReceiverKind::of(&Type::Datetime),
            Some(ReceiverKind::Datetime)
        );
        assert_eq!(
            ReceiverKind::of(&Type::Duration),
            Some(ReceiverKind::Duration)
        );
        assert_eq!(ReceiverKind::of(&Type::Url), Some(ReceiverKind::Url));
        assert_eq!(ReceiverKind::of(&Type::Html), Some(ReceiverKind::Html));
        assert_eq!(ReceiverKind::of(&Type::Asset), Some(ReceiverKind::Asset));
        assert_eq!(ReceiverKind::of(&Type::Float), Some(ReceiverKind::Float));
        assert_eq!(ReceiverKind::of(&Type::Bool), Some(ReceiverKind::Bool));
        assert_eq!(ReceiverKind::of(&Type::Any), None);
        assert_eq!(ReceiverKind::of(&Type::Null), None);
        assert_eq!(ReceiverKind::of(&Type::optional(Type::String)), None);
        assert_eq!(ReceiverKind::of(&Type::function(vec![], Type::Int)), None);
        assert_eq!(ReceiverKind::of(&Type::Named(Rc::from("Article"))), None);
    }

    #[test]
    fn reads_a_receiver_kind_off_a_runtime_value() {
        assert_eq!(
            ReceiverKind::of_value(&Value::str("s")),
            Some(ReceiverKind::String)
        );
        assert_eq!(
            ReceiverKind::of_value(&Value::html("<b>")),
            Some(ReceiverKind::Html)
        );
        assert_eq!(
            ReceiverKind::of_value(&Value::Int(1)),
            Some(ReceiverKind::Int)
        );
        assert_eq!(
            ReceiverKind::of_value(&Value::Float(1.0)),
            Some(ReceiverKind::Float)
        );
        assert_eq!(
            ReceiverKind::of_value(&Value::Bool(true)),
            Some(ReceiverKind::Bool)
        );
        assert_eq!(
            ReceiverKind::of_value(&Value::Duration(1)),
            Some(ReceiverKind::Duration)
        );
        assert_eq!(
            ReceiverKind::of_value(&Value::url("/")),
            Some(ReceiverKind::Url)
        );
        assert_eq!(
            ReceiverKind::of_value(&Value::asset("/a.css")),
            Some(ReceiverKind::Asset)
        );
        assert_eq!(
            ReceiverKind::of_value(&Value::list(vec![])),
            Some(ReceiverKind::List)
        );
        assert_eq!(
            ReceiverKind::of_value(&Value::map(crate::Map::default())),
            Some(ReceiverKind::Map)
        );
        assert_eq!(ReceiverKind::of_value(&Value::Null), None);
        assert_eq!(ReceiverKind::of_value(&func(Value::Null)), None);
    }

    #[test]
    fn counts_a_methods_receiver_out_of_the_arity_an_author_writes() {
        assert_eq!(ambient("raw").written_arity(), 1);
        assert_eq!(of(ReceiverKind::String, "slugify").written_arity(), 0);
        assert_eq!(of(ReceiverKind::String, "replace").written_arity(), 2);
        assert_eq!(ambient("raw").written(), "`raw`");
        assert_eq!(of(ReceiverKind::String, "slugify").written(), "`.slugify`");
    }

    #[test]
    fn shapes_no_argument_of_an_entry_that_takes_none() {
        // Every entry with nothing past its receiver, wherever the table puts it.
        for builtin in BUILTINS.iter().filter(|b| b.written_arity() == 0) {
            assert_eq!((builtin.expect)(&[Type::String]), None, "{}", builtin.name);
        }
    }

    #[test]
    fn passes_a_fault_from_a_function_it_invoked_through_untouched() {
        let placed = RuntimeError::new("t.lem", crate::Span::new(3, 9), "boom");
        fn fail(_: &Value) -> Result<Value, Fault> {
            Err(Fault::Placed(RuntimeError::new(
                "t.lem",
                crate::Span::new(3, 9),
                "boom",
            )))
        }
        let xs = Value::list(vec![Value::Int(1)]);
        assert_eq!(
            call_with(
                of(ReceiverKind::List, "map"),
                fail,
                &[xs, func(Value::Null)]
            )
            .unwrap_err(),
            Fault::Placed(placed)
        );
    }
}

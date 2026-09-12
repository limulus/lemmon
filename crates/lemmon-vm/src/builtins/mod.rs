//! The functions the core ships: the ambient ones every template can call by name
//! (D23), and the methods a builtin type carries (D34).
//!
//! One table holds both, so a call — however it was written — is one `CallBuiltin` with
//! one id. What differs is how the compiler finds the entry: an ambient function by its
//! bare name through the value namespace, a method by the receiver's type through
//! [`method`], which reads no namespace at all. A method's receiver is its first
//! argument, so `s.slugify()` and a one-argument ambient function push the same stack.
//!
//! A builtin runs against a [`Host`] — the VM — so that one taking a function, `map` or
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
mod values;

use crate::types::Type;
use crate::{RuntimeError, Value};

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
            Value::Html(_) => Some(Self::Html),
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
    Method(Kind),
}

/// What a builtin may ask of the VM running it.
pub trait Host {
    /// Calls `callee` with `args`, runs it to completion, and hands back its value.
    ///
    /// # Errors
    ///
    /// A [`Fault::Message`] when `callee` cannot be called with `args` — not a function,
    /// the wrong arity, the depth cap — and a [`Fault::Placed`] when the call ran and
    /// failed inside, already pointing at the source that failed.
    fn invoke(&mut self, callee: &Value, args: Vec<Value>) -> Result<Value, Fault>;
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
    pub ty: fn(&[Type]) -> Result<Type, String>,
    /// The implementation, with the VM to invoke a function argument through.
    pub call: fn(&mut dyn Host, &[Value]) -> Result<Value, Fault>,
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
    // `string`
    method(
        Kind::String,
        "trim",
        1,
        unshaped,
        yields_string,
        strings::trim,
    ),
    method(
        Kind::String,
        "lower",
        1,
        unshaped,
        yields_string,
        strings::lower,
    ),
    method(
        Kind::String,
        "upper",
        1,
        unshaped,
        yields_string,
        strings::upper,
    ),
    method(
        Kind::String,
        "slugify",
        1,
        unshaped,
        yields_string,
        strings::slugify,
    ),
    method(
        Kind::String,
        "length",
        1,
        unshaped,
        yields_int,
        strings::length,
    ),
    method(
        Kind::String,
        "split",
        2,
        wants_string,
        strings::split_type,
        strings::split,
    ),
    method(
        Kind::String,
        "replace",
        3,
        wants_string,
        yields_string,
        strings::replace,
    ),
    method(
        Kind::String,
        "startsWith",
        2,
        wants_string,
        yields_bool,
        strings::starts_with,
    ),
    method(
        Kind::String,
        "endsWith",
        2,
        wants_string,
        yields_bool,
        strings::ends_with,
    ),
    method(
        Kind::String,
        "contains",
        2,
        wants_string,
        yields_bool,
        strings::contains,
    ),
    // `[T]`
    method(
        Kind::List,
        "first",
        1,
        unshaped,
        lists::element_or_null,
        lists::first,
    ),
    method(
        Kind::List,
        "last",
        1,
        unshaped,
        lists::element_or_null,
        lists::last,
    ),
    method(Kind::List, "length", 1, unshaped, yields_int, lists::length),
    method(
        Kind::List,
        "reverse",
        1,
        unshaped,
        yields_receiver,
        lists::reverse,
    ),
    method(
        Kind::List,
        "contains",
        2,
        lists::wants_element,
        yields_bool,
        lists::contains,
    ),
    method(
        Kind::List,
        "take",
        2,
        wants_count,
        yields_receiver,
        lists::take,
    ),
    method(
        Kind::List,
        "drop",
        2,
        wants_count,
        yields_receiver,
        lists::drop,
    ),
    method(
        Kind::List,
        "join",
        2,
        wants_string,
        lists::join_type,
        lists::join,
    ),
    method(
        Kind::List,
        "map",
        2,
        lists::wants_mapper,
        lists::map_type,
        lists::map,
    ),
    method(
        Kind::List,
        "filter",
        2,
        lists::wants_predicate,
        yields_receiver,
        lists::filter,
    ),
    method(
        Kind::List,
        "find",
        2,
        lists::wants_predicate,
        lists::element_or_null,
        lists::find,
    ),
    method(
        Kind::List,
        "any",
        2,
        lists::wants_predicate,
        yields_bool,
        lists::any,
    ),
    method(
        Kind::List,
        "sortBy",
        2,
        lists::wants_mapper,
        lists::sort_by_type,
        lists::sort_by,
    ),
    method(
        Kind::List,
        "groupBy",
        2,
        lists::wants_key,
        lists::group_by_type,
        lists::group_by,
    ),
    // `{string: V}`
    method(Kind::Map, "keys", 1, unshaped, maps::keys_type, maps::keys),
    method(
        Kind::Map,
        "values",
        1,
        unshaped,
        maps::values_type,
        maps::values,
    ),
    method(Kind::Map, "has", 2, wants_string, yields_bool, maps::has),
    // `int` and `float`
    method(
        Kind::Int,
        "abs",
        1,
        unshaped,
        yields_receiver,
        numbers::int_abs,
    ),
    method(
        Kind::Float,
        "abs",
        1,
        unshaped,
        yields_receiver,
        numbers::float_abs,
    ),
    method(
        Kind::Float,
        "round",
        1,
        unshaped,
        yields_int,
        numbers::round,
    ),
    method(
        Kind::Float,
        "floor",
        1,
        unshaped,
        yields_int,
        numbers::floor,
    ),
    method(
        Kind::Float,
        "fixed",
        2,
        wants_count,
        yields_string,
        numbers::fixed,
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
    ty: fn(&[Type]) -> Result<Type, String>,
    call: fn(&mut dyn Host, &[Value]) -> Result<Value, Fault>,
) -> Builtin {
    Builtin {
        name,
        reach: Reach::Ambient,
        arity,
        expect: unshaped,
        ty,
        call,
    }
}

/// An entry `kind` carries, whose first argument is the receiver.
const fn method(
    kind: Kind,
    name: &'static str,
    arity: usize,
    expect: fn(&[Type]) -> Option<Expected>,
    ty: fn(&[Type]) -> Result<Type, String>,
    call: fn(&mut dyn Host, &[Value]) -> Result<Value, Fault>,
) -> Builtin {
    Builtin {
        name,
        reach: Reach::Method(kind),
        arity,
        expect,
        ty,
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
pub fn method_id(kind: Kind, name: &str) -> Option<u32> {
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

/// The [`Builtin::expect`] of an entry that shapes none of its arguments.
fn unshaped(_so_far: &[Type]) -> Option<Expected> {
    None
}

/// The element type of the list a method was dispatched on.
fn element_of(receiver: &Type) -> Type {
    match receiver {
        Type::List(element) => (**element).clone(),
        other => unreachable!("a list method is dispatched on a list, not a `{other}`"),
    }
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

    /// A host that stands in for the VM: `invoke` ignores the callee and applies a Rust
    /// function to the one argument, so an entry taking a function can be tested alone.
    /// It counts what it was asked, which is how `sortBy` is held to one key per
    /// element rather than one per comparison.
    pub(crate) struct Apply {
        apply: fn(&Value) -> Result<Value, Fault>,
        pub(crate) calls: usize,
    }

    impl Apply {
        pub(crate) fn new(apply: fn(&Value) -> Result<Value, Fault>) -> Self {
            Self { apply, calls: 0 }
        }
    }

    impl Host for Apply {
        fn invoke(&mut self, _callee: &Value, args: Vec<Value>) -> Result<Value, Fault> {
            self.calls += 1;
            (self.apply)(&args[0])
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
    pub(crate) fn of(kind: Kind, name: &str) -> &'static Builtin {
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
            (Kind::String, "slugify"),
            (Kind::List, "first"),
            (Kind::Map, "keys"),
            (Kind::List, "map"),
            (Kind::Float, "round"),
        ] {
            assert_eq!(of(kind, name).reach, Reach::Method(kind), "{name}");
        }
    }

    #[test]
    fn offers_a_kind_only_its_own_methods_and_never_an_ambient_function() {
        assert_eq!(method_id(Kind::List, "slugify"), None);
        assert_eq!(method_id(Kind::Int, "first"), None);
        assert_eq!(method_id(Kind::String, "raw"), None);
        // `round` is a `float`'s question: an `int` is already round.
        assert_eq!(method_id(Kind::Int, "round"), None);
    }

    #[test]
    fn lists_the_methods_a_kind_carries_in_the_tables_order() {
        assert_eq!(
            methods_of(Kind::String).collect::<Vec<_>>(),
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
            methods_of(Kind::Map).collect::<Vec<_>>(),
            ["keys", "values", "has"]
        );
        assert_eq!(methods_of(Kind::Bool).count(), 0);
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
        assert_eq!(Kind::of_value(&Value::Duration(1)), Some(Kind::Duration));
        assert_eq!(Kind::of_value(&Value::url("/")), Some(Kind::Url));
        assert_eq!(Kind::of_value(&Value::asset("/a.css")), Some(Kind::Asset));
        assert_eq!(Kind::of_value(&Value::list(vec![])), Some(Kind::List));
        assert_eq!(
            Kind::of_value(&Value::map(crate::Map::default())),
            Some(Kind::Map)
        );
        assert_eq!(Kind::of_value(&Value::Null), None);
        assert_eq!(Kind::of_value(&func(Value::Null)), None);
    }

    #[test]
    fn counts_a_methods_receiver_out_of_the_arity_an_author_writes() {
        assert_eq!(ambient("raw").written_arity(), 1);
        assert_eq!(of(Kind::String, "slugify").written_arity(), 0);
        assert_eq!(of(Kind::String, "replace").written_arity(), 2);
        assert_eq!(ambient("raw").written(), "`raw`");
        assert_eq!(of(Kind::String, "slugify").written(), "`.slugify`");
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
            call_with(of(Kind::List, "map"), fail, &[xs, func(Value::Null)]).unwrap_err(),
            Fault::Placed(placed)
        );
    }
}

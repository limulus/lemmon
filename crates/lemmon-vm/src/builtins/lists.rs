//! What a `[T]` carries.
//!
//! These are the generic entries — the reason [`Builtin::rule`](super::Builtin::rule) is a
//! rule and not a signature (D34). Each one's result is computed from the element type
//! the receiver was built over, and the entries taking a function shape it from that
//! same element type, which is what lets `xs.map(|a| a.date)` type `a` without an
//! annotation.
//!
//! An entry that takes a function runs it through the VM mid-instruction ([`Machine`]), so
//! an error inside the function keeps the function's own span (invariant 8, ∆987).

use std::rc::Rc;

use crate::Value;
use crate::types::Type;
use crate::value::Map;

use super::{Expected, Fault, Machine, element_of, method_receiver};

/// The receiver's elements, or the refusal for a value only an `any` call could have
/// handed a list method.
fn items<'a>(name: &str, args: &'a [Value]) -> Result<&'a Rc<[Value]>, Fault> {
    match &args[0] {
        Value::List(items) => Ok(items),
        other => Err(method_receiver(name, "a list", other)),
    }
}

/// A count argument, clamped to `len` — asking for more of a list than it holds is how
/// a feed says "the newest five", not a mistake. Refused when it is negative, or when
/// `any` let a non-integer past.
///
/// Clamping before converting is what keeps this total: `usize` is thirty-two bits on
/// `wasm32`, so a count past four billion has no `usize` to be, and a conversion that
/// could fail would trap the render rather than report it.
fn count(name: &str, args: &[Value], len: usize) -> Result<usize, Fault> {
    match &args[1] {
        Value::Int(n) if *n >= 0 => Ok(usize::try_from(*n).unwrap_or(usize::MAX).min(len)),
        Value::Int(n) => {
            Err(format!("`.{name}()` needs a count of 0 or more, got {n}").into())
        }
        other => Err(format!("`.{name}()` needs an int, got {}", other.type_name()).into()),
    }
}

/// Calls a predicate on `item` and reads its answer as a `bool`.
fn holds(
    vm: &mut dyn Machine,
    name: &str,
    predicate: &Value,
    item: &Value,
) -> Result<bool, Fault> {
    match vm.invoke(predicate, vec![item.clone()])? {
        Value::Bool(answer) => Ok(answer),
        other => Err(format!(
            "`.{name}()` needs a predicate that returns a bool, got {}",
            other.type_name()
        )
        .into()),
    }
}

/// `[T].first()`, `[T].last()`, `[T].find(…)`: the element type, made optional. An
/// empty list is not an error here, unlike an out-of-range index — that is what the
/// optional is for.
pub(super) fn element_or_null(args: &[Type]) -> Result<Type, String> {
    Ok(Type::optional(element_of(&args[0])))
}

/// `[T].contains(x)` expects an `x` of the element type: asking whether a list of
/// strings holds an `int` is a question with a known answer.
pub(super) fn wants_element(so_far: &[Type]) -> Option<Expected> {
    Some(Expected::Type(element_of(&so_far[0])))
}

/// `[T].filter(p)`, `.find(p)`, `.any(p)` expect `(T): bool`.
pub(super) fn wants_predicate(so_far: &[Type]) -> Option<Expected> {
    Some(Expected::Type(Type::function(
        vec![element_of(&so_far[0])],
        Type::Bool,
    )))
}

/// `[T].map(f)` and `.sortBy(f)` expect a function of `(T)` and leave its return to the
/// argument: `map` yields a list of whatever it returns, and `sortBy` holds it to
/// something ordered.
pub(super) fn wants_mapper(so_far: &[Type]) -> Option<Expected> {
    Some(Expected::Function(vec![element_of(&so_far[0])]))
}

/// `[T].groupBy(f)` expects `(T): string`: a group's name is the key of the map it
/// comes back in, and a map is keyed by `string`.
pub(super) fn wants_key(so_far: &[Type]) -> Option<Expected> {
    Some(Expected::Type(Type::function(
        vec![element_of(&so_far[0])],
        Type::String,
    )))
}

/// `[T].map(f)` yields a list of what `f` returns; of `any` when `f` is `any`.
pub(super) fn map_type(args: &[Type]) -> Result<Type, String> {
    match &args[1] {
        Type::Fn(_, ret) => Ok(Type::list((**ret).clone())),
        Type::Any => Ok(Type::list(Type::Any)),
        other => {
            unreachable!("`map`'s argument was checked against a function, not `{other}`")
        }
    }
}

/// `[T].sortBy(f)` yields the receiver's own type, once `f`'s return is something two
/// values of can be put in an order.
pub(super) fn sort_by_type(args: &[Type]) -> Result<Type, String> {
    let key = match &args[1] {
        Type::Fn(_, ret) => (**ret).clone(),
        // `any` is the VM's to check, key and all.
        _ => return Ok(args[0].clone()),
    };
    match key {
        Type::Int | Type::Float | Type::String | Type::Datetime | Type::Any => {
            Ok(args[0].clone())
        }
        other => Err(format!(
            "`.sortBy()` needs a key that is a number, a string, or a datetime, got \
             `{other}`"
        )),
    }
}

/// `[T].groupBy(f)` yields `{string: [T]}`.
pub(super) fn group_by_type(args: &[Type]) -> Result<Type, String> {
    Ok(Type::map(args[0].clone()))
}

/// `[string].join(sep)` yields a `string`, and refuses a list of anything else: what
/// separator would stand between two records?
pub(super) fn join_type(args: &[Type]) -> Result<Type, String> {
    match element_of(&args[0]) {
        Type::String | Type::Any => Ok(Type::String),
        _ => Err(format!(
            "`.join()` needs a list of strings, got `{}`",
            args[0]
        )),
    }
}

/// `[T].length()` and `.any(p)` say nothing about the element type, but still have to
/// be dispatched on a list at run time.
pub(super) fn length(_vm: &mut dyn Machine, args: &[Value]) -> Result<Value, Fault> {
    let count = items("length", args)?.len();
    Ok(Value::Int(
        i64::try_from(count).expect("a list shorter than 2^63"),
    ))
}

/// `xs.first()`: the first element, or null when there is none.
pub(super) fn first(_vm: &mut dyn Machine, args: &[Value]) -> Result<Value, Fault> {
    Ok(items("first", args)?
        .first()
        .cloned()
        .unwrap_or(Value::Null))
}

/// `xs.last()`: the last element, or null when there is none.
pub(super) fn last(_vm: &mut dyn Machine, args: &[Value]) -> Result<Value, Fault> {
    Ok(items("last", args)?.last().cloned().unwrap_or(Value::Null))
}

/// `xs.reverse()`: the same elements, back to front.
pub(super) fn reverse(_vm: &mut dyn Machine, args: &[Value]) -> Result<Value, Fault> {
    let mut out = items("reverse", args)?.to_vec();
    out.reverse();
    Ok(Value::list(out))
}

/// `xs.contains(x)`: whether any element equals `x`, which is what `x in xs` asks too.
pub(super) fn contains(_vm: &mut dyn Machine, args: &[Value]) -> Result<Value, Fault> {
    for item in items("contains", args)?.iter() {
        if item.equals(&args[1]).map_err(Fault::from)? {
            return Ok(Value::Bool(true));
        }
    }
    Ok(Value::Bool(false))
}

/// `xs.take(n)`: the first `n`, or all of them when there are fewer. Asking for more
/// than a list holds is how a feed says "the newest five", not a mistake.
pub(super) fn take(_vm: &mut dyn Machine, args: &[Value]) -> Result<Value, Fault> {
    let items = items("take", args)?;
    let n = count("take", args, items.len())?;
    Ok(Value::list(items[..n].to_vec()))
}

/// `xs.drop(n)`: all but the first `n`, and none of them when there are fewer.
pub(super) fn drop(_vm: &mut dyn Machine, args: &[Value]) -> Result<Value, Fault> {
    let items = items("drop", args)?;
    let n = count("drop", args, items.len())?;
    Ok(Value::list(items[n..].to_vec()))
}

/// `xs.join(sep)`: the strings with `sep` between them.
pub(super) fn join(_vm: &mut dyn Machine, args: &[Value]) -> Result<Value, Fault> {
    let items = items("join", args)?;
    let Value::Str(separator) = &args[1] else {
        return Err(
            format!("`.join()` needs a string, got {}", args[1].type_name()).into(),
        );
    };
    let mut pieces = Vec::with_capacity(items.len());
    for item in items.iter() {
        match item {
            Value::Str(text) => pieces.push(text.to_string()),
            other => {
                return Err(format!(
                    "`.join()` needs every element to be a string, got {}",
                    other.type_name()
                )
                .into());
            }
        }
    }
    Ok(Value::str(pieces.join(separator)))
}

/// `xs.map(f)`: the list of `f` applied to each element, in order.
pub(super) fn map(vm: &mut dyn Machine, args: &[Value]) -> Result<Value, Fault> {
    let items = items("map", args)?.clone();
    let mut out = Vec::with_capacity(items.len());
    for item in items.iter() {
        out.push(vm.invoke(&args[1], vec![item.clone()])?);
    }
    Ok(Value::list(out))
}

/// `xs.filter(p)`: the elements `p` answers `true` for, in order.
pub(super) fn filter(vm: &mut dyn Machine, args: &[Value]) -> Result<Value, Fault> {
    let items = items("filter", args)?.clone();
    let mut out = Vec::new();
    for item in items.iter() {
        if holds(vm, "filter", &args[1], item)? {
            out.push(item.clone());
        }
    }
    Ok(Value::list(out))
}

/// `xs.find(p)`: the first element `p` answers `true` for, or null. `p` is not asked
/// about the elements past it, which is what makes this different from `filter().first()`.
pub(super) fn find(vm: &mut dyn Machine, args: &[Value]) -> Result<Value, Fault> {
    let items = items("find", args)?.clone();
    for item in items.iter() {
        if holds(vm, "find", &args[1], item)? {
            return Ok(item.clone());
        }
    }
    Ok(Value::Null)
}

/// `xs.any(p)`: whether `p` answers `true` for any element, asking no further once one
/// does.
pub(super) fn any(vm: &mut dyn Machine, args: &[Value]) -> Result<Value, Fault> {
    let items = items("any", args)?.clone();
    for item in items.iter() {
        if holds(vm, "any", &args[1], item)? {
            return Ok(Value::Bool(true));
        }
    }
    Ok(Value::Bool(false))
}

/// `xs.sortBy(f)`: the elements in the order of the keys `f` gives them, ties left as
/// they were written.
///
/// Every key is computed once, before any comparing, so a costly key costs one call per
/// element rather than one per comparison. The keys are then checked to be mutually
/// ordered — against the first, which is enough, since what orders with one number
/// orders with every number — so the sort itself cannot meet a pair it has no answer
/// for. That check is what a mixed list reached through `any` runs into.
pub(super) fn sort_by(vm: &mut dyn Machine, args: &[Value]) -> Result<Value, Fault> {
    let items = items("sortBy", args)?.clone();
    let mut keyed = Vec::with_capacity(items.len());
    for item in items.iter() {
        keyed.push((vm.invoke(&args[1], vec![item.clone()])?, item.clone()));
    }
    if let Some((first, _)) = keyed.first() {
        for (key, _) in &keyed {
            match first.order(key).map_err(Fault::from)? {
                Some(_) => {}
                None => {
                    return Err(format!(
                        "`.sortBy()` needs keys that can be put in an order, got {} \
                         beside {}",
                        first.type_name(),
                        key.type_name()
                    )
                    .into());
                }
            }
        }
    }
    keyed.sort_by(|(a, _), (b, _)| {
        a.order(b)
            .ok()
            .flatten()
            .expect("keys the scan above found an order for")
    });
    Ok(Value::list(
        keyed.into_iter().map(|(_, item)| item).collect(),
    ))
}

/// `xs.groupBy(f)`: the elements under the key `f` gives each, keys in the order they
/// were first seen and elements in the order they were written.
pub(super) fn group_by(vm: &mut dyn Machine, args: &[Value]) -> Result<Value, Fault> {
    let items = items("groupBy", args)?.clone();
    let mut groups: Vec<(Rc<str>, Vec<Value>)> = Vec::new();
    for item in items.iter() {
        let key = match vm.invoke(&args[1], vec![item.clone()])? {
            Value::Str(key) => key,
            other => {
                return Err(format!(
                    "`.groupBy()` needs a key that is a string, got {}",
                    other.type_name()
                )
                .into());
            }
        };
        match groups.iter_mut().find(|(seen, _)| *seen == key) {
            Some((_, group)) => group.push(item.clone()),
            None => groups.push((key, vec![item.clone()])),
        }
    }
    Ok(Value::map(Map::from_unique(
        groups
            .into_iter()
            .map(|(key, group)| (key, Value::list(group))),
    )))
}

#[cfg(test)]
mod tests {
    use super::super::tests::{
        Apply, call, call_with, double, func, is_positive, itself, of,
    };
    use super::*;
    use crate::builtins::ReceiverKind;

    fn on(name: &'static str, args: &[Value]) -> Value {
        call(of(ReceiverKind::List, name), args).expect("a list method")
    }

    fn ints(items: &[i64]) -> Value {
        Value::list(items.iter().map(|&i| Value::Int(i)).collect())
    }

    fn strings(items: &[&str]) -> Value {
        Value::list(items.iter().map(|s| Value::str(*s)).collect())
    }

    #[test]
    fn reads_the_ends_of_a_list_and_answers_null_for_an_empty_one() {
        assert_eq!(on("first", &[ints(&[1, 2])]), Value::Int(1));
        assert_eq!(on("last", &[ints(&[1, 2])]), Value::Int(2));
        assert_eq!(on("first", &[ints(&[])]), Value::Null);
        assert_eq!(on("last", &[ints(&[])]), Value::Null);
        assert_eq!(
            element_or_null(&[Type::list(Type::Int)]),
            Ok(Type::optional(Type::Int))
        );
    }

    #[test]
    fn counts_and_reverses() {
        assert_eq!(on("length", &[ints(&[1, 2, 3])]), Value::Int(3));
        assert_eq!(on("reverse", &[ints(&[1, 2, 3])]), ints(&[3, 2, 1]));
        assert_eq!(on("reverse", &[ints(&[])]), ints(&[]));
    }

    #[test]
    fn answers_whether_a_list_holds_a_value_the_way_in_does() {
        assert_eq!(
            on("contains", &[ints(&[1, 2]), Value::Int(2)]),
            Value::Bool(true)
        );
        assert_eq!(
            on("contains", &[ints(&[1, 2]), Value::Int(3)]),
            Value::Bool(false)
        );
        assert_eq!(
            wants_element(&[Type::list(Type::String)]),
            Some(Expected::Type(Type::String))
        );
    }

    #[test]
    fn takes_and_drops_clamping_to_what_the_list_holds() {
        assert_eq!(
            on("take", &[ints(&[1, 2, 3]), Value::Int(2)]),
            ints(&[1, 2])
        );
        assert_eq!(on("take", &[ints(&[1]), Value::Int(9)]), ints(&[1]));
        // A count no `usize` could hold on `wasm32` still clamps rather than trapping.
        assert_eq!(on("take", &[ints(&[1]), Value::Int(i64::MAX)]), ints(&[1]));
        assert_eq!(on("drop", &[ints(&[1]), Value::Int(i64::MAX)]), ints(&[]));
        assert_eq!(on("take", &[ints(&[1, 2]), Value::Int(0)]), ints(&[]));
        assert_eq!(on("drop", &[ints(&[1, 2, 3]), Value::Int(2)]), ints(&[3]));
        assert_eq!(on("drop", &[ints(&[1]), Value::Int(9)]), ints(&[]));
    }

    #[test]
    fn refuses_a_count_that_runs_backwards() {
        for name in ["take", "drop"] {
            assert_eq!(
                call(of(ReceiverKind::List, name), &[ints(&[1]), Value::Int(-1)])
                    .unwrap_err(),
                Fault::from(format!("`.{name}()` needs a count of 0 or more, got -1"))
            );
            assert_eq!(
                call(of(ReceiverKind::List, name), &[ints(&[1]), Value::str("2")])
                    .unwrap_err(),
                Fault::from(format!("`.{name}()` needs an int, got string"))
            );
        }
    }

    #[test]
    fn joins_strings_and_refuses_a_list_of_anything_else() {
        assert_eq!(
            on("join", &[strings(&["a", "b"]), Value::str(", ")]),
            Value::str("a, b")
        );
        assert_eq!(
            on("join", &[strings(&[]), Value::str(", ")]),
            Value::str("")
        );
        assert_eq!(join_type(&[Type::list(Type::String)]), Ok(Type::String));
        assert_eq!(
            join_type(&[Type::list(Type::Int)]),
            Err("`.join()` needs a list of strings, got `[int]`".to_owned())
        );
        assert_eq!(
            call(
                of(ReceiverKind::List, "join"),
                &[ints(&[1]), Value::str(",")]
            )
            .unwrap_err(),
            Fault::from("`.join()` needs every element to be a string, got int")
        );
        assert_eq!(
            call(
                of(ReceiverKind::List, "join"),
                &[strings(&["a"]), Value::Int(1)]
            )
            .unwrap_err(),
            Fault::from("`.join()` needs a string, got int")
        );
    }

    #[test]
    fn finds_the_first_element_a_predicate_holds_for_and_stops_there() {
        assert_eq!(
            call_with(
                of(ReceiverKind::List, "find"),
                is_positive,
                &[ints(&[-1, 2, 3]), func(Value::Null)]
            )
            .unwrap(),
            Value::Int(2)
        );
        assert_eq!(
            call_with(
                of(ReceiverKind::List, "find"),
                is_positive,
                &[ints(&[-1]), func(Value::Null)]
            )
            .unwrap(),
            Value::Null
        );
        // The elements past the first match are never asked about.
        let mut vm = Apply::new(is_positive);
        (of(ReceiverKind::List, "find").call)(
            &mut vm,
            &[ints(&[1, 2, 3]), func(Value::Null)],
        )
        .unwrap();
        assert_eq!(vm.calls, 1);
    }

    #[test]
    fn answers_whether_any_element_holds() {
        let any = of(ReceiverKind::List, "any");
        assert_eq!(
            call_with(any, is_positive, &[ints(&[-1, 2]), func(Value::Null)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            call_with(any, is_positive, &[ints(&[-1]), func(Value::Null)]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn refuses_a_predicate_that_answers_anything_but_a_bool() {
        for name in ["filter", "find", "any"] {
            let xs = Value::list(vec![Value::str("s")]);
            assert_eq!(
                call_with(
                    of(ReceiverKind::List, name),
                    is_positive,
                    &[xs, func(Value::Null)]
                )
                .unwrap_err(),
                Fault::from(format!(
                    "`.{name}()` needs a predicate that returns a bool, got string"
                ))
            );
        }
    }

    #[test]
    fn sorts_by_a_key_computed_once_per_element_keeping_ties_as_written() {
        // Every element's key is its own value doubled, so the order is the elements'.
        let sorted = call(
            of(ReceiverKind::List, "sortBy"),
            &[ints(&[3, 1, 2]), func(Value::Null)],
        );
        assert_eq!(sorted.unwrap(), ints(&[1, 2, 3]));
        let mut vm = Apply::new(double);
        let entry = of(ReceiverKind::List, "sortBy");
        (entry.call)(&mut vm, &[ints(&[5, 4, 3, 2, 1]), func(Value::Null)]).unwrap();
        assert_eq!(vm.calls, 5, "one key per element, not one per comparison");
    }

    #[test]
    fn leaves_two_elements_sharing_a_key_in_the_order_they_were_written() {
        // Every key is the element doubled, so `2` and `2` tie: an unstable sort would
        // be free to swap them, and a site's index page would reorder on each build.
        let xs = Value::list(vec![Value::str("first"), Value::str("second")]);
        let sorted = call_with(
            of(ReceiverKind::List, "sortBy"),
            |_| Ok(Value::Int(2)),
            &[xs.clone(), func(Value::Null)],
        );
        assert_eq!(sorted.unwrap(), xs);
    }

    #[test]
    fn sorts_strings_and_datetimes_and_refuses_a_key_that_has_no_order() {
        let xs = Type::list(Type::Int);
        for key in [
            Type::Int,
            Type::Float,
            Type::String,
            Type::Datetime,
            Type::Any,
        ] {
            let f = Type::function(vec![Type::Int], key.clone());
            assert_eq!(sort_by_type(&[xs.clone(), f]), Ok(xs.clone()), "{key}");
        }
        assert_eq!(sort_by_type(&[xs.clone(), Type::Any]), Ok(xs.clone()));
        assert_eq!(
            sort_by_type(&[xs, Type::function(vec![Type::Int], Type::Bool)]),
            Err(
                "`.sortBy()` needs a key that is a number, a string, or a datetime, \
                 got `bool`"
                    .to_owned()
            )
        );
    }

    #[test]
    fn refuses_a_key_set_no_order_runs_through() {
        // Only `any` reaches here: the checker holds every other key to one type.
        let mixed = Value::list(vec![Value::Int(1), Value::str("a")]);
        assert_eq!(
            call_with(
                of(ReceiverKind::List, "sortBy"),
                itself,
                &[mixed, func(Value::Null)]
            )
            .unwrap_err(),
            Fault::from(
                "`.sortBy()` needs keys that can be put in an order, got int beside string"
            )
        );
        let nan = Value::list(vec![Value::Float(f64::NAN), Value::Float(1.0)]);
        assert_eq!(
            call_with(
                of(ReceiverKind::List, "sortBy"),
                itself,
                &[nan, func(Value::Null)]
            )
            .unwrap_err(),
            Fault::from("cannot order a NaN")
        );
    }

    #[test]
    fn groups_by_a_key_keeping_the_first_order_of_both_keys_and_elements() {
        let xs = strings(&["rust", "swift", "rust", "css"]);
        let grouped = call_with(
            of(ReceiverKind::List, "groupBy"),
            itself,
            &[xs, func(Value::Null)],
        );
        let Value::Map(map) = grouped.unwrap() else {
            panic!("a map");
        };
        let keys: Vec<&str> = map.iter().map(|(k, _)| &**k).collect();
        assert_eq!(keys, ["rust", "swift", "css"]);
        assert_eq!(map.get("rust"), Some(&strings(&["rust", "rust"])));
        assert_eq!(
            group_by_type(&[Type::list(Type::Int)]),
            Ok(Type::map(Type::list(Type::Int)))
        );
        assert_eq!(
            wants_key(&[Type::list(Type::Int)]),
            Some(Expected::Type(Type::function(
                vec![Type::Int],
                Type::String
            )))
        );
    }

    #[test]
    fn refuses_a_group_key_that_is_no_string() {
        let xs = ints(&[1]);
        assert_eq!(
            call(of(ReceiverKind::List, "groupBy"), &[xs, func(Value::Null)]).unwrap_err(),
            Fault::from("`.groupBy()` needs a key that is a string, got int")
        );
    }

    #[test]
    fn maps_over_every_element_in_order() {
        assert_eq!(
            on("map", &[ints(&[1, 2, 3]), func(Value::Null)]),
            ints(&[2, 4, 6])
        );
        assert_eq!(on("map", &[ints(&[]), func(Value::Null)]), ints(&[]));
        assert_eq!(
            map_type(&[
                Type::list(Type::Int),
                Type::function(vec![Type::Int], Type::String)
            ]),
            Ok(Type::list(Type::String))
        );
        assert_eq!(
            map_type(&[Type::list(Type::Int), Type::Any]),
            Ok(Type::list(Type::Any))
        );
        assert_eq!(
            wants_mapper(&[Type::list(Type::Int)]),
            Some(Expected::Function(vec![Type::Int]))
        );
        assert_eq!(
            wants_predicate(&[Type::list(Type::String)]),
            Some(Expected::Type(Type::function(
                vec![Type::String],
                Type::Bool
            )))
        );
    }

    #[test]
    fn keeps_the_elements_a_predicate_holds_for() {
        assert_eq!(
            call_with(
                of(ReceiverKind::List, "filter"),
                is_positive,
                &[ints(&[-1, 2, 0, 3]), func(Value::Null)]
            )
            .unwrap(),
            ints(&[2, 3])
        );
    }

    #[test]
    fn refuses_a_receiver_only_an_any_call_could_have_handed_it() {
        for name in [
            "first", "last", "length", "reverse", "contains", "take", "drop", "join",
            "map", "filter", "find", "any", "sortBy", "groupBy",
        ] {
            let entry = of(ReceiverKind::List, name);
            let args = [Value::str("s"), Value::Int(1)];
            assert_eq!(
                call(entry, &args[..entry.arity]).unwrap_err(),
                Fault::from(format!("`.{name}()` needs a list, got string")),
                "{name}"
            );
        }
    }
}

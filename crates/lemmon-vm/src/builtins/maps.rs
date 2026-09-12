//! What a `{string: V}` carries.
//!
//! A map's keys are always `string` (`docs/language.md` §Types), so only its values
//! carry a type the rules have to compute. Order is insertion order throughout, which
//! is the order a `lem-for` walks and the order a host's JSON arrived in.

use std::rc::Rc;

use crate::Value;
use crate::types::Type;
use crate::value::Map;

use super::{Fault, Machine, method_receiver};

/// The receiver's entries, or the refusal for a value only an `any` call could have
/// handed a map method.
fn entries<'a>(name: &str, args: &'a [Value]) -> Result<&'a Rc<Map>, Fault> {
    match &args[0] {
        Value::Map(map) => Ok(map),
        other => Err(method_receiver(name, "a map", other)),
    }
}

/// The value type the map a method was dispatched on was built over.
fn value_of(receiver: &Type) -> Type {
    match receiver {
        Type::Map(value) => (**value).clone(),
        other => unreachable!("a map method is dispatched on a map, not a `{other}`"),
    }
}

/// `{string: V}.keys(): [string]`.
pub(super) fn keys_type(_args: &[Type]) -> Result<Type, String> {
    Ok(Type::list(Type::String))
}

/// `{string: V}.values(): [V]`.
pub(super) fn values_type(args: &[Type]) -> Result<Type, String> {
    Ok(Type::list(value_of(&args[0])))
}

/// `m.keys()`: the keys in insertion order.
pub(super) fn keys(_vm: &mut dyn Machine, args: &[Value]) -> Result<Value, Fault> {
    Ok(Value::list(
        entries("keys", args)?
            .iter()
            .map(|(key, _)| Value::Str(key.clone()))
            .collect(),
    ))
}

/// `m.values()`: the values in insertion order, beside what `keys()` gives.
pub(super) fn values(_vm: &mut dyn Machine, args: &[Value]) -> Result<Value, Fault> {
    Ok(Value::list(
        entries("values", args)?
            .iter()
            .map(|(_, value)| value.clone())
            .collect(),
    ))
}

/// `m.has(key)`: whether the map holds an entry under `key`, which is what
/// `key in m` asks too. Indexing a key a map has not got is a render error, so this is
/// how a template asks before it indexes.
pub(super) fn has(_vm: &mut dyn Machine, args: &[Value]) -> Result<Value, Fault> {
    let map = entries("has", args)?;
    match &args[1] {
        Value::Str(key) => Ok(Value::Bool(map.get(key).is_some())),
        other => Err(format!("`.has()` needs a string, got {}", other.type_name()).into()),
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{call, of};
    use super::*;
    use crate::builtins::ReceiverKind;

    fn map(entries: &[(&str, i64)]) -> Value {
        Value::map(Map::from_unique(
            entries
                .iter()
                .map(|(key, value)| (Rc::from(*key), Value::Int(*value))),
        ))
    }

    #[test]
    fn reads_keys_and_values_in_insertion_order() {
        let m = map(&[("b", 1), ("a", 2)]);
        assert_eq!(
            call(of(ReceiverKind::Map, "keys"), std::slice::from_ref(&m)).unwrap(),
            Value::list(vec![Value::str("b"), Value::str("a")])
        );
        assert_eq!(
            call(of(ReceiverKind::Map, "values"), &[m]).unwrap(),
            Value::list(vec![Value::Int(1), Value::Int(2)])
        );
    }

    #[test]
    fn types_keys_as_strings_and_values_as_what_the_map_holds() {
        assert_eq!(
            keys_type(&[Type::map(Type::Int)]),
            Ok(Type::list(Type::String))
        );
        assert_eq!(
            values_type(&[Type::map(Type::Int)]),
            Ok(Type::list(Type::Int))
        );
    }

    #[test]
    fn answers_whether_a_map_holds_a_key() {
        let m = map(&[("a", 1)]);
        let has = of(ReceiverKind::Map, "has");
        assert_eq!(
            call(has, &[m.clone(), Value::str("a")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            call(has, &[m.clone(), Value::str("z")]).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            call(has, &[m, Value::Int(1)]).unwrap_err(),
            Fault::from("`.has()` needs a string, got int")
        );
    }

    #[test]
    fn refuses_a_receiver_only_an_any_call_could_have_handed_it() {
        for name in ["keys", "values", "has"] {
            let entry = of(ReceiverKind::Map, name);
            let args = [Value::Bool(true), Value::str("a")];
            assert_eq!(
                call(entry, &args[..entry.arity]).unwrap_err(),
                Fault::from(format!("`.{name}()` needs a map, got bool")),
                "{name}"
            );
        }
    }
}

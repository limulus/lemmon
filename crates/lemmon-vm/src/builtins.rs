//! The ambient functions every template can call without importing (D23).
//!
//! Only `raw()` lives here today — the one `string` → `html` cast (invariant 7). The
//! stdlib proper arrives with `∆Q6L` and extends this table; the compiler resolves a
//! builtin by name at compile time, types the call through its [`Builtin::ty`], and the
//! VM calls it through `CallBuiltin`.

use crate::Value;
use crate::types::Type;

/// One ambient function.
pub struct Builtin {
    /// The name templates call it by.
    pub name: &'static str,
    /// How many positional arguments it takes.
    pub arity: usize,
    /// The typing rule: the result type for the given argument types, or the compile
    /// error's message. A rule rather than a signature, because the stdlib's list
    /// functions are generic over their element type (∆Q6L).
    pub ty: fn(&[Type]) -> Result<Type, String>,
    /// The implementation. An `Err` is the runtime error's message; the VM adds the span.
    pub call: fn(&[Value]) -> Result<Value, String>,
}

/// Every builtin, indexed by the `id` a `CallBuiltin` carries.
pub const BUILTINS: &[Builtin] = &[Builtin {
    name: "raw",
    arity: 1,
    ty: raw_type,
    call: raw,
}];

/// The id of the builtin called `name`, if there is one.
#[must_use]
pub fn lookup(name: &str) -> Option<u32> {
    BUILTINS
        .iter()
        .position(|b| b.name == name)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_raw_and_nothing_else() {
        assert_eq!(lookup("raw"), Some(0));
        assert_eq!(lookup("slugify"), None);
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
}

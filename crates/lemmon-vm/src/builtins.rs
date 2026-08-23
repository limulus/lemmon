//! The ambient functions every template can call without importing (D23).
//!
//! Only `raw()` lives here today — the one `string` → `html` cast (invariant 7). The
//! stdlib proper arrives with `∆Q6L` and extends this table; the compiler resolves a
//! builtin by name at compile time and the VM calls it through `CallBuiltin`.

use crate::Value;

/// One ambient function.
pub struct Builtin {
    /// The name templates call it by.
    pub name: &'static str,
    /// How many positional arguments it takes.
    pub arity: usize,
    /// The implementation. An `Err` is the runtime error's message; the VM adds the span.
    pub call: fn(&[Value]) -> Result<Value, String>,
}

/// Every builtin, indexed by the `id` a `CallBuiltin` carries.
pub const BUILTINS: &[Builtin] = &[Builtin {
    name: "raw",
    arity: 1,
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
    fn raw_refuses_anything_else() {
        assert_eq!(
            raw(&[Value::Int(1)]).unwrap_err(),
            "raw() takes a string, got int"
        );
    }
}

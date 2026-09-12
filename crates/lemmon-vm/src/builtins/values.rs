//! The ambient functions: the entries that make a value or vouch for one.
//!
//! An entry whose first parameter *is* the value it works on is a method and belongs to
//! that value's type; one that constructs from something else, or casts, is reached by
//! its bare name and lives here (D34). That is the whole of the ambient value namespace
//! the stdlib occupies — three names, each turning a string into something the checker
//! then knows more about than a string.
//!
//! `raw` is the one of them that vouches rather than constructs, and it stays a function
//! precisely so the single `string` → `html` cast stays a grep target (invariant 7).

use crate::Value;
use crate::datetime::Datetime;
use crate::types::Type;
use crate::url;

use super::{Fault, Host};

/// `raw(s: string): html` — `html` in is `html` out, and `any` is the VM's to check.
pub(super) fn raw_type(args: &[Type]) -> Result<Type, String> {
    match &args[0] {
        Type::String | Type::Html | Type::Any => Ok(Type::Html),
        other => Err(format!("raw() takes a string, got `{other}`")),
    }
}

/// `raw(s)`: vouches for a string as trusted markup. Already-trusted markup passes
/// through unchanged.
pub(super) fn raw(_host: &mut dyn Host, args: &[Value]) -> Result<Value, Fault> {
    match &args[0] {
        Value::Str(s) => Ok(Value::Html(s.clone())),
        html @ Value::Html(_) => Ok(html.clone()),
        other => Err(format!("raw() takes a string, got {}", other.type_name()).into()),
    }
}

/// `datetime(s: string): datetime` — `any` is the VM's to check.
pub(super) fn datetime_type(args: &[Type]) -> Result<Type, String> {
    match &args[0] {
        Type::String | Type::Any => Ok(Type::Datetime),
        other => Err(format!("datetime() takes a string, got `{other}`")),
    }
}

/// `datetime(s)`: the moment `s` spells, in the offset it spells it in. A literal
/// argument is read at compile time too (`compile/calls.rs`), so a fixed date that is
/// not a date is a compile error rather than a render one.
pub(super) fn datetime(_host: &mut dyn Host, args: &[Value]) -> Result<Value, Fault> {
    match &args[0] {
        Value::Str(text) => Datetime::parse(text)
            .map(Value::Datetime)
            .map_err(Fault::from),
        other => {
            Err(format!("datetime() takes a string, got {}", other.type_name()).into())
        }
    }
}

/// `url(s: string): url` — `any` is the VM's to check.
pub(super) fn url_type(args: &[Type]) -> Result<Type, String> {
    match &args[0] {
        Type::String | Type::Any => Ok(Type::Url),
        other => Err(format!("url() takes a string, got `{other}`")),
    }
}

/// `url(s)`: the composed text as a URL, percent-encoded once (`url.rs`).
pub(super) fn url(_host: &mut dyn Host, args: &[Value]) -> Result<Value, Fault> {
    match &args[0] {
        Value::Str(text) => Ok(Value::url(url::encode(text))),
        other => Err(format!("url() takes a string, got {}", other.type_name()).into()),
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{ambient, call};
    use super::*;

    #[test]
    fn raw_turns_a_string_into_html_unchanged_and_passes_html_through() {
        assert_eq!(
            call(ambient("raw"), &[Value::str("<b>")]).unwrap(),
            Value::html("<b>")
        );
        assert_eq!(
            call(ambient("raw"), &[Value::html("<b>")]).unwrap(),
            Value::html("<b>")
        );
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
        assert_eq!(
            call(ambient("raw"), &[Value::Int(1)]).unwrap_err(),
            Fault::from("raw() takes a string, got int")
        );
    }

    #[test]
    fn datetime_reads_a_string_and_types_it_as_a_datetime() {
        assert_eq!(
            call(
                ambient("datetime"),
                &[Value::str("2026-04-04T10:30:00-07:00")]
            )
            .unwrap(),
            Value::Datetime(Datetime::parse("2026-04-04T17:30:00Z").unwrap())
        );
        assert_eq!(datetime_type(&[Type::String]), Ok(Type::Datetime));
        assert_eq!(datetime_type(&[Type::Any]), Ok(Type::Datetime));
    }

    #[test]
    fn datetime_passes_the_refusal_of_a_string_that_is_no_datetime_through() {
        assert_eq!(
            call(ambient("datetime"), &[Value::str("tomorrow")]).unwrap_err(),
            Fault::from(Datetime::parse("tomorrow").unwrap_err())
        );
    }

    #[test]
    fn datetime_refuses_anything_that_is_not_a_string() {
        assert_eq!(
            datetime_type(&[Type::Int]),
            Err("datetime() takes a string, got `int`".to_owned())
        );
        assert_eq!(
            call(ambient("datetime"), &[Value::Int(1)]).unwrap_err(),
            Fault::from("datetime() takes a string, got int")
        );
    }

    #[test]
    fn url_percent_encodes_its_string_and_types_it_as_a_url() {
        assert_eq!(
            call(ambient("url"), &[Value::str("/photos/a café/")]).unwrap(),
            Value::url("/photos/a%20caf%C3%A9/")
        );
        assert_eq!(url_type(&[Type::String]), Ok(Type::Url));
        assert_eq!(url_type(&[Type::Any]), Ok(Type::Url));
    }

    #[test]
    fn url_refuses_anything_that_is_not_a_string() {
        assert_eq!(
            url_type(&[Type::Bool]),
            Err("url() takes a string, got `bool`".to_owned())
        );
        assert_eq!(
            call(ambient("url"), &[Value::Bool(true)]).unwrap_err(),
            Fault::from("url() takes a string, got bool")
        );
    }
}

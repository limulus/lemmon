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

/// `asset(path: string): asset` — the type is fixed, since the path is a literal the
/// compiler has already read.
pub(super) fn asset_type(_args: &[Type]) -> Result<Type, String> {
    Ok(Type::Asset)
}

/// `asset(path)` never runs.
///
/// The entry is in the table so that the name resolves, the arity is checked, and the
/// shadowing warning fires as it does for every other ambient function — but
/// `compile/calls.rs` folds the call to the URL the manifest gave it and emits no
/// `CallBuiltin` at all (D10). Reaching here would mean that folding was skipped, which
/// is a bug in the compiler rather than a mistake in a template.
pub(super) fn asset(_host: &mut dyn Host, _args: &[Value]) -> Result<Value, Fault> {
    Err(Fault::from(
        "asset() is resolved while compiling and should never run",
    ))
}

/// `uniqueId(prefix: string): string` — an id is text a template puts in an attribute.
pub(super) fn unique_id_type(args: &[Type]) -> Result<Type, String> {
    match &args[0] {
        Type::String | Type::Any => Ok(Type::String),
        other => Err(format!("uniqueId() takes a string, got `{other}`")),
    }
}

/// `uniqueId(prefix)`: `prefix-0`, then `prefix-1`, counting separately for each prefix
/// and starting again at every render.
///
/// An `id` has to be unique within the page it is written on, and a component that
/// writes one cannot see the rest of the page — so the count is the VM's rather than the
/// template's, which is the only thing in the stdlib that remembers anything between
/// calls. It remembers deterministically: the same page rendered twice gives the same
/// ids, because the counter starts at zero and the walk order is the document's
/// (invariant 1).
pub(super) fn unique_id(host: &mut dyn Host, args: &[Value]) -> Result<Value, Fault> {
    match &args[0] {
        Value::Str(prefix) => {
            let n = host.next_id(prefix);
            Ok(Value::str(format!("{prefix}-{n}")))
        }
        other => {
            Err(format!("uniqueId() takes a string, got {}", other.type_name()).into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{Apply, ambient, call, itself};
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
    fn asset_refuses_to_run_because_the_compiler_answered_it() {
        // The entry exists so the name resolves, the arity is checked, and the
        // shadowing warning fires; `compile/calls.rs` emits no call at all (D10).
        assert_eq!(asset_type(&[Type::String]), Ok(Type::Asset));
        assert_eq!(
            call(ambient("asset"), &[Value::str("styles/site.css")]).unwrap_err(),
            Fault::from("asset() is resolved while compiling and should never run")
        );
    }

    #[test]
    fn unique_id_counts_from_zero_and_keeps_a_prefix_to_itself() {
        let entry = ambient("uniqueId");
        let mut host = Apply::new(itself);
        let ids: Vec<Value> = ["desc", "desc", "field", "desc"]
            .iter()
            .map(|prefix| (entry.call)(&mut host, &[Value::str(*prefix)]).unwrap())
            .collect();
        assert_eq!(
            ids,
            [
                Value::str("desc-0"),
                Value::str("desc-1"),
                Value::str("field-0"),
                Value::str("desc-2")
            ]
        );
        assert_eq!(unique_id_type(&[Type::String]), Ok(Type::String));
        assert_eq!(unique_id_type(&[Type::Any]), Ok(Type::String));
    }

    #[test]
    fn unique_id_refuses_anything_that_is_not_a_string() {
        assert_eq!(
            unique_id_type(&[Type::Int]),
            Err("uniqueId() takes a string, got `int`".to_owned())
        );
        assert_eq!(
            call(ambient("uniqueId"), &[Value::Int(1)]).unwrap_err(),
            Fault::from("uniqueId() takes a string, got int")
        );
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

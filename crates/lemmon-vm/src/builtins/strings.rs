//! What a `string` carries.
//!
//! Every entry works in `char`s rather than bytes, so `length` counts what an author
//! sees and `slugify` may fold one character into several. The receiver refusals here
//! are reachable only through `any`: the checker matched every other receiver before
//! the call was emitted.

use crate::Value;
use crate::types::Type;

use super::{Fault, Host, method_receiver};

/// The receiver as text, or the refusal for a value only an `any` call could have
/// handed a string method.
fn text<'a>(name: &str, args: &'a [Value]) -> Result<&'a str, Fault> {
    match &args[0] {
        Value::Str(text) => Ok(text),
        other => Err(method_receiver(name, "a string", other)),
    }
}

/// An argument that has to be a string, refused only when `any` let it through.
fn arg<'a>(name: &str, args: &'a [Value], at: usize) -> Result<&'a str, Fault> {
    match &args[at] {
        Value::Str(text) => Ok(text),
        other => {
            Err(format!("`.{name}()` needs a string, got {}", other.type_name()).into())
        }
    }
}

/// `s.trim()`: the same text without the whitespace at either end.
pub(super) fn trim(_host: &mut dyn Host, args: &[Value]) -> Result<Value, Fault> {
    Ok(Value::str(text("trim", args)?.trim()))
}

/// `s.lower()`: lower-cased, by Unicode's rules rather than ASCII's.
pub(super) fn lower(_host: &mut dyn Host, args: &[Value]) -> Result<Value, Fault> {
    Ok(Value::str(text("lower", args)?.to_lowercase()))
}

/// `s.upper()`: upper-cased, by Unicode's rules rather than ASCII's.
pub(super) fn upper(_host: &mut dyn Host, args: &[Value]) -> Result<Value, Fault> {
    Ok(Value::str(text("upper", args)?.to_uppercase()))
}

/// `s.length()`: how many characters, not how many bytes — `"café"` is four.
pub(super) fn length(_host: &mut dyn Host, args: &[Value]) -> Result<Value, Fault> {
    let count = text("length", args)?.chars().count();
    Ok(Value::Int(
        i64::try_from(count).expect("a string shorter than 2^63"),
    ))
}

/// `string.split(sep): [string]` — the pieces of a string are strings.
pub(super) fn split_type(_args: &[Type]) -> Result<Type, String> {
    Ok(Type::list(Type::String))
}

/// `s.split(sep)`: the pieces between each occurrence of `sep`, in order. An empty
/// separator gives the characters, which is what an author asking for "every letter"
/// means — Rust's own `split("")` would hand back an empty piece at either end.
pub(super) fn split(_host: &mut dyn Host, args: &[Value]) -> Result<Value, Fault> {
    let text = text("split", args)?;
    let separator = arg("split", args, 1)?;
    let pieces: Vec<Value> = if separator.is_empty() {
        text.chars().map(|c| Value::str(c.to_string())).collect()
    } else {
        text.split(separator).map(Value::str).collect()
    };
    Ok(Value::list(pieces))
}

/// `s.replace(from, to)`: every occurrence, not just the first.
pub(super) fn replace(_host: &mut dyn Host, args: &[Value]) -> Result<Value, Fault> {
    let text = text("replace", args)?;
    let (from, to) = (arg("replace", args, 1)?, arg("replace", args, 2)?);
    Ok(Value::str(text.replace(from, to)))
}

/// `s.startsWith(prefix)`.
pub(super) fn starts_with(_host: &mut dyn Host, args: &[Value]) -> Result<Value, Fault> {
    let text = text("startsWith", args)?;
    Ok(Value::Bool(text.starts_with(arg("startsWith", args, 1)?)))
}

/// `s.endsWith(suffix)`.
pub(super) fn ends_with(_host: &mut dyn Host, args: &[Value]) -> Result<Value, Fault> {
    let text = text("endsWith", args)?;
    Ok(Value::Bool(text.ends_with(arg("endsWith", args, 1)?)))
}

/// `s.contains(part)`. A list answers the same question through `in`; a string cannot,
/// since `in` on a string would have to choose between characters and substrings.
pub(super) fn contains(_host: &mut dyn Host, args: &[Value]) -> Result<Value, Fault> {
    let text = text("contains", args)?;
    Ok(Value::Bool(text.contains(arg("contains", args, 1)?)))
}

/// What folding leaves of an apostrophe, in any of its spellings: dropped from a slug
/// rather than separating the word it sits in.
const APOSTROPHES: [char; 2] = ['\'', '`'];

/// `s.slugify()`: folded to ASCII, lower-cased, with every run of characters that are
/// neither letters nor digits a single `-`, and none at either end.
///
/// Folding is what makes a slug a URL segment a reader can type and an editor can
/// search: `Dürer` gives `durer`, `Hawaiʻi` gives `hawaii`. It also means two titles
/// that differ only outside ASCII can share a slug — `Café` and `Cafe` do — which is
/// the trade the other way costs a reader every time and costs an author once.
///
/// An apostrophe is dropped rather than made a separator, so `You Can't` gives
/// `you-cant` and not `you-can-t`, and `Hawaiʻi` is one word. Folding is what makes
/// that a short rule: a typographic `’` arrives here as `'` and a Hawaiian `okina` as
/// a backtick, so those two characters are the whole of it.
///
/// One letter at a time, with no language in mind: `ü` folds to `u`, where the German
/// convention `@sindresorhus/slugify` follows would give `ue`. limulus.net's photo
/// slugs were minted by that library and are stored beside their photos rather than
/// recomputed, so the port reads them rather than asking for them again.
pub(super) fn slugify(_host: &mut dyn Host, args: &[Value]) -> Result<Value, Fault> {
    let folded = deunicode::deunicode(text("slugify", args)?).to_lowercase();
    let mut out = String::with_capacity(folded.len());
    for ch in folded.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else if !APOSTROPHES.contains(&ch) && !out.ends_with('-') {
            out.push('-');
        }
    }
    Ok(Value::str(out.trim_matches('-')))
}

#[cfg(test)]
mod tests {
    use super::super::tests::{call, of};
    use super::*;
    use crate::builtins::Kind;

    fn on(name: &'static str, args: &[Value]) -> Value {
        call(of(Kind::String, name), args).expect("a string method")
    }

    fn s(text: &str) -> Value {
        Value::str(text)
    }

    #[test]
    fn trims_the_whitespace_at_either_end_and_none_inside() {
        assert_eq!(on("trim", &[s("  a  b  ")]), s("a  b"));
        assert_eq!(on("trim", &[s("\n\ta\t\n")]), s("a"));
    }

    #[test]
    fn changes_case_by_unicodes_rules_rather_than_asciis() {
        assert_eq!(on("lower", &[s("Café LIFE")]), s("café life"));
        assert_eq!(on("upper", &[s("café life")]), s("CAFÉ LIFE"));
    }

    #[test]
    fn counts_characters_and_not_bytes() {
        assert_eq!(on("length", &[s("café")]), Value::Int(4));
        assert_eq!(on("length", &[s("")]), Value::Int(0));
    }

    #[test]
    fn splits_on_a_separator_and_into_characters_on_an_empty_one() {
        assert_eq!(
            on("split", &[s("a, b, c"), s(", ")]),
            Value::list(vec![s("a"), s("b"), s("c")])
        );
        assert_eq!(
            on("split", &[s("abc"), s("")]),
            Value::list(vec![s("a"), s("b"), s("c")])
        );
        // Nothing to split on is one piece, never none.
        assert_eq!(on("split", &[s("a"), s("/")]), Value::list(vec![s("a")]));
    }

    #[test]
    fn replaces_every_occurrence_and_not_just_the_first() {
        assert_eq!(on("replace", &[s("a.b.c"), s("."), s("-")]), s("a-b-c"));
        assert_eq!(on("replace", &[s("abc"), s("x"), s("y")]), s("abc"));
    }

    #[test]
    fn answers_what_a_string_starts_ends_or_carries() {
        assert_eq!(
            on("startsWith", &[s("/tils/rust/"), s("/tils/")]),
            Value::Bool(true)
        );
        assert_eq!(
            on("startsWith", &[s("/feed/"), s("/tils/")]),
            Value::Bool(false)
        );
        assert_eq!(
            on("endsWith", &[s("index.md"), s(".md")]),
            Value::Bool(true)
        );
        assert_eq!(
            on("endsWith", &[s("index.md"), s(".lem")]),
            Value::Bool(false)
        );
        assert_eq!(on("contains", &[s("a b c"), s("b")]), Value::Bool(true));
        assert_eq!(on("contains", &[s("a b c"), s("z")]), Value::Bool(false));
    }

    #[test]
    fn slugifies_by_folding_to_ascii_and_joining_with_one_dash() {
        assert_eq!(
            on("slugify", &[s("Hello,  World! -- Again")]),
            s("hello-world-again")
        );
        assert_eq!(on("slugify", &[s("  ")]), s(""));
        assert_eq!(
            on("slugify", &[s("Lynx After Dürer")]),
            s("lynx-after-durer")
        );
        assert_eq!(on("slugify", &[s("Maui, Hawaiʻi")]), s("maui-hawaii"));
        assert_eq!(on("slugify", &[s("Café Life")]), s("cafe-life"));
    }

    #[test]
    fn drops_an_apostrophe_rather_than_splitting_a_word_on_it() {
        for written in [
            "You Can't Outsource Yourself",
            "You Can’t Outsource Yourself",
        ] {
            assert_eq!(
                on("slugify", &[s(written)]),
                s("you-cant-outsource-yourself")
            );
        }
    }

    #[test]
    fn slugifies_two_titles_that_differ_only_outside_ascii_the_same_way() {
        // The trade the folding makes, stated as a test so it cannot change by accident.
        assert_eq!(on("slugify", &[s("Café")]), on("slugify", &[s("Cafe")]));
    }

    #[test]
    fn refuses_a_receiver_only_an_any_call_could_have_handed_it() {
        for name in [
            "trim",
            "lower",
            "upper",
            "slugify",
            "length",
            "split",
            "replace",
            "startsWith",
            "endsWith",
            "contains",
        ] {
            let args = [Value::Int(1), s(""), s("")];
            let entry = of(Kind::String, name);
            let refusal = call(entry, &args[..entry.arity]).unwrap_err();
            assert_eq!(
                refusal,
                Fault::from(format!("`.{name}()` needs a string, got int")),
                "{name}"
            );
        }
    }

    #[test]
    fn refuses_an_argument_only_an_any_call_could_have_handed_it() {
        for name in ["split", "startsWith", "endsWith", "contains"] {
            let refusal =
                call(of(Kind::String, name), &[s("a"), Value::Int(1)]).unwrap_err();
            assert_eq!(
                refusal,
                Fault::from(format!("`.{name}()` needs a string, got int")),
                "{name}"
            );
        }
        let refusal = call(
            of(Kind::String, "replace"),
            &[s("a"), s("b"), Value::Int(1)],
        );
        assert_eq!(
            refusal.unwrap_err(),
            Fault::from("`.replace()` needs a string, got int")
        );
    }
}

//! What a `string` carries.
//!
//! Every entry counts what an author sees rather than what the encoding holds: a
//! character is a grapheme cluster, so `length` is the number of times a cursor would
//! step and `split("")` gives pieces that are each still text. `slugify` may fold one
//! of them into several. The receiver refusals here are reachable only through `any`:
//! the checker matched every other receiver before the call was emitted.

use unicode_segmentation::UnicodeSegmentation;

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

/// `s.length()`: how many characters a reader sees, not how many bytes and not how many
/// code points — `"café"` is four however its `é` was spelled, and an emoji family is
/// one. A character is an extended grapheme cluster (UAX #29): the unit a cursor steps
/// over and a backspace deletes.
pub(super) fn length(_host: &mut dyn Host, args: &[Value]) -> Result<Value, Fault> {
    let count = text("length", args)?.graphemes(true).count();
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
///
/// Characters, again, are grapheme clusters and not code points. Splitting by code point
/// would hand back pieces that are not text: a combining accent with no letter under it,
/// half of an emoji, a zero-width joiner on its own.
pub(super) fn split(_host: &mut dyn Host, args: &[Value]) -> Result<Value, Fault> {
    let text = text("split", args)?;
    let separator = arg("split", args, 1)?;
    let pieces: Vec<Value> = if separator.is_empty() {
        text.graphemes(true).map(Value::str).collect()
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

/// What folding leaves of an apostrophe, in every spelling a title uses one in: dropped
/// from a slug rather than separating the word it sits in. The Hawaiian `okina` is here
/// because it is one — `Hawaiʻi` is a word, not two.
const APOSTROPHES: [char; 4] = ['\'', '\u{2019}', '\u{02bb}', '\u{02bc}'];

/// The Latin letters canonical decomposition cannot reach, because their mark is part
/// of the letter rather than laid over it: no amount of decomposing turns `ø` into `o`.
/// English writes none of these, and every one of them would otherwise put a `-` in the
/// middle of a word — `Straße` would slug as `stra-e`.
const LIGATURES: [(char, &str); 12] = [
    ('æ', "ae"),
    ('œ', "oe"),
    ('ß', "ss"),
    ('ø', "o"),
    ('đ', "d"),
    ('ð', "d"),
    ('þ', "th"),
    ('ł', "l"),
    ('ħ', "h"),
    ('ı', "i"),
    ('ŋ', "n"),
    ('ĸ', "k"),
];

/// `ch` with its marks taken off, when what is underneath is a plain ASCII letter or
/// digit. `ü` gives `u` and `ǻ` gives `a`; a Hangul syllable decomposes too, but into
/// jamo rather than into ASCII, so it answers `None` and is kept whole by the caller.
fn unmarked(ch: char) -> Option<char> {
    let mut base = None;
    unicode_normalization::char::decompose_canonical(ch, |part| {
        base.get_or_insert(part);
    });
    base.filter(char::is_ascii_alphanumeric)
}

/// `s.slugify()`: lower-cased, Latin marks folded off, every run of what is left that
/// is neither letter nor digit a single `-`, and none at either end.
///
/// Folding is what makes a slug a URL segment a reader can type and an editor can
/// search: `Dürer` gives `durer`, `Brychtová` gives `brychtova`. It also means two
/// titles differing only in their marks share a slug — `Café` and `Cafe` do — which is
/// the trade the other way costs a reader every time and an author once.
///
/// This is a fold, not a transliteration. A script that is not Latin is kept rather
/// than guessed at: a Japanese title gives a Japanese slug, percent-encoded by `url()`
/// like any other text, instead of the empty string dropping what it cannot fold would
/// leave. Transliteration was measured and cut — `deunicode` was 479 KB of the `.wasm`,
/// a fifth of it, to answer a question no site in the corpus asks (D38).
///
/// `ü` folds to `u`. `@sindresorhus/slugify` gives `ue`, which is German's own rule for
/// writing an umlaut without one, applied there to every language's `ü` — it turns the
/// Turkish `Ümit` into `Uemit`. Dropping the mark is what English does with a borrowed
/// name. limulus.net's photo slugs were minted by that library and are stored beside
/// their photos, so the port reads them rather than asking for them again; the one that
/// differs, `lynx-after-duerer-…`, owes a redirect and not a change here.
pub(super) fn slugify(_host: &mut dyn Host, args: &[Value]) -> Result<Value, Fault> {
    let lowered = text("slugify", args)?.to_lowercase();
    let mut out = String::with_capacity(lowered.len());
    for ch in lowered.chars() {
        // Dropped outright rather than separating. An apostrophe belongs to the word it
        // sits in, and this has to be asked before anything else, because an `okina` is
        // a modifier *letter* — ask Unicode whether it is alphanumeric and it says yes,
        // and `Hawaiʻi` keeps it. A bare combining mark is one whose letter was already
        // taken: text that reached here decomposed, which is what a Mac hands over.
        if APOSTROPHES.contains(&ch) || unicode_normalization::char::is_combining_mark(ch) {
            continue;
        }
        if let Some((_, replacement)) = LIGATURES.iter().find(|(from, _)| *from == ch) {
            out.push_str(replacement);
        } else if let Some(base) = unmarked(ch) {
            out.push(base);
        } else if ch.is_alphanumeric() {
            out.push(ch);
        } else if !out.ends_with('-') {
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
    fn counts_a_character_a_reader_sees_and_not_a_code_point() {
        // The same word composed and decomposed is the same length; an emoji built out
        // of four people joined by zero-width joiners is one character, not seven.
        assert_eq!(on("length", &[s("café")]), Value::Int(4));
        assert_eq!(on("length", &[s("cafe\u{301}")]), Value::Int(4));
        assert_eq!(
            on(
                "length",
                &[s("\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}")]
            ),
            Value::Int(1)
        );
    }

    #[test]
    fn splits_into_pieces_that_are_each_still_text() {
        // Splitting by code point would hand back a combining accent with no letter
        // under it, and a zero-width joiner on its own — neither is a character an
        // author asked for.
        assert_eq!(
            on("split", &[s("cafe\u{301}"), s("")]),
            Value::list(vec![s("c"), s("a"), s("f"), s("e\u{301}")])
        );
        assert_eq!(
            on(
                "split",
                &[s("\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}"), s("")]
            ),
            Value::list(vec![s("\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}")])
        );
    }

    #[test]
    fn slugifies_by_folding_the_marks_off_and_joining_with_one_dash() {
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
    fn folds_a_letter_whose_mark_is_part_of_it_rather_than_over_it() {
        // Decomposition reaches none of these, so each is in `LIGATURES` by hand. Left
        // out, every one would put a `-` through the middle of a word.
        assert_eq!(on("slugify", &[s("Straße")]), s("strasse"));
        assert_eq!(on("slugify", &[s("Łódź")]), s("lodz"));
        assert_eq!(on("slugify", &[s("Ærø")]), s("aero"));
        assert_eq!(on("slugify", &[s("Þingvellir")]), s("thingvellir"));
    }

    #[test]
    fn folds_a_mark_off_however_the_source_spelled_the_letter() {
        // The same letter composed and decomposed: a host that hands over NFD text —
        // which is what a Mac filesystem does — must slug the same as one that does not.
        assert_eq!(
            on("slugify", &[s("Du\u{308}rer")]),
            on("slugify", &[s("Dürer")])
        );
    }

    #[test]
    fn folds_every_language_s_umlaut_the_same_way() {
        // `ü` is not a German letter, it is a letter German uses. Expanding it to `ue`
        // is German's own rule for writing one without the mark, and applying it
        // everywhere is how `@sindresorhus/slugify` turns a Turkish name into nonsense.
        assert_eq!(on("slugify", &[s("Ümit Özdağ")]), s("umit-ozdag"));
        assert_eq!(on("slugify", &[s("Dürer")]), s("durer"));
    }

    #[test]
    fn keeps_a_script_it_cannot_fold_rather_than_dropping_it() {
        // The fold is not a transliteration, and a title it has nothing to say about
        // should still give a slug — `url()` percent-encodes it like any other text.
        // Dropping what it cannot fold would give every one of these the empty string.
        assert_eq!(
            on("slugify", &[s("日本語のテキスト")]),
            s("日本語のテキスト")
        );
        assert_eq!(on("slugify", &[s("Привет, мир")]), s("привет-мир"));
        assert_eq!(on("slugify", &[s("한국어 텍스트")]), s("한국어-텍스트"));
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

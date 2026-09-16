//! What an `html` and a `url` carry.
//!
//! Both are text the core has already vouched for — markup safe to emit verbatim
//! (invariant 7), and a URL percent-encoded once (`url.rs`) — so the questions worth
//! asking of them are about the text itself rather than about escaping it again.

use unicode_segmentation::UnicodeSegmentation;

use crate::Value;

use super::{Fault, Machine, method_receiver};

/// How fast a page is read, which is the rate every such estimate quotes and the one
/// the Eleventy build's `reading-time` uses.
const WORDS_PER_MINUTE: u64 = 200;

/// `h.readingTime(): duration` — how long the markup takes to read, so that a schema
/// can declare `readingTime: duration = body.readingTime()` and a chip can say
/// "4 min read".
///
/// Words are counted in the text between tags: a tag separates words the way whitespace
/// does, which counts `<em>re</em>read` as two and is the price of not building a DOM
/// (invariant 2). A partial second rounds up, so anything with a word in it takes time.
///
/// The Eleventy build counts words in the markdown *source* instead, where a link is
/// its URL and a fenced block is its code. A page near a rounding boundary can
/// therefore differ by a minute, which is a parity-harness question (∆hyz) and not a
/// bug here: counting what a reader sees is the better answer.
pub(super) fn reading_time(_vm: &mut dyn Machine, args: &[Value]) -> Result<Value, Fault> {
    // A mark's default content is text a reader sees, so it counts like any other.
    let markup = match &args[0] {
        Value::Html(markup) => &**markup,
        Value::Marked(marked) => &*marked.text,
        other => return Err(method_receiver("readingTime", "html", other)),
    };
    // Counted into a `u64` before the multiply: a `usize` is thirty-two bits in the
    // `.wasm`, where `words * 60` would wrap on a page the host build would never
    // notice — the shape of bug `take(5000000000)` was.
    let words = u64::try_from(words(markup)).expect("a count that came from a usize");
    Ok(Value::Duration(
        i64::try_from((words * 60).div_ceil(WORDS_PER_MINUTE))
            .expect("a page shorter than 2^63 seconds of reading"),
    ))
}

/// How many words of prose `markup` holds, skipping what stands inside `<…>`.
///
/// The counting itself is `unicode-segmentation`'s, which is UAX #29: a language that
/// does not put spaces between its words is counted rather than read as one very long
/// one, and a lone dash between two spaces is punctuation rather than a word. Only the
/// tag skipping is ours, because no crate should be parsing HTML here (invariant 2).
fn words(markup: &str) -> usize {
    let mut prose = String::with_capacity(markup.len());
    let mut in_tag = false;
    for character in markup.chars() {
        match character {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            // A tag ends a word as whitespace does: `<em>re</em>read` is two runs of
            // text, which is what never building a DOM costs.
            _ if in_tag => prose.push(' '),
            _ => prose.push(character),
        }
    }
    prose.unicode_words().count()
}

/// `u.text(): string` — the encoded text back out, which is how two URLs are joined.
///
/// `+` puts two strings together and nothing else, and a `url` is deliberately not a
/// string: it is text something has already encoded. Composing one absolute URL out of
/// a site's base and a page's path is therefore written
/// `url(site.siteUrl.text() + page.url.text())` — the `url()` says where encoding
/// happens, and encoding what is already encoded changes nothing (D38).
pub(super) fn url_text(_vm: &mut dyn Machine, args: &[Value]) -> Result<Value, Fault> {
    match &args[0] {
        Value::Url(text) => Ok(Value::Str(text.clone())),
        other => Err(method_receiver("text", "a url", other)),
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{call, of};
    use super::*;
    use crate::builtins::ReceiverKind;

    fn reading_time_of(markup: &str) -> Value {
        call(
            of(ReceiverKind::Html, "readingTime"),
            &[Value::html(markup)],
        )
        .expect("a reading time")
    }

    #[test]
    fn counts_the_words_a_reader_sees_and_not_the_markup_around_them() {
        assert_eq!(words("<p>one two three</p>"), 3);
        assert_eq!(words("  spaced   out  "), 2);
        assert_eq!(words("<p></p>"), 0);
        assert_eq!(words(""), 0);
        // An attribute is inside the tag, so its text is not prose.
        assert_eq!(words(r#"<a href="/a/b/c" title="one two">link</a>"#), 1);
        // A `>` outside a tag is text an author wrote, not the end of one.
        assert_eq!(words("<p>a &gt; b</p>"), 3);
    }

    #[test]
    fn counts_a_script_that_puts_no_spaces_between_its_words() {
        // Why the counting is UAX #29's rather than a split on whitespace: a paragraph
        // of Japanese is not one very long word, and a core that said so would have
        // assumed the one site it is being built for writes English.
        assert_eq!(words("<p>日本語のテキストです</p>"), 7);
    }

    #[test]
    fn leaves_punctuation_standing_on_its_own_uncounted() {
        // Nobody reads an em dash for three tenths of a second.
        assert_eq!(words("<p>one — two</p>"), 2);
        assert_eq!(words("<p>hello, world!</p>"), 2);
    }

    #[test]
    fn lets_a_tag_break_a_word_the_way_whitespace_does() {
        // The price of never building a DOM: `re` and `read` are two runs of text.
        assert_eq!(words("<p><em>re</em>read</p>"), 2);
    }

    #[test]
    fn rounds_a_partial_second_of_reading_up() {
        // 200 words a minute: one word is three tenths of a second and still takes one.
        assert_eq!(reading_time_of("<p>word</p>"), Value::Duration(1));
        assert_eq!(reading_time_of("<p></p>"), Value::Duration(0));
    }

    #[test]
    fn reads_two_hundred_words_in_a_minute() {
        let body = format!("<p>{}</p>", "word ".repeat(400));
        assert_eq!(reading_time_of(&body), Value::Duration(120));
    }

    #[test]
    fn gives_a_urls_text_back_for_composing_another() {
        assert_eq!(
            call(of(ReceiverKind::Url, "text"), &[Value::url("/a%20b/")]).unwrap(),
            Value::str("/a%20b/")
        );
    }

    #[test]
    fn refuses_a_receiver_only_an_any_call_could_have_handed_it() {
        assert_eq!(
            call(
                of(ReceiverKind::Html, "readingTime"),
                &[Value::str("<p>a</p>")]
            )
            .unwrap_err(),
            Fault::from("`.readingTime()` needs html, got string")
        );
        assert_eq!(
            call(of(ReceiverKind::Url, "text"), &[Value::str("/a/")]).unwrap_err(),
            Fault::from("`.text()` needs a url, got string")
        );
    }
}

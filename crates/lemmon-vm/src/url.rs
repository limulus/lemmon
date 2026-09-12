//! The `url` value: text percent-encoded once, on the way in (D38).
//!
//! A `url` is not parsed, only encoded. What a site composes is mostly a relative
//! reference — `/feed/the-rebound/`, `../photos/` — which a WHATWG parser refuses
//! without a base it has no way to know, so parsing would make the common case the
//! error case. Encoding is the part that has to happen and cannot be left to the
//! author: a space or a `é` in a path is what breaks an `href`.
//!
//! Encoding is idempotent, because `%` is left alone: a string that already carries
//! `%20` keeps it rather than growing `%2520`. The cost of that choice is that a
//! literal per-cent sign in a path stays a literal per-cent sign, which is the rarer
//! mistake and the one an author can see in the output.
//!
//! Emitting a `url` still escapes it as HTML (`value.rs`): percent-encoding leaves `&`
//! and `'` alone, and an attribute needs them escaped.

use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};

/// What percent-encoding replaces: the C0 controls and space, the characters a URL may
/// not carry unencoded, and — through [`utf8_percent_encode`] — every byte outside
/// ASCII. Reserved characters that mean something in a URL (`/ ? # & = : @ + , ; $`)
/// are left alone: this runs over a URL an author composed, not over one field of it.
const ENCODED: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'<')
    .add(b'>')
    .add(b'\\')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}');

/// `text` as a URL: the same text with everything that cannot stand in one encoded.
#[must_use]
pub fn encode(text: &str) -> String {
    utf8_percent_encode(text, ENCODED).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaves_a_url_that_needs_nothing_untouched() {
        for text in [
            "/feed/the-rebound/",
            "https://limulus.net/tils/?topic=rust&sort=new#top",
            "../photos/lynx-after-duerer/",
        ] {
            assert_eq!(encode(text), text);
        }
    }

    #[test]
    fn encodes_a_space_and_the_characters_a_url_may_not_carry() {
        assert_eq!(encode("/a b/"), "/a%20b/");
        assert_eq!(encode("/a\"b<c>d/"), "/a%22b%3Cc%3Ed/");
        assert_eq!(encode("/a{b}c|d^e`f\\g/"), "/a%7Bb%7Dc%7Cd%5Ee%60f%5Cg/");
        assert_eq!(encode("/a\nb/"), "/a%0Ab/");
    }

    #[test]
    fn encodes_everything_outside_ascii_as_utf_8() {
        assert_eq!(encode("/photos/café/"), "/photos/caf%C3%A9/");
        assert_eq!(encode("/hawaiʻi/"), "/hawai%CA%BBi/");
    }

    #[test]
    fn leaves_what_a_url_reserves_for_its_own_syntax_alone() {
        assert_eq!(
            encode("/a?b=c&d=e#f/g:h@i+j,k;l$m"),
            "/a?b=c&d=e#f/g:h@i+j,k;l$m"
        );
    }

    #[test]
    fn encodes_the_same_text_twice_to_the_same_url() {
        let once = encode("/photos/a café/");
        assert_eq!(encode(&once), once);
        assert_eq!(once, "/photos/a%20caf%C3%A9/");
    }
}

//! Helpers shared by this crate's unit tests.

use crate::parse;

/// Every diagnostic `src` parses with, as its message alone.
pub(crate) fn messages(src: &str) -> Vec<String> {
    parse(src)
        .diagnostics
        .into_iter()
        .map(|d| d.message)
        .collect()
}

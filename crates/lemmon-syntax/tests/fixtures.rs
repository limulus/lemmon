//! Snapshot tests: every fixture's dump must match its checked-in `.txt`, and parsing
//! arbitrary input must neither panic nor lose a byte.
//!
//! Set `UPDATE_FIXTURES=1` to rewrite the expected files from the current output; set
//! `DUMPS_DIR=<path>` to also copy them there.

use std::fs;
use std::path::{Path, PathBuf};

use lemmon_syntax::{dump, parse};
use proptest::prelude::*;

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn check_dir(group: &str) {
    let dir = fixtures_dir().join(group);
    let mut names: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("fixture dir")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "lem"))
        .collect();
    names.sort();
    assert!(!names.is_empty(), "no fixtures in {}", dir.display());
    let update = std::env::var_os("UPDATE_FIXTURES").is_some();
    let dumps_dir = std::env::var_os("DUMPS_DIR").map(PathBuf::from);
    let mut failures = Vec::new();
    for lem in names {
        let src = fs::read_to_string(&lem).expect("read fixture");
        let actual = dump(&src);
        let expected_path = lem.with_extension("txt");
        if update {
            fs::write(&expected_path, &actual).expect("write expected");
        }
        if let Some(dumps) = &dumps_dir {
            let out = dumps.join(group);
            fs::create_dir_all(&out).expect("create dumps dir");
            fs::write(out.join(expected_path.file_name().expect("name")), &actual)
                .expect("write dump");
        }
        let expected = fs::read_to_string(&expected_path).unwrap_or_default();
        if expected != actual {
            failures.push(format!(
                "{}:\n--- expected\n{expected}\n--- actual\n{actual}",
                lem.display()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn dumps_every_ok_fixture_as_expected() {
    check_dir("ok");
}

#[test]
fn dumps_every_broken_fixture_as_expected() {
    check_dir("broken");
}

#[test]
fn reports_no_diagnostics_for_ok_fixtures() {
    for entry in fs::read_dir(fixtures_dir().join("ok")).expect("dir") {
        let path = entry.expect("entry").path();
        if path.extension().is_some_and(|e| e == "lem") {
            let src = fs::read_to_string(&path).expect("read");
            assert_eq!(parse(&src).diagnostics, vec![], "{}", path.display());
        }
    }
}

/// Biased towards Lemmon syntax so that the generator actually reaches the parser's
/// interesting paths rather than producing text runs alone.
fn lemmon_like() -> impl Strategy<Value = String> {
    let atom = prop_oneof![
        Just("<lem."),
        Just("<lem-if "),
        Just("<lem-for "),
        Just("<lem-let "),
        Just("<lem-fn "),
        Just("<lem-else"),
        Just("<lem-else-if "),
        Just("<lem-match "),
        Just("<lem-iff "),
        Just("</lem-if>"),
        Just("</lem-for>"),
        Just("</lem-let>"),
        Just("</lem-fn>"),
        Just(">"),
        Just("/>"),
        Just(" x"),
        Just(" in "),
        Just(" = "),
        Just(": int"),
        Just("("),
        Just(")"),
        Just("["),
        Just("]"),
        Just("{"),
        Just("}"),
        Just(","),
        Just("?"),
        Just(":"),
        Just("|"),
        Just("||"),
        Just("??"),
        Just("+"),
        Just("lt "),
        Just("is T"),
        Just("\"s\""),
        Just("1.5"),
        Just("\n<p>t</p>\n"),
        Just("<"),
        Just("é"),
    ];
    prop::collection::vec(atom, 0..40).prop_map(|parts| parts.concat())
}

proptest! {
    #[test]
    fn never_panics_and_round_trips_arbitrary_text(src in "\\PC*") {
        let parse = parse(&src);
        prop_assert_eq!(parse.syntax().text().to_string(), src);
    }

    #[test]
    fn never_panics_and_round_trips_lemmon_like_input(src in lemmon_like()) {
        let parse = parse(&src);
        prop_assert_eq!(parse.syntax().text().to_string(), src.as_str());
        for d in &parse.diagnostics {
            prop_assert!(d.span.end as usize <= src.len());
        }
    }
}

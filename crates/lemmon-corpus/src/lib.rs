//! Case discovery and expectation blessing for the golden corpus in `tests/corpus`.
//!
//! Deliberately stage-agnostic: this crate knows where cases live and how an expectation
//! file is compared and rewritten, and nothing about scanning, parsing, or rendering. So
//! every stage crate can take it as a dev-dependency without a cycle, and adding a stage
//! means adding an extension to [`EXPECTATIONS`] and one [`check`] call — never a second
//! walker with its own blessing protocol.

use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Every extension a stage may check a case against, one per stage artifact.
///
/// `lemmon-syntax` owns `cst` (the tree with its diagnostics trailer) and `tags` (the
/// tag-boundary projection the tree-sitter grammar has to agree with). The VM adds
/// `html` with `∆c8h`. An extension listed here that no test checks is not an error;
/// one that is *not* listed is, so a typo'd or stale expectation cannot sit unread.
pub const EXPECTATIONS: &[&str] = &["cst", "tags"];

/// The extension every corpus input carries today. Documents (`.md`) join it with
/// `∆Ldn`.
const INPUT: &str = "lem";

/// Set this to rewrite every expectation file from what the stage currently produces.
const UPDATE: &str = "UPDATE_CORPUS";

/// The corpus root, `tests/corpus`.
///
/// Resolved from *this* crate's manifest rather than the caller's, so that a consumer in
/// another crate never spells out a relative path to it.
///
/// # Panics
///
/// If this crate is not two directories below the repository root.
#[must_use]
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the corpus crate sits at <repo>/crates/lemmon-corpus")
        .join("tests/corpus")
}

/// One corpus case: an input file, and everything a stage needs to check it.
#[derive(Clone, Debug)]
pub struct Case {
    /// The path below the corpus root without its extension, as
    /// `syntax/recovery/wrong-close` — what failures are reported by.
    pub name: String,
    /// The input file.
    pub input: PathBuf,
    /// The input's contents.
    pub source: String,
}

impl Case {
    /// The path of this case's `ext` expectation, whether or not it exists.
    #[must_use]
    pub fn expectation(&self, ext: &str) -> PathBuf {
        self.input.with_extension(ext)
    }
}

/// Every case under `tests/corpus/<group>`, recursively, in a deterministic order.
///
/// # Panics
///
/// If the group does not exist, if a case cannot be read as UTF-8, or if the group holds
/// no cases at all — an empty group means a test that silently checks nothing.
#[must_use]
pub fn cases(group: &str) -> Vec<Case> {
    let dir = root().join(group);
    assert!(dir.is_dir(), "no corpus group at {group}");
    let mut cases = Vec::new();
    walk(&dir, &mut |path| {
        if path.extension().is_some_and(|e| e == INPUT) {
            cases.push(Case {
                name: relative(path)
                    .with_extension("")
                    .to_string_lossy()
                    .into_owned(),
                source: read(path),
                input: path.to_path_buf(),
            });
        }
    });
    assert!(!cases.is_empty(), "no corpus cases under {group}");
    cases
}

/// Checks every case in `group` against its `ext` expectation, reporting every
/// mismatch at once rather than stopping at the first.
///
/// With `UPDATE_CORPUS` set in the environment, rewrites the expectations instead of
/// comparing — the blessing path behind `npm run corpus:bless`.
///
/// # Panics
///
/// If `ext` is not in [`EXPECTATIONS`], if an expectation cannot be written, or — the
/// point of the function — if any case's expectation does not match.
pub fn check(group: &str, ext: &str, produce: impl Fn(&Case) -> String) {
    assert!(
        EXPECTATIONS.contains(&ext),
        "`{ext}` is not a corpus expectation; add it to `EXPECTATIONS`"
    );
    let update = std::env::var_os(UPDATE).is_some();
    let mut report = String::new();
    for case in cases(group) {
        let actual = produce(&case);
        let path = case.expectation(ext);
        if update {
            fs::write(&path, &actual)
                .unwrap_or_else(|e| panic!("write {}: {e}", relative(&path).display()));
            continue;
        }
        let expected = read_or_empty(&path);
        if expected != actual {
            describe(&mut report, &case, &expected, &actual);
        }
    }
    assert!(
        report.is_empty(),
        "{report}\nrun `npm run corpus:bless` to accept these, then review the diff"
    );
}

/// Appends the first line at which `actual` departs from `expected`, and how many more
/// follow. A whole tree dump on both sides buries the one line that moved.
fn describe(report: &mut String, case: &Case, expected: &str, actual: &str) {
    const MISSING: &str = "<end of file>";
    let expected: Vec<&str> = expected.lines().collect();
    let actual: Vec<&str> = actual.lines().collect();
    let differing: Vec<usize> = (0..expected.len().max(actual.len()))
        .filter(|&i| expected.get(i) != actual.get(i))
        .collect();
    let out = &mut *report;
    let _ = writeln!(out, "\n{}:", case.name);
    let Some(&line) = differing.first() else {
        let _ = writeln!(out, "  differs only in trailing whitespace");
        return;
    };
    let _ = writeln!(out, "  line {}:", line + 1);
    let _ = writeln!(
        out,
        "    expected: {}",
        expected.get(line).unwrap_or(&MISSING)
    );
    let _ = writeln!(
        out,
        "      actual: {}",
        actual.get(line).unwrap_or(&MISSING)
    );
    if differing.len() > 1 {
        let _ = writeln!(out, "  ({} more differing lines)", differing.len() - 1);
    }
}

/// Visits every file under `dir`, depth first, entries sorted at each level so that the
/// order a test reports failures in never depends on the filesystem.
fn walk(dir: &Path, visit: &mut dyn FnMut(&Path)) {
    let entries = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", relative(dir).display()));
    let mut paths: Vec<PathBuf> = entries
        .map(|entry| entry.expect("corpus directory entry").path())
        .collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            walk(&path, visit);
        } else {
            visit(&path);
        }
    }
}

fn read(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read {}: {e}", relative(path).display()))
}

/// An expectation that does not exist yet reads as empty, so a new case fails with the
/// same "line 1 differs" report as a changed one rather than a file-not-found panic.
///
/// Only that one error is empty. Any other — unreadable, not UTF-8 — panics, because an
/// expectation reported as empty is a diff a blessing run would quietly absorb.
///
/// # Panics
///
/// If `path` exists but cannot be read as UTF-8.
fn read_or_empty(path: &Path) -> String {
    match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => panic!("read {}: {e}", relative(path).display()),
    }
}

/// `path` relative to the corpus root, for messages — absolute paths bury the name.
fn relative(path: &Path) -> PathBuf {
    path.strip_prefix(root()).unwrap_or(path).to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Everything under `dir` that is not an input, a known expectation belonging to
    /// one, or the README, as one message each — so a renamed case cannot leave its old
    /// expectations behind.
    ///
    /// This is a test rather than an exported check because the corpus layout is this
    /// crate's to own: the gate below runs once for the whole corpus, however many stage
    /// crates come to check expectations against it. Taking a `dir` is what lets the
    /// reporting be tested against a deliberately broken tree — a harness whose failure
    /// path has never run is a harness that can fail silently.
    fn orphans_in(dir: &Path) -> Vec<String> {
        let mut problems = Vec::new();
        walk(dir, &mut |path| {
            let shown = path.strip_prefix(dir).unwrap_or(path).display().to_string();
            if path.file_name().is_some_and(|n| n == "README.md") {
                return;
            }
            match path.extension().and_then(|e| e.to_str()) {
                Some(INPUT) => {}
                Some(ext) if EXPECTATIONS.contains(&ext) => {
                    if !path.with_extension(INPUT).exists() {
                        problems
                            .push(format!("{shown}: expectation with no `.{INPUT}` input"));
                    }
                }
                Some(ext) => {
                    problems.push(format!("{shown}: `.{ext}` is not a corpus extension"))
                }
                None => problems.push(format!("{shown}: not a corpus file")),
            }
        });
        problems
    }

    #[test]
    fn finds_the_corpus_beside_the_crates_directory() {
        assert!(root().join("README.md").is_file(), "{}", root().display());
    }

    #[test]
    fn names_a_case_by_its_path_below_the_corpus_root() {
        let cases = cases("syntax");
        assert!(
            cases
                .iter()
                .any(|c| c.name == "syntax/recovery/stray-close"),
            "{:?}",
            cases.iter().map(|c| &c.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reads_the_source_of_every_case_it_finds() {
        assert!(cases("syntax").iter().all(|case| !case.source.is_empty()));
    }

    #[test]
    fn derives_an_expectation_path_from_the_input() {
        let case = &cases("syntax")[0];
        assert_eq!(case.expectation("cst"), case.input.with_extension("cst"));
    }

    #[test]
    #[should_panic(expected = "no corpus group at render")]
    fn refuses_a_group_that_does_not_exist_yet() {
        let _ = cases("render");
    }

    #[test]
    #[should_panic(expected = "is not a corpus expectation")]
    fn refuses_an_extension_no_stage_declared() {
        check("syntax", "sexp", |_| String::new());
    }

    #[test]
    fn reports_the_first_differing_line_and_how_many_follow() {
        let case = Case {
            name: "syntax/x".to_owned(),
            input: PathBuf::from("x.lem"),
            source: String::new(),
        };
        let mut report = String::new();
        describe(&mut report, &case, "a\nb\nc", "a\nB\nC");
        assert!(report.contains("line 2:"), "{report}");
        assert!(report.contains("expected: b"), "{report}");
        assert!(report.contains("actual: B"), "{report}");
        assert!(report.contains("(1 more differing lines)"), "{report}");
    }

    #[test]
    fn reports_a_line_that_only_one_side_has_as_the_end_of_file() {
        let case = Case {
            name: "syntax/x".to_owned(),
            input: PathBuf::from("x.lem"),
            source: String::new(),
        };
        let mut report = String::new();
        describe(&mut report, &case, "a", "a\nb");
        assert!(report.contains("expected: <end of file>"), "{report}");
        assert!(!report.contains("more differing"), "{report}");
    }

    /// A corpus tree with one file of each shape `orphans_in` has an opinion about.
    fn broken_corpus() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("temp dir");
        for name in [
            "README.md",
            "kept.lem",
            "kept.cst",
            "renamed-away.cst",
            "notes.md",
            "Makefile",
        ] {
            fs::write(dir.path().join(name), "").expect("write");
        }
        dir
    }

    #[test]
    fn reports_an_expectation_whose_input_was_renamed_away() {
        let dir = broken_corpus();
        let problems = orphans_in(dir.path());
        assert!(
            problems
                .contains(&"renamed-away.cst: expectation with no `.lem` input".to_owned()),
            "{problems:?}"
        );
    }

    #[test]
    fn reports_a_file_whose_extension_no_stage_owns() {
        let dir = broken_corpus();
        let problems = orphans_in(dir.path());
        assert!(
            problems.contains(&"notes.md: `.md` is not a corpus extension".to_owned()),
            "{problems:?}"
        );
    }

    #[test]
    fn reports_a_file_with_no_extension_at_all() {
        let dir = broken_corpus();
        let problems = orphans_in(dir.path());
        assert!(
            problems.contains(&"Makefile: not a corpus file".to_owned()),
            "{problems:?}"
        );
    }

    #[test]
    fn accepts_an_input_its_readme_and_an_expectation_that_has_one() {
        let dir = broken_corpus();
        let problems = orphans_in(dir.path());
        assert!(
            !problems.iter().any(|p| p.starts_with("kept")),
            "{problems:?}"
        );
        assert!(
            !problems.iter().any(|p| p.starts_with("README")),
            "{problems:?}"
        );
    }

    #[test]
    fn finds_nothing_to_report_in_the_real_corpus() {
        assert_eq!(orphans_in(&root()), Vec::<String>::new());
    }

    #[test]
    #[should_panic(expected = "read")]
    fn refuses_an_expectation_that_exists_but_cannot_be_read() {
        let dir = tempfile::tempdir().expect("temp dir");
        let _ = read_or_empty(dir.path());
    }

    #[test]
    fn reads_an_expectation_that_does_not_exist_yet_as_empty() {
        let dir = tempfile::tempdir().expect("temp dir");
        assert_eq!(read_or_empty(&dir.path().join("absent.cst")), "");
    }

    #[test]
    fn reports_a_difference_no_line_shows() {
        let case = Case {
            name: "syntax/x".to_owned(),
            input: PathBuf::from("x.lem"),
            source: String::new(),
        };
        let mut report = String::new();
        describe(&mut report, &case, "a\n", "a");
        assert!(report.contains("trailing whitespace"), "{report}");
    }
}

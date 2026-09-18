//! Case discovery and expectation blessing for the golden corpus in `tests/corpus`.
//!
//! Deliberately stage-agnostic: this crate knows where cases live, how an expectation
//! file is compared and rewritten, and how a diagnostics trailer is displayed — and
//! nothing about scanning, parsing, or rendering. So adding a stage means adding an
//! extension to [`EXPECTATIONS`] and one [`check`] call, never a second walker with its
//! own blessing protocol.
//!
//! The one dependency is `lemmon-syntax`, for the [`LineIndex`] behind that display.
//! `lemmon-syntax` takes this crate back as a *dev*-dependency, which Cargo allows: its
//! library is built before this one, and only its test target links this one.

use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use lemmon_syntax::{LineIndex, Position};
use similar::TextDiff;

/// Every extension a stage may check a case against, one per stage artifact.
///
/// `lemmon-syntax` owns `cst` (the tree with its diagnostics trailer) and `tags` (the
/// tag-boundary projection the tree-sitter grammar has to agree with); `lemmon-vm` owns
/// `html` (the rendered output with its diagnostics trailer). An extension listed here
/// that no test checks is not an error; one that is *not* listed is, so a typo'd or
/// stale expectation cannot sit unread.
pub const EXPECTATIONS: &[&str] = &["cst", "tags", "html"];

/// The extensions a corpus input may carry: a template, or a document in document mode.
/// Which one a case is reaches the stage through [`Case::input`], so a stage that treats
/// the two differently — the compiler does — decides by the path, as it does in a site.
const INPUTS: &[&str] = &["lem", "md"];

fn is_input(ext: &str) -> bool {
    INPUTS.contains(&ext)
}

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

    /// The path this case is compiled under — its name with the input's extension, as
    /// `render/canonical/article-card.lem`.
    ///
    /// A stage that compiles a case decides document mode by this path, the way it will
    /// for a site; the trailer display uses it to tell a span in this case's own source
    /// from one in another file.
    ///
    /// # Panics
    ///
    /// If the input's extension is not UTF-8, which [`cases`] cannot produce — it only
    /// collects paths whose extension it matched against `INPUTS`.
    #[must_use]
    pub fn path(&self) -> String {
        let ext = self
            .input
            .extension()
            .and_then(|e| e.to_str())
            .expect("an input extension");
        format!("{}.{ext}", self.name)
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
    cases_in(&root().join(group))
}

/// The discovery behind [`cases`], over any directory.
///
/// Taking a `dir` is what lets the duplicate-name refusal below be tested against a tree
/// built to break it. The alternative — planting a twin in the real corpus and removing
/// it again — makes every other test that walks the corpus race against this one, and it
/// has failed CI.
fn cases_in(dir: &Path) -> Vec<Case> {
    // `relative` strips the corpus root, so this is the group name for a real group and
    // an absolute path for the temporary trees the tests build.
    let group = relative(dir).display().to_string();
    assert!(dir.is_dir(), "no corpus group at {group}");
    let mut cases = Vec::new();
    walk(dir, &mut |path| {
        if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(is_input)
        {
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
    // A case is known by its name without the extension, and so are its expectations:
    // two inputs of the same name would compare against — and bless over — one set of
    // files, the second silently winning.
    let mut names: Vec<&str> = cases.iter().map(|case| case.name.as_str()).collect();
    names.sort_unstable();
    if let Some(pair) = names.windows(2).find(|pair| pair[0] == pair[1]) {
        panic!(
            "two corpus inputs are named {}: they would share expectations",
            pair[0]
        );
    }
    cases
}

/// Checks every case in `group` against its `ext` expectation, naming every mismatch at
/// once rather than stopping at the first — the nearest `CASE_BUDGET` of them with a
/// diff, the rest by name.
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
    let mut differing: Vec<String> = Vec::new();
    for case in cases(group) {
        let actual = with_positions(&case, &produce(&case));
        let path = case.expectation(ext);
        if update {
            fs::write(&path, &actual)
                .unwrap_or_else(|e| panic!("write {}: {e}", relative(&path).display()));
            continue;
        }
        let expected = read_or_empty(&path);
        if expected != actual {
            if differing.len() < CASE_BUDGET {
                describe(&mut report, &case, &expected, &actual);
            }
            differing.push(case.name);
        }
    }
    name_the_rest(&mut report, &differing);
    assert!(
        differing.is_empty(),
        "{report}\nrun `npm run corpus:bless` to accept these, then review the diff"
    );
}

/// The line that opens a diagnostics trailer in every stage's expectation format.
const TRAILER: &str = "--- diagnostics";

/// Rewrites the byte spans of a diagnostics trailer as one-based `line:col..line:col`
/// against the case's own source.
///
/// The offsets a stage produces are exact but unreadable: nobody reviewing a blessed diff
/// can match `89..94` to a place in the file, and reviewing that diff is the corpus's
/// whole safety mechanism — blessing is a proposal, not a fix. Positions are rendered
/// here, once, rather than by each stage, so both trailers agree without either crate
/// having to hold the convention.
///
/// Only the trailer moves. A `.tags` projection has none and passes through untouched, as
/// does the tree above the marker in a `.cst`: those spans are the tree-sitter agreement
/// target (invariant 9), and byte ranges are what a context-free grammar can be held to.
///
/// One-based, unlike the zero-based [`Position`] it renders — that is the LSP's type, and
/// this is a display for people.
fn with_positions(case: &Case, produced: &str) -> String {
    let Some(start) = trailer_start(produced) else {
        return produced.to_owned();
    };
    let (head, trailer) = produced.split_at(start);
    let index = LineIndex::new(&case.source);
    let path = case.path();
    let mut out = String::from(head);
    for line in trailer.split_inclusive('\n') {
        out.push_str(&placed(line, &index, &path, &case.name));
    }
    out
}

/// The byte offset just past the last [`TRAILER`] line, or `None` if there is none.
///
/// The *last*, because a rendered page is free to contain the marker as ordinary output;
/// the trailer a stage appends is always the final one.
fn trailer_start(produced: &str) -> Option<usize> {
    let mut found = None;
    let mut offset = 0;
    for line in produced.split_inclusive('\n') {
        if line.strip_suffix('\n').unwrap_or(line) == TRAILER {
            found = Some(offset + line.len());
        }
        offset += line.len();
    }
    found
}

/// One trailer line with its span replaced by positions, keeping its indent, its path
/// qualifier, and its line terminator.
///
/// A span qualified by *another* file's path is left in bytes: the harness holds one
/// case's source and has nothing to place it in. Rendering it against this case's source
/// would be worse than leaving it — a position that looks right and points nowhere.
///
/// # Panics
///
/// If the line carries no span. Every trailer line does by construction, so one that
/// does not is a stage regression worth stopping the run for rather than a line to skip
/// past (invariant 8, and "errors over guesses").
fn placed(line: &str, index: &LineIndex, case_path: &str, case_name: &str) -> String {
    let (text, newline) = match line.strip_suffix('\n') {
        Some(text) => (text, "\n"),
        None => (line, ""),
    };
    let Some((indent, path, span, message)) = split_trailer_line(text) else {
        // Named, because this fires mid-run over a hundred-odd cases — and under
        // `corpus:bless`, before any diff exists to read it out of.
        panic!("{case_name}: this trailer line carries no span: {text}");
    };
    if !path.is_empty() && path != case_path {
        return line.to_owned();
    }
    let qualifier = if path.is_empty() {
        String::new()
    } else {
        format!("{path}:")
    };
    let (start, end) = span;
    format!(
        "{indent}{qualifier}{}..{}: {message}{newline}",
        shown(index.position(start)),
        shown(index.position(end))
    )
}

/// A trailer line as `(indent, path, (start, end), message)`, or `None` if it is not one.
///
/// Anchored at the front — the span is everything before the first `": "` — so that a
/// message which itself contains a byte range is never mistaken for the line's own.
fn split_trailer_line(text: &str) -> Option<(&str, &str, (u32, u32), &str)> {
    let indent_len = text.len() - text.trim_start_matches(' ').len();
    let (indent, rest) = text.split_at(indent_len);
    let (locator, message) = rest.split_once(": ")?;
    let (path, span) = locator.rsplit_once(':').unwrap_or(("", locator));
    let (start, end) = span.split_once("..")?;
    Some((
        indent,
        path,
        (start.parse().ok()?, end.parse().ok()?),
        message,
    ))
}

/// A [`Position`] as a reader counts: from one, on both axes.
fn shown(position: Position) -> String {
    format!("{}:{}", position.line + 1, position.character + 1)
}

/// How much of one case's diff is worth printing before the rest is in the way.
const DIFF_BUDGET: usize = 40;

/// How many differing cases get a diff before the rest are named and nothing more.
///
/// A change at a stage moves every case at once, and what a reader needs then is the
/// list of which ones moved — not the same rewrite shown sixty times, with the one case
/// that moved for a different reason somewhere inside it. Reading all of them is what
/// blessing and `git diff` are for (`tests/corpus/README.md`).
const CASE_BUDGET: usize = 5;

/// Appends the names of the differing cases past [`CASE_BUDGET`], which got no diff of
/// their own. Nothing at all when every differing case already has one — including the
/// run where there are exactly [`CASE_BUDGET`] of them.
fn name_the_rest(report: &mut String, differing: &[String]) {
    let Some(rest) = differing.get(CASE_BUDGET..).filter(|rest| !rest.is_empty()) else {
        return;
    };
    let _ = writeln!(
        report,
        "\nand {} more differing: {}",
        rest.len(),
        rest.join(", ")
    );
}

/// Appends a unified diff of what the case produced against what is checked in.
///
/// A diff, because the question a failure has to answer is *what changed*. Naming the
/// first line that differs answers only where to start looking, which left blessing as
/// the way to see the change — and that inverts the corpus's safety mechanism, since
/// blessing is a proposal to be judged, not how one finds out what is being judged.
fn describe(report: &mut String, case: &Case, expected: &str, actual: &str) {
    let out = &mut *report;
    let _ = writeln!(out, "\n{}:", case.name);
    // Line-equal but not byte-equal: a trailing newline one side has and the other does
    // not. A diff of that is two lines that look identical, so it is said in words.
    if expected.lines().eq(actual.lines()) {
        let _ = writeln!(out, "  differs only in trailing whitespace");
        return;
    }
    let diff = TextDiff::from_lines(expected, actual)
        .unified_diff()
        .header("expected", "actual")
        .to_string();
    let mut lines = diff.lines();
    for line in lines.by_ref().take(DIFF_BUDGET) {
        let _ = writeln!(out, "  {line}");
    }
    let rest = lines.count();
    if rest > 0 {
        let _ = writeln!(out, "  ({rest} more diff lines)");
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
                Some(ext) if is_input(ext) => {}
                Some(ext) if EXPECTATIONS.contains(&ext) => {
                    if !INPUTS.iter().any(|i| path.with_extension(i).exists()) {
                        problems.push(format!("{shown}: expectation with no input"));
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

    /// A template and a document of the same stem: both would derive `twin.cst`, and the
    /// second walked would bless over the first's expectations unnoticed.
    #[test]
    #[should_panic(expected = "they would share expectations")]
    fn refuses_two_inputs_that_would_share_one_set_of_expectations() {
        let dir = tempfile::tempdir().expect("temp dir");
        for name in ["twin.lem", "twin.md"] {
            fs::write(dir.path().join(name), "").expect("write");
        }
        let _ = cases_in(dir.path());
    }

    /// A group with no inputs is a stage checking nothing while reporting success, so
    /// discovery refuses it rather than returning an empty run.
    #[test]
    #[should_panic(expected = "no corpus cases under")]
    fn refuses_a_group_that_holds_no_cases() {
        let dir = tempfile::tempdir().expect("temp dir");
        let _ = cases_in(dir.path());
    }

    #[test]
    #[should_panic(expected = "no corpus group at plan")]
    fn refuses_a_group_that_does_not_exist_yet() {
        let _ = cases("plan");
    }

    #[test]
    #[should_panic(expected = "is not a corpus expectation")]
    fn refuses_an_extension_no_stage_declared() {
        check("syntax", "sexp", |_| String::new());
    }

    /// A case whose source is `text`, named so that [`Case::path`] is `syntax/x.lem`.
    fn case_of(text: &str) -> Case {
        Case {
            name: "syntax/x".to_owned(),
            input: PathBuf::from("syntax/x.lem"),
            source: text.to_owned(),
        }
    }

    /// The extension is the part that matters: it is what a stage decides document mode
    /// by, so a template and a document must not come out named alike.
    #[test]
    fn names_the_path_a_case_is_compiled_under_from_its_input_extension() {
        let template = &cases("syntax")[0];
        assert_eq!(template.path(), format!("{}.lem", template.name));
        let document = &cases("document")[0];
        assert_eq!(document.path(), format!("{}.md", document.name));
    }

    #[test]
    fn renders_a_bare_span_as_one_based_line_and_column() {
        let produced = format!("{TRAILER}\n5..7: unexpected\n");
        assert_eq!(
            with_positions(&case_of("ab\ncdef"), &produced),
            format!("{TRAILER}\n2:3..2:5: unexpected\n")
        );
    }

    #[test]
    fn renders_a_span_qualified_by_this_case_s_own_path() {
        let produced = format!("{TRAILER}\nsyntax/x.lem:0..2: not callable\n");
        assert_eq!(
            with_positions(&case_of("ab\ncd"), &produced),
            format!("{TRAILER}\nsyntax/x.lem:1:1..1:3: not callable\n")
        );
    }

    /// The harness holds one case's source, so a span in another file has nothing to be
    /// placed against. Bytes are honest there; a position would look right and point
    /// nowhere.
    #[test]
    fn leaves_a_span_in_another_file_in_bytes() {
        let produced = format!("{TRAILER}\nsyntax/other.lem:0..2: defined here\n");
        assert_eq!(with_positions(&case_of("ab\ncd"), &produced), produced);
    }

    #[test]
    fn keeps_the_indent_of_a_label() {
        let produced = format!("{TRAILER}\n5..7: redefined\n  0..1: first here\n");
        assert_eq!(
            with_positions(&case_of("ab\ncdef"), &produced),
            format!("{TRAILER}\n2:3..2:5: redefined\n  1:1..1:2: first here\n")
        );
    }

    /// Everything above the marker is the tree, whose byte ranges are the tree-sitter
    /// agreement target.
    #[test]
    fn leaves_the_tree_above_the_marker_untouched() {
        let produced =
            format!("Root@0..7\n  Text@0..7 \"ab\\ncdef\"\n\n{TRAILER}\n5..7: x\n");
        let placed = with_positions(&case_of("ab\ncdef"), &produced);
        assert!(
            placed.starts_with("Root@0..7\n  Text@0..7 \"ab\\ncdef\"\n\n"),
            "{placed}"
        );
        assert!(placed.ends_with("2:3..2:5: x\n"), "{placed}");
    }

    /// What `.tags` goes through: a projection with no trailer at all.
    #[test]
    fn leaves_a_dump_with_no_trailer_alone() {
        let produced = "Root@0..7\n  Text@0..7\n";
        assert_eq!(with_positions(&case_of("ab\ncdef"), produced), produced);
    }

    #[test]
    fn leaves_an_empty_trailer_alone() {
        let produced = format!("Root@0..2\n\n{TRAILER}\n");
        assert_eq!(with_positions(&case_of("ab"), &produced), produced);
    }

    /// A rendered page may print the marker as ordinary output; the trailer a stage
    /// appends is always the last one.
    #[test]
    fn takes_the_last_marker_as_the_one_that_opens_the_trailer() {
        let produced = format!("{TRAILER}\nnot a diagnostic at all\n{TRAILER}\n0..2: x\n");
        assert_eq!(
            with_positions(&case_of("ab"), &produced),
            format!("{TRAILER}\nnot a diagnostic at all\n{TRAILER}\n1:1..1:3: x\n")
        );
    }

    /// The span is what precedes the first `": "`, so a message quoting a byte range of
    /// its own is not mistaken for it.
    #[test]
    fn reads_the_span_from_the_front_of_the_line() {
        let produced = format!("{TRAILER}\n0..2: expected 4..9 here\n");
        assert_eq!(
            with_positions(&case_of("ab"), &produced),
            format!("{TRAILER}\n1:1..1:3: expected 4..9 here\n")
        );
    }

    #[test]
    #[should_panic(
        expected = "syntax/x: this trailer line carries no span: something went wrong"
    )]
    fn refuses_a_trailer_line_that_carries_no_span() {
        let produced = format!("{TRAILER}\nsomething went wrong\n");
        let _ = with_positions(&case_of("ab"), &produced);
    }

    #[test]
    #[should_panic(expected = "syntax/x: this trailer line carries no span: 0..zz: bad")]
    fn refuses_a_trailer_line_whose_span_is_not_a_byte_range() {
        let produced = format!("{TRAILER}\n0..zz: bad\n");
        let _ = with_positions(&case_of("ab"), &produced);
    }

    /// A case with nothing about it that matters here — `describe` reads only its name.
    fn described(expected: &str, actual: &str) -> String {
        let case = Case {
            name: "syntax/x".to_owned(),
            input: PathBuf::from("x.lem"),
            source: String::new(),
        };
        let mut report = String::new();
        describe(&mut report, &case, expected, actual);
        report
    }

    #[test]
    fn diffs_every_line_that_changed_under_the_case_name() {
        let report = described("a\nb\nc\n", "a\nB\nC\n");
        assert!(report.contains("syntax/x:"), "{report}");
        assert!(report.contains("--- expected"), "{report}");
        assert!(report.contains("+++ actual"), "{report}");
        for line in ["-b", "+B", "-c", "+C"] {
            assert!(
                report.lines().any(|l| l.trim_end() == format!("  {line}")),
                "{line} missing from {report}"
            );
        }
    }

    #[test]
    fn shows_a_line_only_one_side_has_as_an_addition() {
        let report = described("a\n", "a\nb\n");
        assert!(report.lines().any(|l| l.trim_end() == "  +b"), "{report}");
        assert!(!report.contains("-b"), "{report}");
        assert!(!report.contains("more diff lines"), "{report}");
    }

    /// A stage change rewrites whole trees, and every one of them at once would bury the
    /// case whose rewrite was the unintended one.
    #[test]
    fn stops_after_a_budget_of_diff_lines_and_says_how_many_are_left() {
        let expected: String = (0..60).map(|i| format!("line {i}\n")).collect();
        let actual: String = (0..60).map(|i| format!("moved {i}\n")).collect();
        let report = described(&expected, &actual);
        // The budget's worth of diff, plus the line saying how much was left out.
        let shown = report.lines().filter(|l| l.starts_with("  ")).count();
        assert_eq!(shown, DIFF_BUDGET + 1, "{report}");
        assert!(report.contains("more diff lines"), "{report}");
    }

    fn names(count: usize) -> Vec<String> {
        (0..count).map(|i| format!("syntax/case-{i}")).collect()
    }

    #[test]
    fn names_the_differing_cases_that_got_no_diff_of_their_own() {
        let mut report = String::new();
        name_the_rest(&mut report, &names(CASE_BUDGET + 2));
        assert!(report.contains("and 2 more differing:"), "{report}");
        let last = format!("syntax/case-{}", CASE_BUDGET + 1);
        assert!(report.contains(&last), "{report}");
        assert!(!report.contains("syntax/case-0"), "{report}");
    }

    /// Including the run that differs in exactly a budget's worth of cases, which has
    /// nothing left over to name.
    #[test]
    fn names_nothing_when_every_differing_case_has_a_diff() {
        let mut report = String::new();
        name_the_rest(&mut report, &names(CASE_BUDGET));
        name_the_rest(&mut report, &names(0));
        assert_eq!(report, "");
    }

    /// A corpus tree with one file of each shape `orphans_in` has an opinion about.
    fn broken_corpus() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("temp dir");
        for name in [
            "README.md",
            "kept.lem",
            "kept.cst",
            "renamed-away.cst",
            "notes.txt",
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
            problems.contains(&"renamed-away.cst: expectation with no input".to_owned()),
            "{problems:?}"
        );
    }

    #[test]
    fn reports_a_file_whose_extension_no_stage_owns() {
        let dir = broken_corpus();
        let problems = orphans_in(dir.path());
        assert!(
            problems.contains(&"notes.txt: `.txt` is not a corpus extension".to_owned()),
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
        let report = described("a\n", "a");
        assert!(report.contains("trailing whitespace"), "{report}");
        assert!(!report.contains("--- expected"), "{report}");
    }
}

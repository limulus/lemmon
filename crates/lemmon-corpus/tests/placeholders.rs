//! The placeholder gate (∆DA2).
//!
//! A stage that does not exist yet is refused with a diagnostic — "not supported yet"
//! in the parser, "not here yet" in the compiler — and described in the docs with a
//! "not yet" note. Each names the backlog item it waits on, and this test fails the build
//! once that item has a journal entry, so landing a stage cannot leave a stale refusal or
//! note behind for an author to read. The rule spans every crate and the docs, which is
//! why it lives beside the corpus's orphan check rather than in one stage's crate.
//!
//! The phrase and the deltoid must share a line: the gate reads text, not values, so a
//! site that picks its item from a table carries the phrase in the table's rows. This
//! file is the one exclusion, since it has to spell the phrases out.

use std::fs;
use std::path::{Path, PathBuf};

/// The phrases that mark a placeholder in Rust source. Ordinary prose says "not yet
/// consumed"; a placeholder says what is not here.
const CODE_PHRASES: &[&str] = &["not here yet", "not supported yet", "not yet supported"];

/// In the docs a placeholder is any "not yet" note.
const DOC_PHRASES: &[&str] = &["not yet"];

/// A line that promises something a backlog item will deliver.
#[derive(Debug, PartialEq, Eq)]
struct Placeholder {
    path: String,
    line: usize,
    text: String,
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the corpus crate sits at <repo>/crates/lemmon-corpus")
        .to_path_buf()
}

/// Every file under `dir` with the extension `ext`, skipping the directories in `skip`.
fn files(dir: &Path, ext: &str, skip: &[&str], out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .map(|entry| entry.expect("a readable directory entry").path())
        .collect();
    entries.sort();
    for path in entries {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if path.is_dir() {
            if !skip.contains(&name) {
                files(&path, ext, skip, out);
            }
        } else if path.extension().is_some_and(|e| e == ext) {
            out.push(path);
        }
    }
}

/// The lines of `text` that carry one of `phrases`, case-insensitively.
fn placeholders(path: &str, text: &str, phrases: &[&str]) -> Vec<Placeholder> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| {
            let lower = line.to_lowercase();
            phrases.iter().any(|phrase| lower.contains(phrase))
        })
        .map(|(i, line)| Placeholder {
            path: path.to_owned(),
            line: i + 1,
            text: line.trim().to_owned(),
        })
        .collect()
}

/// Every deltoid in `text`: a `∆` followed by exactly three ASCII alphanumerics.
fn deltoids(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find('∆') {
        rest = &rest[at + '∆'.len_utf8()..];
        let id: String = rest
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect();
        if id.len() == 3 {
            out.push(format!("∆{id}"));
        }
    }
    out
}

/// What is wrong with a placeholder, if anything: it names no item, names one the
/// backlog does not hold, or names one that has already shipped.
fn fault(placeholder: &Placeholder, backlog: &str, journal: &Path) -> Option<String> {
    let ids = deltoids(&placeholder.text);
    let at = format!("{}:{}", placeholder.path, placeholder.line);
    if ids.is_empty() {
        return Some(format!("{at}: names no backlog item: {}", placeholder.text));
    }
    for id in ids {
        if let Some(entry) = journal_entry(journal, &id) {
            return Some(format!(
                "{at}: {id} has shipped ({}); sweep the placeholder",
                entry.display()
            ));
        }
        if !backlog.contains(&format!("- {id} ")) {
            return Some(format!("{at}: {id} is not an item in BACKLOG.md"));
        }
    }
    None
}

/// The journal entry for `id`, if the item has been completed.
fn journal_entry(journal: &Path, id: &str) -> Option<PathBuf> {
    fs::read_dir(journal)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", journal.display()))
        .map(|entry| entry.expect("a readable directory entry").path())
        .find(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&format!("{id}-")) && n.ends_with(".md"))
        })
}

#[test]
fn every_placeholder_names_a_backlog_item_that_has_not_shipped() {
    let repo = repo();
    let backlog = fs::read_to_string(repo.join("BACKLOG.md")).expect("BACKLOG.md");
    let journal = repo.join("docs/journal");

    let mut sources = Vec::new();
    files(&repo.join("crates"), "rs", &["target"], &mut sources);
    sources.retain(|path| !path.ends_with(file!()));
    let mut docs = Vec::new();
    files(&repo.join("docs"), "md", &["journal"], &mut docs);

    let mut found = Vec::new();
    for (paths, phrases) in [(sources, CODE_PHRASES), (docs, DOC_PHRASES)] {
        for path in paths {
            let text = fs::read_to_string(&path).expect("a UTF-8 file");
            let relative = path.strip_prefix(&repo).expect("a path below the repo");
            found.extend(placeholders(
                &relative.display().to_string(),
                &text,
                phrases,
            ));
        }
    }
    assert!(
        !found.is_empty(),
        "the walk found no placeholder at all; is it looking?"
    );

    let faults: Vec<String> = found
        .iter()
        .filter_map(|p| fault(p, &backlog, &journal))
        .collect();
    assert!(
        faults.is_empty(),
        "placeholders that no longer point at pending work:\n  {}",
        faults.join("\n  ")
    );
}

#[test]
fn finds_placeholder_lines_by_phrase_and_not_ordinary_prose() {
    let text = "let x = 1;\n// At its own close tag, not yet consumed.\nerror(\"`is` is NOT here yet (∆abc)\");\n";
    let found = placeholders("f.rs", text, CODE_PHRASES);
    assert_eq!(
        found,
        vec![Placeholder {
            path: "f.rs".into(),
            line: 3,
            text: "error(\"`is` is NOT here yet (∆abc)\");".into(),
        }]
    );
}

#[test]
fn reads_deltoids_of_exactly_three_alphanumerics() {
    assert_eq!(deltoids("not yet (∆C8d, ∆iCW)"), vec!["∆C8d", "∆iCW"]);
    assert_eq!(deltoids("∆ab and ∆abcd and ∆"), Vec::<String>::new());
}

#[test]
fn flags_a_placeholder_that_names_no_item() {
    let journal = tempfile::tempdir().expect("a temp dir");
    let p = Placeholder {
        path: "a.rs".into(),
        line: 7,
        text: "not here yet".into(),
    };
    assert_eq!(
        fault(&p, "- ∆abc x\n", journal.path()),
        Some("a.rs:7: names no backlog item: not here yet".into())
    );
}

#[test]
fn flags_an_item_that_has_shipped() {
    let journal = tempfile::tempdir().expect("a temp dir");
    let entry = journal.path().join("∆abc-done.md");
    fs::write(&entry, "").expect("a journal entry");
    let p = Placeholder {
        path: "a.rs".into(),
        line: 7,
        text: "not here yet (∆abc)".into(),
    };
    assert_eq!(
        fault(&p, "- ∆abc x\n", journal.path()),
        Some(format!(
            "a.rs:7: ∆abc has shipped ({}); sweep the placeholder",
            entry.display()
        ))
    );
}

#[test]
fn flags_an_item_the_backlog_does_not_hold() {
    let journal = tempfile::tempdir().expect("a temp dir");
    let p = Placeholder {
        path: "docs/x.md".into(),
        line: 1,
        text: "_Not yet (∆zzz)_".into(),
    };
    assert_eq!(
        fault(&p, "- ∆abc x\n", journal.path()),
        Some("docs/x.md:1: ∆zzz is not an item in BACKLOG.md".into())
    );
}

#[test]
fn passes_a_placeholder_naming_a_pending_item() {
    let journal = tempfile::tempdir().expect("a temp dir");
    let p = Placeholder {
        path: "a.rs".into(),
        line: 7,
        text: "not here yet (∆abc)".into(),
    };
    assert_eq!(fault(&p, "- ∆abc x\n", journal.path()), None);
}

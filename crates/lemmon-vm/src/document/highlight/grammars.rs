//! The grammars a host declares, and the syntect set built from them (D40).
//!
//! A host sends its grammars as `.sublime-syntax` text on the declaration. The first code
//! block that names a language parses all of them into one set, linked in declaration
//! order, and every lookup after that is syntect's own — a block's language by extension
//! or name, a grammar's `embed` or `include` by scope or name — so the grammar declared
//! last with a word wins. Built once and never grown, the set gives a block the same HTML
//! whatever an instance highlighted before it (invariant 1).
//!
//! A grammar syntect refuses is left out of the set, and its header is read with the YAML
//! loader syntect uses, so that a block naming its language is told the grammar does not
//! load rather than that there is none.

use std::cell::OnceCell;
use std::fmt;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::Rc;

use lemmon_syntax::{Span, nearest};
use syntect::html::{ClassStyle, ClassedHTMLGenerator};
use syntect::parsing::syntax_definition::{Pattern, SyntaxDefinition};
use syntect::parsing::{ParseSyntaxError, SyntaxSet, SyntaxSetBuilder};
use syntect::util::LinesWithEndings;
use yaml_rust::{Yaml, YamlLoader};

use crate::compile::index_u32;

/// The prefix every highlighted span's classes carry, so that a scope atom — `comment`,
/// `title`, `name` — never collides with a class the site's own markup uses.
const CLASS_PREFIX: &str = "syn-";

/// The grammars a host declared, shared by every view of the declaration.
#[derive(Clone)]
pub(crate) struct Grammars(Rc<Declared>);

struct Declared {
    /// Each grammar's path and text, in declaration order.
    files: Vec<(Rc<str>, Rc<str>)>,
    /// A digest of `files`: what a document highlighted against them records (D25).
    digest: u64,
    /// The set, parsed when the first block that names a language needs it.
    loaded: OnceCell<Loaded>,
}

struct Loaded {
    /// Every grammar syntect accepted, in declaration order, then a plain-text fallback.
    set: SyntaxSet,
    /// Every grammar syntect refused, with the languages its header names where its YAML
    /// could be read at all.
    refused: Vec<(Failure, Option<Header>)>,
}

/// What a block's language word found.
pub(crate) enum Lookup {
    /// The grammar that answers it, by its place in the set.
    Found(usize),
    /// A grammar that names the language but does not load.
    Refused(Failure),
    /// No grammar names it: the nearest language the set has, and each grammar whose
    /// YAML could not be read, since one of those may be the one meant.
    Missing {
        suggestion: Option<String>,
        unreadable: Vec<Failure>,
    },
}

/// A grammar syntect refused, pointed at within its own file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Failure {
    pub path: Rc<str>,
    pub span: Span,
    pub message: String,
}

impl Grammars {
    /// The grammars `files` holds, as `(path, text)` pairs in declaration order.
    pub(crate) fn new<'a>(files: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        let files: Vec<(Rc<str>, Rc<str>)> = files
            .into_iter()
            .map(|(path, text)| (Rc::from(path), Rc::from(text)))
            .collect();
        // `DefaultHasher::new` is keyed the same in every instance, so two cores digest
        // one declaration alike; a digest is compared within one build of the core.
        let mut hasher = DefaultHasher::new();
        files.hash(&mut hasher);
        Self(Rc::new(Declared {
            digest: hasher.finish(),
            files,
            loaded: OnceCell::new(),
        }))
    }

    pub(crate) fn digest(&self) -> u64 {
        self.0.digest
    }

    /// Finds the grammar a block's language word names, parsing every declared grammar
    /// if this is the first block to ask.
    pub(crate) fn lookup(&self, word: &str) -> Lookup {
        let Loaded { set, refused } = self.loaded();
        if let Some(found) = set.find_syntax_by_token(word) {
            let position = set.syntaxes().iter().position(|s| std::ptr::eq(s, found));
            return Lookup::Found(position.expect("a syntax is in its own set"));
        }
        // A grammar that does not load claims only what no grammar that does answers.
        let named = |matches: &dyn Fn(&Header) -> bool| {
            refused
                .iter()
                .rev()
                .find(|(_, header)| header.as_ref().is_some_and(matches))
        };
        let by_extension =
            |h: &Header| h.extensions.iter().any(|e| e.eq_ignore_ascii_case(word));
        if let Some((failure, _)) =
            named(&by_extension).or_else(|| named(&|h| h.name.eq_ignore_ascii_case(word)))
        {
            return Lookup::Refused(failure.clone());
        }
        let (_fallback, accepted) = set
            .syntaxes()
            .split_last()
            .expect("the fallback is in every set");
        let words = accepted.iter().flat_map(|syntax| {
            syntax
                .file_extensions
                .iter()
                .map(String::as_str)
                .chain([syntax.name.as_str()])
        });
        Lookup::Missing {
            suggestion: nearest(word, words).map(str::to_owned),
            unreadable: refused
                .iter()
                .filter(|(_, header)| header.is_none())
                .map(|(failure, _)| failure.clone())
                .collect(),
        }
    }

    /// `code` as HTML, highlighted by the grammar a [`lookup`](Self::lookup) found.
    ///
    /// # Errors
    ///
    /// syntect's, as one line: a grammar that loads can still fail on the code, as when
    /// a context it pushes was never defined.
    pub(crate) fn highlight(&self, syntax: usize, code: &str) -> Result<String, String> {
        let set = &self.loaded().set;
        let mut generator = ClassedHTMLGenerator::new_with_class_style(
            &set.syntaxes()[syntax],
            set,
            ClassStyle::SpacedPrefixed {
                prefix: CLASS_PREFIX,
            },
        );
        for line in LinesWithEndings::from(code) {
            generator
                .parse_html_for_line_which_includes_newline(line)
                .map_err(|e| one_line(&e.to_string()))?;
        }
        Ok(generator.finalize())
    }

    fn loaded(&self) -> &Loaded {
        self.0.loaded.get_or_init(|| {
            let mut builder = SyntaxSetBuilder::new();
            let mut refused = Vec::new();
            for (path, text) in &self.0.files {
                match load(text) {
                    Ok(definition) => builder.add(definition),
                    Err((span, message)) => refused.push((
                        Failure {
                            path: Rc::clone(path),
                            span,
                            message,
                        },
                        read_header(text),
                    )),
                }
            }
            // syntect falls back to a grammar named `Plain Text` for an `embed` whose scope
            // nothing declares, and fails outright without one. It names no extension, and
            // a language word holds no space, so no block finds it.
            builder.add(
                SyntaxDefinition::load_from_str(
                    "name: Plain Text\nscope: text.plain\ncontexts:\n  main: []\n",
                    true,
                    None,
                )
                .expect("the fallback grammar parses"),
            );
            Loaded {
                set: builder.build(),
                refused,
            }
        })
    }
}

impl PartialEq for Grammars {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0) || self.0.files == other.0.files
    }
}

impl Eq for Grammars {}

impl fmt::Debug for Grammars {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.0.files.iter().map(|(path, _)| path))
            .finish()
    }
}

/// `text` as syntect loads it, or where it refused and why: at the character YAML
/// stopped at, or across the whole file for anything else.
fn load(text: &str) -> Result<SyntaxDefinition, (Span, String)> {
    let whole = Span::new(0, index_u32(text.len()));
    let definition = SyntaxDefinition::load_from_str(text, true, None).map_err(|e| {
        let span = match &e {
            ParseSyntaxError::InvalidYaml(scan) => {
                // yaml-rust counts characters; a span counts bytes.
                let at = text
                    .char_indices()
                    .nth(scan.marker().index())
                    .map_or(text.len(), |(byte, _)| byte);
                Span::new(index_u32(at), index_u32(at))
            }
            _ => whole,
        };
        (span, one_line(&e.to_string()))
    })?;
    match unwritable_scope(&definition) {
        Some(scope) => Err((
            whole,
            format!(
                "the scope `{scope}` holds a character HTML reads as markup, and a scope \
                 is written into a class attribute as it is"
            ),
        )),
        None => Ok(definition),
    }
}

/// The first scope, in sorted order, that `definition` gives a span and a class
/// attribute could not hold as written. syntect writes a scope's atoms into `class="…"`
/// without escaping them, so a grammar naming one with a quote or an angle bracket
/// would put markup the host wrote into the page unescaped (invariant 7); such a
/// grammar is refused instead.
fn unwritable_scope(definition: &SyntaxDefinition) -> Option<String> {
    let mut scopes = vec![&definition.scope];
    for context in definition.contexts.values() {
        scopes.extend(&context.meta_scope);
        scopes.extend(&context.meta_content_scope);
        for pattern in &context.patterns {
            if let Pattern::Match(pattern) = pattern {
                scopes.extend(&pattern.scope);
                scopes.extend(pattern.captures.iter().flatten().flat_map(|(_, s)| s));
            }
        }
    }
    scopes
        .into_iter()
        .map(|scope| scope.build_string())
        .filter(|scope| scope.contains(['"', '\'', '&', '<', '>']))
        .min()
}

/// What a refused grammar's header says it is found by.
#[derive(Debug, PartialEq, Eq)]
struct Header {
    name: String,
    extensions: Vec<String>,
}

/// The name and extensions `text` declares, read as syntect's loader reads them — a
/// value that is not a string, or not a list of them, is absent to both — or `None`
/// where the text is not a YAML mapping.
fn read_header(text: &str) -> Option<Header> {
    let documents = YamlLoader::load_from_str(text).ok()?;
    let Some(Yaml::Hash(keys)) = documents.first() else {
        return None;
    };
    let key = |name: &str| keys.get(&Yaml::String(name.to_owned()));
    Some(Header {
        name: key("name")
            .and_then(Yaml::as_str)
            .unwrap_or("Unnamed")
            .to_owned(),
        extensions: ["file_extensions", "hidden_file_extensions"]
            .into_iter()
            .filter_map(|name| key(name)?.as_vec())
            .flatten()
            .filter_map(Yaml::as_str)
            .map(str::to_owned)
            .collect(),
    })
}

/// A message syntect wrote across lines — a regex it quotes, say — on one.
fn one_line(message: &str) -> String {
    message.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{GRAMMARS, at};

    /// A grammar with nothing to highlight, found by `extensions` and named `name`.
    fn grammar(name: &str, scope: &str, extensions: &str) -> String {
        format!(
            "name: {name}\nscope: {scope}\nfile_extensions: [{extensions}]\n\
             contexts:\n  main: []\n"
        )
    }

    /// A grammar syntect refuses — it has no `main` context — found by `extensions`.
    fn refused(name: &str, extensions: &str) -> String {
        format!(
            "name: {name}\nscope: source.refused\nfile_extensions: [{extensions}]\n\
             contexts:\n  other: []\n"
        )
    }

    fn found(lookup: Lookup) -> usize {
        match lookup {
            Lookup::Found(syntax) => syntax,
            Lookup::Refused(_) | Lookup::Missing { .. } => panic!("no grammar was found"),
        }
    }

    fn missing(lookup: Lookup) -> (Option<String>, Vec<Failure>) {
        match lookup {
            Lookup::Missing {
                suggestion,
                unreadable,
            } => (suggestion, unreadable),
            Lookup::Found(_) | Lookup::Refused(_) => panic!("a grammar was found"),
        }
    }

    fn refusal(lookup: Lookup) -> Failure {
        match lookup {
            Lookup::Refused(failure) => failure,
            Lookup::Found(_) | Lookup::Missing { .. } => panic!("no grammar was refused"),
        }
    }

    /// The keys where syntect reads a grammar unlike a glance at its text would: after
    /// `contexts`, given twice, through an alias, typed as something other than a
    /// string, and hidden.
    const UNUSUAL: &str = r#"%YAML 1.2
---
base: &name Unusual
name: First
contexts:
  main:
    - match: x
      scope: keyword
file_extensions: [one, 1, "2", !!int 3, true, [nested]]
hidden_file_extensions: [hidden]
name: *name
scope: source.unusual
"#;

    #[test]
    fn reads_a_header_as_syntects_loader_does() {
        const NAMELESS: &str =
            "scope: source.x\nfile_extensions: sh\ncontexts: {main: []}\n";
        for text in GRAMMARS
            .iter()
            .map(|(_, text)| *text)
            .chain([UNUSUAL, NAMELESS])
        {
            let definition = SyntaxDefinition::load_from_str(text, true, None).unwrap();
            assert_eq!(
                read_header(text),
                Some(Header {
                    name: definition.name,
                    extensions: definition.file_extensions,
                })
            );
        }
    }

    #[test]
    fn finds_a_language_by_extension_before_name_and_without_regard_to_case() {
        let named = grammar("sh", "source.named", "named");
        let grammars = Grammars::new([
            ("extension.sublime-syntax", GRAMMARS[2].1),
            ("named.sublime-syntax", named.as_str()),
        ]);
        assert_eq!(found(grammars.lookup("sh")), 0);
        assert_eq!(found(grammars.lookup("BASH")), 0);
        assert_eq!(found(grammars.lookup("shell")), 0);
        assert_eq!(found(grammars.lookup("Named")), 1);
    }

    #[test]
    fn finds_the_last_grammar_declared_for_a_language() {
        let (first, second) =
            (grammar("A", "source.a", "x"), grammar("B", "source.b", "x"));
        let grammars = Grammars::new([("a", first.as_str()), ("b", second.as_str())]);
        assert_eq!(found(grammars.lookup("x")), 1);
    }

    #[test]
    fn keeps_a_grammar_that_shares_its_scope_with_a_later_one() {
        let early = grammar("Early", "source.same", "early");
        let late = grammar("Late", "source.same", "late");
        let grammars = Grammars::new([("early", early.as_str()), ("late", late.as_str())]);
        assert_eq!(found(grammars.lookup("early")), 0);
        assert_eq!(found(grammars.lookup("late")), 1);
    }

    #[test]
    fn reaches_the_last_grammar_declared_with_a_name_it_includes() {
        let target = |scope: &str, atom: &str| {
            format!(
                "name: Target\nscope: {scope}\ncontexts:\n  main:\n    - match: t\n      \
                 scope: {atom}\n"
            )
        };
        let (early, late) = (
            target("source.early", "early"),
            target("source.late", "late"),
        );
        let includer = "name: I\nscope: source.i\nfile_extensions: [i]\ncontexts:\n  \
                        main:\n    - include: Packages/Target/Target.sublime-syntax\n";
        let grammars = Grammars::new([
            ("early", early.as_str()),
            ("late", late.as_str()),
            ("i", includer),
        ]);
        let html = grammars
            .highlight(found(grammars.lookup("i")), "t\n")
            .unwrap();
        assert!(html.contains("syn-late"), "{html}");
    }

    #[test]
    fn suggests_the_nearest_language_the_set_has() {
        let grammars = Grammars::new(GRAMMARS.iter().copied());
        assert_eq!(missing(grammars.lookup("bsah")).0.as_deref(), Some("bash"));
        assert_eq!(missing(grammars.lookup("lem")).0, None);
        // The fallback is not a language anyone wrote.
        assert_eq!(missing(grammars.lookup("plaintext")).0, None);
    }

    #[test]
    fn refuses_a_language_whose_grammar_does_not_load_across_its_whole_file() {
        let broken = refused("Broken", "broken");
        let grammars = Grammars::new([("broken.sublime-syntax", broken.as_str())]);
        let failure = Failure {
            path: Rc::from("broken.sublime-syntax"),
            span: at(&broken, &broken),
            message: "Context 'main' is missing".to_owned(),
        };
        assert_eq!(refusal(grammars.lookup("broken")), failure);
        assert_eq!(refusal(grammars.lookup("Broken")), failure);
        // And again, however many blocks ask: nothing about a failure is remembered as
        // already said.
        assert_eq!(refusal(grammars.lookup("broken")), failure);
    }

    #[test]
    fn answers_a_language_with_a_grammar_that_loads_before_one_that_does_not() {
        let broken = refused("Broken", "x");
        let working = grammar("Working", "source.w", "x");
        let grammars = Grammars::new([("w", working.as_str()), ("b", broken.as_str())]);
        assert_eq!(found(grammars.lookup("x")), 0);
    }

    #[test]
    fn lists_a_grammar_whose_yaml_cannot_be_read_where_a_language_is_missing() {
        // yaml-rust counts characters; a span counts bytes.
        let text = "name: é\nscope: x: y\n";
        let grammars = Grammars::new([("bad.sublime-syntax", text), ("list", "- a\n")]);
        let (_, unreadable) = missing(grammars.lookup("x"));
        assert_eq!(
            unreadable,
            [
                Failure {
                    path: Rc::from("bad.sublime-syntax"),
                    span: at(text, "x«»: y"),
                    message: "Invalid YAML file syntax: mapping values are not allowed in \
                              this context at line 2 column 9"
                        .to_owned(),
                },
                Failure {
                    path: Rc::from("list"),
                    span: at("- a\n", "- a\n"),
                    message: "Type mismatch".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn writes_what_syntect_refused_on_one_line() {
        let pattern = "name: P\nscope: source.p\nfile_extensions: [p]\ncontexts:\n  \
                       main:\n    - match: '(?x)\n        (?<=a+)b'\n";
        let grammars = Grammars::new([("p", pattern)]);
        let failure = refusal(grammars.lookup("p"));
        assert!(!failure.message.contains('\n'), "{}", failure.message);
        assert!(failure.message.starts_with("Error while compiling regex"));
    }

    #[test]
    fn highlights_without_an_embedded_grammar_that_does_not_load() {
        let broken_css = "name: CSS\nscope: source.css\ncontexts:\n  other: []\n";
        let grammars = Grammars::new([("css", broken_css), ("html", GRAMMARS[1].1)]);
        let html =
            grammars.highlight(found(grammars.lookup("html")), "<style>a {}</style>\n");
        assert!(html.unwrap().contains("a {}"));
    }

    #[test]
    fn falls_back_to_plain_text_for_an_embed_no_grammar_declares() {
        let grammars = Grammars::new([("html", GRAMMARS[1].1)]);
        let html =
            grammars.highlight(found(grammars.lookup("html")), "<style>a {}</style>\n");
        assert!(html.unwrap().contains("a {}"));
    }

    #[test]
    fn refuses_a_grammar_whose_scope_a_class_attribute_cannot_hold() {
        // syntect would write `class="syn-source syn-e"onclick=x"`: the grammar's text,
        // unescaped, in the page.
        for scopes in [
            "scope: 'source.e\"onclick=x'\ncontexts:\n  main: []\n",
            "scope: source.e\ncontexts:\n  main:\n    - meta_scope: 'a<b>'\n",
            "scope: source.e\ncontexts:\n  main:\n    - match: x\n      \
             scope: 'a&amp;'\n",
            "scope: source.e\ncontexts:\n  main:\n    - match: (x)\n      \
             captures:\n        1: \"it's\"\n",
        ] {
            let text = format!("name: E\nfile_extensions: [e]\n{scopes}");
            let grammars = Grammars::new([("e", text.as_str())]);
            let failure = refusal(grammars.lookup("e"));
            assert_eq!(failure.span, at(&text, &text), "{scopes}");
            assert!(
                failure.message.ends_with(
                    "holds a character HTML reads as markup, and a scope is written into a \
                     class attribute as it is"
                ),
                "{}",
                failure.message
            );
        }
    }

    #[test]
    fn refuses_code_a_loaded_grammar_fails_on() {
        let pushes_nowhere = "name: N\nscope: source.n\nfile_extensions: [n]\ncontexts:\n  \
                              main:\n    - match: x\n      push: nowhere\n";
        let grammars = Grammars::new([("n", pushes_nowhere)]);
        assert!(
            grammars
                .highlight(found(grammars.lookup("n")), "x\n")
                .is_err()
        );
    }

    #[test]
    fn prefixes_every_class_it_writes() {
        let grammars = Grammars::new(GRAMMARS.iter().copied());
        let html = grammars
            .highlight(
                found(grammars.lookup("sh")),
                "if true; then echo \"x\"; fi\n",
            )
            .unwrap();
        for classes in html.split("class=\"").skip(1) {
            let classes = &classes[..classes.find('"').unwrap()];
            assert!(
                classes.split(' ').all(|c| c.starts_with(CLASS_PREFIX)),
                "{classes}"
            );
        }
    }

    #[test]
    fn compares_and_digests_declarations_by_their_files() {
        let one = Grammars::new(GRAMMARS.iter().copied());
        let same = Grammars::new(GRAMMARS.iter().copied());
        let edited = Grammars::new(GRAMMARS.iter().map(|&(path, text)| {
            (
                path,
                if path.ends_with("css.sublime-syntax") {
                    "x"
                } else {
                    text
                },
            )
        }));
        assert_eq!(one, one.clone());
        assert_eq!(one, same);
        assert_eq!(one.digest(), same.digest());
        assert_ne!(one, edited);
        assert_ne!(one.digest(), edited.digest());
        assert_eq!(
            format!("{one:?}"),
            concat!(
                r#"["document/grammars/css.sublime-syntax", "#,
                r#""document/grammars/html.sublime-syntax", "#,
                r#""document/grammars/shell.sublime-syntax"]"#,
            )
        );
    }
}

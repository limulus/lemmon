//! A generator's `for:` pair, checked against the program, and the pages it makes
//! (D43).
//!
//! A page template whose front matter carries `for:` is a generator. The host reads and
//! strips the key as it strips `type:`, and sends it as it parsed it, with the
//! positions of the field and of the list; the loader words every way the pair can be
//! malformed, so no host has to check it — the shell walks the mapping only to place
//! each entry's value, and reads the one entry back once pages have been made from it.
//! Once every binding has loaded, the pair is checked against the program — the ambient
//! value is one the host binds and types, the field is declared on it as a list, the
//! page type declares the named field, and the list's element is assignable to it — and
//! the front matter is validated once. Each item then makes a page: the item preset
//! into the named field, as loaded and never a string resolved again as data, and the
//! type's defaults run for it, the url among them. The pages join the page table where
//! the generator stood, in the list's order.

use std::iter;
use std::rc::Rc;

use lemmon_syntax::{Span, nearest};

use crate::compile::index_u32;
use crate::decls::{Decls, no_field};
use crate::host::{Ambient, Host};
use crate::types::{Type, assignable};
use crate::value::{Map, Value};

use super::pages::{Generated, LoadedPage};
use super::place::{At, HostSpan, Place};
use super::validate::Building;
use super::{Doc, Loaded, Loader, Spans};

/// A generator's `for:` pair as the host read it (D43): the value as parsed — one
/// entry, from the field each page's item fills to the list it pages over, when it is
/// well formed — with the positions of both.
#[derive(Clone, Debug, PartialEq)]
pub struct GeneratorKey {
    /// The `for:` key's value, as parsed.
    pub value: Value,
    /// Where it stands, by field path as any table is: `''` the value as a whole, and
    /// each entry from its key through its value — what a diagnostic about the field
    /// points at.
    pub spans: Spans,
    /// Where each entry's value stands alone, by the entry's key — what a diagnostic
    /// about the list points at, so that it underlines `site.topics` and not the
    /// field beside it.
    pub value_spans: Spans,
}

impl GeneratorKey {
    /// Where the pair stands in the file at `path`: the value as a whole, else where the
    /// page's record was `named`.
    pub(super) fn whole(&self, path: &str, named: Span) -> Place {
        self.placed(path, named, None)
    }

    /// Where the entry for `field` stands, from its key through its value.
    fn entry(&self, path: &str, named: Span, field: &str) -> Place {
        self.placed(path, named, self.spans.get(field))
    }

    /// Where the list the entry for `field` names stands, the value alone.
    fn list(&self, path: &str, named: Span, field: &str) -> Place {
        let found = self
            .value_spans
            .get(field)
            .or_else(|| self.spans.get(field));
        self.placed(path, named, found)
    }

    fn placed(&self, path: &str, named: Span, found: Option<&HostSpan>) -> Place {
        match found.or_else(|| self.spans.get("")) {
            Some(at) => Place::hosted(path, at),
            None => Place::resident(path, named),
        }
    }
}

/// How a pair is written: one entry, the field and the list.
const SHAPE: &str = "`for:` takes one entry, the field each page's item fills and the \
                     list it pages over: `topic: site.topics`";

/// How the list is written.
const LIST: &str = "`for:` names its list as `<ambient value>.<field>`, as `site.topics` \
                    does";

/// A value as a refusal shows what was written instead: a string as itself, anything
/// else by its type.
fn shown(value: &Value) -> String {
    match value {
        Value::Str(text) => format!("\"{text}\""),
        other => format!("`{}`", other.type_name()),
    }
}

/// The generator being made: the page, its pair, and where each part stands.
struct Generator<'g> {
    path: &'g str,
    key: &'g GeneratorKey,
    record: &'g Rc<str>,
    /// Where the page's record was named — what a part of the pair the host sent no
    /// position for falls back to.
    named: Span,
}

impl Generator<'_> {
    fn whole(&self) -> Place {
        self.key.whole(self.path, self.named)
    }

    fn entry(&self, field: &str) -> Place {
        self.key.entry(self.path, self.named, field)
    }

    fn list(&self, field: &str) -> Place {
        self.key.list(self.path, self.named, field)
    }
}

/// A well-formed pair: `field`, filled from the list `list` of the ambient value
/// `value`.
struct Pair {
    field: Rc<str>,
    value: Rc<str>,
    list: Rc<str>,
}

impl Pair {
    /// The list as `for:` names it: `site.topics`.
    fn named(&self) -> String {
        format!("{}.{}", self.value, self.list)
    }
}

impl Loader<'_> {
    /// Makes the pages of the generator `loaded`, typed `record` as named at `named`,
    /// from the list its pair `key` names: one per item, in the list's order, each
    /// joining the page table where the generator stands. Every check stops the
    /// generator where it fails, as does a default that faults for an item, so that
    /// one mistake costs one diagnostic however long the list — but the front matter's
    /// keys are each checked before it stops, so that one mapping's mistakes are told
    /// together. An empty list makes no pages, and says nothing.
    pub(super) fn generate(
        &mut self,
        loaded: &Loaded,
        key: &GeneratorKey,
        record: &Rc<str>,
        named: Span,
    ) {
        let generator = Generator {
            path: loaded.path.as_deref().expect("checked by `check_envelope`"),
            key,
            record,
            named,
        };
        let Some(pair) = self.pair(&generator) else {
            return;
        };
        let Some(element) = self.element(&generator, &pair) else {
            return;
        };
        if !self.fills(&generator, &pair, &element) {
            return;
        }
        let at = At::root(
            Some((generator.path, &loaded.spans)),
            "",
            self.type_place(record),
        );
        let Some(given) = self.front_matter(loaded, record, &at) else {
            return;
        };
        let left = self.leaves(&generator, &pair, given, &at) && self.places(&generator);
        // The item and the url are the generator's: the rest of the front matter is
        // validated without either key, which `leaves` refused if given, so that its
        // other mistakes are told beside that refusal, and no key twice.
        let host: &Host = self.host;
        let held: Vec<&str> = iter::once(&*pair.field).chain(host.url()).collect();
        let rest: Map = given
            .iter()
            .filter(|(key, _)| !held.contains(&&***key))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let doc = Doc {
            path: generator.path,
            body: None,
        };
        let shared = self.prepared(&rest, record, &at, Some(&doc), &held);
        let (true, Some(shared)) = (left, shared) else {
            return;
        };
        let Some(items) = self.items(&generator, &pair) else {
            return;
        };
        let entry = generator.entry(&pair.field);
        let list = pair.named();
        let mut made = Vec::with_capacity(items.len());
        for (index, item) in items.iter().enumerate() {
            let mut fields = shared.clone();
            fields.push((pair.field.clone(), item.clone()));
            let building = Building::Item {
                list: &list,
                index,
                entry: &entry,
            };
            let Some(fields) = self.planned(record, fields, Some(&doc), &building) else {
                return;
            };
            made.push(fields);
        }
        // Each page's neighbours are the pages either side of it in the list, by their
        // positions in the table this generator's pages join here; the first has no
        // previous, the last no next.
        let neighbours = self.neighbours(record, Some(&doc));
        let first = self.pages.len();
        let count = made.len();
        let list: Rc<str> = Rc::from(list);
        let at = key.spans.get(&*pair.field).or_else(|| key.spans.get(""));
        for (index, mut fields) in made.into_iter().enumerate() {
            if let Some((previous, next)) = neighbours {
                let sides = [
                    (previous, index.checked_sub(1)),
                    (next, Some(index + 1).filter(|after| *after < count)),
                ];
                for (field, side) in sides {
                    let value = side.map_or(Value::Null, |side| {
                        Value::Neighbour(Some(index_u32(first + side)))
                    });
                    fields.push((Rc::from(field), value));
                }
            }
            self.pages.push(LoadedPage {
                path: generator.path.to_owned(),
                record: self.assembled(record, &fields),
                spans: loaded.spans.clone(),
                record_span: named,
                generated: Some(Generated {
                    index: index_u32(index),
                    list: list.clone(),
                    entry: at.copied(),
                }),
            });
        }
    }

    /// The pair's one entry, or nothing once what is wrong with its shape is reported:
    /// a value that is no mapping, a mapping of no entry or of two, or a list that is
    /// not written `<ambient value>.<field>`.
    fn pair(&mut self, generator: &Generator<'_>) -> Option<Pair> {
        let Value::Map(entries) = &generator.key.value else {
            let got = shown(&generator.key.value);
            self.diagnostics
                .push(generator.whole().error(format!("{SHAPE}; got {got}")));
            return None;
        };
        let mut entries = entries.iter();
        let Some((field, list)) = entries.next() else {
            self.diagnostics
                .push(generator.whole().error(format!("{SHAPE}; got none")));
            return None;
        };
        if let Some((second, _)) = entries.next() {
            self.diagnostics.push(
                generator
                    .entry(second)
                    .error(format!("{SHAPE}; this is a second")),
            );
            return None;
        }
        let split = match list {
            Value::Str(text) => text
                .split_once('.')
                .filter(|(value, list)| {
                    !value.is_empty() && !list.is_empty() && !list.contains('.')
                })
                .map(|(value, list)| (Rc::from(value), Rc::from(list))),
            _ => None,
        };
        let Some((value, list_name)) = split else {
            let got = shown(list);
            self.diagnostics
                .push(generator.list(field).error(format!("{LIST}; got {got}")));
            return None;
        };
        Some(Pair {
            field: field.clone(),
            value,
            list: list_name,
        })
    }

    /// The element type of the list the pair names, or nothing once what is wrong has
    /// been reported: the host binds no such value, or binds the document value, or
    /// binds one without a type; the value declares no such field, or declares it as
    /// something other than a list, or as a list of `any` — which nothing could check
    /// an item of against the field it fills.
    fn element(&mut self, generator: &Generator<'_>, pair: &Pair) -> Option<Type> {
        let at = generator.list(&pair.field);
        let named = pair.named();
        let value = &*pair.value;
        let refused = |loader: &mut Self, message: String| {
            loader.diagnostics.push(at.error(message));
            None
        };
        let binding = match self.host.binding(value) {
            None => {
                let mut message = format!("the host binds no `{value}`");
                if let Some(near) = nearest(value, self.host.names()) {
                    message.push_str(&format!("; did you mean `{near}`?"));
                }
                return refused(self, message);
            }
            Some(_) if self.host.document() == Some(value) => {
                return refused(
                    self,
                    format!(
                        "`{value}` is bound to a record once it is loaded, so `for:` \
                         cannot page over a field of it"
                    ),
                );
            }
            Some(binding) => binding.clone(),
        };
        let record = match &binding {
            Ambient::Untyped => {
                return refused(
                    self,
                    format!(
                        "the host binds `{value}` without a type, so `{named}` cannot \
                         be checked against `{}`",
                        pair.field
                    ),
                );
            }
            Ambient::Named(record) | Ambient::Bindings(record) => record.clone(),
        };
        let Some(ty) = self.module.field(&record, &pair.list) else {
            return refused(self, no_field(&record, &pair.list, self.module));
        };
        match &ty {
            Type::List(element) if **element != Type::Any => Some((**element).clone()),
            Type::List(_) | Type::Any => refused(
                self,
                format!(
                    "`{named}` is declared `{ty}`, so its items cannot be checked against \
                     `{}`",
                    pair.field
                ),
            ),
            Type::Map(_) => {
                let mut message =
                    format!("`{named}` is a `{ty}`, and a generator pages over a list");
                if let Ambient::Bindings(file) = &binding {
                    message.push_str(&format!(
                        ": a `{file}` binding of `{}.values()` makes one",
                        pair.list
                    ));
                }
                refused(self, message)
            }
            other => refused(
                self,
                format!("`{named}` is a `{other}`, and a generator pages over a list"),
            ),
        }
    }

    /// Whether each item of a list of `element` can fill the pair's field: the page's
    /// record declares it, it is neither the url field, whose value a generated page's
    /// type computes (D41), nor a neighbour, which the loader fills, and the element is
    /// assignable to it.
    fn fills(&mut self, generator: &Generator<'_>, pair: &Pair, element: &Type) -> bool {
        let at = generator.entry(&pair.field);
        let record = generator.record;
        let field = &*pair.field;
        let message = match self.module.field(record, field) {
            None => no_field(record, field, self.module),
            Some(_) if self.host.url() == Some(field) => format!(
                "a generated page's `{field}` comes from `{record}`'s default, so `for:` \
                 cannot fill it"
            ),
            Some(_) if self.host.is_neighbour(field) => format!(
                "`{field}` is a page's neighbour, which the loader fills, so `for:` cannot \
                 fill it"
            ),
            Some(ty) if !assignable(element, &ty, self.module) => format!(
                "`{}` holds `{element}` items, and `{field}` is a `{ty}`",
                pair.named()
            ),
            Some(_) => return true,
        };
        self.diagnostics.push(at.error(message));
        false
    }

    /// Whether the front matter `given` leaves to the generator what is the
    /// generator's: the field each item fills, and the url its type computes. Each
    /// key given is an error where it stands, labelled at the `for:` entry.
    fn leaves(
        &mut self,
        generator: &Generator<'_>,
        pair: &Pair,
        given: &Map,
        at: &At<'_>,
    ) -> bool {
        let mut left = true;
        let mut refuse = |loader: &mut Self, field: &str, message: String| {
            let place = loader.field_place(generator.record, field);
            let error = at.child(field, place).error(message);
            let entry = generator.entry(&pair.field);
            loader
                .diagnostics
                .push(entry.label(error, "`for:` makes this page a generator here"));
            left = false;
        };
        if given.get(&pair.field).is_some() {
            refuse(
                self,
                &pair.field,
                format!(
                    "`{}` is each page's item from `{}`, so the front matter cannot give it",
                    pair.field,
                    pair.named()
                ),
            );
        }
        if let Some(url) = self.host.url()
            && given.get(url).is_some()
        {
            refuse(
                self,
                url,
                format!(
                    "a generated page's `{url}` comes from `{}`'s default, so the front \
                     matter cannot give it",
                    generator.record
                ),
            );
        }
        left
    }

    /// Whether every page the generator makes gets its url from a default, where the
    /// host places pages: one diagnostic for a type that gives none, before any item —
    /// not one "missing" per page — and with an empty list too, since it is a property
    /// of the template.
    fn places(&mut self, generator: &Generator<'_>) -> bool {
        let Some(url) = self.host.url() else {
            return true;
        };
        let record = generator.record;
        let has_default = self
            .plan(record)
            .steps
            .iter()
            .any(|step| *step.field == *url);
        if !has_default {
            self.diagnostics.push(generator.whole().error(format!(
                "`{record}` gives `{url}` no default, and every page a generator makes \
                 takes its url from one"
            )));
        }
        has_default
    }

    /// The items of the list the pair names, as loaded. The value is a record the
    /// loader validated, and the list a field of it; but a computed binding's value is
    /// what its function returned, which may have come through `any`, so a value that
    /// is no list is said, not trusted.
    fn items(&mut self, generator: &Generator<'_>, pair: &Pair) -> Option<Rc<[Value]>> {
        const NOTHING: &Value = &Value::Null;
        let held = self
            .ambient
            .iter()
            .find(|(name, _)| *name == pair.value)
            .and_then(|(_, value)| match value {
                Value::Record(record) => record.fields.get(&pair.list),
                _ => None,
            })
            .unwrap_or(NOTHING);
        if let Value::List(items) = held {
            return Some(items.clone());
        }
        let message = format!(
            "`{}` holds {}, not a list, so `for:` cannot page over it",
            pair.named(),
            shown(held)
        );
        self.diagnostics
            .push(generator.list(&pair.field).error(message));
        None
    }
}

#[cfg(test)]
mod tests {
    use lemmon_syntax::spans::at;

    use super::*;
    use crate::bytecode::Module;
    use crate::load::tests::{build, object, page, shell, spans};
    use crate::load::{Data, Source, Store, load};
    use crate::{SourceFile, compile};

    const AMBIENT: &str = "<lem-type Page>\n  url: url\n</lem-type>\n\
         <lem-type Build>\n  generator: string\n</lem-type>";
    /// A topic page type whose url and title its defaults derive from the item, a
    /// static page type beside it, and the lists a generator pages over.
    const SITE: &str = "<lem-type Topic extends Page>\n  topic: string\n  \
         title: string = \"TILs: \" + self.topic\n  \
         url: url = url(\"/t/\" + self.topic + \"/\")\n  \
         layout: (Topic, html): html = topical\n</lem-type>\n\
         <lem-type Note extends Page>\n  layout: (Note, html): html = noted\n</lem-type>\n\
         <lem-fn topical(t: Topic, b: html)><lem.b></lem-fn>\n\
         <lem-fn noted(n: Note, b: html)><lem.b></lem-fn>\n\
         <lem-let topics: [string] = [\"git\", \"node\", \"rust\"] />\n\
         <lem-let none: [string] = [] />";
    const TOPICS: &str =
        "---\ntype: Topic\nfor:\n  topic: site.topics\n---\n<lem.page.title>";
    const NOTE: &str = "---\ntype: Note\n---\nnote";

    /// The shell, placing pages by `url`.
    fn placed() -> Host {
        shell().addressed_by("url")
    }

    /// `site` and the page templates `pages`, each `(path, text, record)`, compiled
    /// under `host` with the record named at its `type:` key.
    fn compiled(host: &Host, site: &str, pages: &[(&str, &str, &str)]) -> Module {
        let mut files = vec![
            SourceFile::new("ambient.lem", AMBIENT),
            SourceFile::new("site.lem", site),
        ];
        for (path, text, record) in pages {
            let mut file = SourceFile::new(*path, *text).with_record(*record);
            // A glob's member is typed by its glob, and names its record nowhere.
            if text.contains(record) {
                file = file.named_at(at(text, record));
            }
            files.push(file);
        }
        compile(&files, host).expect_output()
    }

    /// The pair `field: list`, as a host reads it from `text`, which writes it once.
    fn key(text: &str, field: &str, list: &str) -> GeneratorKey {
        let entry = format!("{field}: {list}");
        GeneratorKey {
            value: object(&[(field, Value::str(list))]),
            spans: spans(text, &[("", at(text, &entry)), (field, at(text, &entry))]),
            value_spans: spans(text, &[(field, at(text, &format!("{field}: «{list}»")))]),
        }
    }

    /// The generator at `path`, its front matter `front`, its pair `key`.
    fn generator(path: &str, front: Value, key: GeneratorKey) -> Loaded {
        Loaded {
            generator: Some(key),
            ..page(path, front)
        }
    }

    /// An envelope carrying `pages` on their own, and nothing else but `build`.
    fn carrying(pages: Vec<Loaded>) -> Data {
        Data {
            ambient: vec![build()],
            sources: Vec::new(),
            pages,
        }
    }

    fn loaded(module: &Module, host: &Host, pages: Vec<Loaded>) -> Store {
        load(module, host, carrying(pages)).unwrap().expect_output()
    }

    fn errors(module: &Module, host: &Host, pages: Vec<Loaded>) -> Vec<String> {
        crate::load::tests::errors(module, host, carrying(pages))
    }

    fn field<'v>(page: &'v LoadedPage, name: &str) -> &'v Value {
        let Value::Record(record) = &page.record else {
            panic!("a record: {:?}", page.record);
        };
        record.fields.get(name).expect("a field the record has")
    }

    fn topics() -> Loaded {
        generator("t.lem", object(&[]), key(TOPICS, "topic", "site.topics"))
    }

    #[test]
    fn makes_one_page_per_item_in_the_lists_order() {
        let module = compiled(&placed(), SITE, &[("t.lem", TOPICS, "Topic")]);
        let store = loaded(&module, &placed(), vec![topics()]);
        let made: Vec<(Option<u32>, &Value, &Value)> = store
            .pages
            .iter()
            .map(|page| (page.index(), field(page, "topic"), field(page, "title")))
            .collect();
        assert_eq!(
            made,
            [
                (Some(0), &Value::str("git"), &Value::str("TILs: git")),
                (Some(1), &Value::str("node"), &Value::str("TILs: node")),
                (Some(2), &Value::str("rust"), &Value::str("TILs: rust")),
            ]
        );
        // Each page is read back by its index, and the path alone names none of them.
        let second = store.page("t.lem", Some(1)).expect("the second page");
        assert_eq!(field(second, "topic"), &Value::str("node"));
        assert_eq!(store.page("t.lem", None), None);
        assert_eq!(store.page("t.lem", Some(3)), None);
        let host = placed();
        let globals = store.globals(&host, "t.lem", Some(2));
        assert_eq!(globals.last().map(|(name, _)| *name), Some("page"));
        assert_eq!(
            globals.last().map(|(_, value)| value),
            Some(&store.pages[2].record)
        );
        let urls: Vec<(Option<u32>, Option<String>)> = store
            .plan(&placed())
            .into_iter()
            .map(|planned| (planned.index, planned.url))
            .collect();
        assert_eq!(
            urls,
            [
                (Some(0), Some("/t/git/".to_owned())),
                (Some(1), Some("/t/node/".to_owned())),
                (Some(2), Some("/t/rust/".to_owned())),
            ]
        );
        assert_eq!(store.pages[1].name(), "`t.lem` for `site.topics[1]`");
    }

    #[test]
    fn makes_its_pages_where_it_stands_among_the_envelopes() {
        let module = compiled(
            &placed(),
            SITE,
            &[
                ("a.lem", NOTE, "Note"),
                ("t.lem", TOPICS, "Topic"),
                ("b.lem", NOTE, "Note"),
            ],
        );
        let note = |path: &str, url: &str| page(path, object(&[("url", Value::str(url))]));
        let store = loaded(
            &module,
            &placed(),
            vec![note("a.lem", "/a/"), topics(), note("b.lem", "/b/")],
        );
        let planned: Vec<(String, Option<u32>)> = store
            .plan(&placed())
            .into_iter()
            .map(|planned| (planned.path, planned.index))
            .collect();
        let at = |path: &str, index: Option<u32>| (path.to_owned(), index);
        assert_eq!(
            planned,
            [
                at("a.lem", None),
                at("t.lem", Some(0)),
                at("t.lem", Some(1)),
                at("t.lem", Some(2)),
                at("b.lem", None),
            ]
        );
        assert_eq!(store.pages[0].name(), "`a.lem`");
    }

    #[test]
    fn makes_no_page_of_an_empty_list_and_says_nothing() {
        let module = compiled(&placed(), SITE, &[("t.lem", TOPICS, "Topic")]);
        let store = loaded(&module, &placed(), vec![over("site.none").1]);
        assert_eq!(store.pages, []);
    }

    #[test]
    fn refuses_a_pair_of_any_other_shape_once_and_without_panicking() {
        let module = compiled(&placed(), SITE, &[("t.lem", TOPICS, "Topic")]);
        let whole = at(TOPICS, "topic: site.topics");
        let list = at(TOPICS, "topic: «site.topics»");
        let refused = |value: Value| {
            let key = GeneratorKey {
                value,
                spans: spans(TOPICS, &[("", whole), ("topic", whole)]),
                value_spans: spans(TOPICS, &[("topic", list)]),
            };
            errors(
                &module,
                &placed(),
                vec![generator("t.lem", object(&[]), key)],
            )
        };
        let shape = "`for:` takes one entry, the field each page's item fills and the list \
                     it pages over: `topic: site.topics`";
        for (value, got) in [
            (Value::list(vec![Value::str("site.topics")]), "`list`"),
            (Value::Int(3), "`int`"),
            (Value::Null, "`null`"),
            (Value::str("site.topics"), "\"site.topics\""),
            (object(&[]), "none"),
        ] {
            assert_eq!(
                refused(value),
                [format!("t.lem:{whole}: {shape}; got {got}")]
            );
        }
        let named = "`for:` names its list as `<ambient value>.<field>`, as \
                     `site.topics` does";
        for (list_value, got) in [
            (Value::Int(3), "`int`"),
            (Value::list(vec![]), "`list`"),
            (Value::str(""), "\"\""),
            (Value::str("."), "\".\""),
            (Value::str("site."), "\"site.\""),
            (Value::str(".topics"), "\".topics\""),
            (Value::str("site.topics.x"), "\"site.topics.x\""),
            (Value::str("topics"), "\"topics\""),
        ] {
            assert_eq!(
                refused(object(&[("topic", list_value)])),
                [format!("t.lem:{list}: {named}; got {got}")]
            );
        }
    }

    #[test]
    fn refuses_a_second_entry_where_it_stands() {
        let text =
            "---\ntype: Topic\nfor:\n  topic: site.topics\n  title: site.topics\n---\n";
        let module = compiled(&placed(), SITE, &[("t.lem", text, "Topic")]);
        let key = GeneratorKey {
            value: object(&[
                ("topic", Value::str("site.topics")),
                ("title", Value::str("site.topics")),
            ]),
            spans: spans(
                text,
                &[
                    ("", at(text, "topic: site.topics\n  title: site.topics")),
                    ("topic", at(text, "topic: site.topics")),
                    ("title", at(text, "title: site.topics")),
                ],
            ),
            value_spans: Spans::new(),
        };
        assert_eq!(
            errors(
                &module,
                &placed(),
                vec![generator("t.lem", object(&[]), key)]
            ),
            [format!(
                "t.lem:{}: `for:` takes one entry, the field each page's item fills and \
                 the list it pages over: `topic: site.topics`; this is a second",
                at(text, "title: site.topics")
            )]
        );
    }

    #[test]
    fn places_a_pair_the_host_sent_no_positions_for_at_its_record() {
        let module = compiled(&placed(), SITE, &[("t.lem", TOPICS, "Topic")]);
        let unplaced = GeneratorKey {
            value: Value::Null,
            spans: Spans::new(),
            value_spans: Spans::new(),
        };
        let shown = errors(
            &module,
            &placed(),
            vec![generator("t.lem", object(&[]), unplaced)],
        );
        assert_eq!(shown.len(), 1);
        assert!(
            shown[0].starts_with(&format!("t.lem:{}: ", at(TOPICS, "Topic"))),
            "{shown:?}"
        );
    }

    #[test]
    fn falls_back_from_the_list_to_its_entry_then_to_the_whole() {
        let module = compiled(&placed(), SITE, &[("t.lem", TOPICS, "Topic")]);
        let entry = at(TOPICS, "topic: site.topics");
        let refused = |spans: Spans| {
            let key = GeneratorKey {
                value: object(&[("topic", Value::str("site.topik"))]),
                spans,
                value_spans: Spans::new(),
            };
            errors(
                &module,
                &placed(),
                vec![generator("t.lem", object(&[]), key)],
            )
        };
        let message = "`site.lem` has no field `topik`; did you mean `topics`?";
        assert_eq!(
            refused(spans(TOPICS, &[("topic", entry)])),
            [format!("t.lem:{entry}: {message}")]
        );
        let whole = at(TOPICS, "for:\n  topic: site.topics");
        assert_eq!(
            refused(spans(TOPICS, &[("", whole)])),
            [format!("t.lem:{whole}: {message}")]
        );
    }

    /// The generator `t.lem` paging `topic` over `list`, as a host reads it from the
    /// text it writes it in — returned too, for where a diagnostic stands.
    fn over(list: &str) -> (String, Loaded) {
        let text = TOPICS.replace("site.topics", list);
        let loaded = generator("t.lem", object(&[]), key(&text, "topic", list));
        (text, loaded)
    }

    #[test]
    fn refuses_a_list_whose_items_it_cannot_check_against_the_field() {
        // An untyped value, a list of `any`, and a field of `any`: nothing to check an
        // item against, so nothing is assumed (invariant 10). A map the host typed is
        // refused as any other non-list is — the hint is for a bindings value, which a
        // `site.lem` binding could turn into a list.
        let host = placed().bind("data", Ambient::Untyped);
        let ambient = "<lem-type Page>\n  url: url\n</lem-type>\n\
             <lem-type Build>\n  generator: string\n  tags: {string: string}\n  \
             motto: string\n</lem-type>";
        let site = format!(
            "{SITE}\n<lem-let anyList: [any] = [] />\n<lem-let anything: any = [] />"
        );
        let files = [
            SourceFile::new("ambient.lem", ambient),
            SourceFile::new("site.lem", site),
            SourceFile::new("t.lem", TOPICS)
                .with_record("Topic")
                .named_at(at(TOPICS, "Topic")),
        ];
        let module = compile(&files, &host).expect_output();
        let refused = |list: &str, message: &str| {
            let (text, loaded) = over(list);
            let data = Data {
                ambient: vec![
                    (
                        "build".to_owned(),
                        object(&[
                            ("generator", Value::str("lemmon")),
                            ("tags", object(&[])),
                            ("motto", Value::str("m")),
                        ]),
                    ),
                    ("data".to_owned(), object(&[])),
                ],
                sources: Vec::new(),
                pages: vec![loaded],
            };
            assert_eq!(
                crate::load::tests::errors(&module, &host, data),
                [format!(
                    "t.lem:{}: {message}",
                    at(&text, &format!("topic: «{list}»"))
                )]
            );
        };
        refused(
            "data.topics",
            "the host binds `data` without a type, so `data.topics` cannot be checked \
             against `topic`",
        );
        refused(
            "site.anyList",
            "`site.anyList` is declared `[any]`, so its items cannot be checked against \
             `topic`",
        );
        refused(
            "site.anything",
            "`site.anything` is declared `any`, so its items cannot be checked against \
             `topic`",
        );
        refused(
            "build.tags",
            "`build.tags` is a `{string: string}`, and a generator pages over a list",
        );
        refused(
            "build.motto",
            "`build.motto` is a `string`, and a generator pages over a list",
        );
        refused(
            "build.mottos",
            "`Build` has no field `mottos`; did you mean `motto`?",
        );
    }

    #[test]
    fn says_so_when_a_list_holds_no_list_through_any() {
        // `any` is assignable to `[string]`, so what the binding's function returned is
        // whatever the untyped value held.
        let host = placed().bind("data", Ambient::Untyped);
        let site = format!("{SITE}\n<lem-let fetched: [string] = data.topics />");
        let files = [
            SourceFile::new("ambient.lem", AMBIENT),
            SourceFile::new("site.lem", site),
            SourceFile::new("t.lem", TOPICS)
                .with_record("Topic")
                .named_at(at(TOPICS, "Topic")),
        ];
        let module = compile(&files, &host).expect_output();
        let (text, loaded) = over("site.fetched");
        let data = Data {
            ambient: vec![
                build(),
                ("data".to_owned(), object(&[("topics", Value::str("git"))])),
            ],
            sources: Vec::new(),
            pages: vec![loaded],
        };
        assert_eq!(
            crate::load::tests::errors(&module, &host, data),
            [format!(
                "t.lem:{}: `site.fetched` holds \"git\", not a list, so `for:` cannot page \
                 over it",
                at(&text, "topic: «site.fetched»")
            )]
        );
    }

    #[test]
    fn takes_each_item_as_loaded_and_runs_no_default_or_index_of_its_field() {
        // `photo` has a default and a `by` index of its own; the item fills it as the
        // list holds it, so the default never runs and no string is looked up.
        let site = "<lem-type Photo>\n  caption: string\n</lem-type>\n\
             <lem-type PhotoPage extends Page>\n  \
             photo: Photo by site.photos = {caption: \"the default\"}\n  \
             url: url = url(\"/p/\" + self.photo.caption + \"/\")\n  \
             layout: (PhotoPage, html): html = pictured\n</lem-type>\n\
             <lem-fn pictured(p: PhotoPage, b: html)><lem.b></lem-fn>\n\
             <lem-let photos: {string: Photo} = {\"a\": {caption: \"dawn\"}, \
             \"b\": {caption: \"dusk\"}} />\n\
             <lem-let gallery: [Photo] = photos.values() />";
        let text = "---\ntype: PhotoPage\nfor:\n  photo: site.gallery\n---\n";
        let module = compiled(&placed(), site, &[("p.lem", text, "PhotoPage")]);
        let photos = generator("p.lem", object(&[]), key(text, "photo", "site.gallery"));
        let store = loaded(&module, &placed(), vec![photos]);
        let captions: Vec<&Value> = store
            .pages
            .iter()
            .map(|page| {
                let Value::Record(photo) = field(page, "photo") else {
                    panic!("a photo record");
                };
                photo.fields.get("caption").expect("a caption")
            })
            .collect();
        assert_eq!(captions, [&Value::str("dawn"), &Value::str("dusk")]);
    }

    #[test]
    fn reports_a_front_matter_key_once_however_long_the_list() {
        let module = compiled(&placed(), SITE, &[("t.lem", TOPICS, "Topic")]);
        let front = object(&[("titel", Value::str("x"))]);
        let typo = generator("t.lem", front, key(TOPICS, "topic", "site.topics"));
        assert_eq!(
            errors(&module, &placed(), vec![typo]),
            [format!(
                "site.lem:{}: `Topic` has no field `titel`; did you mean `title`?",
                at(SITE, "<lem-type «Topic» extends")
            )]
        );
    }

    #[test]
    fn reports_a_type_with_no_url_default_once_with_three_items_and_with_none() {
        let site = SITE.replace("  url: url = url(\"/t/\" + self.topic + \"/\")\n", "");
        let module = compiled(&placed(), &site, &[("t.lem", TOPICS, "Topic")]);
        for list in ["site.topics", "site.none"] {
            let (text, loaded) = over(list);
            assert_eq!(
                errors(&module, &placed(), vec![loaded]),
                [format!(
                    "t.lem:{}: `Topic` gives `url` no default, and every page a generator \
                     makes takes its url from one",
                    at(&text, &format!("topic: {list}"))
                )]
            );
        }
        // A host that places no page asks for no url default, and plans none.
        let unplaced = compiled(&shell(), SITE, &[("t.lem", TOPICS, "Topic")]);
        let store = loaded(&unplaced, &shell(), vec![topics()]);
        assert_eq!(store.pages.len(), 3);
        assert!(
            store
                .plan(&shell())
                .iter()
                .all(|planned| planned.url.is_none())
        );
    }

    #[test]
    fn labels_a_default_faulting_for_one_item_with_that_item() {
        let site = SITE.replace(
            "  title: string = \"TILs: \" + self.topic\n",
            "  title: string = {\"git\": \"Git\", \"rust\": \"Rust\"}[self.topic]\n",
        );
        let module = compiled(&placed(), &site, &[("t.lem", TOPICS, "Topic")]);
        let result = load(&module, &placed(), carrying(vec![topics()])).unwrap();
        assert!(result.output.is_none());
        let shown: Vec<String> =
            result.diagnostics.iter().map(ToString::to_string).collect();
        assert_eq!(
            shown,
            [format!(
                "site.lem:{}: no key \"node\"\n  t.lem:{}: computing `title` for \
                 `site.topics[1]`",
                at(&site, "{\"git\": \"Git\", \"rust\": \"Rust\"}[self.topic]"),
                at(TOPICS, "topic: site.topics")
            )]
        );
    }

    #[test]
    fn says_once_that_its_url_default_gives_its_items_one_url() {
        // Three items at one url are one mistake, the default's: said of the first two,
        // where the two stand at one place, so with no label pointing back at it.
        let site = SITE.replace("url(\"/t/\" + self.topic + \"/\")", "url(\"/t/\")");
        let module = compiled(&placed(), &site, &[("t.lem", TOPICS, "Topic")]);
        let message = "`t.lem` for `site.topics[1]` is at `/t/`, as `site.topics[0]` is: \
                       the url default gives two of its items one url";
        assert_eq!(
            errors(&module, &placed(), vec![topics()]),
            [format!(
                "t.lem:{}: {message}",
                at(TOPICS, "topic: site.topics")
            )]
        );
        // With no position for the pair, it stands where the record was named.
        let unplaced = GeneratorKey {
            spans: Spans::new(),
            value_spans: Spans::new(),
            ..key(TOPICS, "topic", "site.topics")
        };
        assert_eq!(
            errors(
                &module,
                &placed(),
                vec![generator("t.lem", object(&[]), unplaced)]
            ),
            [format!("t.lem:{}: {message}", at(TOPICS, "Topic"))]
        );
    }

    #[test]
    fn names_a_generated_page_at_another_pages_url_by_its_item() {
        // Another file's page is another mistake each time, labelled where it stands.
        let module = compiled(
            &placed(),
            SITE,
            &[("a.lem", NOTE, "Note"), ("t.lem", TOPICS, "Topic")],
        );
        let note = page("a.lem", object(&[("url", Value::str("/t/node/"))]));
        assert_eq!(
            errors(&module, &placed(), vec![note, topics()]),
            [format!(
                "t.lem:{}: `t.lem` for `site.topics[1]` and `a.lem` are both at \
                 `/t/node/`\n  a.lem:{}: the first page at this url",
                at(TOPICS, "topic: site.topics"),
                at(NOTE, "Note")
            )]
        );
    }

    #[test]
    fn refuses_a_generator_a_glob_holds_with_one_diagnostic() {
        // Before its front matter is validated: a member's url is not derived for a
        // generator, and its absence is not a second thing to say.
        let site = format!("{SITE}\n<lem-let held: [Topic] from \"held/*.lem\" />");
        let text = "---\nfor:\n  topic: site.topics\n---\n";
        let module = compiled(&placed(), &site, &[("held/t.lem", text, "Topic")]);
        let member =
            generator("held/t.lem", object(&[]), key(text, "topic", "site.topics"));
        let data = Data {
            ambient: vec![build()],
            sources: vec![("held".to_owned(), Source::Many(vec![member]))],
            pages: Vec::new(),
        };
        assert_eq!(
            crate::load::tests::errors(&module, &placed(), data),
            [format!(
                "held/t.lem:{}: `for:` makes this page a generator, whose pages are made \
                 once every binding has loaded, so no `from` glob can hold it\n  \
                 site.lem:{}: the glob of `held` matches it",
                at(text, "topic: site.topics"),
                at(&site, "<lem-let «held»: [Topic]")
            )]
        );
    }

    #[test]
    fn refuses_a_document_carrying_a_pair_whether_a_glob_holds_it_or_not() {
        let site = format!("{SITE}\n<lem-let held: [Topic] from \"held/*.md\" />");
        let text = "---\ntype: Topic\nfor:\n  topic: site.topics\n---\nprose";
        let module = compiled(
            &placed(),
            &site,
            &[("held/t.md", text, "Topic"), ("t.md", text, "Topic")],
        );
        let pair = || key(text, "topic", "site.topics");
        let data = Data {
            ambient: vec![build()],
            sources: vec![(
                "held".to_owned(),
                Source::Many(vec![generator("held/t.md", object(&[]), pair())]),
            )],
            pages: vec![generator("t.md", object(&[]), pair())],
        };
        let refusal = |path: &str| {
            format!(
                "{path}:{}: a document is one page, of its own prose, so it takes no \
                 `for:` key: a generator is a page template",
                at(text, "topic: site.topics")
            )
        };
        assert_eq!(
            crate::load::tests::errors(&module, &placed(), data),
            [refusal("held/t.md"), refusal("t.md")]
        );
    }

    #[test]
    fn refuses_a_value_that_is_no_page_carrying_a_pair() {
        let site = format!("{SITE}\n<lem-let motto: string from env(\"MOTTO\") />");
        let module = compiled(&placed(), &site, &[]);
        let motto = Loaded {
            value: Value::str("m"),
            path: None,
            spans: Spans::new(),
            generator: Some(key(TOPICS, "topic", "site.topics")),
        };
        let data = Data {
            ambient: vec![build()],
            sources: vec![("motto".to_owned(), Source::One(motto))],
            pages: Vec::new(),
        };
        assert_eq!(
            load(&module, &placed(), data).unwrap_err(),
            "`motto` is no page, so it carries no `for:` key"
        );
    }

    #[test]
    fn refuses_a_generators_front_matter_that_is_no_mapping() {
        let module = compiled(&placed(), SITE, &[("t.lem", TOPICS, "Topic")]);
        let listed = generator(
            "t.lem",
            Value::list(vec![]),
            key(TOPICS, "topic", "site.topics"),
        );
        assert_eq!(
            errors(&module, &placed(), vec![listed]),
            [format!(
                "site.lem:{}: expected the front matter as a mapping of `Topic` fields, got \
                 `list`",
                at(SITE, "<lem-type «Topic» extends")
            )]
        );
    }

    /// The shell placing pages by `url` and filling a generated page's neighbours.
    fn neighboured() -> Host {
        placed().neighboured_by("prev", "next")
    }

    /// [`SITE`] with `Topic` declaring both neighbours, and a static page type that
    /// declares them too.
    fn paged() -> String {
        SITE.replace(
            "  topic: string\n",
            "  topic: string\n  prev: Topic?\n  next: Topic?\n",
        )
        .replace(
            "<lem-type Note extends Page>\n",
            "<lem-type Note extends Page>\n  prev: Note?\n  next: Note?\n",
        )
    }

    /// Where the page `field` of `page` names in the table, if it names one.
    fn names(page: &LoadedPage, name: &str) -> Option<u32> {
        match field(page, name) {
            Value::Neighbour(Some(position)) => Some(*position),
            Value::Null => None,
            other => panic!("a neighbour or null: {other:?}"),
        }
    }

    #[test]
    fn fills_each_pages_neighbours_from_its_own_generator_in_list_order() {
        let notes = "---\ntype: Topic\nfor:\n  topic: site.notes\n---\n";
        let site = format!("{}\n<lem-let notes: [string] = [\"a\", \"b\"] />", paged());
        let module = compiled(
            &neighboured(),
            &site,
            &[
                ("a.lem", NOTE, "Note"),
                ("t.lem", TOPICS, "Topic"),
                ("u.lem", notes, "Topic"),
            ],
        );
        let note = page("a.lem", object(&[("url", Value::str("/a/"))]));
        let more = generator("u.lem", object(&[]), key(notes, "topic", "site.notes"));
        let store = loaded(&module, &neighboured(), vec![note, topics(), more]);
        let sides: Vec<(&str, Option<u32>, Option<u32>)> = store
            .pages
            .iter()
            .map(|page| (page.path.as_str(), names(page, "prev"), names(page, "next")))
            .collect();
        assert_eq!(
            sides,
            [
                ("a.lem", None, None),
                ("t.lem", None, Some(2)),
                ("t.lem", Some(1), Some(3)),
                ("t.lem", Some(2), None),
                ("u.lem", None, Some(5)),
                ("u.lem", Some(4), None),
            ]
        );
    }

    #[test]
    fn fills_the_neighbours_a_page_type_inherits() {
        // The fields are found through the chain, so a generator typed by a subtype of
        // the type that declares them has them filled too.
        let site = format!(
            "{}\n<lem-type Sub extends Topic>\n  extra: string = \"x\"\n</lem-type>",
            paged()
        );
        let text = TOPICS.replace("type: Topic", "type: Sub");
        let module = compiled(&neighboured(), &site, &[("t.lem", &text, "Sub")]);
        let subs = generator("t.lem", object(&[]), key(&text, "topic", "site.topics"));
        let store = loaded(&module, &neighboured(), vec![subs]);
        let sides: Vec<(Option<u32>, Option<u32>)> = store
            .pages
            .iter()
            .map(|page| (names(page, "prev"), names(page, "next")))
            .collect();
        assert_eq!(
            sides,
            [(None, Some(1)), (Some(0), Some(2)), (Some(1), None)]
        );
    }

    #[test]
    fn gives_a_page_no_generator_made_no_neighbour_whether_a_glob_holds_it_or_not() {
        // The member is what `site` holds: a null, never a reference into a table
        // nothing that reads `site` is given.
        let site = format!("{}\n<lem-let held: [Note] from \"held/*.lem\" />", paged());
        let module = compiled(
            &neighboured(),
            &site,
            &[
                ("held/n.lem", "---\n---\nn", "Note"),
                ("a.lem", NOTE, "Note"),
            ],
        );
        let data = Data {
            ambient: vec![build()],
            sources: vec![(
                "held".to_owned(),
                Source::Many(vec![page(
                    "held/n.lem",
                    object(&[("url", Value::str("/n/"))]),
                )]),
            )],
            pages: vec![page("a.lem", object(&[("url", Value::str("/a/"))]))],
        };
        let store = load(&module, &neighboured(), data).unwrap().expect_output();
        assert_eq!(field(&store.pages[0], "prev"), &Value::Null);
        assert_eq!(field(&store.pages[1], "prev"), &Value::Null);
        let Value::Record(site) = &store.ambient[1].1 else {
            panic!("site");
        };
        let Some(Value::List(held)) = site.fields.get("held") else {
            panic!("a list");
        };
        assert_eq!(held[0], store.pages[0].record);
    }

    #[test]
    fn refuses_a_neighbour_the_front_matter_gives_once_whatever_it_gives() {
        // `null` would validate as the optional's, and a mapping would be refused as a
        // page nested in another page's front matter: neither is said, since the field
        // is not the front matter's to give at all (D43).
        let module = compiled(&neighboured(), &paged(), &[("a.lem", NOTE, "Note")]);
        let text = "---\ntype: Note\nprev: {url: /b/}\n---\nnote";
        for given in [Value::Null, object(&[("url", Value::str("/b/"))])] {
            let front = Loaded {
                spans: spans(text, &[("prev", at(text, "prev: {url: /b/}"))]),
                ..page(
                    "a.lem",
                    object(&[("url", Value::str("/a/")), ("prev", given)]),
                )
            };
            assert_eq!(
                errors(&module, &neighboured(), vec![front]),
                [format!(
                    "a.lem:{}: `prev` is a page's neighbour, which the loader fills, so \
                     the front matter cannot give it",
                    at(text, "prev: {url: /b/}")
                )]
            );
        }
    }

    #[test]
    fn reports_a_generators_front_matter_keys_together_and_each_once() {
        // The item and the url are refused by the generator and validated no further —
        // a list is no `string` and a blank is no `url`, which would say each key twice
        // — while the mapping's other mistakes are told beside them, not on a next run.
        let text = "---\ntype: Topic\ntopic: [git, node]\nurl:\nprev: null\ntitel: TILs\n\
                    for:\n  topic: site.topics\n---\n<lem.page.title>";
        let module = compiled(&neighboured(), &paged(), &[("t.lem", text, "Topic")]);
        let front = object(&[
            (
                "topic",
                Value::list(vec![Value::str("git"), Value::str("node")]),
            ),
            ("url", Value::Null),
            ("prev", Value::Null),
            ("titel", Value::str("TILs")),
        ]);
        let given = Loaded {
            spans: spans(
                text,
                &[
                    ("topic", at(text, "topic: [git, node]")),
                    ("url", at(text, "url:")),
                    ("prev", at(text, "prev: null")),
                    ("titel", at(text, "titel: TILs")),
                ],
            ),
            ..generator("t.lem", front, key(text, "topic", "site.topics"))
        };
        let label = format!(
            "\n  t.lem:{}: `for:` makes this page a generator here",
            at(text, "topic: site.topics")
        );
        assert_eq!(
            errors(&module, &neighboured(), vec![given]),
            [
                format!(
                    "t.lem:{}: `topic` is each page's item from `site.topics`, so the front \
                     matter cannot give it{label}",
                    at(text, "topic: [git, node]")
                ),
                format!(
                    "t.lem:{}: a generated page's `url` comes from `Topic`'s default, so the \
                     front matter cannot give it{label}",
                    at(text, "url:")
                ),
                format!(
                    "t.lem:{}: `Topic` has no field `titel`; did you mean `title`?",
                    at(text, "titel: TILs")
                ),
                format!(
                    "t.lem:{}: `prev` is a page's neighbour, which the loader fills, so the \
                     front matter cannot give it",
                    at(text, "prev: null")
                ),
            ]
        );
    }

    #[test]
    fn says_nothing_missing_of_a_url_it_refused_where_the_type_gives_it_no_default() {
        // The url is the generator's whether or not its type computes one, so the refusal
        // of the key is not followed by a "missing" for the field it took away.
        let site = SITE.replace("  url: url = url(\"/t/\" + self.topic + \"/\")\n", "");
        let text = "---\ntype: Topic\nurl: /tils/\nfor:\n  topic: site.topics\n---\n";
        let module = compiled(&placed(), &site, &[("t.lem", text, "Topic")]);
        let given = Loaded {
            spans: spans(text, &[("url", at(text, "url: /tils/"))]),
            ..generator(
                "t.lem",
                object(&[("url", Value::str("/tils/"))]),
                key(text, "topic", "site.topics"),
            )
        };
        assert_eq!(
            errors(&module, &placed(), vec![given]),
            [format!(
                "t.lem:{}: a generated page's `url` comes from `Topic`'s default, so the \
                 front matter cannot give it\n  t.lem:{}: `for:` makes this page a generator \
                 here",
                at(text, "url: /tils/"),
                at(text, "topic: site.topics")
            )]
        );
    }

    /// What the compiler and the link step say of a default's read of a neighbour.
    const OUT_OF_REACH: &str = "is out of reach of a default: the loader fills a page's \
                                neighbours once every page of its generator is built";

    #[test]
    fn refuses_a_read_the_compiler_could_not_see_in_the_compilers_words() {
        // Through a lambda's parameter, or off a record a default took of `self`, a read
        // is no read off `self` to the compiler, so the loader withholds the neighbours
        // while the defaults run: the read finds no page, never the null a neighbour
        // holds before it is filled, and is refused as a read the compiler saw is.
        for (default, read) in [
            (
                "[self].map(|p| p.prev?.topic ?? \"\").join(\"\")",
                "«p.prev»?.topic",
            ),
            ("self.me?.prev?.topic ?? \"\"", "«self.me?.prev»?.topic"),
        ] {
            let site = paged().replace(
                "  title: string = \"TILs: \" + self.topic\n",
                &format!("  me: Topic? = self\n  title: string = {default}\n"),
            );
            let module = compiled(&neighboured(), &site, &[("t.lem", TOPICS, "Topic")]);
            assert_eq!(
                errors(&module, &neighboured(), vec![topics()]),
                [format!(
                    "site.lem:{}: `prev` {OUT_OF_REACH}\n  t.lem:{}: computing `title` for \
                     `site.topics[0]`",
                    at(&site, read),
                    at(TOPICS, "topic: site.topics")
                )]
            );
        }
    }

    #[test]
    fn keeps_a_record_a_default_took_of_itself_without_its_neighbours() {
        // A default that hands `self` on hands the record as built so far: its
        // neighbours are withheld in it, and a read of one is refused at the read,
        // never the null a stale snapshot would otherwise hold.
        let site = paged().replace(
            "  topic: string\n",
            "  topic: string\n  me: Topic? = self\n",
        );
        let text = format!("{TOPICS}<lem.page.me?.prev?.topic ?? \"none\">");
        let module = compiled(&neighboured(), &site, &[("t.lem", &text, "Topic")]);
        let store = loaded(&module, &neighboured(), vec![topics()]);
        let host = neighboured();
        let second = store.page("t.lem", Some(1)).expect("the second page");
        let globals = store.globals(&host, "t.lem", Some(1));
        let error =
            crate::render_page(&module, second, &store.pages, None, &globals).unwrap_err();
        assert_eq!(error.message, format!("`prev` {OUT_OF_REACH}"));
        // The page itself has its neighbour.
        assert_eq!(field(second, "prev"), &Value::Neighbour(Some(0)));
    }

    #[test]
    fn loads_a_field_named_as_a_neighbour_on_a_record_no_page_has_as_any_other() {
        // The suppression half of R2 and the withholding: a record that is no page's —
        // a data binding's, or one nested in a page's front matter — keeps its `prev`.
        let site = format!(
            "{}\n<lem-type Revision>\n  prev: string?\n  text: string\n</lem-type>\n\
             <lem-let revision: Revision from \"_data/r.json\" />",
            paged().replace("  prev: Note?\n", "  prev: Note?\n  rev: Revision?\n")
        );
        let module = compiled(&neighboured(), &site, &[("a.lem", NOTE, "Note")]);
        let revision = || object(&[("prev", Value::str("b")), ("text", Value::str("t"))]);
        let data = Data {
            ambient: vec![build()],
            sources: vec![(
                "revision".to_owned(),
                Source::One(Loaded {
                    value: revision(),
                    path: None,
                    spans: Spans::new(),
                    generator: None,
                }),
            )],
            pages: vec![page(
                "a.lem",
                object(&[("url", Value::str("/a/")), ("rev", revision())]),
            )],
        };
        let store = load(&module, &neighboured(), data).unwrap().expect_output();
        let prev_of = |value: &Value| {
            let Value::Record(record) = value else {
                panic!("a record: {value:?}");
            };
            record.fields.get("prev").cloned()
        };
        let Value::Record(site) = &store.ambient[1].1 else {
            panic!("site");
        };
        assert_eq!(
            prev_of(site.fields.get("revision").expect("loaded")),
            Some(Value::str("b"))
        );
        assert_eq!(
            prev_of(field(&store.pages[0], "rev")),
            Some(Value::str("b"))
        );
        assert_eq!(field(&store.pages[0], "prev"), &Value::Null);
    }

    #[test]
    fn fills_only_the_neighbour_a_page_type_declares() {
        let site = paged().replace("  prev: Topic?\n", "");
        let text = format!("{TOPICS}|<lem.page.next?.topic ?? \"-\">");
        let module = compiled(&neighboured(), &site, &[("t.lem", &text, "Topic")]);
        let store = loaded(&module, &neighboured(), vec![topics()]);
        let Value::Record(first) = &store.pages[0].record else {
            panic!("a record");
        };
        assert_eq!(first.fields.get("prev"), None);
        assert_eq!(first.fields.get("next"), Some(&Value::Neighbour(Some(1))));
        let host = neighboured();
        let rendered = crate::render_page(
            &module,
            &store.pages[0],
            &store.pages,
            None,
            &store.globals(&host, "t.lem", Some(0)),
        )
        .expect("a render");
        assert_eq!(rendered.html, "TILs: git|node");
    }
}

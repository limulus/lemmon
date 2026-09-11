//! A loaded value against its declared type (D37, `docs/language.md` §Schemas).
//!
//! Every mismatch is one diagnostic at the value's position, after which that value is
//! given up — a record with three bad keys reports three things, not thirty — and a
//! string resolves by the type expected of it: an enum's variant, a project function
//! where a function type stands, the record a `by` index maps it to. A record is built
//! field by field and then its plan runs: a default for each field the data omitted, an
//! index for each string the data gave, in the order the link step settled.

use std::rc::Rc;

use crate::compile::{Diagnostic, omittable};
use crate::decls::{Decls, Kind, no_field};
use crate::plan::{Step, StepKind};
use crate::types::{Type, assignable};
use crate::value::{Closure, Map, Value};
use crate::vm::invoke;

use super::place::At;
use super::{Doc, Loader};

impl Loader<'_> {
    /// `value` as a `ty`, or `None` once every problem with it has been reported.
    /// `doc` is the document a page-backed record here is loaded from, if any.
    pub(super) fn value(
        &mut self,
        value: Value,
        ty: &Type,
        at: &At<'_>,
        doc: Option<&Doc<'_>>,
    ) -> Option<Value> {
        self.check(value, ty, ty, at, doc)
    }

    /// [`value`](Self::value), with `declared` the type a mismatch names: the `T?` a
    /// field was declared as, where `ty` is the `T` a non-null value is held to.
    fn check(
        &mut self,
        value: Value,
        ty: &Type,
        declared: &Type,
        at: &At<'_>,
        doc: Option<&Doc<'_>>,
    ) -> Option<Value> {
        let mismatch = |loader: &mut Self, value: &Value| {
            loader.diagnostics.push(at.error(format!(
                "expected `{declared}` for `{}`, got `{}`",
                at.label(),
                value.type_name()
            )));
            None
        };
        match (ty, value) {
            (Type::Any, value) => Some(value),
            (Type::Optional(_) | Type::Null, Value::Null) => Some(Value::Null),
            (Type::Optional(inner), value) => self.check(value, inner, declared, at, doc),
            (Type::String, value @ Value::Str(_))
            | (Type::Int, value @ Value::Int(_))
            | (Type::Float, value @ Value::Float(_))
            | (Type::Bool, value @ Value::Bool(_))
            | (Type::Html, value @ Value::Html(_)) => Some(value),
            // A whole number in data is an integer to the parser whatever the author
            // wrote — JSON has no `2.0` once a shell has serialised it — so a `float`
            // field takes one, as long as a float holds it exactly. The checker's rule
            // that an `int` is not a `float` stands: this is the loader reading data,
            // not the language widening a type.
            (Type::Float, Value::Int(int)) => {
                if int.unsigned_abs() > EXACT_IN_A_FLOAT {
                    self.diagnostics.push(at.error(format!(
                        "expected `float` for `{}`, got an integer a float cannot hold \
                         exactly: {int}",
                        at.label()
                    )));
                    return None;
                }
                #[allow(clippy::cast_precision_loss, reason = "bounded just above")]
                Some(Value::Float(int as f64))
            }
            // `raw()` is the only string that becomes `html` (invariant 7): a string in
            // data is text, wherever it came from.
            (Type::Html, Value::Str(_)) => {
                self.diagnostics.push(at.error(format!(
                    "expected `html` for `{}`, got a string: `html` is trusted markup, and \
                     `raw()` is the only string that becomes one",
                    at.label()
                )));
                None
            }
            // Not here yet (∆Q6L): these types have no values of their own, so a string
            // stays a string until the stdlib gives them one.
            (
                Type::Datetime | Type::Duration | Type::Url | Type::Asset,
                value @ Value::Str(_),
            ) => Some(value),
            (Type::List(element), Value::List(items)) => {
                let mut out = Vec::with_capacity(items.len());
                let mut failed = false;
                for (i, item) in items.iter().enumerate() {
                    match self.value(
                        item.clone(),
                        element,
                        &at.element(&i.to_string()),
                        doc,
                    ) {
                        Some(item) => out.push(item),
                        None => failed = true,
                    }
                }
                (!failed).then(|| Value::list(out))
            }
            (Type::Map(element), Value::Map(entries)) => {
                let mut out: Vec<(Rc<str>, Value)> = Vec::with_capacity(entries.len());
                let mut failed = false;
                for (key, item) in entries.iter() {
                    match self.value(item.clone(), element, &at.element(key), doc) {
                        Some(item) => out.push((key.clone(), item)),
                        None => failed = true,
                    }
                }
                (!failed).then(|| Value::map(Map::from_unique(out)))
            }
            (Type::Fn(..), Value::Str(name)) => self.function(&name, ty, at),
            (Type::Named(name), value) => match self.module.kind(name) {
                Some(Kind::Enum) => match value {
                    Value::Str(variant) => self.variant(variant, name, at),
                    other => mismatch(self, &other),
                },
                Some(Kind::Record) => match value {
                    Value::Map(given) => self.record(&given, name, at, doc),
                    Value::Str(_) => {
                        self.diagnostics.push(at.error(format!(
                            "expected `{declared}` for `{}`, got a string: only a field \
                             with a `by` index resolves a string to a record",
                            at.label()
                        )));
                        None
                    }
                    other => mismatch(self, &other),
                },
                None => unreachable!("a linked module declares `{name}`"),
            },
            (_, other) => mismatch(self, &other),
        }
    }

    /// The variant of the enum `declared` a string names.
    fn variant(&mut self, variant: Rc<str>, declared: &str, at: &At<'_>) -> Option<Value> {
        let variants = self
            .module
            .variants(declared)
            .expect("an enum has variants");
        if variants.iter().any(|(name, _)| *name == variant) {
            return Some(Value::Str(variant));
        }
        let names: Vec<String> = variants.iter().map(|(v, _)| format!("`{v}`")).collect();
        self.diagnostics.push(at.error(format!(
            "`{}` is a `{declared}` — {} — and \"{variant}\" is none of them",
            at.label(),
            listed(&names)
        )));
        None
    }

    /// The project function a string names where a function type is expected, checked
    /// to be of an assignable type: `layout: article`.
    fn function(&mut self, name: &str, ty: &Type, at: &At<'_>) -> Option<Value> {
        let Some(export) = self.module.export(name) else {
            self.diagnostics.push(at.error(format!(
                "`{}` names `{name}`, and no `lem-fn` has that name",
                at.label()
            )));
            return None;
        };
        if !assignable(&export.ty, ty, self.module) {
            self.diagnostics.push(at.error(format!(
                "`{}` names `{name}`, a `{}`, where a `{ty}` is expected",
                at.label(),
                export.ty
            )));
            return None;
        }
        Some(Value::Fn(Rc::new(Closure {
            func: export.func,
            captures: Vec::new(),
        })))
    }

    /// The record `name` built from `given`: every key a declared field, every present
    /// field validated as its type, every omission settled — an optional or `any` field
    /// with no default is `null`, a required one with no default is an error — and
    /// then the record's plan: a default for each field the data omitted, an index for
    /// each string it gave where a `by` field stands. A problem with this record's own
    /// data stops its plan, and a value that failed earlier — one a step may read —
    /// stops every plan after it: a function run over data that already failed could
    /// only fault after the real error.
    pub(super) fn record(
        &mut self,
        given: &Map,
        name: &Rc<str>,
        at: &At<'_>,
        doc: Option<&Doc<'_>>,
    ) -> Option<Value> {
        let declared = self.module.fields(name);
        let plan = self
            .module
            .records
            .iter()
            .find(|plan| plan.name == *name)
            .expect("the link step plans every record");
        let page_backed = self
            .document_type
            .as_ref()
            .is_some_and(|document| self.module.is_a(name, document));
        if page_backed && doc.is_none() {
            let document = self
                .document_type
                .as_ref()
                .expect("page-backed means typed");
            self.diagnostics.push(at.error(format!(
                "`{name}` extends `{document}`, so it is loaded from a document of its \
                 own, not from data"
            )));
            return None;
        }
        let has_step = |field: &Rc<str>, kind: StepKind| {
            plan.steps
                .iter()
                .any(|step| step.field == *field && step.kind == kind)
        };
        let mut failed = false;
        for (key, _) in given.iter() {
            if !declared.iter().any(|(field, _)| field == key) {
                let child = at.child(key, self.type_place(name));
                let message = no_field(name, key, self.module);
                self.diagnostics.push(child.error(message));
                failed = true;
            }
        }
        let mut fields: Vec<(Rc<str>, Value)> = Vec::new();
        for (field, ty) in &declared {
            let Some(value) = given.get(field) else {
                continue;
            };
            // A string where a `by` field stands is the plan's to resolve.
            if has_step(field, StepKind::Index) && matches!(value, Value::Str(_)) {
                continue;
            }
            let child = at.child(field, self.field_place(name, field));
            // A record inside this one is a value of its own: a document's path and
            // body reach the record the document is, and no further.
            match self.value(value.clone(), ty, &child, None) {
                Some(value) => fields.push((field.clone(), value)),
                None => failed = true,
            }
        }
        if failed {
            return None;
        }
        for (field, ty) in &declared {
            if given.get(field).is_some() || has_step(field, StepKind::Default) {
                continue;
            }
            if omittable(ty) {
                fields.push((field.clone(), Value::Null));
            } else {
                self.diagnostics.push(at.error(format!(
                    "missing `{field}`: `{name}` declares it with no default"
                )));
                failed = true;
            }
        }
        if failed || self.incomplete {
            return None;
        }
        for step in &plan.steps {
            match (step.kind, given.get(&step.field)) {
                (StepKind::Default, None) => {
                    let value = self.step(step, name, &fields, doc, at)?;
                    fields.push((step.field.clone(), value));
                }
                (StepKind::Index, Some(Value::Str(key))) => {
                    let index = self.step(step, name, &fields, doc, at)?;
                    let child = at.child(&step.field, self.field_place(name, &step.field));
                    let ty = declared
                        .iter()
                        .find(|(field, _)| *field == step.field)
                        .map(|(_, ty)| ty.strip_optional())
                        .expect("a step's field is declared");
                    let value = self.resolved(&index, key, ty, &child)?;
                    fields.push((step.field.clone(), value));
                }
                _ => {}
            }
        }
        Some(self.assembled(name, &fields))
    }

    /// Runs one step of a record's plan with the record so far as `self` — and, when
    /// the function was compiled to take them, its document's path and body — and
    /// reports a fault where it happened, with the record's position beside it.
    fn step(
        &mut self,
        step: &Step,
        record: &Rc<str>,
        fields: &[(Rc<str>, Value)],
        doc: Option<&Doc<'_>>,
        at: &At<'_>,
    ) -> Option<Value> {
        if let Some(value) = self.unloaded(&step.reads) {
            let file = &self.module.files[step.file as usize];
            self.diagnostics.push(Diagnostic::error(
                file,
                step.span,
                format!(
                    "`{}` reads `{value}`, which is not loaded yet: the host's own values \
                     load before the bindings files, so a `{record}` may read only the \
                     host's other values here",
                    step.field
                ),
            ));
            return None;
        }
        let mut args = vec![self.assembled(record, fields)];
        // What the function takes is what the compiler gave it: a default declared on
        // a page-backed record takes the document, one declared on a record above the
        // document type — which a page-backed subtype still inherits — does not.
        if self.module.functions[step.func as usize].params.len() > 1 {
            let doc = doc.expect("a step taking the document runs for a document's record");
            args.push(Value::str(doc.path));
            args.push(Value::Html(doc.body.clone()));
        }
        match invoke(self.module, step.func, args, &self.globals()) {
            Ok(value) => Some(value),
            Err(error) => {
                let place = at.place();
                self.diagnostics.push(
                    Diagnostic::error(error.path, error.span, error.message)
                        .with_placed_label(
                            place.path,
                            place.span,
                            place.range,
                            format!("computing `{}` for this `{record}`", step.field),
                        ),
                );
                None
            }
        }
    }

    /// The value the index a `by` step produced maps `key` to, checked to be the
    /// record `ty` the field is declared as.
    fn resolved(
        &mut self,
        index: &Value,
        key: &str,
        ty: &Type,
        at: &At<'_>,
    ) -> Option<Value> {
        let found = match index {
            Value::Map(entries) => entries.get(key).cloned(),
            // The compiler held the index to `{string: R}`; anything else came through
            // `any`, and has no entry for anything.
            _ => None,
        };
        let Some(found) = found else {
            self.diagnostics.push(at.error(format!(
                "`{}` names \"{key}\", which its index has no entry for",
                at.label()
            )));
            return None;
        };
        let is_record = match (&found, ty) {
            (Value::Record(record), Type::Named(name)) => {
                self.module.is_a(&record.ty, name)
            }
            _ => false,
        };
        if !is_record {
            let what = match &found {
                Value::Record(_) => found.type_name().into_owned(),
                other => format!("`{}`", other.type_name()),
            };
            self.diagnostics.push(at.error(format!(
                "`{}` names \"{key}\", which its index resolves to a {what} rather than a \
                 `{ty}`",
                at.label()
            )));
            return None;
        }
        Some(found)
    }
}

/// The largest integer a 64-bit float holds exactly.
const EXACT_IN_A_FLOAT: u64 = 1 << 53;

/// `names` as prose: "`a`", "`a` or `b`", "`a`, `b`, or `c`".
fn listed(names: &[String]) -> String {
    match names {
        [] => unreachable!("an enum has a variant"),
        [one] => one.clone(),
        [a, b] => format!("{a} or {b}"),
        [rest @ .., last] => format!("{}, or {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use lemmon_syntax::Span;

    use super::super::tests::{
        AMBIENT, build, document, errors, module, object, shell, spans,
    };
    use super::super::{Data, Loaded, Source, Spans, load};
    use super::*;
    use crate::testing::at;
    use crate::{Module, SourceFile, compile};

    const SITE: &str = "<lem-enum Placement = hero | left />\n\
         <lem-type Photo>\n  caption: string\n  placement: Placement = Placement.left\n</lem-type>\n\
         <lem-type Article extends Page>\n\
           title: string\n\
           subhead: string?\n\
           words: int\n\
           ratio: float\n\
           when: datetime\n\
           hero: Photo? by site.photos\n\
           tags: [string] = []\n\
           layout: (Article, html): html = article\n\
           slug: string = path\n\
           excerpt: html = body\n\
           teaser: string = tease(self)\n\
           heading: string = \"Re: \" + self.title\n\
         </lem-type>\n\
         <lem-fn article(a: Article, body: html)><lem.a.title><lem.body></lem-fn>\n\
         <lem-fn card(p: Photo): string = p.caption />\n\
         <lem-fn tease(a: Article): string = a.heading + \"!\" />\n\
         <lem-let photos: {string: Photo} from \"_data/photos.json\" />\n\
         <lem-let posts: [Article] from \"pages/*.md\" />";

    const TEXT: &str = "---\ntitle: A\n---\n\nprose\n";

    fn photos() -> (String, Source) {
        (
            "photos".to_owned(),
            Source::One(Loaded {
                value: object(&[("sunset", object(&[("caption", Value::str("Sunset"))]))]),
                path: Some("_data/photos.json".to_owned()),
                spans: Spans::new(),
            }),
        )
    }

    fn site_module() -> Module {
        let files = [
            SourceFile::new("ambient.lem", AMBIENT),
            SourceFile::new("site.lem", SITE),
            document("pages/a.md", TEXT, "Article"),
        ];
        compile(&files, &shell()).expect_output()
    }

    fn data(front: Value) -> Data {
        Data {
            ambient: vec![build()],
            sources: vec![
                photos(),
                (
                    "posts".to_owned(),
                    Source::Many(vec![Loaded {
                        value: front,
                        path: Some("pages/a.md".to_owned()),
                        spans: spans(TEXT, &[("", record()), ("title", record())]),
                    }]),
                ),
            ],
        }
    }

    fn complete() -> Vec<(&'static str, Value)> {
        vec![
            ("title", Value::str("A")),
            ("words", Value::Int(3)),
            ("ratio", Value::Int(2)),
            ("when", Value::str("2026-09-08")),
            ("url", Value::str("/a/")),
        ]
    }

    fn loaded_article(front: &[(&str, Value)]) -> Rc<crate::value::Record> {
        let store = load(&site_module(), &shell(), data(object(front)))
            .unwrap()
            .expect_output();
        let Value::Record(article) = &store.documents["pages/a.md"] else {
            panic!("a tagged record");
        };
        article.clone()
    }

    #[test]
    fn builds_a_record_with_every_declared_field_in_declaration_order() {
        let article = loaded_article(&complete());
        let fields: Vec<&str> = article.fields.iter().map(|(k, _)| &**k).collect();
        assert_eq!(
            fields,
            [
                "title", "subhead", "words", "ratio", "when", "hero", "tags", "layout",
                "slug", "excerpt", "teaser", "heading", "url", "draft"
            ]
        );
        assert_eq!(article.fields.get("subhead"), Some(&Value::Null));
        assert_eq!(article.fields.get("hero"), Some(&Value::Null));
        assert_eq!(article.fields.get("ratio"), Some(&Value::Float(2.0)));
        assert_eq!(article.fields.get("when"), Some(&Value::str("2026-09-08")));
        assert_eq!(article.fields.get("draft"), Some(&Value::Bool(false)));
    }

    #[test]
    fn runs_defaults_with_self_path_and_body() {
        let article = loaded_article(&complete());
        assert_eq!(article.fields.get("tags"), Some(&Value::list(vec![])));
        assert_eq!(article.fields.get("slug"), Some(&Value::str("pages/a.md")));
        assert_eq!(
            article.fields.get("excerpt"),
            Some(&Value::html("<p>prose</p>\n"))
        );
        assert_eq!(article.fields.get("heading"), Some(&Value::str("Re: A")));
        assert!(matches!(article.fields.get("layout"), Some(Value::Fn(_))));
        // `teaser` is declared before `heading` and reads it inside `tease`: the
        // link step followed the call, so the loader had `heading` first (∆HF0).
        assert_eq!(article.fields.get("teaser"), Some(&Value::str("Re: A!")));
    }

    #[test]
    fn faults_at_the_read_when_a_default_reaches_a_field_through_a_call_by_value() {
        // A field holding a function names no `lem-fn` the link step could follow, so
        // the loader runs `late` where it stands and the read of `heading` — not yet
        // computed — is the error, at the read (invariant 10).
        let site = "<lem-type Article extends Page>\n\
                      title: string\n\
                      describe: (Article): string = describe\n\
                      late: string = self.describe(self)\n\
                      heading: string = \"Re: \" + self.title\n\
                      layout: (Article, html): html = f\n\
                    </lem-type>\n\
                    <lem-fn describe(a: Article): string = a.heading />\n\
                    <lem-fn f(a: Article, b: html)><lem.a.title><lem.b></lem-fn>\n\
                    <lem-let posts: [Article] from \"pages/*.md\" />";
        let files = [
            SourceFile::new("ambient.lem", AMBIENT),
            SourceFile::new("site.lem", site),
            document("pages/a.md", TEXT, "Article"),
        ];
        let module = compile(&files, &shell()).expect_output();
        let data = Data {
            ambient: vec![build()],
            sources: vec![(
                "posts".to_owned(),
                Source::Many(vec![Loaded {
                    value: object(&[
                        ("title", Value::str("A")),
                        ("url", Value::str("/a/")),
                    ]),
                    path: Some("pages/a.md".to_owned()),
                    spans: spans(TEXT, &[("", record()), ("title", record())]),
                }]),
            )],
        };
        let result = load(&module, &shell(), data).unwrap();
        assert!(result.output.is_none());
        assert_eq!(
            result
                .diagnostics
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            [format!(
                "site.lem:{}: no field `heading` on `Article` record\n  \
                 pages/a.md:{}: computing `late` for this `Article`",
                at(site, "a.heading"),
                record()
            )]
        );
    }

    #[test]
    fn leaves_a_field_the_data_gave_alone_even_when_it_has_a_default() {
        let mut front = complete();
        front.push(("tags", Value::list(vec![Value::str("t")])));
        front.push(("heading", Value::str("mine")));
        let article = loaded_article(&front);
        assert_eq!(
            article.fields.get("tags"),
            Some(&Value::list(vec![Value::str("t")]))
        );
        assert_eq!(article.fields.get("heading"), Some(&Value::str("mine")));
    }

    #[test]
    fn resolves_a_string_through_the_fields_by_index() {
        let mut front = complete();
        front.push(("hero", Value::str("sunset")));
        let article = loaded_article(&front);
        let Some(Value::Record(photo)) = article.fields.get("hero") else {
            panic!("a resolved record");
        };
        assert_eq!(&*photo.ty, "Photo");
        assert_eq!(photo.fields.get("placement"), Some(&Value::str("left")));
    }

    #[test]
    fn takes_an_inline_record_where_a_by_field_stands() {
        let mut front = complete();
        front.push(("hero", object(&[("caption", Value::str("inline"))])));
        let article = loaded_article(&front);
        let Some(Value::Record(photo)) = article.fields.get("hero") else {
            panic!("a nested record");
        };
        assert_eq!(photo.fields.get("caption"), Some(&Value::str("inline")));
    }

    #[test]
    fn resolves_a_string_to_a_project_function_of_an_assignable_type() {
        let mut front = complete();
        front.push(("layout", Value::str("article")));
        let article = loaded_article(&front);
        let Some(Value::Fn(closure)) = article.fields.get("layout") else {
            panic!("a function");
        };
        assert_eq!(&*closure.captures, &[] as &[Value]);
    }

    /// Where the host places the record: `TEXT`'s one front-matter entry, which every
    /// diagnostic about a field of it points at.
    fn record() -> Span {
        at(TEXT, "title: A")
    }

    fn front_errors(front: &[(&str, Value)]) -> Vec<String> {
        errors(&site_module(), &shell(), data(object(front)))
    }

    #[test]
    fn reports_each_bad_field_once_at_its_own_position_and_stops_there() {
        let mut front = complete();
        front[0] = ("title", Value::Int(1));
        front.push(("byline", Value::str("x")));
        front.push(("words", Value::str("many")));
        assert_eq!(
            front_errors(&front),
            [
                format!("pages/a.md:{}: `Article` has no field `byline`", record()),
                format!(
                    "pages/a.md:{}: expected `string` for `title`, got `int`",
                    record()
                ),
                format!(
                    "pages/a.md:{}: expected `int` for `words`, got `string`",
                    record()
                ),
            ]
        );
    }

    #[test]
    fn reports_a_missing_required_field_at_the_record() {
        let front: Vec<(&str, Value)> = complete()
            .into_iter()
            .filter(|(name, _)| *name != "words")
            .collect();
        assert_eq!(
            front_errors(&front),
            [format!(
                "pages/a.md:{}: missing `words`: `Article` declares it with no default",
                record()
            )]
        );
    }

    #[test]
    fn reports_a_string_no_variant_and_a_string_no_index_can_resolve() {
        let mut front = complete();
        front.push((
            "hero",
            object(&[
                ("caption", Value::str("c")),
                ("placement", Value::str("top")),
            ]),
        ));
        assert_eq!(
            front_errors(&front),
            [format!(
                "pages/a.md:{}: `hero.placement` is a `Placement` — `hero` or `left` \
                 — and \"top\" is none of them",
                record()
            )]
        );
        let mut front = complete();
        front.push(("hero", Value::str("dawn")));
        assert_eq!(
            front_errors(&front),
            [format!(
                "pages/a.md:{}: `hero` names \"dawn\", which its index has no entry for",
                record()
            )]
        );
    }

    #[test]
    fn reports_a_function_name_that_is_unknown_or_of_the_wrong_type() {
        let mut front = complete();
        front.push(("layout", Value::str("articl")));
        assert_eq!(
            front_errors(&front),
            [format!(
                "pages/a.md:{}: `layout` names `articl`, and no `lem-fn` has that name",
                record()
            )]
        );
        let mut front = complete();
        front.push(("layout", Value::str("card")));
        assert_eq!(
            front_errors(&front),
            [format!(
                "pages/a.md:{}: `layout` names `card`, a `(Photo): string`, where a \
                 `(Article, html): html` is expected",
                record()
            )]
        );
        let mut front = complete();
        front.push(("layout", Value::Int(1)));
        assert_eq!(
            front_errors(&front),
            [format!(
                "pages/a.md:{}: expected `(Article, html): html` for `layout`, got `int`",
                record()
            )]
        );
    }

    #[test]
    fn reports_a_list_element_and_a_map_entry_by_their_paths() {
        let mut front = complete();
        front.push(("tags", Value::list(vec![Value::str("a"), Value::Int(2)])));
        assert_eq!(
            front_errors(&front),
            [format!(
                "pages/a.md:{}: expected `string` for `tags.1`, got `int`",
                record()
            )]
        );
        let site = "<lem-let counts: {string: int} from \"_data/counts.json\" />";
        let module = module(&shell(), &[("ambient.lem", AMBIENT), ("site.lem", site)]);
        let data = Data {
            ambient: vec![build()],
            sources: vec![(
                "counts".to_owned(),
                Source::One(Loaded {
                    value: object(&[("a", Value::Int(1)), ("b", Value::Bool(true))]),
                    path: None,
                    spans: Spans::new(),
                }),
            )],
        };
        assert_eq!(
            errors(&module, &shell(), data),
            [format!(
                "site.lem:{}: expected `int` for `counts.b`, got `bool`",
                at(site, "«counts»: {string:")
            )]
        );
    }

    #[test]
    fn reports_a_fault_in_a_default_where_it_happened_with_the_record_beside_it() {
        let site = "<lem-type Article extends Page>\n  tags: [string] = []\n  lead: string = self.tags[0]\n  layout: (Article, html): html\n</lem-type>\n\
                    <lem-fn f(a: Article, b: html)><lem.a.url><lem.b></lem-fn>\n\
                    <lem-let posts: [Article] from \"pages/*.md\" />";
        let files = [
            SourceFile::new("ambient.lem", AMBIENT),
            SourceFile::new("site.lem", site),
            document("pages/a.md", TEXT, "Article"),
        ];
        let module = compile(&files, &shell()).expect_output();
        let data = Data {
            ambient: vec![build()],
            sources: vec![(
                "posts".to_owned(),
                Source::Many(vec![Loaded {
                    value: object(&[
                        ("url", Value::str("/a/")),
                        ("layout", Value::str("f")),
                    ]),
                    path: Some("pages/a.md".to_owned()),
                    spans: spans(TEXT, &[("", record())]),
                }]),
            )],
        };
        let result = load(&module, &shell(), data).unwrap();
        assert_eq!(
            result.diagnostics[0].to_string(),
            format!(
                "site.lem:{}: index 0 is out of range for a list of 0\n  \
                 pages/a.md:{}: computing `lead` for this `Article`",
                at(site, "self.tags[0]"),
                record()
            )
        );
        assert!(result.diagnostics[0].labels[0].range.is_some());
    }

    #[test]
    fn refuses_a_page_backed_record_loaded_from_data() {
        let site = "<lem-type Article extends Page>\n  layout: (Article, html): html\n</lem-type>\n\
                    <lem-let featured: Article from \"_data/featured.json\" />";
        let module = module(&shell(), &[("ambient.lem", AMBIENT), ("site.lem", site)]);
        let data = Data {
            ambient: vec![build()],
            sources: vec![(
                "featured".to_owned(),
                Source::One(Loaded {
                    value: object(&[]),
                    path: None,
                    spans: Spans::new(),
                }),
            )],
        };
        assert_eq!(
            errors(&module, &shell(), data),
            [format!(
                "site.lem:{}: `Article` extends `Page`, so it is loaded from a document \
                 of its own, not from data",
                at(site, "«featured»: Article")
            )]
        );
    }

    #[test]
    fn reports_an_index_entry_that_is_no_record_of_the_fields_type() {
        // The index is typed `{string: Photo}` at compile; `any` is how a wrong entry
        // gets in.
        //
        // `TEXT`'s front matter has no `hero` key to point at, so the host's position
        // for it is any span inside the record's and distinct from it: what the test
        // asserts is that the diagnostic uses the entry's span, not the record's.
        let hero = Span::new(5, 9);
        let site = "<lem-type Photo>\n  caption: string\n</lem-type>\n\
                    <lem-type Article extends Page>\n  hero: Photo? by site.index\n  layout: (Article, html): html\n</lem-type>\n\
                    <lem-fn f(a: Article, b: html)><lem.a.url><lem.b></lem-fn>\n\
                    <lem-let index: any from \"_data/index.json\" />\n\
                    <lem-let posts: [Article] from \"pages/*.md\" />";
        let files = [
            SourceFile::new("ambient.lem", AMBIENT),
            SourceFile::new("site.lem", site),
            document("pages/a.md", TEXT, "Article"),
        ];
        let module = compile(&files, &shell()).expect_output();
        let data = Data {
            ambient: vec![build()],
            sources: vec![
                (
                    "index".to_owned(),
                    Source::One(Loaded {
                        value: object(&[("x", Value::Int(1))]),
                        path: None,
                        spans: Spans::new(),
                    }),
                ),
                (
                    "posts".to_owned(),
                    Source::Many(vec![Loaded {
                        value: object(&[
                            ("url", Value::str("/a/")),
                            ("layout", Value::str("f")),
                            ("hero", Value::str("x")),
                        ]),
                        path: Some("pages/a.md".to_owned()),
                        spans: spans(TEXT, &[("", record()), ("hero", hero)]),
                    }]),
                ),
            ],
        };
        assert_eq!(
            errors(&module, &shell(), data),
            [format!(
                "pages/a.md:{hero}: `hero` names \"x\", which its index resolves to \
                     a `int` rather than a `Photo`"
            )]
        );
    }

    #[test]
    fn validates_every_scalar_type_and_a_null_only_for_an_optional() {
        let site = "<lem-type Scalars>\n  s: string\n  i: int\n  f: float\n  b: bool\n  h: html\n  d: duration\n  u: url\n  a: asset\n  o: int?\n  n: int?\n</lem-type>\n\
                    <lem-let scalars: Scalars from \"_data/s.json\" />";
        let module = module(&shell(), &[("ambient.lem", AMBIENT), ("site.lem", site)]);
        let good = object(&[
            ("s", Value::str("s")),
            ("i", Value::Int(1)),
            ("f", Value::Float(1.5)),
            ("b", Value::Bool(true)),
            ("h", Value::Html(Rc::from("<b>"))),
            ("d", Value::str("PT1M")),
            ("u", Value::str("/")),
            ("a", Value::str("a.png")),
            ("o", Value::Null),
        ]);
        let with = |value: Value| Data {
            ambient: vec![build()],
            sources: vec![(
                "scalars".to_owned(),
                Source::One(Loaded {
                    value,
                    path: None,
                    spans: Spans::new(),
                }),
            )],
        };
        let store = load(&module, &shell(), with(good)).unwrap().expect_output();
        let Value::Record(loaded) = &store.ambient[1].1 else {
            panic!("site");
        };
        let Some(Value::Record(scalars)) = loaded.fields.get("scalars") else {
            panic!("scalars");
        };
        assert_eq!(scalars.fields.get("h"), Some(&Value::html("<b>")));
        assert_eq!(scalars.fields.get("n"), Some(&Value::Null));
        assert_eq!(scalars.fields.get("o"), Some(&Value::Null));
        let bad = object(&[
            ("s", Value::Null),
            ("i", Value::Float(1.5)),
            ("f", Value::str("1")),
            ("b", Value::Int(1)),
            ("h", Value::str("<b>")),
            ("d", Value::Int(1)),
            ("u", Value::Bool(false)),
            ("a", Value::Null),
            ("o", Value::str("1")),
        ]);
        assert_eq!(
            errors(&module, &shell(), with(bad)),
            [
                format!(
                    "site.lem:{}: expected `string` for `scalars.s`, got `null`",
                    at(site, "«s»: string")
                ),
                format!(
                    "site.lem:{}: expected `int` for `scalars.i`, got `float`",
                    at(site, "«i»: int\n")
                ),
                format!(
                    "site.lem:{}: expected `float` for `scalars.f`, got `string`",
                    at(site, "«f»: float")
                ),
                format!(
                    "site.lem:{}: expected `bool` for `scalars.b`, got `int`",
                    at(site, "«b»: bool")
                ),
                format!(
                    "site.lem:{}: expected `html` for `scalars.h`, got a string: `html` \
                     is trusted markup, and `raw()` is the only string that becomes one",
                    at(site, "«h»: html")
                ),
                format!(
                    "site.lem:{}: expected `duration` for `scalars.d`, got `int`",
                    at(site, "«d»: duration")
                ),
                format!(
                    "site.lem:{}: expected `url` for `scalars.u`, got `bool`",
                    at(site, "«u»: url")
                ),
                format!(
                    "site.lem:{}: expected `asset` for `scalars.a`, got `null`",
                    at(site, "«a»: asset")
                ),
                format!(
                    "site.lem:{}: expected `int?` for `scalars.o`, got `string`",
                    at(site, "«o»: int?")
                ),
            ]
        );
    }

    #[test]
    fn passes_anything_through_where_any_is_declared() {
        let site = "<lem-let blob: any from plugin(\"blob\") />";
        let module = module(&shell(), &[("ambient.lem", AMBIENT), ("site.lem", site)]);
        let value = object(&[("k", Value::list(vec![Value::Int(1)]))]);
        let data = Data {
            ambient: vec![build()],
            sources: vec![(
                "blob".to_owned(),
                Source::One(Loaded {
                    value: value.clone(),
                    path: None,
                    spans: Spans::new(),
                }),
            )],
        };
        let store = load(&module, &shell(), data).unwrap().expect_output();
        let Value::Record(site) = &store.ambient[1].1 else {
            panic!("site");
        };
        assert_eq!(site.fields.get("blob"), Some(&value));
    }

    #[test]
    fn reports_a_string_where_a_record_with_no_index_is_expected() {
        let site = "<lem-type Photo>\n  caption: string\n</lem-type>\n\
                    <lem-let photo: Photo from \"_data/p.json\" />";
        let module = module(&shell(), &[("ambient.lem", AMBIENT), ("site.lem", site)]);
        let data = Data {
            ambient: vec![build()],
            sources: vec![(
                "photo".to_owned(),
                Source::One(Loaded {
                    value: Value::str("sunset"),
                    path: None,
                    spans: Spans::new(),
                }),
            )],
        };
        assert_eq!(
            errors(&module, &shell(), data),
            [format!(
                "site.lem:{}: expected `Photo` for `photo`, got a string: only a field \
                 with a `by` index resolves a string to a record",
                at(site, "photo")
            )]
        );
    }

    #[test]
    fn runs_an_inherited_default_with_the_arguments_its_declaration_takes() {
        // `Base` stands above the document type, so its default was compiled without
        // the document; an `Article` inherits it and is loaded from one.
        let ambient = "<lem-type Base>\n  kind: string = \"note\"\n</lem-type>\n\
                       <lem-type Page extends Base>\n  url: string\n</lem-type>\n\
                       <lem-type Build>\n  generator: string\n</lem-type>";
        let site = "<lem-type Article extends Page>\n  slug: string = path\n  layout: (Article, html): html = f\n</lem-type>\n\
                    <lem-fn f(a: Article, b: html)><lem.a.url><lem.b></lem-fn>\n\
                    <lem-let posts: [Article] from \"pages/*.md\" />";
        let files = [
            SourceFile::new("ambient.lem", ambient),
            SourceFile::new("site.lem", site),
            document("pages/a.md", TEXT, "Article"),
        ];
        let module = compile(&files, &shell()).expect_output();
        let data = Data {
            ambient: vec![build()],
            sources: vec![(
                "posts".to_owned(),
                Source::Many(vec![Loaded {
                    value: object(&[("url", Value::str("/a/"))]),
                    path: Some("pages/a.md".to_owned()),
                    spans: Spans::new(),
                }]),
            )],
        };
        let store = load(&module, &shell(), data).unwrap().expect_output();
        let Value::Record(article) = &store.documents["pages/a.md"] else {
            panic!("a record");
        };
        assert_eq!(article.fields.get("kind"), Some(&Value::str("note")));
        assert_eq!(article.fields.get("slug"), Some(&Value::str("pages/a.md")));
    }

    #[test]
    fn refuses_a_page_backed_record_nested_in_a_documents_front_matter() {
        // A span inside the record's and distinct from it, standing for where the host
        // found the nested key — as in the index test above.
        let related = Span::new(6, 10);
        let site = "<lem-type Article extends Page>\n  slug: string = path\n  related: Article?\n  layout: (Article, html): html = f\n</lem-type>\n\
                    <lem-fn f(a: Article, b: html)><lem.a.url><lem.b></lem-fn>\n\
                    <lem-let posts: [Article] from \"pages/*.md\" />";
        let files = [
            SourceFile::new("ambient.lem", AMBIENT),
            SourceFile::new("site.lem", site),
            document("pages/a.md", TEXT, "Article"),
        ];
        let module = compile(&files, &shell()).expect_output();
        let data = Data {
            ambient: vec![build()],
            sources: vec![(
                "posts".to_owned(),
                Source::Many(vec![Loaded {
                    value: object(&[
                        ("url", Value::str("/a/")),
                        ("related", object(&[("url", Value::str("/r/"))])),
                    ]),
                    path: Some("pages/a.md".to_owned()),
                    spans: spans(TEXT, &[("", record()), ("related", related)]),
                }]),
            )],
        };
        assert_eq!(
            errors(&module, &shell(), data),
            [format!(
                "pages/a.md:{related}: `Article` extends `Page`, so it is loaded \
                     from a document of its own, not from data"
            )]
        );
    }

    #[test]
    fn gives_a_default_the_null_of_an_optional_sibling_the_data_omitted() {
        let site = "<lem-type Article extends Page>\n  subhead: string?\n  blurb: string = self.subhead ?? \"none\"\n  layout: (Article, html): html = f\n</lem-type>\n\
                    <lem-fn f(a: Article, b: html)><lem.a.url><lem.b></lem-fn>\n\
                    <lem-let posts: [Article] from \"pages/*.md\" />";
        let files = [
            SourceFile::new("ambient.lem", AMBIENT),
            SourceFile::new("site.lem", site),
            document("pages/a.md", TEXT, "Article"),
        ];
        let module = compile(&files, &shell()).expect_output();
        let data = Data {
            ambient: vec![build()],
            sources: vec![(
                "posts".to_owned(),
                Source::Many(vec![Loaded {
                    value: object(&[("url", Value::str("/a/"))]),
                    path: Some("pages/a.md".to_owned()),
                    spans: Spans::new(),
                }]),
            )],
        };
        let store = load(&module, &shell(), data).unwrap().expect_output();
        let Value::Record(article) = &store.documents["pages/a.md"] else {
            panic!("a record");
        };
        assert_eq!(article.fields.get("blurb"), Some(&Value::str("none")));
    }

    #[test]
    fn reports_a_missing_required_field_before_a_default_can_read_it() {
        // `title` reads `topic`; the author omitted `topic`, and that is the error to
        // show — not the fault the default would raise reading it.
        let site = "<lem-type Til extends Page>\n  topic: string\n  title: string = \"TIL: \" + self.topic\n  layout: (Til, html): html = f\n</lem-type>\n\
                    <lem-fn f(t: Til, b: html)><lem.t.url><lem.b></lem-fn>\n\
                    <lem-let tils: [Til] from \"tils/*.md\" />\n\
                    <lem-let first: Til? = tils.first() />";
        let files = [
            SourceFile::new("ambient.lem", AMBIENT),
            SourceFile::new("site.lem", site),
            document("tils/a.md", TEXT, "Til"),
        ];
        let module = compile(&files, &shell()).expect_output();
        let data = Data {
            ambient: vec![build()],
            sources: vec![(
                "tils".to_owned(),
                Source::Many(vec![Loaded {
                    value: object(&[("url", Value::str("/a/"))]),
                    path: Some("tils/a.md".to_owned()),
                    spans: spans(TEXT, &[("", record())]),
                }]),
            )],
        };
        // One diagnostic: the computed binding that reads `tils` is not run over a
        // value that failed, so it cannot add a fault of its own.
        assert_eq!(
            errors(&module, &shell(), data),
            [format!(
                "tils/a.md:{}: missing `topic`: `Til` declares it with no default",
                record()
            )]
        );
    }

    #[test]
    fn fills_an_omitted_field_with_null_exactly_when_its_type_admits_null() {
        // The rule a record literal follows too: `T?` may be left out, and so may
        // `any`, which admits `null` and is what `any?` folds to; `int` may not.
        let site = "<lem-type Blob>\n  count: int\n  data: any\n  more: any?\n</lem-type>\n\
                    <lem-let blob: Blob from \"_data/blob.json\" />";
        let module = module(&shell(), &[("ambient.lem", AMBIENT), ("site.lem", site)]);
        let data = |value: Value| Data {
            ambient: vec![build()],
            sources: vec![(
                "blob".to_owned(),
                Source::One(Loaded {
                    value,
                    path: None,
                    spans: Spans::new(),
                }),
            )],
        };
        assert_eq!(
            errors(&module, &shell(), data(object(&[]))),
            [format!(
                "site.lem:{}: missing `count`: `Blob` declares it with no default",
                at(site, "«blob»: Blob")
            )]
        );
        let store = load(&module, &shell(), data(object(&[("count", Value::Int(1))])))
            .unwrap()
            .expect_output();
        let Value::Record(site) = &store.ambient[1].1 else {
            panic!("site");
        };
        let Some(Value::Record(blob)) = site.fields.get("blob") else {
            panic!("blob");
        };
        assert_eq!(blob.fields.get("data"), Some(&Value::Null));
        assert_eq!(blob.fields.get("more"), Some(&Value::Null));
    }

    #[test]
    fn takes_a_whole_number_for_a_float_only_while_a_float_holds_it_exactly() {
        let site = "<lem-type Stat>\n  ratio: float\n</lem-type>\n\
                    <lem-let stat: Stat from \"_data/stat.json\" />";
        let module = module(&shell(), &[("ambient.lem", AMBIENT), ("site.lem", site)]);
        let data = |ratio: i64| Data {
            ambient: vec![build()],
            sources: vec![(
                "stat".to_owned(),
                Source::One(Loaded {
                    value: object(&[("ratio", Value::Int(ratio))]),
                    path: None,
                    spans: Spans::new(),
                }),
            )],
        };
        assert!(
            load(&module, &shell(), data(1 << 53))
                .unwrap()
                .output
                .is_some()
        );
        assert_eq!(
            errors(&module, &shell(), data((1 << 53) + 1)),
            [format!(
                "site.lem:{}: expected `float` for `stat.ratio`, got an integer a float \
                 cannot hold exactly: 9007199254740993",
                at(site, "«ratio»: float")
            )]
        );
    }

    #[test]
    fn reports_a_default_that_reads_a_value_not_loaded_yet_before_running_it() {
        // The host's own values load first, so a `Build` default cannot read `site`.
        let ambient = "<lem-type Page>\n  url: string\n</lem-type>\n\
                       <lem-type Build>\n  generator: string\n  banner: string = site.name\n</lem-type>";
        let site = "<lem-let name: string = \"limulus.net\" />";
        let module = module(&shell(), &[("ambient.lem", ambient), ("site.lem", site)]);
        let data = Data {
            ambient: vec![build()],
            sources: Vec::new(),
        };
        assert_eq!(
            errors(&module, &shell(), data),
            [format!(
                "ambient.lem:{}: `banner` reads `site`, which is not loaded yet: the \
                 host's own values load before the bindings files, so a `Build` may \
                 read only the host's other values here",
                at(ambient, "banner")
            )]
        );
    }

    #[test]
    fn reports_a_default_that_reads_a_value_not_loaded_yet_through_a_call() {
        // The same rule, with the read inside `motto()`: the link step closed it into
        // the step's reads, so the loader refuses the step before running it (∆HF0).
        let ambient = "<lem-type Page>\n  url: string\n</lem-type>\n\
                       <lem-type Build>\n  generator: string\n  banner: string = motto()\n</lem-type>";
        let site = "<lem-let name: string = \"limulus.net\" />\n\
                    <lem-fn motto(): string = site.name />";
        let module = module(&shell(), &[("ambient.lem", ambient), ("site.lem", site)]);
        let data = Data {
            ambient: vec![build()],
            sources: Vec::new(),
        };
        assert_eq!(
            errors(&module, &shell(), data),
            [format!(
                "ambient.lem:{}: `banner` reads `site`, which is not loaded yet: the \
                 host's own values load before the bindings files, so a `Build` may \
                 read only the host's other values here",
                at(ambient, "banner")
            )]
        );
    }
}

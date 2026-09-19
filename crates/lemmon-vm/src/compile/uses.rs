//! What a function reads and calls, recorded as it is lowered, so the link step can
//! order what the loader runs (D37, ∆HF0).
//!
//! A collector is open while a `lem-fn` or a function the loader itself runs is being
//! lowered, and stays open through every lambda inside it: a lambda's reads are its
//! enclosing function's. What goes in is what decides order — a field read off the
//! record being built, off a parameter, or off an ambient value, an ambient value read
//! whole, and every call to a project function with the root handed each parameter. A
//! read through anything else, a local bound to a parameter among them, is left out;
//! the loader reports such a read where it happens (invariant 10).

use std::rc::Rc;

use lemmon_syntax::Span;

use super::{Compiler, Runs, index_u32, narrowing};
use crate::unit::{Access, Call, Root, Uses};

/// The binders of the implicit names a function the loader may run takes: the `self`
/// of a default, and the `body` of one on a page-backed record. Matched by binder and
/// not by name, since a lambda parameter may be called `body` too.
#[derive(Clone, Copy, Default)]
pub(super) struct Implicit {
    pub(super) self_binder: Option<Span>,
    pub(super) body_binder: Option<Span>,
}

/// The reads and calls of a function the loader may run, being collected.
pub(super) struct Collecting {
    /// What the loader runs this function as, when it runs it itself — which decides
    /// the ambient values it may read ([`refused_at_load`](super::refused_at_load));
    /// `None` for a `lem-fn`, which runs at render too and may read them all.
    pub(super) runs: Option<Runs>,
    /// The `self` and `body` of a default, by binder — what tells a read of the
    /// record being built, or of the page's body, from any other.
    implicit: Implicit,
    /// Where `body` was first read, when it was.
    pub(super) body_read: Option<Span>,
    /// A `lem-fn`'s parameters, by binder, in declaration order.
    params: Vec<Span>,
    pub(super) uses: Uses,
}

impl Collecting {
    /// The root the local `binder` is, if it is one.
    fn root(&self, binder: Span) -> Option<Root> {
        if self.implicit.self_binder == Some(binder) {
            return Some(Root::Own);
        }
        self.params
            .iter()
            .position(|param| *param == binder)
            .map(|index| Root::Param(index_u32(index)))
    }
}

impl Compiler<'_> {
    /// Runs `lower` with a collector open for the function it lowers — `runs` set when
    /// the loader itself runs that function — and returns what was collected. Nested
    /// under an outer collector, which is restored after: a lambda is its enclosing
    /// function's, so nothing opens one for it.
    pub(super) fn collecting(
        &mut self,
        runs: Option<Runs>,
        implicit: Implicit,
        params: Vec<Span>,
        lower: impl FnOnce(&mut Self),
    ) -> Collecting {
        let outer = self.uses.replace(Collecting {
            runs,
            implicit,
            body_read: None,
            params,
            uses: Uses::default(),
        });
        lower(self);
        let collected = self.uses.take().expect("the collector opened above");
        self.uses = outer;
        collected
    }

    /// Records that the local `binder` was read at `span`, when it is the `body` of
    /// the default being compiled and this is its first read.
    pub(super) fn note_local_read(&mut self, binder: Span, span: Span) {
        if let Some(collecting) = &mut self.uses
            && collecting.implicit.body_binder == Some(binder)
            && collecting.body_read.is_none()
        {
            collecting.body_read = Some(span);
        }
    }

    /// Records `read` as one the function being compiled makes, once.
    pub(super) fn note_read(&mut self, read: Access) {
        if let Some(collecting) = &mut self.uses
            && !collecting.uses.reads.contains(&read)
        {
            collecting.uses.reads.push(read);
        }
    }

    /// Records the read `object.field` makes, when a function the loader may run is
    /// being compiled and the object is one whose fields decide order: a root, or an
    /// ambient value, by name.
    pub(super) fn note_member_read(
        &mut self,
        object: &lemmon_syntax::ast::Expr,
        field: &str,
    ) {
        let Some(collecting) = &self.uses else {
            return;
        };
        let read = match self.bare_root(object) {
            Some(narrowing::PathRoot::Ambient(value)) => Access::Ambient {
                value,
                field: Rc::from(field),
            },
            Some(narrowing::PathRoot::Local(binder)) => match collecting.root(binder) {
                Some(Root::Own) => Access::Own(Rc::from(field)),
                Some(Root::Param(index)) => Access::Param {
                    index,
                    field: Rc::from(field),
                },
                Some(Root::Ambient(_)) | None => return,
            },
            None => return,
        };
        self.note_read(read);
    }

    /// Records a naming of the project function `name`, when a function the loader
    /// may run is being compiled, once per distinct `args` — what the call handed
    /// each parameter, in the callee's declaration order.
    pub(super) fn note_call(&mut self, name: &str, args: Vec<Option<Root>>) {
        if let Some(collecting) = &mut self.uses {
            let call = Call {
                name: Rc::from(name),
                args,
            };
            if !collecting.uses.calls.contains(&call) {
                collecting.uses.calls.push(call);
            }
        }
    }

    /// The root `expr` is, when it is one written bare — `self`, a parameter, or an
    /// ambient value — and the function being compiled is one the loader may run.
    pub(super) fn root_of(&self, expr: &lemmon_syntax::ast::Expr) -> Option<Root> {
        let collecting = self.uses.as_ref()?;
        match self.bare_root(expr)? {
            narrowing::PathRoot::Local(binder) => collecting.root(binder),
            narrowing::PathRoot::Ambient(value) => Some(Root::Ambient(value)),
        }
    }
}

#[cfg(test)]
mod tests {
    use lemmon_syntax::spans::at;

    use super::*;
    use crate::SourceFile;
    use crate::testing::{AMBIENT, shell_host, shown_with, unit_with, untyped_host};
    use crate::unit::Unit;

    #[test]
    fn records_the_fields_a_default_reads_and_the_calls_it_makes() {
        let unit = unit_with(
            &untyped_host(),
            "types.lem",
            &[SourceFile::new(
                "types.lem",
                "<lem-type A>\n  first: string\n  words: [string]\n  \
                 full: string = self.first + words(self)\n  \
                 tags: [string] = self.words.filter(|w| w != self.first + join(self, w))\n\
                 </lem-type>\n\
                 <lem-fn words(a: A): string = a.first />\n\
                 <lem-fn join(a: A, w: string): string = w />",
            )],
        );
        let rows: Vec<(String, &Uses)> = unit
            .defaults
            .iter()
            .map(|d| (format!("{}.{}", d.record, d.field), &unit.uses[&d.func]))
            .collect();
        let own = |f: &str| Access::Own(Rc::from(f));
        let call = |name: &str, args: &[Option<Root>]| Call {
            name: Rc::from(name),
            args: args.to_vec(),
        };
        assert_eq!(
            rows,
            [
                (
                    "A.full".to_owned(),
                    // `words(self)` hands the record over whole: a call, not a read.
                    &Uses {
                        reads: vec![own("first")],
                        calls: vec![call("words", &[Some(Root::Own)])],
                    }
                ),
                // Read inside a lambda, and through `self` captured into it — and
                // called there too, `w` being the lambda's own and no root.
                (
                    "A.tags".to_owned(),
                    &Uses {
                        reads: vec![own("words"), own("first")],
                        calls: vec![call("join", &[Some(Root::Own), None])],
                    }
                ),
            ]
        );
        // `words(self)` passes the record to a function: no field read, no fact.
        assert_eq!(unit.facts.len(), 0);
    }

    /// Functions whose uses the tests below read: what the link step closes a
    /// default's calls through (∆HF0).
    const FNS: &str = "<lem-type Photo>\n  alt: string\n</lem-type>\n\
                       <lem-type A>\n  first: string\n  hero: Photo?\n  words: [string]\n\
                       </lem-type>\n\
                       <lem-fn tail(a: A, sep: string): string = sep + a.first />\n\
                       <lem-fn describe(p: Photo?): string = p?.alt ?? \"\" />\n\
                       <lem-fn tally(s: any): string = s.name />\n\
                       <lem-fn full(a: A, sep: string): string = \
                       a.first + tail(sep: sep, a: a) + site.name + describe(a.hero) + \
                       a.words.map(|w| tail(a, w))[0] + tally(site) />";

    /// The uses of the `lem-fn` `name` in `unit`.
    fn uses_of<'a>(unit: &'a Unit, name: &str) -> &'a Uses {
        let export = unit.exports.iter().find(|e| &*e.name == name).unwrap();
        &unit.uses[&export.func]
    }

    fn param(index: u32, field: &str) -> Access {
        Access::Param {
            index,
            field: Rc::from(field),
        }
    }

    #[test]
    fn records_the_fields_a_function_reads_off_its_parameters_and_ambient_values() {
        let unit = unit_with(
            &untyped_host(),
            "fns.lem",
            &[SourceFile::new("fns.lem", FNS)],
        );
        assert_eq!(
            uses_of(&unit, "full").reads,
            [
                param(0, "first"),
                Access::Ambient {
                    value: Rc::from("site"),
                    field: Rc::from("name"),
                },
                // `describe(a.hero)` reads `hero` and hands a field over, not a root:
                // `describe`'s reads off its parameter are the photo's.
                param(0, "hero"),
                param(0, "words"),
                Access::Whole(Rc::from("site")),
            ]
        );
        assert_eq!(uses_of(&unit, "tail").reads, [param(0, "first")]);
        assert_eq!(uses_of(&unit, "tally").reads, [param(0, "name")]);
    }

    #[test]
    fn records_the_calls_a_function_makes_with_the_root_handed_each_parameter() {
        // In the callee's parameter order, whatever order the arguments were written
        // in; a root is `self`, a parameter, or an ambient value, handed whole.
        let unit = unit_with(
            &untyped_host(),
            "fns.lem",
            &[SourceFile::new("fns.lem", FNS)],
        );
        assert_eq!(
            uses_of(&unit, "full").calls,
            [
                Call {
                    name: Rc::from("tail"),
                    args: vec![Some(Root::Param(0)), Some(Root::Param(1))],
                },
                Call {
                    name: Rc::from("describe"),
                    args: vec![None],
                },
                // Inside the lambda, `a` is still the function's parameter and `w`
                // is the lambda's own.
                Call {
                    name: Rc::from("tail"),
                    args: vec![Some(Root::Param(0)), None],
                },
                Call {
                    name: Rc::from("tally"),
                    args: vec![Some(Root::Ambient(Rc::from("site")))],
                },
            ]
        );
    }

    #[test]
    fn records_a_repeated_call_once() {
        let unit = unit_with(
            &untyped_host(),
            "fns.lem",
            &[SourceFile::new(
                "fns.lem",
                "<lem-fn twice(s: string): string = once(s) + once(s) + once(\"\") />\n\
                 <lem-fn once(s: string): string = s />",
            )],
        );
        assert_eq!(
            uses_of(&unit, "twice").calls,
            [
                Call {
                    name: Rc::from("once"),
                    args: vec![Some(Root::Param(0))],
                },
                Call {
                    name: Rc::from("once"),
                    args: vec![None],
                },
            ]
        );
    }

    #[test]
    fn records_no_call_for_a_function_named_as_a_value() {
        // `layout = card` stores `card` for the render to call, and `map(loud)` hands
        // `loud` to a builtin: neither is a call written here, so neither is followed
        // — what `card` reads of `page` is the render's business, and what `loud`
        // reads at load is an error at the read, never a guess (invariant 10).
        let unit = unit_with(
            &untyped_host(),
            "types.lem",
            &[SourceFile::new(
                "types.lem",
                "<lem-type A>\n  words: [string]\n  \
                 shouted: [string] = self.words.map(loud)\n  \
                 layout: (A, html): html = card\n</lem-type>\n\
                 <lem-fn loud(w: string): string = w + site.name />\n\
                 <lem-fn card(a: A, body: html)><lem.page.url><lem.body></lem-fn>",
            )],
        );
        for row in &unit.defaults {
            assert!(unit.uses[&row.func].calls.is_empty(), "{}", row.field);
        }
    }

    #[test]
    fn records_a_block_functions_reads_but_not_those_through_a_local() {
        // A block body's reads count like a value's; a local bound to a parameter is
        // not a root, so a field read through it is not counted — the loader's error
        // at the read is what an author who aliases `self` or a parameter gets.
        let unit = unit_with(
            &untyped_host(),
            "fns.lem",
            &[SourceFile::new(
                "fns.lem",
                "<lem-type A>\n  first: string\n  last: string\n</lem-type>\n\
                 <lem-fn card(a: A)><lem-let b = a /><lem.a.first><lem.b.last></lem-fn>",
            )],
        );
        assert_eq!(uses_of(&unit, "card").reads, [param(0, "first")]);
    }

    #[test]
    fn lets_a_function_read_the_document_value_and_records_it() {
        // A `lem-fn` runs at render, where `page` is bound; the read is recorded, a
        // field or the value whole, so the link step can refuse a default that
        // reaches it through the call (∆HF0).
        const FNS: &str = "<lem-fn where(): url = page.url />\n\
                           <lem-fn here(): Page = page />";
        let files = [
            SourceFile::new("ambient.lem", AMBIENT),
            SourceFile::new("fns.lem", FNS),
        ];
        assert_eq!(shown_with(&shell_host(), &files), Vec::<String>::new());
        let unit = unit_with(&shell_host(), "fns.lem", &files);
        assert_eq!(
            uses_of(&unit, "where").reads,
            [Access::Ambient {
                value: Rc::from("page"),
                field: Rc::from("url")
            }]
        );
        assert_eq!(
            uses_of(&unit, "here").reads,
            [Access::Whole(Rc::from("page"))]
        );
    }

    #[test]
    fn records_where_a_default_first_reads_body() {
        // The first read, by binder: a lambda parameter called `body` is a local of
        // its own, and a default reading only `path` or `self` reads no body.
        const SCHEMA: &str = "<lem-type Post extends Page>\n  \
             slug: string = path\n  \
             excerpt: html = body\n  \
             twice: int = body.readingTime().seconds() + body.readingTime().seconds()\n  \
             ids: [int] = [1].map(|body| body)\n  \
             layout: (Post, html): html\n</lem-type>";
        let files = [
            SourceFile::new("ambient.lem", AMBIENT),
            SourceFile::new("schema.lem", SCHEMA),
        ];
        assert_eq!(shown_with(&shell_host(), &files), Vec::<String>::new());
        let unit = unit_with(&shell_host(), "schema.lem", &files);
        let reads: Vec<(&str, Option<Span>)> = unit
            .defaults
            .iter()
            .map(|row| (&*row.field, row.reads_body))
            .collect();
        assert_eq!(
            reads,
            [
                ("slug", None),
                ("excerpt", Some(at(SCHEMA, "excerpt: html = «body»"))),
                ("twice", Some(at(SCHEMA, "= «body».readingTime"))),
                ("ids", None),
            ]
        );
    }

    #[test]
    fn records_a_field_read_by_calling_it() {
        // `self.fmt(…)` reads the field before calling what it holds, and the loader
        // must have it computed first like any other read.
        let unit = unit_with(
            &untyped_host(),
            "types.lem",
            &[SourceFile::new(
                "types.lem",
                "<lem-type A>\n  fmt: (string): string\n  \
                 full: string = self.fmt(\"x\")\n</lem-type>",
            )],
        );
        let uses = &unit.uses[&unit.defaults[0].func];
        assert_eq!(uses.reads, [Access::Own(Rc::from("fmt"))]);
        // A call through a value names no function the link step could follow.
        assert!(uses.calls.is_empty());
    }
}

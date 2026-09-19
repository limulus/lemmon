//! The loader's plans, built at link (D37, ∆HF0).
//!
//! [`plans`] turns the functions each unit compiled for the loader into the module's
//! tables: every declared record's defaults and `by` indexes, each taken from the
//! nearest declaration in the record's chain that gives one, and every bindings file's
//! bindings. Each table is ordered so that nothing runs before what it needs — what a
//! step reads, the calls it makes followed, and what the records a loaded binding holds
//! look a string up in while they load — and the loader then runs it front to back.
//!
//! What no order would satisfy is refused here, and the link fails with it: a field or
//! binding that needs itself, reported at the first one on the cycle. Two rules about
//! what a loader-run function may read are enforced here as well, because only the link
//! step has a function's reads followed through the calls it makes: a default or a
//! computed binding that reaches an ambient value it may not read (∆eag), and a page
//! template whose record's plan reads `body` (D43).

use std::collections::BTreeMap;
use std::rc::Rc;

use lemmon_syntax::Span;

use crate::compile::{Runs, index_u32, refused_at_load};
use crate::decls::{DeclKind, Decls, Shape};
use crate::diagnostics::Diagnostic;
use crate::exports::ExportIndex;
use crate::host::Host;
use crate::plan::{BindingStep, Bindings, Fulfilled, Index, RecordPlan, Step};
use crate::types::Type;
use crate::unit::{Access, Supplied, Unit};

use super::add_u32;
use super::closure::{self, Closed, ReadClosure};

/// Every declared record's plan and every bindings file's, with what refused them
/// pushed onto `diagnostics`. This module's one way in: [`Places`], and the order the
/// three builders run in — which is the order their diagnostics come out — stay this
/// module's business.
pub(super) fn plans(
    units: &[Unit],
    index: &ExportIndex,
    host: &Host,
    positions: &BTreeMap<&str, usize>,
    function_offsets: &[u32],
    diagnostics: &mut Vec<Diagnostic>,
) -> (Vec<RecordPlan>, Vec<Bindings>) {
    let closure = ReadClosure::new(units, index, positions);
    let places = Places {
        units,
        index,
        host,
        positions,
        function_offsets,
        closure: &closure,
    };
    let records = places.record_plans(diagnostics);
    let bindings = places.bindings_plans(&records, diagnostics);
    diagnostics.extend(places.refuse_page_templates(&records));
    (records, bindings)
}

/// Where each unit's functions land in the module — what turns a unit's loader-run
/// functions into the module's plans.
struct Places<'a> {
    units: &'a [Unit],
    index: &'a ExportIndex,
    host: &'a Host,
    positions: &'a BTreeMap<&'a str, usize>,
    function_offsets: &'a [u32],
    /// Every project function's reads closed through its calls, for the steps' own
    /// closing (∆HF0).
    closure: &'a ReadClosure,
}

/// A step with its reads closed through its calls — kept together until the plan is
/// ordered, so that a cycle can name the call a read came through.
#[derive(Clone)]
struct Planned<T> {
    step: T,
    closed: Vec<Closed>,
}

/// One dependency of a binding on a sibling: for a computed binding, a read of its
/// own, `via` the call it came through if it did; for a `from` binding, a lookup a
/// record its value may hold makes while it loads, `through` that record's index
/// naming the sibling. One or the other, never both.
struct Edge<'a> {
    to: usize,
    via: Option<Rc<str>>,
    through: Option<(&'a Rc<str>, &'a Index)>,
}

impl Places<'_> {
    /// A unit-local function index, in the module.
    fn func(&self, position: usize, func: u32) -> u32 {
        add_u32(self.function_offsets[position], func)
    }

    /// The loader's plan for every declared record — the bindings record included,
    /// whose plan is empty since its fields are bindings. Each field's default and
    /// index come from the nearest declaration in the chain that gives one: a
    /// redeclaration changes the default (D32), and one that gives none leaves the
    /// ancestor's in force. The steps are relocated into the module and ordered by
    /// what each reads through `self`, the calls it makes followed (∆HF0); a field
    /// that reads itself through others is a link error at the field, since no order
    /// could run it, and so is one that reaches an ambient value through a call,
    /// since a default reads only the record it builds (∆eag).
    fn record_plans(&self, diagnostics: &mut Vec<Diagnostic>) -> Vec<RecordPlan> {
        let mut plans = Vec::new();
        for export in self.index.types() {
            let name = &export.decl.name;
            if export.decl.kind() != DeclKind::Record {
                continue;
            }
            let mut steps: Vec<Planned<Step>> = Vec::new();
            let mut indexes: Vec<Index> = Vec::new();
            for decl in self.index.chain(name) {
                let Shape::Record { fields, .. } = &decl.shape else {
                    continue;
                };
                let owner = self
                    .index
                    .lookup_type(&decl.name)
                    .expect("a chain walks declared types");
                let position = self.positions[owner.path.as_str()];
                let unit = &self.units[position];
                // A default is refused once, where it is declared; a subtype inherits
                // a default already refused.
                let refuse = decl.name == *name;
                let page_backed = self.host.document_type().is_some_and(|document| {
                    document == decl.name || self.index.extends(&decl.name, &document)
                });
                for field in fields {
                    let of_field = |record: &Rc<str>, name: &Rc<str>| {
                        *record == decl.name && *name == field.name
                    };
                    if !steps.iter().any(|planned| planned.step.field == field.name)
                        && let Some(row) = unit
                            .defaults
                            .iter()
                            .find(|row| of_field(&row.record, &row.field))
                    {
                        let closed = self.closure.step(&unit.uses[&row.func]);
                        if refuse {
                            diagnostics.extend(self.refuse_reach(
                                &closed,
                                &unit.path,
                                row.span,
                                &row.field,
                                Runs::Default { page_backed },
                            ));
                        }
                        steps.push(Planned {
                            step: Step {
                                field: row.field.clone(),
                                func: self.func(position, row.func),
                                reads: closure::own_reads(&closed),
                                file: index_u32(position),
                                span: row.span,
                            },
                            closed,
                        });
                    }
                    if !indexes.iter().any(|index| index.field == field.name)
                        && let Some(row) = unit
                            .indexes
                            .iter()
                            .find(|row| of_field(&row.record, &row.field))
                    {
                        indexes.push(Index {
                            field: row.field.clone(),
                            value: row.value.clone(),
                            key: row.key.clone(),
                            file: index_u32(position),
                            span: row.span,
                        });
                    }
                }
            }
            let deps = |i: usize| -> Vec<usize> {
                steps[i]
                    .step
                    .reads
                    .iter()
                    .filter_map(|field| steps.iter().position(|p| p.step.field == *field))
                    .collect()
            };
            let steps = ordered(&steps, deps, |cycle| {
                let first = &steps[cycle[0]].step;
                diagnostics.push(Diagnostic::error(
                    &self.units[first.file as usize].path,
                    first.span,
                    format!(
                        "`{}` depends on itself: {}",
                        first.field,
                        chain(cycle, |a, b| {
                            let (a, b) = (&steps[a], &steps[b].step.field);
                            let via = closure::via(
                                &a.closed,
                                |access| matches!(access, Access::Own(field) if field == b),
                            );
                            edge(&a.step.field, "reads", b, via)
                        })
                    ),
                ));
            });
            plans.push(RecordPlan {
                name: name.clone(),
                steps: steps.into_iter().map(|planned| planned.step).collect(),
                indexes,
            });
        }
        plans
    }

    /// The refusal of every page template whose record's plan reads `body` (D43): a
    /// template has no body until it renders, and the loader would hand such a
    /// default null, so the record cannot type it. One error per template, at where
    /// its record was named, labelled at each default that reads `body` — an
    /// inherited one included, since the plan runs it all the same. A document of the
    /// same record is untouched: its prose is the body.
    fn refuse_page_templates(&self, records: &[RecordPlan]) -> Vec<Diagnostic> {
        self.units
            .iter()
            .filter(|unit| unit.page_shaped && unit.body.is_none())
            .filter_map(|template| {
                let record = template.record.as_deref()?;
                let plan = records
                    .iter()
                    .find(|plan| &*plan.name == record)
                    .expect("a unit's record is a declared record, or it never compiled");
                let readers: Vec<(&str, Span)> = plan
                    .steps
                    .iter()
                    .filter_map(|step| {
                        let position = step.file as usize;
                        let owner = &self.units[position];
                        let row = owner
                            .defaults
                            .iter()
                            .find(|row| self.func(position, row.func) == step.func)
                            .expect("a step runs a default of the file it names");
                        row.reads_body.map(|span| (owner.path.as_str(), span))
                    })
                    .collect();
                if readers.is_empty() {
                    return None;
                }
                let mut diagnostic = Diagnostic::error(
                    &template.path,
                    template.record_span,
                    format!(
                        "`{record}` cannot type a page template: its defaults read \
                         `body`, and a template has no body until it renders"
                    ),
                );
                for (path, span) in readers {
                    diagnostic = diagnostic.with_label(path, span, "reads `body` here");
                }
                Some(diagnostic)
            })
            .collect()
    }

    /// The refusals of a function the loader runs as `runs` that reaches an ambient
    /// value it may not read ([`refused_at_load`]) — a field of it, or the value whole
    /// — one per value, at `span` in `path`, for the field or binding `subject`. Read
    /// directly, the compiler refused it where it stood, in the same words, and no
    /// program with that error links; through a call, only the link step can see it,
    /// and names the call (∆HF0).
    fn refuse_reach(
        &self,
        closed: &[Closed],
        path: &str,
        span: Span,
        subject: &str,
        runs: Runs,
    ) -> Vec<Diagnostic> {
        let mut refused: Vec<&Rc<str>> = Vec::new();
        closed
            .iter()
            .filter_map(|closed| {
                let (Access::Ambient { value, .. } | Access::Whole(value)) = &closed.access
                else {
                    return None;
                };
                let why = refused_at_load(runs, self.host.document(), value)?;
                if refused.contains(&value) {
                    return None;
                }
                refused.push(value);
                let how = match &closed.via {
                    Some(via) => format!("reaches `{value}` through `{via}`"),
                    None => format!("reads `{value}`"),
                };
                Some(Diagnostic::error(
                    path,
                    span,
                    format!("`{subject}` {how}, but {why}"),
                ))
            })
            .collect()
    }

    /// A plan per ambient value the host reads from a file the program has, in the
    /// host's order.
    fn bindings_plans(
        &self,
        records: &[RecordPlan],
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Vec<Bindings> {
        let plans: Vec<Bindings> = self
            .host
            .bindings_files()
            .filter_map(|(value, file)| {
                self.bindings_plan(value, file, records, diagnostics)
            })
            .collect();
        // The host fulfils a `from` binding by its name alone (D37), so one name may
        // stand for one binding across every file the host reads bindings from.
        let mut loaded: Vec<(&Bindings, &BindingStep)> = Vec::new();
        for plan in &plans {
            for step in &plan.steps {
                if !matches!(step.fulfilled, Fulfilled::ByHost(_)) {
                    continue;
                }
                match loaded.iter().find(|(_, first)| first.name == step.name) {
                    Some((first_plan, first)) => diagnostics.push(
                        Diagnostic::error(
                            &self.units[plan.file as usize].path,
                            step.span,
                            format!(
                                "`{}` is already loaded, for `{}`: the host fulfils a \
                                 `from` binding by name, so no two files it reads bindings \
                                 from may load one called `{}`",
                                step.name, first_plan.value, step.name
                            ),
                        )
                        .with_label(
                            &self.units[first_plan.file as usize].path,
                            first.span,
                            "first loaded here",
                        ),
                    ),
                    None => loaded.push((plan, step)),
                }
            }
        }
        plans
    }

    /// The plan for the bindings file `file`, read as `value`: its bindings in load
    /// order. A computed binding follows the siblings it reads, the calls it makes
    /// followed (∆HF0); a `from` binding follows every sibling the records its value
    /// may hold look a string up in while they load — through `extends` and through
    /// field types, since a subtype's or a nested record's index resolves too. A
    /// binding that needs itself is a link error at the binding, and so is a
    /// computed one that reaches the host's document value through a call. `None`
    /// when the file is not in the program.
    fn bindings_plan(
        &self,
        value: &str,
        file: &str,
        records: &[RecordPlan],
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<Bindings> {
        let &position = self.positions.get(file)?;
        let unit = &self.units[position];
        let steps: Vec<Planned<BindingStep>> = unit
            .bindings
            .iter()
            .map(|binding| {
                let (fulfilled, closed) = match &binding.supplied {
                    Supplied::ByFunction(func) => {
                        let closed = self.closure.step(&unit.uses[func]);
                        diagnostics.extend(self.refuse_reach(
                            &closed,
                            &unit.path,
                            binding.span,
                            &binding.name,
                            Runs::Binding,
                        ));
                        let fulfilled = Fulfilled::Computed {
                            func: self.func(position, *func),
                            reads: closure::ambient_values(&closed),
                        };
                        (fulfilled, closed)
                    }
                    Supplied::ByHost(kind) => (Fulfilled::ByHost(kind.clone()), Vec::new()),
                };
                Planned {
                    step: BindingStep {
                        name: binding.name.clone(),
                        fulfilled,
                        span: binding.span,
                    },
                    closed,
                }
            })
            .collect();
        let sibling = |field: &str| {
            steps
                .iter()
                .position(|planned| &*planned.step.name == field)
        };
        // Only a read of this value's own field orders its bindings; the value whole
        // is the record so far, and another value's field is the loader's order.
        let of_value = |closed: &Closed| match &closed.access {
            Access::Ambient { value: read, field } if **read == *value => sibling(field),
            Access::Ambient { .. } | Access::Whole(_) => None,
            Access::Own(_) | Access::Param { .. } => {
                unreachable!("a binding reads off ambient values alone")
            }
        };
        let edges: Vec<Vec<Edge<'_>>> = steps
            .iter()
            .map(|planned| match &planned.step.fulfilled {
                Fulfilled::Computed { .. } => planned
                    .closed
                    .iter()
                    .filter_map(|closed| {
                        Some(Edge {
                            to: of_value(closed)?,
                            via: closed.via.clone(),
                            through: None,
                        })
                    })
                    .collect(),
                Fulfilled::ByHost(_) => {
                    let Some(ty) = self.index.field(file, &planned.step.name) else {
                        return Vec::new();
                    };
                    self.records_reachable(&ty)
                        .iter()
                        .filter_map(|record| records.iter().find(|r| r.name == *record))
                        .flat_map(|record| {
                            record.indexes.iter().filter_map(move |index| {
                                if *index.value != *value {
                                    return None;
                                }
                                Some(Edge {
                                    to: sibling(&index.key)?,
                                    via: None,
                                    through: Some((&record.name, index)),
                                })
                            })
                        })
                        .collect()
                }
            })
            .collect();
        let deps =
            |i: usize| -> Vec<usize> { edges[i].iter().map(|edge| edge.to).collect() };
        let steps = ordered(&steps, deps, |cycle| {
            let first = &steps[cycle[0]].step;
            let mut diagnostic = Diagnostic::error(
                &unit.path,
                first.span,
                format!(
                    "`{}` cannot be loaded before itself: {}",
                    first.name,
                    chain(cycle, |a, b| {
                        let edge = edges[a]
                            .iter()
                            .find(|edge| edge.to == b)
                            .expect("a cycle runs along dependencies");
                        needs(&steps[a].step.name, &steps[b].step.name, edge)
                    })
                ),
            );
            // The first lookup a record makes on the cycle is where the author looks.
            let through =
                cycle
                    .iter()
                    .zip(cycle.iter().cycle().skip(1))
                    .find_map(|(&a, &b)| {
                        edges[a]
                            .iter()
                            .find(|edge| edge.to == b)
                            .and_then(|edge| edge.through.as_ref())
                    });
            if let Some((_, index)) = through {
                diagnostic = diagnostic.with_label(
                    &self.units[index.file as usize].path,
                    index.span,
                    "looked up here",
                );
            }
            diagnostics.push(diagnostic);
        });
        Some(Bindings {
            value: Rc::from(value),
            record: Rc::from(file),
            file: index_u32(position),
            steps: steps.into_iter().map(|planned| planned.step).collect(),
        })
    }

    /// Every declared record a value of type `ty` may hold: the records it names, the
    /// records extending those (a document may be any subtype of its collection's
    /// element type), and the records their fields name, however deep.
    fn records_reachable(&self, ty: &Type) -> Vec<Rc<str>> {
        let mut pending = Vec::new();
        ty.named_types(&mut pending);
        let mut seen: Vec<Rc<str>> = Vec::new();
        while let Some(name) = pending.pop() {
            if seen.contains(&name) || self.index.kind(&name) != Some(DeclKind::Record) {
                continue;
            }
            seen.push(name.clone());
            for export in self.index.types() {
                if self.index.extends(&export.decl.name, &name) {
                    pending.push(export.decl.name.clone());
                }
            }
            for (_, field_ty) in self.index.fields(&name) {
                field_ty.named_types(&mut pending);
            }
        }
        seen
    }
}

/// `items` in the order `deps` admits, or nothing once `report` has been told each
/// cycle — the diagnostics it pushes are what stop the link.
fn ordered<T: Clone>(
    items: &[T],
    deps: impl Fn(usize) -> Vec<usize>,
    mut report: impl FnMut(&[usize]),
) -> Vec<T> {
    match order(items.len(), deps) {
        Ok(ordered) => ordered.into_iter().map(|i| items[i].clone()).collect(),
        Err(cycles) => {
            for cycle in cycles {
                report(&cycle);
            }
            Vec::new()
        }
    }
}

/// A cycle as a sentence, one link per member: "`a` reads `b`, `b` reads `a`".
fn chain(cycle: &[usize], link: impl Fn(usize, usize) -> String) -> String {
    let links: Vec<String> = cycle
        .iter()
        .zip(cycle.iter().cycle().skip(1))
        .map(|(&a, &b)| link(a, b))
        .collect();
    links.join(", ")
}

/// One link of a cycle: "`a` reads `b`", naming the call the read came through when
/// it came through one, since that is the line the author has to look at.
fn edge(a: &str, verb: &str, b: &str, via: Option<&Rc<str>>) -> String {
    match via {
        Some(via) => format!("`{a}` {verb} `{b}` through `{via}`"),
        None => format!("`{a}` {verb} `{b}`"),
    }
}

/// One link of a cycle among bindings: what `a` needs `b` for — a read of its own,
/// through a call or not, or a lookup a record its value holds makes while `a`
/// loads, which names that record's field, since the binding itself says nothing of
/// `b`.
fn needs(a: &str, b: &str, edge: &Edge<'_>) -> String {
    match (&edge.through, &edge.via) {
        (Some((record, index)), _) => format!(
            "`{a}` needs `{b}`, which `{record}.{}` looks up while `{a}` loads",
            index.field
        ),
        (None, Some(via)) => format!("`{a}` needs `{b}` through `{via}`"),
        (None, None) => format!("`{a}` needs `{b}`"),
    }
}

/// Orders `n` nodes so that every node comes after everything `deps` says it depends
/// on, keeping declaration order wherever the dependencies allow it. `deps(i)` lists
/// the nodes `i` must follow; an index out of range is ignored, and a node that lists
/// itself is a cycle of one — a default that reads the field it computes, a collection
/// whose records look a string up in the collection while it loads.
///
/// # Errors
///
/// Every cycle, when the dependencies admit no order: each as the nodes on it, entered
/// from the first node (in declaration order) that leads into it. Every problem is
/// reported rather than the first, as a compile reports every problem in a file.
fn order(
    n: usize,
    deps: impl Fn(usize) -> Vec<usize>,
) -> Result<Vec<usize>, Vec<Vec<usize>>> {
    let deps: Vec<Vec<usize>> = (0..n)
        .map(|i| deps(i).into_iter().filter(|&d| d < n).collect())
        .collect();
    let mut placed = vec![false; n];
    let mut ordered = Vec::with_capacity(n);
    while ordered.len() < n {
        let next = (0..n).find(|&i| !placed[i] && deps[i].iter().all(|&d| placed[d]));
        let Some(next) = next else {
            return Err(cycles_among(&deps, &placed));
        };
        placed[next] = true;
        ordered.push(next);
    }
    Ok(ordered)
}

/// Every distinct cycle the unplaced nodes of `deps` form. A node that is merely
/// downstream of a cycle leads into that same cycle, which is kept once.
fn cycles_among(deps: &[Vec<usize>], placed: &[bool]) -> Vec<Vec<usize>> {
    let mut cycles: Vec<Vec<usize>> = Vec::new();
    for start in (0..deps.len()).filter(|&i| !placed[i]) {
        let cycle = cycle_from(deps, placed, start);
        let same = |a: &[usize], b: &[usize]| {
            a.len() == b.len() && a.iter().all(|node| b.contains(node))
        };
        if !cycles.iter().any(|seen| same(seen, &cycle)) {
            cycles.push(cycle);
        }
    }
    cycles
}

/// The cycle reached from `start` through unplaced nodes: each node's first unplaced
/// dependency, until one repeats.
fn cycle_from(deps: &[Vec<usize>], placed: &[bool], start: usize) -> Vec<usize> {
    let mut path = vec![start];
    loop {
        let last = *path.last().expect("a start");
        let next = deps[last]
            .iter()
            .copied()
            .find(|&d| !placed[d])
            .expect("an unplaced node has an unplaced dependency");
        if let Some(at) = path.iter().position(|&p| p == next) {
            return path.split_off(at);
        }
        path.push(next);
    }
}

#[cfg(test)]
mod tests {
    use lemmon_syntax::spans::at;

    use super::*;
    use crate::bytecode::Module;
    use crate::diagnostics::Compiled;
    use crate::host::Ambient;
    use crate::link::link;
    use crate::testing::{
        AMBIENT, addressed_host, linked, shell_host, shown, unit_of, untyped_host,
    };
    use crate::{SourceFile, compile};

    fn steps_of<'m>(module: &'m Module, record: &str) -> Vec<&'m str> {
        module
            .records
            .iter()
            .find(|plan| &*plan.name == record)
            .expect("a plan per record")
            .steps
            .iter()
            .map(|step| &*step.field)
            .collect()
    }

    /// A page schema whose `excerpt` reads `body`, and one below it that adds a
    /// default reading nothing of the page.
    const READS_BODY: &str = "<lem-type Post extends Page>\n  \
         excerpt: html = body\n  slug: string = path\n  \
         layout: (Post, html): html\n</lem-type>\n\
         <lem-type Tip extends Post>\n  n: int = 1\n</lem-type>\n\
         <lem-type Memo extends Page>\n  slug: string = path\n  \
         layout: (Memo, html): html\n</lem-type>";

    /// A page template typed as `record` by its own key, beside a document of the
    /// same record, compiled and linked under the shell host.
    fn page_template_of(record: &str) -> (String, Compiled<Module>) {
        let template = format!("---\ntype: {record}\n---\n<p>the body</p>");
        let files = [
            SourceFile::new("ambient.lem", AMBIENT),
            SourceFile::new("schema.lem", READS_BODY),
            SourceFile::new("t.lem", &template)
                .with_record(record)
                .named_at(at(&template, record)),
            SourceFile::new("d.md", format!("---\ntype: {record}\n---\nprose"))
                .with_record(record),
        ];
        (template, compile(&files, &addressed_host()))
    }

    #[test]
    fn refuses_a_page_template_typed_by_a_record_whose_defaults_read_body() {
        // The document of the same record is untouched: its prose is the body. The
        // template is refused where its record was named, and the label says which
        // default would read what the loader can only hand it as null.
        let (template, linked) = page_template_of("Post");
        assert!(linked.output.is_none());
        assert_eq!(
            shown(&linked.diagnostics),
            [format!(
                "t.lem:{}: `Post` cannot type a page template: its defaults read \
                 `body`, and a template has no body until it renders\n  \
                 schema.lem:{}: reads `body` here",
                at(&template, "Post"),
                at(READS_BODY, "excerpt: html = «body»")
            )]
        );
    }

    #[test]
    fn refuses_a_page_template_through_the_default_its_record_inherits() {
        // `Tip` declares no default reading `body`; its plan runs `Post`'s all the same.
        let (template, linked) = page_template_of("Tip");
        assert!(linked.output.is_none());
        assert_eq!(
            shown(&linked.diagnostics),
            [format!(
                "t.lem:{}: `Tip` cannot type a page template: its defaults read \
                 `body`, and a template has no body until it renders\n  \
                 schema.lem:{}: reads `body` here",
                at(&template, "Tip"),
                at(READS_BODY, "excerpt: html = «body»")
            )]
        );
    }

    #[test]
    fn links_a_page_template_typed_by_a_record_that_reads_only_its_path() {
        let (_, linked) = page_template_of("Memo");
        let module = linked.expect_output();
        let t = module
            .document("t.lem")
            .expect("a page source for the template");
        assert_eq!(t.record.as_deref(), Some("Memo"));
        assert_eq!(t.body, None);
        let d = module
            .document("d.md")
            .expect("a page source for the document");
        assert_eq!(d.body.as_deref(), Some("<p>prose</p>\n"));
        assert_eq!(module.document("schema.lem"), None);
    }

    #[test]
    fn plans_a_records_steps_in_the_order_their_reads_dictate() {
        let module = linked(
            &untyped_host(),
            &[(
                "types.lem",
                "<lem-type A>\n  a: string = self.b + \"!\"\n  b: string = self.c\n  \
                 c: string = \"C\"\n  d: string = \"D\"\n</lem-type>",
            )],
        )
        .expect_output();
        assert_eq!(steps_of(&module, "A"), ["c", "b", "a", "d"]);
    }

    #[test]
    fn plans_inherited_steps_from_the_nearest_declaration_of_each_field() {
        // `LinkPost` redeclares `title` to change its default (D32): its plan runs its
        // own `title` and `Page`'s `draft`; `Article`'s runs its own `title`.
        let module = linked(
            &addressed_host(),
            &[
                (
                    "ambient.lem",
                    "<lem-type Page>\n  url: url\n  draft: bool = false\n</lem-type>",
                ),
                (
                    "schema.lem",
                    "<lem-type Article extends Page>\n  title: string = \"A\"\n  \
                     layout: (Article, html): html\n</lem-type>\n\
                     <lem-type LinkPost extends Article>\n  title: string = \"L\"\n\
                     </lem-type>",
                ),
            ],
        )
        .expect_output();
        assert_eq!(steps_of(&module, "LinkPost"), ["title", "draft"]);
        assert_eq!(steps_of(&module, "Article"), ["title", "draft"]);
        assert_eq!(steps_of(&module, "Page"), ["draft"]);
        let name = |record: &str| {
            let plan = module.records.iter().find(|p| &*p.name == record).unwrap();
            module.functions[plan.steps[0].func as usize].name.clone()
        };
        assert_eq!(name("LinkPost"), "LinkPost.title");
        assert_eq!(name("Article"), "Article.title");
    }

    #[test]
    fn keeps_an_inherited_default_under_a_redeclaration_that_gives_none() {
        // A redeclaration changes the default (D32); one that gives none changes
        // nothing, so `Page`'s step still runs for a `Note`.
        let module = linked(
            &addressed_host(),
            &[
                (
                    "ambient.lem",
                    "<lem-type Page>\n  url: url\n  draft: bool = false\n</lem-type>",
                ),
                (
                    "schema.lem",
                    "<lem-type Note extends Page>\n  draft: bool\n  \
                     layout: (Note, html): html\n</lem-type>",
                ),
            ],
        )
        .expect_output();
        assert_eq!(steps_of(&module, "Note"), ["draft"]);
        let plan = module.records.iter().find(|p| &*p.name == "Note").unwrap();
        assert_eq!(
            module.functions[plan.steps[0].func as usize].name,
            "Page.draft"
        );
    }

    #[test]
    fn refuses_to_link_a_default_that_reads_itself_through_another() {
        const CYCLE: &str = "<lem-type A>\n  ok: string = \"x\"\n  a: string = self.b\n  \
                             b: string = self.a\n  c: string = self.c\n</lem-type>";
        let linked = linked(&untyped_host(), &[("types.lem", CYCLE)]);
        assert!(linked.output.is_none());
        assert_eq!(
            shown(&linked.diagnostics),
            [
                format!(
                    "types.lem:{}: `a` depends on itself: `a` reads `b`, `b` reads `a`",
                    at(CYCLE, "«a»: string = self.b")
                ),
                format!(
                    "types.lem:{}: `c` depends on itself: `c` reads `c`",
                    at(CYCLE, "«c»: string")
                ),
            ]
        );
    }

    fn binding_names(module: &Module) -> Vec<&str> {
        assert_eq!(module.bindings.len(), 1, "one bindings plan");
        module.bindings[0]
            .steps
            .iter()
            .map(|step| &*step.name)
            .collect()
    }

    #[test]
    fn plans_computed_bindings_after_the_siblings_they_read() {
        let module = linked(
            &addressed_host(),
            &[(
                "site.lem",
                "<lem-let tagline: string = \"Notes from \" + name />\n\
                 <lem-let name: string = \"limulus.net\" />",
            )],
        )
        .expect_output();
        assert_eq!(binding_names(&module), ["name", "tagline"]);
        // The bindings record is a record like any other, with nothing to run: its
        // fields are the bindings planned here.
        assert_eq!(steps_of(&module, "site.lem"), Vec::<&str>::new());
        let bindings = &module.bindings[0];
        assert_eq!(&*bindings.value, "site");
        assert_eq!(&*bindings.record, "site.lem");
        assert_eq!(&module.files[bindings.file as usize], "site.lem");
        let Fulfilled::Computed { func, .. } = &bindings.steps[1].fulfilled else {
            panic!("computed");
        };
        assert_eq!(module.functions[*func as usize].name, "tagline");
    }

    #[test]
    fn plans_a_loaded_binding_after_the_map_its_records_index() {
        // `Article.hero` names `site.photos`: `posts` waits for `photos`, declared
        // after it, on the strength of the pair alone (∆eag).
        let module = linked(
            &addressed_host(),
            &[
                ("ambient.lem", AMBIENT),
                (
                    "site.lem",
                    "<lem-let posts: [Article] from \"feed/*.md\" />\n\
                     <lem-let photos: {string: Photo} from \"photos.json\" />",
                ),
                (
                    "schema.lem",
                    "<lem-type Photo>\n  alt: string\n</lem-type>\n\
                     <lem-type Article extends Page>\n  hero: Photo? by site.photos\n  \
                     layout: (Article, html): html\n</lem-type>",
                ),
            ],
        )
        .expect_output();
        assert_eq!(binding_names(&module), ["photos", "posts"]);
        let article = module
            .records
            .iter()
            .find(|p| &*p.name == "Article")
            .unwrap();
        assert_eq!(article.indexes.len(), 1);
        assert_eq!(&*article.indexes[0].key, "photos");
        assert_eq!(
            &module.files[article.indexes[0].file as usize],
            "schema.lem"
        );
    }

    #[test]
    fn refuses_to_link_a_binding_its_records_index_while_it_loads() {
        const SITE: &str = "<lem-let posts: {string: Article} from \"feed/*.md\" />";
        const SCHEMA: &str = "<lem-type Article extends Page>\n  \
                              related: Article? by site.posts\n  \
                              layout: (Article, html): html\n</lem-type>";
        let linked = linked(
            &addressed_host(),
            &[
                ("ambient.lem", AMBIENT),
                ("site.lem", SITE),
                ("schema.lem", SCHEMA),
            ],
        );
        assert!(linked.output.is_none());
        assert_eq!(
            shown(&linked.diagnostics),
            [format!(
                "site.lem:{}: `posts` cannot be loaded before itself: `posts` needs \
                 `posts`, which `Article.related` looks up while `posts` loads\n  \
                 schema.lem:{}: looked up here",
                at(SITE, "«posts»: {string: Article}"),
                at(SCHEMA, "related")
            )]
        );
    }

    #[test]
    fn takes_a_fields_default_and_index_each_from_the_nearest_declaration_giving_one() {
        // `LinkPost` redeclares `hero` to give it a default and says nothing of the
        // index: the default is its own, the index is `Article`'s.
        let module = linked(
            &addressed_host(),
            &[
                ("ambient.lem", AMBIENT),
                (
                    "site.lem",
                    "<lem-let photos: {string: Photo} from \"photos.json\" />\n\
                     <lem-let posts: [Article] from \"feed/*.md\" />",
                ),
                (
                    "schema.lem",
                    "<lem-type Photo>\n  alt: string\n</lem-type>\n\
                     <lem-type Article extends Page>\n  hero: Photo? by site.photos\n  \
                     layout: (Article, html): html\n</lem-type>\n\
                     <lem-type LinkPost extends Article>\n  hero: Photo? = null\n\
                     </lem-type>",
                ),
            ],
        )
        .expect_output();
        let link_post = module
            .records
            .iter()
            .find(|p| &*p.name == "LinkPost")
            .unwrap();
        assert_eq!(steps_of(&module, "LinkPost"), ["hero"]);
        assert_eq!(
            &module.files[link_post.steps[0].file as usize],
            "schema.lem"
        );
        assert_eq!(
            module.functions[link_post.steps[0].func as usize].name,
            "LinkPost.hero"
        );
        assert_eq!(link_post.indexes.len(), 1);
        assert_eq!(&*link_post.indexes[0].key, "photos");
    }

    #[test]
    fn plans_no_bindings_without_a_bindings_file_in_the_program() {
        let module = linked(&untyped_host(), &[("a.lem", "x")]).expect_output();
        assert!(module.bindings.is_empty());
        // The host reads `site.lem`, which has not been compiled.
        let module = linked(&addressed_host(), &[("a.lem", "x")]).expect_output();
        assert!(module.bindings.is_empty());
    }

    #[test]
    fn plans_every_bindings_file_a_host_reads() {
        let host = Host::new()
            .bind("site", Ambient::bindings("site.lem"))
            .bind("data", Ambient::bindings("data.lem"));
        let module = linked(
            &host,
            &[
                ("site.lem", "<lem-let name: string = \"n\" />"),
                (
                    "data.lem",
                    "<lem-let n: int = 1 />\n<lem-let m: int = n + 1 />",
                ),
            ],
        )
        .expect_output();
        let plans: Vec<(&str, &str, Vec<&str>)> = module
            .bindings
            .iter()
            .map(|b| {
                (
                    &*b.value,
                    &*b.record,
                    b.steps.iter().map(|s| &*s.name).collect(),
                )
            })
            .collect();
        assert_eq!(
            plans,
            [
                ("site", "site.lem", vec!["name"]),
                ("data", "data.lem", vec!["n", "m"]),
            ]
        );
    }

    #[test]
    fn refuses_one_from_binding_name_in_two_bindings_files() {
        let host = Host::new()
            .bind("site", Ambient::bindings("site.lem"))
            .bind("data", Ambient::bindings("data.lem"));
        const SITE: &str = "<lem-let motto: string from env(\"MOTTO\") />";
        const DATA: &str = "<lem-let motto: string from env(\"OTHER\") />";
        let files = [("site.lem", SITE), ("data.lem", DATA)];
        let errors: Vec<String> = linked(&host, &files)
            .diagnostics
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            errors,
            [format!(
                "data.lem:{}: `motto` is already loaded, for `site`: the host fulfils a \
                 `from` binding by name, so no two files it reads bindings from may \
                 load one called `motto`\n  site.lem:{}: first loaded here",
                at(DATA, "«motto»: string"),
                at(SITE, "«motto»: string")
            )]
        );
    }

    // --- Reads through calls (∆HF0) --------------------------------------------------

    #[test]
    fn orders_a_default_after_the_fields_a_called_function_reads() {
        // `a` hands `self` to `f`, which reads `b`: the loader computes `b` first.
        let module = linked(
            &untyped_host(),
            &[(
                "types.lem",
                "<lem-type A>\n  a: string = f(self)\n  b: string = \"B\"\n</lem-type>\n\
                 <lem-fn f(x: A): string = x.b />",
            )],
        )
        .expect_output();
        assert_eq!(steps_of(&module, "A"), ["b", "a"]);
    }

    #[test]
    fn closes_reads_through_a_chain_of_calls_across_files_and_through_recursion() {
        // `a` → `f` → `g` → `h`, where `h` reads `c` off what `g` passed on, and `g`
        // calls itself.
        let module = linked(
            &untyped_host(),
            &[
                (
                    "types.lem",
                    "<lem-type A>\n  a: string = f(self)\n  b: string = \"B\"\n  \
                     c: string = \"C\"\n</lem-type>",
                ),
                (
                    "lib.lem",
                    "<lem-fn f(x: A): string = g(x, \"\") />\n\
                     <lem-fn g(y: A, s: string): string = s + h(y) + g(y, s) />\n\
                     <lem-fn h(z: A): string = z.c />",
                ),
            ],
        )
        .expect_output();
        assert_eq!(steps_of(&module, "A"), ["b", "c", "a"]);
        let a = &module.records[0].steps[2];
        assert_eq!(a.reads, [Rc::from("c")]);
    }

    #[test]
    fn drops_a_callees_read_off_a_parameter_handed_something_other_than_a_root() {
        // `f(self.hero)` hands a field, so `f`'s reads off it are the photo's own
        // business; `a` waits for `hero` and not for the `alt` of its own.
        let module = linked(
            &untyped_host(),
            &[(
                "types.lem",
                "<lem-type Photo>\n  alt: string = \"x\"\n</lem-type>\n\
                 <lem-type A>\n  a: string = f(self.hero)\n  \
                 hero: Photo = {alt: \"h\"}\n  alt: string = \"A\"\n</lem-type>\n\
                 <lem-fn f(p: Photo): string = p.alt />",
            )],
        )
        .expect_output();
        assert_eq!(steps_of(&module, "A"), ["hero", "a", "alt"]);
    }

    #[test]
    fn refuses_a_cycle_through_a_call_and_names_the_call() {
        const CYCLE: &str = "<lem-type A>\n  a: string = f(self)\n  b: string = self.a\n\
                             </lem-type>\n\
                             <lem-fn f(x: A): string = x.b />";
        let linked = linked(&untyped_host(), &[("types.lem", CYCLE)]);
        assert!(linked.output.is_none());
        assert_eq!(
            shown(&linked.diagnostics),
            [format!(
                "types.lem:{}: `a` depends on itself: `a` reads `b` through `f`, `b` \
                 reads `a`",
                at(CYCLE, "«a»: string")
            )]
        );
    }

    #[test]
    fn orders_a_computed_binding_after_the_siblings_a_called_function_reads() {
        let module = linked(
            &addressed_host(),
            &[
                ("ambient.lem", AMBIENT),
                (
                    "site.lem",
                    "<lem-fn all(): [string] = site.posts />\n\
                     <lem-let articles: [string] = all() />\n\
                     <lem-let posts: [string] from \"feed/*.md\" />",
                ),
            ],
        )
        .expect_output();
        assert_eq!(binding_names(&module), ["posts", "articles"]);
    }

    #[test]
    fn refuses_a_binding_that_needs_itself_through_a_call_and_names_the_call() {
        const SITE: &str = "<lem-fn all(): [string] = site.articles />\n\
                            <lem-let articles: [string] = all() />";
        let linked = linked(
            &addressed_host(),
            &[("ambient.lem", AMBIENT), ("site.lem", SITE)],
        );
        assert!(linked.output.is_none());
        assert_eq!(
            shown(&linked.diagnostics),
            [format!(
                "site.lem:{}: `articles` cannot be loaded before itself: `articles` \
                 needs `articles` through `all`",
                at(SITE, "«articles»: [string] =")
            )]
        );
    }

    #[test]
    fn refuses_a_default_that_reaches_an_ambient_value_through_a_call() {
        // Read directly, `site` is refused where it stands; inside `caption()`, only
        // the link step can see it, and it says so at the field, naming the call.
        const SCHEMA: &str = "<lem-type Article extends Page>\n  \
                              caption: string = caption()\n  \
                              layout: (Article, html): html\n</lem-type>\n\
                              <lem-fn caption(): string = site.captions[\"x\"] />";
        let linked = linked(
            &addressed_host(),
            &[
                ("ambient.lem", AMBIENT),
                (
                    "site.lem",
                    "<lem-let posts: [Article] from \"feed/*.md\" />\n\
                     <lem-let captions: {string: string} from \"captions.json\" />",
                ),
                ("schema.lem", SCHEMA),
            ],
        );
        assert!(linked.output.is_none());
        assert_eq!(
            shown(&linked.diagnostics),
            [format!(
                "schema.lem:{}: `caption` reaches `site` through `caption`, but `site` \
                 is out of scope in a default: a default reads only the record it \
                 builds — `self`, and `path` and `body` on a page",
                at(SCHEMA, "«caption»: string")
            )]
        );
    }

    #[test]
    fn refuses_every_ambient_value_a_default_reaches_and_once_per_declaration() {
        // `banner` reaches `site` and `build` through two calls: both are said, in
        // one link. `LinkPost` inherits the default and repeats nothing.
        const SCHEMA: &str = "<lem-type Article extends Page>\n  \
                              banner: string = brand() + year()\n  \
                              layout: (Article, html): html\n</lem-type>\n\
                              <lem-type LinkPost extends Article>\n  via: string\n\
                              </lem-type>\n\
                              <lem-fn brand(): string = site.name />\n\
                              <lem-fn year(): string = build.generator />";
        let host = shell_host()
            .addressed_by("url")
            .bind("build", Ambient::named("Build"));
        let linked = linked(
            &host,
            &[
                (
                    "ambient.lem",
                    "<lem-type Page>\n  url: url\n</lem-type>\n\
                     <lem-type Build>\n  generator: string\n</lem-type>",
                ),
                ("site.lem", "<lem-let name: string = \"n\" />"),
                ("schema.lem", SCHEMA),
            ],
        );
        assert!(linked.output.is_none());
        let refused = |value: &str, via: &str| {
            format!(
                "schema.lem:{}: `banner` reaches `{value}` through `{via}`, but `{value}` \
                 is out of scope in a default: a default reads only the record it \
                 builds — `self`, and `path` and `body` on a page",
                at(SCHEMA, "«banner»:")
            )
        };
        assert_eq!(
            shown(&linked.diagnostics),
            [refused("site", "brand"), refused("build", "year")]
        );
    }

    #[test]
    fn refuses_a_default_that_reaches_the_document_value_through_a_call() {
        // Read directly, `page` is refused where it stands (∆iCW); reached through
        // `where` or `here` — a field of it, or the value whole — only the link step
        // can see it, and it says so at the field.
        const SCHEMA: &str = "<lem-type Post extends Page>\n  at: url = where()\n  \
                              origin: Page = here()\n  layout: (Post, html): html\n\
                              </lem-type>";
        const FNS: &str = "<lem-fn where(): url = page.url />\n\
                           <lem-fn here(): Page = page />";
        let linked = linked(
            &addressed_host(),
            &[
                ("ambient.lem", AMBIENT),
                ("schema.lem", SCHEMA),
                ("fns.lem", FNS),
            ],
        );
        assert!(linked.output.is_none());
        let refused = |field: &str, via: &str| {
            format!(
                "schema.lem:{}: `{field}` reaches `page` through `{via}`, but `page` is \
                 out of scope in a default: a default reads only the record it builds \
                 — `self`, and `path` and `body` on a page",
                at(SCHEMA, &format!("«{field}»:"))
            )
        };
        assert_eq!(
            shown(&linked.diagnostics),
            [refused("at", "where"), refused("origin", "here")]
        );
    }

    #[test]
    fn refuses_a_computed_binding_that_reaches_the_document_value_through_a_call() {
        const SITE: &str = "<lem-let home: url = where() />";
        let linked = linked(
            &addressed_host(),
            &[
                ("ambient.lem", AMBIENT),
                ("site.lem", SITE),
                ("fns.lem", "<lem-fn where(): url = page.url />"),
            ],
        );
        assert!(linked.output.is_none());
        assert_eq!(
            shown(&linked.diagnostics),
            [format!(
                "site.lem:{}: `home` reaches `page` through `where`, but `page` is bound \
                 per page, and a binding here is computed once, at load",
                at(SITE, "home")
            )]
        );
    }

    #[test]
    fn leaves_a_function_named_as_a_value_out_of_the_order() {
        // `layout = post` stores `post` for the render to call, when `page` is bound
        // and `site` is whole: what it reads is no part of the load order, however
        // much of `page` and `site` it reads, and `posts` does not need itself.
        const SCHEMA: &str = "<lem-type Post extends Page>\n  title: string\n  \
                              layout: (Post, html): html = post\n</lem-type>\n\
                              <lem-fn post(p: Post, body: html)>\
                              <lem.page.url><lem.site.posts[0].title><lem.p.title>\
                              <lem.body></lem-fn>";
        let module = linked(
            &addressed_host(),
            &[
                ("ambient.lem", AMBIENT),
                ("site.lem", "<lem-let posts: [Post] from \"feed/*.md\" />"),
                ("schema.lem", SCHEMA),
            ],
        )
        .expect_output();
        assert_eq!(steps_of(&module, "Post"), ["layout"]);
        let post = module.records.iter().find(|p| &*p.name == "Post").unwrap();
        assert!(post.steps[0].reads.is_empty());
    }

    #[test]
    fn reorders_a_plan_when_a_callee_body_changes_leaving_the_caller_unit_as_it_was() {
        // Invariant 11 with the closure in play: `f`'s body decides the order, and it
        // is the link step, not the schema's unit, that changes when the body does.
        let types = (
            "types.lem",
            "<lem-type A>\n  a: string = f(self)\n  b: string = \"B\"\n</lem-type>",
        );
        let plan_against = |lib: &str| {
            let files = [types, ("lib.lem", lib)];
            let schema = unit_of("types.lem", &files);
            let lib = unit_of("lib.lem", &files);
            let units = [schema.clone(), lib];
            let module =
                link(&units, &ExportIndex::of(&units), &untyped_host()).expect_output();
            (schema, steps_of(&module, "A").join(","))
        };
        let (unit, plan) = plan_against("<lem-fn f(x: A): string = \"f\" />");
        let (edited, replanned) = plan_against("<lem-fn f(x: A): string = x.b />");
        assert_eq!(unit, edited);
        assert_eq!((plan.as_str(), replanned.as_str()), ("a,b", "b,a"));
    }

    #[test]
    fn keeps_declaration_order_where_nothing_depends_on_anything() {
        assert_eq!(order(3, |_| vec![]), Ok(vec![0, 1, 2]));
    }

    #[test]
    fn places_a_dependency_before_what_reads_it() {
        // 0 reads 2; 1 reads 0.
        assert_eq!(
            order(3, |i| match i {
                0 => vec![2],
                1 => vec![0],
                _ => vec![],
            }),
            Ok(vec![2, 0, 1])
        );
    }

    #[test]
    fn ignores_an_index_out_of_range() {
        assert_eq!(order(2, |_| vec![9]), Ok(vec![0, 1]));
    }

    #[test]
    fn treats_a_node_that_needs_itself_as_a_cycle_of_one() {
        assert_eq!(
            order(2, |i| if i == 1 { vec![1] } else { vec![] }),
            Err(vec![vec![1]])
        );
    }

    #[test]
    fn names_the_cycle_from_its_first_member() {
        // 0 is fine; 1 → 2 → 3 → 1.
        assert_eq!(
            order(4, |i| match i {
                1 => vec![2],
                2 => vec![3],
                3 => vec![1],
                _ => vec![],
            }),
            Err(vec![vec![1, 2, 3]])
        );
    }

    #[test]
    fn names_every_cycle_once() {
        // 0 ↔ 1, 2 ↔ 3, and 4 leads into the second: two cycles, not three.
        assert_eq!(
            order(5, |i| match i {
                0 => vec![1],
                1 => vec![0],
                2 => vec![3],
                3 => vec![2],
                4 => vec![3],
                _ => vec![],
            }),
            Err(vec![vec![0, 1], vec![2, 3]])
        );
    }

    #[test]
    fn names_only_the_loop_when_a_tail_leads_into_it() {
        // 0 → 1 → 2 → 1: node 0 is not on the cycle.
        assert_eq!(
            order(3, |i| match i {
                0 => vec![1],
                1 => vec![2],
                2 => vec![1],
                _ => vec![],
            }),
            Err(vec![vec![1, 2]])
        );
    }

    #[test]
    fn orders_nothing() {
        assert_eq!(order(0, |_| vec![]), Ok(vec![]));
    }
}

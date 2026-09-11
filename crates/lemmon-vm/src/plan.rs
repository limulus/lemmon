//! What a module tells the loader to run: the plan behind `loadData` (∆iCW,
//! `docs/language.md` §Schemas, §Ambient types).
//!
//! A schema's computed defaults and the bindings file's computed bindings are
//! expressions the *loader* evaluates rather than a render — a default sees the record
//! being built, a binding sees the site so far. The compiler lowers each into a
//! function of its own unit and records what it reads; [`link`](crate::link) joins
//! those into the tables here, in the order the reads dictate, and refuses a cycle as
//! it refuses an unresolved import. A field's `by` index is no function: it names one
//! field of one ambient value, and the loader indexes that map as loaded (∆eag). The
//! loader then runs each table front to back and never has to reason about order
//! itself.
//!
//! Everything here is plain data on the [`Module`](crate::Module), so it serialises
//! with the rest of the program.

use std::rc::Rc;

use lemmon_syntax::Span;

/// What the host loads to fulfil a `from` binding: the literal after `from`, by its
/// form (`docs/language.md` §Ambient types).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceKind {
    /// `from "…"`: a glob when the binding's type is a list of the host's document
    /// type, a data file otherwise — the host decides by the type.
    File(Rc<str>),
    /// `from plugin("…")`: a plugin's data hook.
    Plugin(Rc<str>),
    /// `from env("…")`: an environment value, resolved by the binding's type.
    Env(Rc<str>),
}

/// One thing a loader-run function reads that decides when it can run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Read {
    /// A field of the record being built, read through `self`. A computed binding
    /// never makes one: a sibling read there, by bare name or as `site.x`, is an
    /// [`Ambient`](Self::Ambient) read of the value it is a field of.
    Own(Rc<str>),
    /// A field of an ambient value: `site.photos`.
    Ambient { value: Rc<str>, field: Rc<str> },
}

/// One field's `= default`: the function the loader runs when the loaded data omits
/// the field. Its parameters are `self` and, on a record that extends the host's
/// document type, `path` and `body`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub field: Rc<str>,
    /// The function, in the module.
    pub func: u32,
    /// Everything the function reads that the loader must have ready first.
    pub reads: Vec<Read>,
    /// The file the field is declared in, as a module file index — what an error in
    /// the step names.
    pub file: u32,
    /// The field's name in that file.
    pub span: Span,
}

/// One field's `by value.field` index: the map a string the loaded data gives for the
/// field resolves through, named as a field of an ambient value (∆eag). Resolved
/// before any default runs, since a default may read the field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Index {
    pub field: Rc<str>,
    /// The ambient value the map is a field of: `site`.
    pub value: Rc<str>,
    /// The map's name in that value: `photos`.
    pub key: Rc<str>,
    /// The file the field is declared in, as a module file index — what an error in
    /// the lookup names.
    pub file: u32,
    /// The field's name in that file.
    pub span: Span,
}

/// Every step of one declared record, its ancestors' included, in an order that has
/// each field's reads computed before it, and every index of one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordPlan {
    pub name: Rc<str>,
    pub steps: Vec<Step>,
    pub indexes: Vec<Index>,
}

/// How one binding of the bindings file gets its value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fulfilled {
    /// A `from` binding: the host loads it.
    ByHost(SourceKind),
    /// An `= expr` or capture binding: the loader runs the function.
    Computed { func: u32, reads: Vec<Read> },
}

/// One top-level binding of the bindings file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindingStep {
    pub name: Rc<str>,
    pub fulfilled: Fulfilled,
    /// The binding's name in the bindings file.
    pub span: Span,
}

/// The bindings file's plan: the record the ambient value is, and its bindings in an
/// order that has everything a computed one reads — and every ambient field the
/// records a loaded one holds may read while they load — ready first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bindings {
    /// The ambient name the host reads the bindings as: `site`.
    pub value: Rc<str>,
    /// The synthesized record's name — the bindings file's path.
    pub record: Rc<str>,
    /// The bindings file, as a module file index.
    pub file: u32,
    pub steps: Vec<BindingStep>,
}

/// Orders `n` nodes so that every node comes after everything `deps` says it depends
/// on, keeping declaration order wherever the dependencies allow it. `deps(i)` lists
/// the nodes `i` must follow; an index out of range is ignored, and a node that lists
/// itself is a cycle of one — a default that reads the field it computes, a collection
/// whose records read the collection while it loads.
///
/// # Errors
///
/// Every cycle, when the dependencies admit no order: each as the nodes on it, entered
/// from the first node (in declaration order) that leads into it. Every problem is
/// reported rather than the first, as a compile reports every problem in a file.
pub(crate) fn order(
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
    use super::*;

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

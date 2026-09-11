//! Closing what a loader-run function reads through the calls it makes (∆HF0, D37).
//!
//! For the loader's order, a default that calls a `lem-fn` reads whatever that function
//! reads: off the record it was handed, off ambient values, and through its own calls,
//! however deep. The compiler cannot know that — a callee's body is another file's
//! (invariant 11) — so the link step, the first place every unit is in hand, works it
//! out here: per project function, the reads its body makes closed through its calls;
//! then per step, its own reads plus each callee's, with a read off a parameter
//! translated through what the step handed that parameter. Only a call written as
//! `f(…)` is followed. A function named as a value — stored in a field, handed to a
//! builtin — is a value like any other, and a call through a value names nothing.

use std::collections::BTreeMap;
use std::rc::Rc;

use crate::exports::ExportIndex;
use crate::plan::Read;
use crate::unit::{Access, Root, Unit, Uses};

/// One read a step makes once its calls are followed, with the call it arrived
/// through, if any — what a cycle or a refusal names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Closed {
    pub access: Access,
    pub via: Option<Rc<str>>,
}

/// Every project function's reads, closed through its calls.
pub(crate) struct ReadClosure {
    /// By project-flat name. Never an [`Access::Own`]: a `lem-fn` has no `self`.
    functions: BTreeMap<Rc<str>, Vec<Access>>,
}

impl ReadClosure {
    /// Closes every function the index knows, to a fixpoint: a set only grows, and
    /// there are finitely many fields to read off finitely many roots, so a recursive
    /// call — direct or mutual — settles like any other.
    ///
    /// Runs after the link step has refused an unresolved import and a stale
    /// signature: every call a unit recorded names a function the index has, with as
    /// many arguments as that function has parameters.
    pub(crate) fn new(
        units: &[Unit],
        index: &ExportIndex,
        positions: &BTreeMap<&str, usize>,
    ) -> Self {
        let table: Vec<(Rc<str>, &Uses)> = index
            .exports()
            .map(|export| {
                let unit = &units[positions[export.path.as_str()]];
                (
                    export.signature.name.clone(),
                    &unit.uses[&export.signature.func],
                )
            })
            .collect();
        let mut functions: BTreeMap<Rc<str>, Vec<Access>> = table
            .iter()
            .map(|(name, uses)| (name.clone(), uses.reads.clone()))
            .collect();
        loop {
            let mut grew = false;
            for (name, uses) in &table {
                let mut closed = functions[name].clone();
                for call in &uses.calls {
                    for access in through(&functions[&call.name], &call.args) {
                        if !closed.contains(&access) {
                            closed.push(access);
                        }
                    }
                }
                if closed.len() > functions[name].len() {
                    functions.insert(name.clone(), closed);
                    grew = true;
                }
            }
            if !grew {
                return Self { functions };
            }
        }
    }

    /// What one step reads once its calls are followed: its own reads first, then
    /// each callee's translated through what it was handed, each read kept with the
    /// first way it arrived.
    pub(crate) fn step(&self, uses: &Uses) -> Vec<Closed> {
        let mut closed: Vec<Closed> = uses
            .reads
            .iter()
            .map(|access| Closed {
                access: access.clone(),
                via: None,
            })
            .collect();
        for call in &uses.calls {
            for access in through(&self.functions[&call.name], &call.args) {
                if !closed.iter().any(|seen| seen.access == access) {
                    closed.push(Closed {
                        access,
                        via: Some(call.name.clone()),
                    });
                }
            }
        }
        closed
    }
}

/// A callee's closed reads as the caller makes them: a read off the callee's
/// parameter becomes a read off the root the caller handed it, or nothing when the
/// caller handed something else; every other read is the caller's as it stands.
fn through<'a>(
    callee: &'a [Access],
    args: &'a [Option<Root>],
) -> impl Iterator<Item = Access> + 'a {
    callee.iter().filter_map(move |access| match access {
        Access::Param { index, field } => {
            let root = args[*index as usize].as_ref()?;
            Some(match root {
                Root::Own => Access::Own(field.clone()),
                Root::Param(index) => Access::Param {
                    index: *index,
                    field: field.clone(),
                },
                Root::Ambient(value) => Access::Ambient {
                    value: value.clone(),
                    field: field.clone(),
                },
            })
        }
        Access::Own(_) => unreachable!("a `lem-fn` has no `self` to read off"),
        Access::Ambient { .. } | Access::Whole(_) => Some(access.clone()),
    })
}

/// What a step's plan carries of its closed reads: the fields of `self` and of
/// ambient values. A whole ambient value orders nothing — a binding may read the site
/// so far — and a step has no parameter to have read off.
pub(crate) fn reads(closed: &[Closed]) -> Vec<Read> {
    closed
        .iter()
        .filter_map(|closed| match &closed.access {
            Access::Own(field) => Some(Read::Own(field.clone())),
            Access::Ambient { value, field } => Some(Read::Ambient {
                value: value.clone(),
                field: field.clone(),
            }),
            Access::Whole(_) => None,
            Access::Param { .. } => {
                unreachable!("a step reads off `self`, not a parameter")
            }
        })
        .collect()
}

/// The call the first read `matches` arrived through, if it arrived through one —
/// what a cycle names, and what a refusal of a read no function may make names.
pub(crate) fn via(
    closed: &[Closed],
    matches: impl Fn(&Access) -> bool,
) -> Option<&Rc<str>> {
    closed
        .iter()
        .find(|closed| matches(&closed.access))
        .and_then(|closed| closed.via.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn param(index: u32, field: &str) -> Access {
        Access::Param {
            index,
            field: Rc::from(field),
        }
    }

    #[test]
    fn translates_a_parameter_read_through_the_root_handed_it() {
        let callee = [
            param(0, "a"),
            param(1, "b"),
            param(2, "c"),
            Access::Whole(Rc::from("page")),
        ];
        let args = [Some(Root::Own), Some(Root::Ambient(Rc::from("site"))), None];
        let closed: Vec<Access> = through(&callee, &args).collect();
        assert_eq!(
            closed,
            [
                Access::Own(Rc::from("a")),
                Access::Ambient {
                    value: Rc::from("site"),
                    field: Rc::from("b"),
                },
                Access::Whole(Rc::from("page")),
            ]
        );
    }

    #[test]
    fn keeps_a_read_through_a_parameter_a_parameter_of_the_caller() {
        let callee = [param(0, "a")];
        let closed: Vec<Access> = through(&callee, &[Some(Root::Param(3))]).collect();
        assert_eq!(closed, [param(3, "a")]);
    }
}

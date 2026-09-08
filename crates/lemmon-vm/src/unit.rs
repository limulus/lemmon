//! Per-file compiled units and the link step that joins them (D25, invariant 11).
//!
//! A [`Unit`] is one file's compiled output: its own constant pool (document prose
//! included) and its own functions, with every reference to another file's function left
//! symbolic in [`Unit::imports`]. [`link`] is a pass over plain data that concatenates
//! the pools, rewrites the indices, and resolves the symbols — producing the one
//! [`Module`] the VM runs, unchanged in shape. That split is what makes a unit a
//! function of its file's text plus the signatures it consumed ([`Unit::facts`]) and
//! nothing else: no file's bytecode depends on another file's body.

use std::collections::BTreeMap;
use std::rc::Rc;

use lemmon_syntax::Span;

use crate::Value;
use crate::bytecode::{Function, Module, Op};
use crate::compile::{Compiled, Diagnostic, index_u32};
use crate::decls::{Declaration, Decls, Kind, Shape, TypeFact};
use crate::exports::ExportIndex;
use crate::host::Host;
use crate::plan::{
    BindingStep, Bindings, Fulfilled, Read, RecordPlan, Step, StepKind, order,
};
use crate::types::Type;

/// One file's compiled output: self-contained except for the functions it names in
/// other files, which stay symbolic until [`link`] resolves them.
#[derive(Clone, Debug, PartialEq)]
pub struct Unit {
    /// The file this unit was compiled from.
    pub path: String,
    /// The file-local constant pool: literals, text runs, names, and rendered prose.
    pub constants: Vec<Value>,
    /// The file's functions; `functions[0]` is its entry. A `Call`, `Global`, or
    /// `MakeClosure` operand `>= functions.len()` names
    /// `imports[operand - functions.len()]` instead of a function here.
    pub functions: Vec<Function>,
    /// The foreign functions the bytecode references, symbolically.
    pub imports: Vec<ImportRef>,
    /// The top-level `lem-fn`s this file defines, as other files may call them.
    pub exports: Vec<ExportedSignature>,
    /// The top-level `lem-type`s and `lem-enum`s this file declares, as other files
    /// may name them.
    pub types: Vec<Declaration>,
    /// The compile-time facts this unit consumed: the foreign signatures its calls were
    /// compiled against. With the file's text and [`type_refs`](Self::type_refs),
    /// these are the whole cache key (D25) — an edit that changes none of them leaves
    /// this unit byte-identical.
    pub facts: Vec<ImportedSignature>,
    /// The second kind of fact: the foreign types this file's checking depended on,
    /// each with the declaration it was compiled against, at the first use that reached
    /// it — an annotation, an `extends`, or a consumed signature.
    pub type_refs: Vec<TypeRef>,
    /// The third: what the host bound each ambient value this file read to, where it
    /// first read it. The declaration behind such a type is a `TypeRef` like any
    /// other; this is the binding itself — that the host called `page` a `Page` (D30).
    pub ambient_refs: Vec<AmbientRef>,
    /// The record the host compiled this file as, which types its document value: a
    /// fact of the unit like the rest, and the reason two documents of different
    /// schemas cannot share one.
    pub record: Option<String>,
    /// The functions the loader runs for the records this file declares: each field's
    /// default and `by` index, with what it reads (∆iCW). Part of this unit and no
    /// consumer's: a default is not part of a type's shape fact.
    pub defaults: Vec<DefaultFn>,
    /// The top-level bindings of this file when it is the one the host reads an
    /// ambient value from — what fulfils each, in binding order. Empty elsewhere.
    pub bindings: Vec<BindingFn>,
}

/// One loader-run function a unit compiled for a field of a record it declares.
#[derive(Clone, Debug, PartialEq)]
pub struct DefaultFn {
    pub record: Rc<str>,
    pub field: Rc<str>,
    pub kind: StepKind,
    /// The function's index in its unit.
    pub func: u32,
    pub reads: Vec<Read>,
    /// The field's name in the declaration.
    pub span: Span,
}

/// One top-level binding of the bindings file, as compiled: a computed one's `func`
/// is an index in its unit.
#[derive(Clone, Debug, PartialEq)]
pub struct BindingFn {
    pub name: Rc<str>,
    pub fulfilled: Fulfilled,
    /// The binding's name in the file.
    pub span: Span,
}

/// A foreign type a unit was compiled against: where this file first reached it, and
/// the declaration it consumed there.
#[derive(Clone, Debug, PartialEq)]
pub struct TypeRef {
    pub name: Rc<str>,
    pub span: Span,
    pub fact: TypeFact,
}

/// A host binding a unit was compiled against: the ambient name, where this file first
/// read it, and the type it had there.
#[derive(Clone, Debug, PartialEq)]
pub struct AmbientRef {
    pub name: Rc<str>,
    pub span: Span,
    pub ty: Type,
}

/// A symbolic reference to a function or type another file defines.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportRef {
    /// The project-flat name.
    pub name: Rc<str>,
    /// Where this file first uses the name — where an unresolved import points.
    pub span: Span,
}

/// One parameter of a signature: its name, which named arguments reorder against, and
/// its declared type, which arguments are checked against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Param {
    pub name: Rc<str>,
    pub ty: Type,
}

/// A top-level `lem-fn` a unit defines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportedSignature {
    /// The function's project-flat name.
    pub name: Rc<str>,
    /// The parameters, in declaration order.
    pub params: Vec<Param>,
    /// The declared return type: `html` for a block body.
    pub ret: Type,
    /// The function's index in its unit.
    pub func: u32,
    /// The name token in the definition — where a duplicate of it is reported, and what
    /// a duplicate's label points at.
    pub name_span: Span,
}

impl ExportedSignature {
    /// The function's type, as a value of it has.
    #[must_use]
    pub fn ty(&self) -> Type {
        Type::function(
            self.params.iter().map(|p| p.ty.clone()).collect(),
            self.ret.clone(),
        )
    }
}

/// A foreign signature a unit's calls were compiled against: the callable shape only,
/// never the body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportedSignature {
    /// The function's project-flat name.
    pub name: Rc<str>,
    /// The parameters, in declaration order.
    pub params: Vec<Param>,
    /// The declared return type.
    pub ret: Type,
}

/// Joins units into the [`Module`] the VM runs: constant pools concatenated, function
/// indices offset, imports resolved to the functions `index` says define them.
///
/// The index is the project's one answer to what a name means (`∆e5i`), so linking
/// derives nothing about names itself: it decides only where each unit's functions land.
/// An import the index has no definition for is an error pointing at the source that
/// names it, in the order the units were given; with any error there is no module. A
/// type a unit was compiled against is checked the same way: one the index no longer
/// declares, or declares differently, leaves the unit stale — its field reads and
/// assignability checks were made against the old shape — and the error names the
/// file to recompile.
///
/// # Panics
///
/// If the index names a file that is not among `units` — the caller is expected to link
/// every unit the index was offered.
pub fn link(units: &[Unit], index: &ExportIndex, host: &Host) -> Compiled<Module> {
    let mut diagnostics = Vec::new();
    let mut constant_offsets = Vec::with_capacity(units.len());
    let mut function_offsets = Vec::with_capacity(units.len());
    let (mut constants, mut functions) = (0u32, 0u32);
    for unit in units {
        constant_offsets.push(constants);
        function_offsets.push(functions);
        constants = add_u32(constants, index_u32(unit.constants.len()));
        functions = add_u32(functions, index_u32(unit.functions.len()));
    }

    let positions: BTreeMap<&str, usize> = units
        .iter()
        .enumerate()
        .map(|(position, unit)| (unit.path.as_str(), position))
        .collect();

    let resolved: Vec<Vec<Option<u32>>> = units
        .iter()
        .map(|unit| {
            // What the host binds each ambient name to is a fact like a signature: a
            // unit that read `page` as a `Page` has its field reads and assignability
            // checks made against that type, so a host that rebinds the name — or drops
            // it — leaves the unit stale rather than merely relinkable. Compared
            // against the host as *this file* saw it, since a file compiled as a record
            // reads the document value as that record.
            let seen = host.for_file(&unit.path, unit.record.as_deref());
            for reference in &unit.ambient_refs {
                let bound = seen.binding(&reference.name).map(|a| a.ty());
                match bound {
                    Some(ty) if ty == reference.ty => {}
                    Some(ty) => diagnostics.push(Diagnostic::error(
                        &unit.path,
                        reference.span,
                        format!(
                            "the host binds `{}` as a `{ty}` now, and this file was \
                             compiled against `{}`",
                            reference.name, reference.ty
                        ),
                    )),
                    None => diagnostics.push(Diagnostic::error(
                        &unit.path,
                        reference.span,
                        format!("the host no longer binds `{}`", reference.name),
                    )),
                }
            }
            for reference in &unit.type_refs {
                match index.lookup_type(&reference.name) {
                    Some(export) if export.decl.fact() == reference.fact => {}
                    Some(export) => diagnostics.push(
                        Diagnostic::error(
                            &unit.path,
                            reference.span,
                            stale_type_message(&export.decl.fact(), &reference.fact),
                        )
                        .with_label(
                            &export.path,
                            export.decl.name_span,
                            "now declared here",
                        ),
                    ),
                    None => diagnostics.push(Diagnostic::error(
                        &unit.path,
                        reference.span,
                        format!("unknown type `{}`", reference.name),
                    )),
                }
            }
            unit.imports
                .iter()
                .map(|import| {
                    match index.lookup(&import.name) {
                        Some(export) => {
                            let owner = positions[export.path.as_str()];
                            let export = &export.signature;
                            // The signature this unit was compiled against is one of its
                            // facts (D25). If the definition no longer matches it, the
                            // caller's bytecode is stale — named arguments were already
                            // reordered against the old parameter list, and every
                            // argument and result was checked against the old types —
                            // so this unit has to be recompiled, not relinked. Reporting
                            // it here is what tells the shell which file that is.
                            let fact = unit
                                .facts
                                .iter()
                                .find(|fact| fact.name == import.name)
                                .expect("an import records the signature it consumed");
                            if fact.params != export.params || fact.ret != export.ret {
                                diagnostics.push(
                                    Diagnostic::error(
                                        &unit.path,
                                        import.span,
                                        stale_signature_message(
                                            &import.name,
                                            (&fact.params, &fact.ret),
                                            (&export.params, &export.ret),
                                        ),
                                    )
                                    .with_label(
                                        &units[owner].path,
                                        export.name_span,
                                        "now defined here",
                                    ),
                                );
                            }
                            Some(add_u32(function_offsets[owner], export.func))
                        }
                        None => {
                            diagnostics.push(Diagnostic::error(
                                &unit.path,
                                import.span,
                                format!("undefined name `{}`", import.name),
                            ));
                            None
                        }
                    }
                })
                .collect()
        })
        .collect();
    // An unresolved import is always an error, so past this point every entry of
    // `resolved` is `Some`; a warning alone builds the module like a clean link does.
    if diagnostics.iter().any(Diagnostic::is_error) {
        return Compiled::new(None, diagnostics);
    }

    // Every record the project declares, whichever unit it came from, with its chain
    // closed here: a type test asks the table at run time, so the table is the module's
    // and not any unit's, and the walk over declarations happens once rather than per
    // test.
    let types = index
        .types()
        .filter(|export| matches!(export.decl.shape, Shape::Record { .. }))
        .map(|export| {
            let chain = index
                .chain(&export.decl.name)
                .iter()
                .map(|decl| decl.name.clone())
                .collect();
            (export.decl.name.clone(), chain)
        })
        .collect();
    let places = Places {
        units,
        index,
        positions: &positions,
        function_offsets: &function_offsets,
    };
    let records = places.record_plans(&mut diagnostics);
    let bindings = places.bindings_plans(host, &records, &mut diagnostics);
    // A cycle among what the loader runs is as fatal as an unresolved import: the
    // program could never load.
    if diagnostics.iter().any(Diagnostic::is_error) {
        return Compiled::new(None, diagnostics);
    }
    let exports = index
        .exports()
        .map(|export| {
            let owner = positions[export.path.as_str()];
            (
                export.signature.name.clone(),
                add_u32(function_offsets[owner], export.signature.func),
            )
        })
        .collect();
    let mut module = Module {
        types,
        exports,
        records,
        bindings,
        ..Module::default()
    };
    for (position, unit) in units.iter().enumerate() {
        let (constant_offset, function_offset) =
            (constant_offsets[position], function_offsets[position]);
        let locals = index_u32(unit.functions.len());
        let func = |func: u32| {
            if func < locals {
                add_u32(function_offset, func)
            } else {
                resolved[position][(func - locals) as usize].expect("resolved above")
            }
        };
        module.files.push(unit.path.clone());
        module.entries.push(function_offset);
        module.constants.extend(unit.constants.iter().cloned());
        for function in &unit.functions {
            let mut function = function.clone();
            function.file = index_u32(position);
            for op in &mut function.code {
                match op {
                    Op::Const { index } | Op::EmitText { index } => {
                        *index = add_u32(constant_offset, *index);
                    }
                    Op::Ambient { name }
                    | Op::GetField { name }
                    | Op::OptField { name }
                    | Op::CallDynamic { name, .. }
                    | Op::MakeRecord { name, .. }
                    | Op::TypeTest { name } => {
                        *name = add_u32(constant_offset, *name);
                    }
                    Op::Global { func: target }
                    | Op::Call { func: target, .. }
                    | Op::MakeClosure { func: target, .. } => *target = func(*target),
                    // Spelled out rather than `_`: an op that names a unit-local table
                    // and is missed here reads another file's constants or functions,
                    // and only in a module with more than one file — which no
                    // single-file corpus case ever builds. A new op must be placed.
                    Op::Local { .. }
                    | Op::StoreLocal { .. }
                    | Op::Index
                    | Op::CallValue { .. }
                    | Op::CallBuiltin { .. }
                    | Op::MakeList { .. }
                    | Op::MakeMap { .. }
                    | Op::Jump { .. }
                    | Op::JumpIfFalse { .. }
                    | Op::JumpUnlessNull { .. }
                    | Op::JumpIfFalseKeep { .. }
                    | Op::JumpIfTrueKeep { .. }
                    | Op::IterInit { .. }
                    | Op::IterNext { .. }
                    | Op::Neg
                    | Op::Not
                    | Op::Add
                    | Op::Sub
                    | Op::Mul
                    | Op::Div
                    | Op::Rem
                    | Op::Eq
                    | Op::Ne
                    | Op::Lt
                    | Op::Le
                    | Op::Gt
                    | Op::Ge
                    | Op::In
                    | Op::Emit
                    | Op::Capture
                    | Op::EndCapture
                    | Op::Return => {}
                }
            }
            module.functions.push(function);
        }
    }
    Compiled::new(Some(module), diagnostics)
}

fn add_u32(a: u32, b: u32) -> u32 {
    a.checked_add(b).expect("fewer than 2^32 items")
}

/// Where each unit's functions land in the module — what turns a unit's loader-run
/// functions into the module's plans.
struct Places<'a> {
    units: &'a [Unit],
    index: &'a ExportIndex,
    positions: &'a BTreeMap<&'a str, usize>,
    function_offsets: &'a [u32],
}

impl Places<'_> {
    /// A unit-local function index, in the module.
    fn func(&self, position: usize, func: u32) -> u32 {
        add_u32(self.function_offsets[position], func)
    }

    /// The loader's plan for every declared record — the bindings record included,
    /// whose plan is empty since its fields are bindings. Each field's steps come from
    /// the nearest declaration in the chain that gives it any: a redeclaration changes
    /// the default (D32), and one that gives none leaves the ancestor's in force. The
    /// steps are relocated into the module and ordered by what each reads through
    /// `self`; a field that reads itself through others is a link error at the field,
    /// since no order could run it.
    fn record_plans(&self, diagnostics: &mut Vec<Diagnostic>) -> Vec<RecordPlan> {
        let mut plans = Vec::new();
        for export in self.index.types() {
            let name = &export.decl.name;
            if export.decl.kind() != Kind::Record {
                continue;
            }
            let mut steps: Vec<Step> = Vec::new();
            let mut seen: Vec<Rc<str>> = Vec::new();
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
                for field in fields {
                    if seen.contains(&field.name) {
                        continue;
                    }
                    let rows: Vec<&DefaultFn> = unit
                        .defaults
                        .iter()
                        .filter(|row| row.record == decl.name && row.field == field.name)
                        .collect();
                    if rows.is_empty() {
                        continue;
                    }
                    seen.push(field.name.clone());
                    for row in rows {
                        steps.push(Step {
                            field: row.field.clone(),
                            kind: row.kind,
                            func: self.func(position, row.func),
                            reads: row.reads.clone(),
                            file: index_u32(position),
                            span: row.span,
                        });
                    }
                }
            }
            let deps = |i: usize| -> Vec<usize> {
                steps[i]
                    .reads
                    .iter()
                    .flat_map(|read| {
                        let Read::Own(field) = read else {
                            return Vec::new();
                        };
                        steps
                            .iter()
                            .enumerate()
                            .filter(|(_, step)| step.field == *field)
                            .map(|(j, _)| j)
                            .collect()
                    })
                    .collect()
            };
            let steps = ordered(&steps, deps, |cycle| {
                let first = &steps[cycle[0]];
                diagnostics.push(Diagnostic::error(
                    &self.units[first.file as usize].path,
                    first.span,
                    format!(
                        "`{}` depends on itself: {}",
                        first.field,
                        chain(cycle, |i| steps[i].field.clone(), "reads")
                    ),
                ));
            });
            plans.push(RecordPlan {
                name: name.clone(),
                steps,
            });
        }
        plans
    }

    /// A plan per ambient value the host reads from a file the program has, in the
    /// host's order.
    fn bindings_plans(
        &self,
        host: &Host,
        records: &[RecordPlan],
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Vec<Bindings> {
        host.bindings_files()
            .filter_map(|(value, file)| {
                self.bindings_plan(value, file, records, diagnostics)
            })
            .collect()
    }

    /// The plan for the bindings file `file`, read as `value`: its bindings in load
    /// order. A computed binding follows the siblings it reads; a `from` binding
    /// follows every sibling the records its value may hold read while they load —
    /// through `extends` and through field types, since a subtype's or a nested
    /// record's default runs too. A binding that needs itself is a link error at the
    /// binding. `None` when the file is not in the program.
    fn bindings_plan(
        &self,
        value: &str,
        file: &str,
        records: &[RecordPlan],
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<Bindings> {
        let &position = self.positions.get(file)?;
        let unit = &self.units[position];
        let steps: Vec<BindingStep> = unit
            .bindings
            .iter()
            .map(|binding| BindingStep {
                name: binding.name.clone(),
                fulfilled: match &binding.fulfilled {
                    Fulfilled::Computed { func, reads } => Fulfilled::Computed {
                        func: self.func(position, *func),
                        reads: reads.clone(),
                    },
                    by_host @ Fulfilled::ByHost(_) => by_host.clone(),
                },
                span: binding.span,
            })
            .collect();
        let sibling = |field: &str| steps.iter().position(|step| &*step.name == field);
        // Only a read of this value's own field orders its bindings. A record step's
        // `Own` read names a field of that record, which a binding's name may happen
        // to match and has nothing to do with.
        let of_value = |read: &Read| match read {
            Read::Ambient { value: read, field } if **read == *value => sibling(field),
            Read::Own(_) | Read::Ambient { .. } => None,
        };
        let deps = |i: usize| -> Vec<usize> {
            match &steps[i].fulfilled {
                Fulfilled::Computed { reads, .. } => {
                    reads.iter().filter_map(of_value).collect()
                }
                Fulfilled::ByHost(_) => {
                    let Some(ty) = self.index.field(file, &steps[i].name) else {
                        return Vec::new();
                    };
                    self.records_reachable(&ty)
                        .iter()
                        .filter_map(|record| {
                            records.iter().find(|plan| plan.name == *record)
                        })
                        .flat_map(|plan| plan.steps.iter().flat_map(|step| &step.reads))
                        .filter_map(of_value)
                        .collect()
                }
            }
        };
        let steps = ordered(&steps, deps, |cycle| {
            let first = &steps[cycle[0]];
            diagnostics.push(Diagnostic::error(
                &unit.path,
                first.span,
                format!(
                    "`{}` cannot be loaded before itself: {}",
                    first.name,
                    chain(cycle, |i| steps[i].name.clone(), "needs")
                ),
            ));
        });
        Some(Bindings {
            value: Rc::from(value),
            record: Rc::from(file),
            file: index_u32(position),
            steps,
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
            if seen.contains(&name) || self.index.kind(&name) != Some(Kind::Record) {
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
fn chain(cycle: &[usize], name: impl Fn(usize) -> Rc<str>, verb: &str) -> String {
    let links: Vec<String> = cycle
        .iter()
        .zip(cycle.iter().cycle().skip(1))
        .map(|(&a, &b)| format!("`{}` {verb} `{}`", name(a), name(b)))
        .collect();
    links.join(", ")
}

/// What a caller is told when the function it was compiled against has changed shape
/// under it: both signatures, so the difference is visible without opening the callee.
fn stale_signature_message(
    name: &str,
    compiled: (&[Param], &Type),
    defined: (&[Param], &Type),
) -> String {
    format!(
        "`{name}` is now `{}`, but this file was compiled against `{}` — recompile it",
        signature(name, defined.0, defined.1),
        signature(name, compiled.0, compiled.1)
    )
}

/// What a consumer is told when the type it was compiled against has changed shape
/// under it: both shapes, so the difference is visible without opening the declaration.
fn stale_type_message(declared: &TypeFact, compiled: &TypeFact) -> String {
    format!(
        "`{}` is now `{}`, but this file was compiled against `{}` — recompile it",
        declared.name,
        declared.text(),
        compiled.text()
    )
}

/// A signature the way it is declared: `name(a: string, b: int): html`.
fn signature(name: &str, params: &[Param], ret: &Type) -> String {
    let params: Vec<String> = params
        .iter()
        .map(|p| format!("{}: {}", p.name, p.ty))
        .collect();
    format!("{name}({}): {ret}", params.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::{compile_unit, prepare};
    use crate::host::Ambient;
    use crate::{SourceFile, Value, render};

    /// The host every test below compiles against unless it needs another: the three
    /// names the standard shell binds, none of them typed.
    fn untyped_host() -> Host {
        Host::untyped(["site", "page", "build"])
    }

    /// Compiles the file at `path` into a unit against an index of every given file's
    /// exports.
    fn unit_of(path: &str, index_of: &[(&str, &str)]) -> Unit {
        let prepared: Vec<_> = index_of
            .iter()
            .map(|(path, text)| {
                prepare(&SourceFile::new(*path, *text), &untyped_host()).unwrap()
            })
            .collect();
        let mut index = ExportIndex::new();
        for p in &prepared {
            index.offer(p.path(), p.exports());
            index.offer_types(p.path(), p.types());
        }
        let this = prepared
            .iter()
            .find(|p| p.path() == path)
            .expect("the file compiles against an index that includes it");
        compile_unit(this, &index, &untyped_host()).expect_output()
    }

    /// Compiles the file at `path` against `host`, and against an index of every given
    /// file's exports.
    fn unit_against(host: &Host, path: &str, files: &[(&str, &str)]) -> Unit {
        let prepared: Vec<_> = files
            .iter()
            .map(|(path, text)| prepare(&SourceFile::new(*path, *text), host).unwrap())
            .collect();
        let mut index = ExportIndex::new();
        for p in &prepared {
            index.offer(p.path(), p.exports());
            index.offer_types(p.path(), p.types());
        }
        let this = prepared
            .iter()
            .find(|p| p.path() == path)
            .expect("the file compiles against an index that includes it");
        compile_unit(this, &index, host).expect_output()
    }

    /// Links `units` against the index they make, asserting there is no module and
    /// returning what was reported.
    fn link_errors(units: &[Unit]) -> Vec<Diagnostic> {
        let linked = link(units, &ExportIndex::of(units), &untyped_host());
        assert!(
            linked.output.is_none(),
            "linked despite {:?}",
            linked.diagnostics
        );
        linked.diagnostics
    }

    #[test]
    fn refuses_to_link_an_import_no_unit_exports() {
        // The unit compiled against a table that knows lib.lem's function, then linked
        // without lib.lem — the shape of a stale unit after a file disappears.
        let page = unit_of(
            "page.lem",
            &[
                ("lib.lem", "<lem-fn f(): int = 1 />"),
                ("page.lem", "<lem.f()>"),
            ],
        );
        let errors = link_errors(&[page]);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "page.lem");
        assert!(
            errors[0].to_string().contains("undefined name `f`"),
            "{errors:?}"
        );
    }

    #[test]
    fn refuses_to_link_a_unit_whose_callee_changed_shape_under_it() {
        // page.lem compiled against `pair(a, b)` — its named arguments are already in
        // that order — while lib.lem now defines `pair(b, a)`. Resolving by name alone
        // would link cleanly and silently swap the arguments.
        let page = unit_of(
            "page.lem",
            &[
                (
                    "lib.lem",
                    "<lem-fn pair(a: string, b: string): string = a + b />",
                ),
                ("page.lem", "<lem.pair(b: \"B\", a: \"A\")>"),
            ],
        );
        let lib = unit_of(
            "lib.lem",
            &[(
                "lib.lem",
                "<lem-fn pair(b: string, a: string): string = a + b />",
            )],
        );
        let errors = link_errors(&[lib, page]);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "page.lem");
        assert_eq!(
            errors[0].to_string(),
            "page.lem:5..25: `pair` is now `pair(b: string, a: string): string`, but this \
             file was compiled against `pair(a: string, b: string): string` — recompile \
             it\n  lib.lem:8..12: now defined here"
        );
    }

    #[test]
    fn refuses_to_link_a_unit_whose_callee_changed_a_type_under_it() {
        // The caller checked its argument against `int` and printed the result as a
        // `string`; both were facts, and either changing makes the unit stale.
        let against = |lib: &str| {
            unit_of("page.lem", &[("lib.lem", lib), ("page.lem", "<lem.f(1)>")])
        };
        let page = against("<lem-fn f(a: int): string = \"s\" />");
        let lib = unit_of("lib.lem", &[("lib.lem", "<lem-fn f(a: int): int = a />")]);
        let errors = link_errors(&[lib, page.clone()]);
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0].message.contains("`f` is now `f(a: int): int`"),
            "{errors:?}"
        );
        let lib = unit_of(
            "lib.lem",
            &[("lib.lem", "<lem-fn f(a: any): string = \"s\" />")],
        );
        let errors = link_errors(&[lib, page]);
        assert!(
            errors[0].message.contains("`f` is now `f(a: any): string`"),
            "{errors:?}"
        );
    }

    #[test]
    fn refuses_to_link_a_unit_whose_callee_changed_arity_under_it() {
        let page = unit_of(
            "page.lem",
            &[
                ("lib.lem", "<lem-fn greet(who: string): string = who />"),
                ("page.lem", "<lem.greet(\"you\")>"),
            ],
        );
        let lib = unit_of(
            "lib.lem",
            &[(
                "lib.lem",
                "<lem-fn greet(who: string, loudly: bool): string = who />",
            )],
        );
        let errors = link_errors(&[lib, page]);
        assert_eq!(errors[0].path, "page.lem");
        assert!(
            errors[0]
                .message
                .contains("`greet(who: string, loudly: bool): string`"),
            "{errors:?}"
        );
    }

    #[test]
    fn links_a_unit_whose_callee_only_changed_its_body() {
        // The other half of the rule: a body edit is exactly what units exist to
        // survive, so relinking against it must stay clean (invariant 11).
        let page = unit_of(
            "page.lem",
            &[
                (
                    "lib.lem",
                    "<lem-fn greet(who: string): string = \"hi \" + who />",
                ),
                ("page.lem", "<lem.greet(\"you\")>"),
            ],
        );
        let lib = unit_of(
            "lib.lem",
            &[(
                "lib.lem",
                "<lem-fn greet(who: string): string = \"HELLO \" + who />",
            )],
        );
        let units = [lib, page];
        let module =
            link(&units, &ExportIndex::of(&units), &untyped_host()).expect_output();
        assert_eq!(
            render(&module, "page.lem", &[("page", Value::Null)]).unwrap(),
            "HELLO you"
        );
    }

    #[test]
    fn relocates_a_dynamic_calls_name_into_the_linked_constant_pool() {
        // `x.f(…)` on an `any` carries its method name as a constant index, which only
        // a second unit's offset can show is relocated: every single-file module links
        // at offset zero.
        let files = [
            ("lib.lem", "<lem-fn f(): string = \"first\" + \"second\" />"),
            (
                "page.lem",
                "<lem-let u: any = \"Hello There\" /><lem.u.slugify()> <lem.f()>",
            ),
        ];
        let lib = unit_of("lib.lem", &files);
        let page = unit_of("page.lem", &files);
        let units = [lib, page];
        let module =
            link(&units, &ExportIndex::of(&units), &untyped_host()).expect_output();
        assert_eq!(
            render(&module, "page.lem", &[("page", Value::Null)]).unwrap(),
            "hello-there firstsecond"
        );
    }

    #[test]
    fn relocates_a_record_literals_and_a_type_tests_name_into_the_linked_pool() {
        // Both ops carry a constant index, which only a second unit's offset can show
        // is relocated: every single-file module — every corpus case — links at offset
        // zero. Without the relocation the record is tagged with, and the test asks
        // about, whatever string sits at that index in the first unit's pool.
        let files = [
            (
                "types.lem",
                "<lem-type Photo>\n  url: string\n</lem-type>\
                 <lem-type Framed extends Photo>\n  frame: string\n</lem-type>\
                 <lem-fn pad(): string = \"first\" + \"second\" />",
            ),
            (
                "page.lem",
                "<lem-let framed: Framed = {url: \"/p.jpg\", frame: \"oak\"} />\
                 <lem-let photos: [Photo] = [framed] />\
                 <lem-for p in photos>\
                 <lem-if p is Framed><lem.p.frame><lem-else />plain</lem-if>\
                 </lem-for>",
            ),
        ];
        let units = [unit_of("types.lem", &files), unit_of("page.lem", &files)];
        let module =
            link(&units, &ExportIndex::of(&units), &untyped_host()).expect_output();
        assert_eq!(
            render(&module, "page.lem", &[("page", Value::Null)]).unwrap(),
            "oak"
        );
    }

    #[test]
    fn records_a_type_that_only_a_type_test_names() {
        // `x is T` is a place a foreign type is written with no annotation behind it to
        // have recorded it. Without the fact, dropping the declaration would leave this
        // unit fresh and its `Op::TypeTest` asking about a type the project no longer
        // has.
        fn files(types: &str) -> [(&str, &str); 2] {
            [
                ("types.lem", types),
                // `LinkPost` is named nowhere else here: `a` is annotated `Article`.
                (
                    "page.lem",
                    "<lem-let a: Article = {title: \"T\"} />\
                     <lem-if a is LinkPost>l</lem-if>",
                ),
            ]
        }
        let both = "<lem-type Article>\n  title: string\n</lem-type>\
                    <lem-type LinkPost extends Article>\n  link: string\n</lem-type>";
        let page = unit_of("page.lem", &files(both));
        let names: Vec<&str> = page.type_refs.iter().map(|r| &*r.name).collect();
        assert!(names.contains(&"LinkPost"), "{names:?}");
        let changed = "<lem-type Article>\n  title: string\n</lem-type>\
                       <lem-type LinkPost extends Article>\n  href: string\n</lem-type>";
        let types = unit_of("types.lem", &[("types.lem", changed)]);
        let errors = link_errors(&[types, page]);
        assert!(
            errors[0].message.starts_with("`LinkPost` is now"),
            "{errors:?}"
        );
    }

    #[test]
    fn tables_every_records_base_from_whichever_unit_declares_it() {
        let files = [
            (
                "types.lem",
                "<lem-type Page>\n  url: string\n</lem-type>\
                 <lem-type Article extends Page>\n  title: string\n</lem-type>\
                 <lem-enum Placement = hero | left />",
            ),
            (
                "page.lem",
                "<lem-type LinkPost extends Article>\n  link: string\n</lem-type>x",
            ),
        ];
        let units = [unit_of("types.lem", &files), unit_of("page.lem", &files)];
        let module =
            link(&units, &ExportIndex::of(&units), &untyped_host()).expect_output();
        let mut types: Vec<String> = module
            .types
            .iter()
            .map(|(name, chain)| {
                let chain: Vec<&str> = chain.iter().map(|a| &**a).collect();
                format!("{name}: {}", chain.join(", "))
            })
            .collect();
        types.sort();
        assert_eq!(
            types,
            [
                "Article: Article, Page",
                "LinkPost: LinkPost, Article, Page",
                "Page: Page",
            ]
        );
        assert!(module.is_a("LinkPost", "Page"));
    }

    #[test]
    fn relinks_the_same_units_into_the_same_module() {
        // Linking is a pure pass over plain data: the boundary relinks resident units
        // after every compile, so the result must not depend on when it runs.
        let files = [
            ("lib.lem", "<lem-fn f(): int = 40 + 2 />"),
            ("page.lem", "a<lem.f()>b"),
        ];
        let lib = unit_of("lib.lem", &files);
        let page = unit_of("page.lem", &files);
        let units = [lib, page];
        let index = ExportIndex::of(&units);
        let module = link(&units, &index, &untyped_host()).expect_output();
        assert_eq!(
            module,
            link(&units, &index, &untyped_host()).expect_output()
        );
        assert_eq!(
            render(&module, "page.lem", &[("page", Value::Null)]).unwrap(),
            "a42b"
        );
    }

    const ARTICLE: &str =
        "<lem-type Article>\n  title: string\n  date: datetime\n</lem-type>";

    #[test]
    fn refuses_to_link_a_unit_whose_type_changed_shape_under_it() {
        // page.lem read `a.title` as a `string` and printed it; with `title` now an
        // `int`, or gone, that check no longer holds and the unit must be recompiled.
        let page = unit_of(
            "page.lem",
            &[
                ("types.lem", ARTICLE),
                ("page.lem", "<lem-fn card(a: Article): string = a.title />"),
            ],
        );
        for (edit, expect) in [
            (
                "<lem-type Article>\n  title: int\n</lem-type>",
                "page.lem:16..23: `Article` is now `Article {title: int}`, but this file \
                 was compiled against `Article {title: string, date: datetime}` — \
                 recompile it\n  types.lem:10..17: now declared here",
            ),
            (
                "<lem-type Photo>\n  alt: string\n</lem-type>",
                "page.lem:16..23: unknown type `Article`",
            ),
        ] {
            let types = unit_of("types.lem", &[("types.lem", edit)]);
            let errors = link_errors(&[types, page.clone()]);
            assert_eq!(errors.len(), 1, "{errors:?}");
            assert_eq!(errors[0].to_string(), expect);
        }
    }

    #[test]
    fn links_a_unit_whose_type_only_changed_its_defaults() {
        // A default runs at load, never at compile, so it is not part of the shape a
        // consumer depends on — the other half of the rule, and invariant 11 for
        // declarations: the consumer's unit is byte-identical either way, and only
        // the declaring unit carries the new function.
        let against = |types: &str| {
            unit_of(
                "page.lem",
                &[
                    ("types.lem", types),
                    ("page.lem", "<lem-fn card(a: Article): string = a.title />"),
                ],
            )
        };
        let page = against(ARTICLE);
        let defaulted =
            "<lem-type Article>\n  title: string = \"T\"\n  date: datetime\n</lem-type>";
        let edited = against(defaulted);
        assert_eq!(page, edited);
        let plain = unit_of("types.lem", &[("types.lem", ARTICLE)]);
        let types = unit_of("types.lem", &[("types.lem", defaulted)]);
        assert!(plain.defaults.is_empty());
        assert_eq!(types.defaults.len(), 1);
        let units = [types, page];
        let module =
            link(&units, &ExportIndex::of(&units), &untyped_host()).expect_output();
        let article = module
            .records
            .iter()
            .find(|plan| &*plan.name == "Article")
            .expect("a plan per record");
        assert_eq!(article.steps.len(), 1);
        assert_eq!(&*article.steps[0].field, "title");
        assert_eq!(article.steps[0].kind, StepKind::Default);
        assert_eq!(
            module.functions[article.steps[0].func as usize].name,
            "Article.title"
        );
        assert_eq!(&module.files[article.steps[0].file as usize], "types.lem");
    }

    /// A host shaped like the standard shell's, reading `site` from `site.lem`.
    fn shell_host() -> Host {
        Host::new()
            .bind("page", Ambient::named("Page"))
            .bind("site", Ambient::bindings("site.lem"))
            .documents("page")
            .rendered_through("layout")
    }

    /// Every file compiled against `host` and linked: the module, or what stopped it.
    fn linked(host: &Host, files: &[(&str, &str)]) -> Compiled<Module> {
        let units: Vec<Unit> = files
            .iter()
            .map(|(path, _)| unit_against(host, path, files))
            .collect();
        link(&units, &ExportIndex::of(&units), host)
    }

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
            &shell_host(),
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
            &shell_host(),
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
        let linked = linked(
            &untyped_host(),
            &[(
                "types.lem",
                "<lem-type A>\n  ok: string = \"x\"\n  a: string = self.b\n  \
                 b: string = self.a\n  c: string = self.c\n</lem-type>",
            )],
        );
        assert!(linked.output.is_none());
        assert_eq!(
            shown_all(&linked.diagnostics),
            [
                "types.lem:34..35: `a` depends on itself: `a` reads `b`, `b` reads `a`",
                "types.lem:76..77: `c` depends on itself: `c` reads `c`",
            ]
        );
    }

    fn shown_all(diagnostics: &[Diagnostic]) -> Vec<String> {
        diagnostics.iter().map(ToString::to_string).collect()
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
            &shell_host(),
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
    fn plans_a_loaded_binding_after_the_fields_its_records_read_while_loading() {
        // `posts` holds `Article`s and their subtypes, and an `Article` holds a
        // `Photo`: a default on any of them that reads `site.captions` or
        // `site.authors` puts `posts` after both, however the file orders them.
        let module = linked(
            &shell_host(),
            &[
                ("ambient.lem", "<lem-type Page>\n  url: url\n</lem-type>"),
                (
                    "site.lem",
                    "<lem-let posts: [Article] from \"feed/*.md\" />\n\
                     <lem-let authors: {string: string} from \"authors.json\" />\n\
                     <lem-let captions: {string: string} from \"captions.json\" />\n\
                     <lem-let count: int = 1 />",
                ),
                (
                    "schema.lem",
                    "<lem-type Photo>\n  caption: string = site.captions[\"x\"]\n\
                     </lem-type>\n\
                     <lem-type Article extends Page>\n  hero: Photo?\n  \
                     layout: (Article, html): html\n</lem-type>\n\
                     <lem-type LinkPost extends Article>\n  \
                     via: string = site.authors[\"e\"]\n</lem-type>",
                ),
            ],
        )
        .expect_output();
        assert_eq!(
            binding_names(&module),
            ["authors", "captions", "posts", "count"]
        );
    }

    #[test]
    fn refuses_to_link_a_binding_that_needs_itself() {
        // A default on `Article` reads `site.posts`, the collection the article is
        // being loaded into.
        let linked = linked(
            &shell_host(),
            &[
                ("ambient.lem", "<lem-type Page>\n  url: url\n</lem-type>"),
                (
                    "site.lem",
                    "<lem-let posts: [Article] from \"feed/*.md\" />\n\
                     <lem-let latest: [Article] = posts.filter(|p| true) />",
                ),
                (
                    "schema.lem",
                    "<lem-type Article extends Page>\n  \
                     related: [Article] = site.posts.filter(|p| p != self)\n  \
                     layout: (Article, html): html\n</lem-type>",
                ),
            ],
        );
        assert!(linked.output.is_none());
        assert_eq!(
            shown_all(&linked.diagnostics),
            [
                "site.lem:9..14: `posts` cannot be loaded before itself: `posts` needs `posts`"
            ]
        );
    }

    #[test]
    fn plans_no_bindings_without_a_bindings_file_in_the_program() {
        let module = linked(&untyped_host(), &[("a.lem", "x")]).expect_output();
        assert!(module.bindings.is_empty());
        // The host reads `site.lem`, which has not been compiled.
        let module = linked(&shell_host(), &[("a.lem", "x")]).expect_output();
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
    fn orders_a_loaded_binding_by_its_records_ambient_reads_alone() {
        // `Article.slug` reads `self.title`, and the bindings file has a binding named
        // `title` too: a record's own field is not a sibling, so `posts` does not wait
        // for `title` — which itself reads `posts`, and would otherwise make a cycle.
        let module = linked(
            &shell_host(),
            &[
                ("ambient.lem", "<lem-type Page>\n  url: url\n</lem-type>"),
                (
                    "site.lem",
                    "<lem-let posts: [Article] from \"feed/*.md\" />\n\
                     <lem-let title: string = posts.filter(|p| true)[0].title />",
                ),
                (
                    "schema.lem",
                    "<lem-type Article extends Page>\n  title: string\n  \
                     slug: string = self.title\n  layout: (Article, html): html\n\
                     </lem-type>",
                ),
            ],
        )
        .expect_output();
        assert_eq!(binding_names(&module), ["posts", "title"]);
    }

    #[test]
    fn tables_every_project_function_by_name() {
        let module = linked(
            &untyped_host(),
            &[
                ("a.lem", "<lem-fn f(): int = 1 />"),
                ("b.lem", "<lem-fn g(): int = 2 /><lem-fn h(): int = 3 />"),
            ],
        )
        .expect_output();
        let names: Vec<(&str, &str)> = module
            .exports
            .iter()
            .map(|(name, func)| (&**name, module.functions[*func as usize].name.as_str()))
            .collect();
        assert_eq!(names, [("f", "f"), ("g", "g"), ("h", "h")]);
    }

    #[test]
    fn records_an_enum_a_qualified_variant_named() {
        // `Placement.hero` is the one place an enum's name is written with no
        // annotation behind it to have recorded it (∆EN6). Without the fact, dropping
        // the variant would leave this unit fresh and its `Op::Const` a variant the
        // enum no longer has.
        fn files(types: &str) -> [(&str, &str); 2] {
            [
                ("types.lem", types),
                // No annotation anywhere: the qualified variant is the only thing in
                // this file that names the enum, so it is the only thing that can
                // have recorded it.
                ("page.lem", "<lem.Placement.hero>"),
            ]
        }
        let both = "<lem-enum Placement = hero | left />";
        let page = unit_of("page.lem", &files(both));
        let names: Vec<&str> = page.type_refs.iter().map(|r| &*r.name).collect();
        assert_eq!(names, ["Placement"]);
        let dropped = "<lem-enum Placement = left />";
        let types = unit_of("types.lem", &[("types.lem", dropped)]);
        let errors = link_errors(&[types, page]);
        assert!(
            errors[0]
                .message
                .starts_with("`Placement` is now `Placement = left`"),
            "{errors:?}"
        );
    }

    #[test]
    fn records_a_type_reached_only_through_another_types_chain() {
        // page.lem names `LinkPost` alone; checking it against `Article` walked the
        // chain, so `Article`'s declaration is a fact too, and a change to it — say a
        // `LinkPost` that no longer extends it — is caught at link time.
        fn files(types: &str) -> [(&str, &str); 2] {
            [
                ("types.lem", types),
                (
                    "page.lem",
                    "<lem-fn card(a: Article): string = a.title />\n\
                     <lem-fn link(l: LinkPost): string = card(l) />",
                ),
            ]
        }
        let chained = "<lem-type Article>\n  title: string\n</lem-type>\n\
                       <lem-type LinkPost extends Article>\n  link: url\n</lem-type>";
        let page = unit_of("page.lem", &files(chained));
        let names: Vec<&str> = page.type_refs.iter().map(|r| &*r.name).collect();
        assert_eq!(names, ["Article", "LinkPost"]);
        let severed = "<lem-type Article>\n  title: string\n</lem-type>\n\
                       <lem-type LinkPost>\n  link: url\n</lem-type>";
        let types = unit_of("types.lem", &[("types.lem", severed)]);
        let errors = link_errors(&[types, page]);
        assert!(
            errors[0]
                .message
                .starts_with("`LinkPost` is now `LinkPost {link: url}`"),
            "{errors:?}"
        );
    }

    #[test]
    fn reports_a_units_stale_types_beside_its_stale_imports() {
        // One pass per unit, so a file's diagnostics stay together and in span order
        // whatever kind they are.
        let page = unit_of(
            "page.lem",
            &[
                (
                    "lib.lem",
                    "<lem-type Article>\n  title: string\n</lem-type>\n\
                     <lem-fn f(s: string): string = s />",
                ),
                (
                    "page.lem",
                    "<lem-fn card(a: Article): string = f(a.title) />\n\
                     <lem-fn other(): string = f(\"x\") />",
                ),
            ],
        );
        let lib = unit_of(
            "lib.lem",
            &[(
                "lib.lem",
                "<lem-type Article>\n  title: int\n</lem-type>\n\
                 <lem-fn f(s: int): string = \"s\" />",
            )],
        );
        let errors = link_errors(&[lib, page]);
        let starts: Vec<u32> = errors.iter().map(|e| e.span.start).collect();
        assert_eq!(starts, [16, 35]);
    }

    #[test]
    fn refuses_to_link_a_unit_the_host_rebound_an_ambient_name_under() {
        // page.lem read `page.url` as a `Page`'s field; a host that now calls `page`
        // something else checked that read against a declaration this unit never saw.
        let files = [
            ("ambient.lem", "<lem-type Page>\n  url: url\n</lem-type>"),
            ("page.lem", "<lem.page.url>"),
        ];
        let host = Host::new().bind("page", Ambient::named("Page"));
        let unit = unit_against(&host, "page.lem", &files);
        let ambient = unit_against(&host, "ambient.lem", &files);
        for (rebound, expect) in [
            (
                Host::new().bind("page", Ambient::named("Photo")),
                "page.lem:5..9: the host binds `page` as a `Photo` now, and this file \
                 was compiled against `Page`",
            ),
            (
                Host::new(),
                "page.lem:5..9: the host no longer binds `page`",
            ),
        ] {
            let units = [ambient.clone(), unit.clone()];
            let linked = link(&units, &ExportIndex::of(&units), &rebound);
            assert_eq!(
                linked
                    .diagnostics
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
                [expect]
            );
            assert!(linked.output.is_none());
        }
    }

    #[test]
    fn links_a_document_against_the_host_as_that_document_saw_it() {
        // The document value is typed by the file's own record, so the binding the host
        // declares for it is not what this unit was compiled against — and comparing
        // the two would call every document stale.
        let files = [
            ("ambient.lem", "<lem-type Page>\n  url: url\n</lem-type>"),
            (
                "schema.lem",
                "<lem-type Post extends Page>\n  title: string\n  \
                 layout: (Post, html): html\n</lem-type>",
            ),
            ("post.md", "<lem.page.title>"),
        ];
        let host = Host::new()
            .bind("page", Ambient::named("Page"))
            .documents("page")
            .rendered_through("layout");
        let prepared: Vec<_> = files
            .iter()
            .map(|(path, text)| {
                let mut file = SourceFile::new(*path, *text);
                if *path == "post.md" {
                    file.record = Some("Post".to_owned());
                }
                prepare(&file, &host).unwrap()
            })
            .collect();
        let mut index = ExportIndex::new();
        for p in &prepared {
            index.offer(p.path(), p.exports());
            index.offer_types(p.path(), p.types());
        }
        let units: Vec<Unit> = prepared
            .iter()
            .map(|p| compile_unit(p, &index, &host).expect_output())
            .collect();
        assert!(link(&units, &index, &host).output.is_some());
    }
}

//! Joining units into the [`Module`] the VM runs (D25, invariant 11).
//!
//! [`link`] checks each unit's compile-time facts against what the index and the host
//! say now: a unit whose facts have moved under it is stale rather than relinkable, and
//! the error names the file to recompile. It also checks the two fields the host
//! declaration names — where a page goes and what renders it — against what the project
//! declares, since neither is a fact of any unit (D43). With every check passed it
//! concatenates the constant pools, rebases the operands that index them, resolves each
//! import to the function the index says defines it, and asks [`plans`](plans()) for the
//! tables the loader runs. Building those can fail the link too: a cycle among what the
//! loader runs, or a page template whose record's plan reads `body` (D43).

mod closure;
mod plans;

use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;

use lemmon_syntax::Span;

use crate::bytecode::{Declared, Document, Exported, MarkKey, MarkPayload, Module, Op};
use crate::compile::index_u32;
use crate::decls::{
    Declaration, Decls, EntryFault, TypeFact, entry_fault, misshaped_entry_field,
    no_entry_field,
};
use crate::diagnostics::{Compiled, Diagnostic};
use crate::exports::ExportIndex;
use crate::host::{ClaimAnswer, Host};
use crate::types::Type;
use crate::unit::{ClaimRef, Param, Unit};

use plans::plans;

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
            let seen = host.for_file(unit.page_shaped, unit.record.as_deref());
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
            // A manifest entry is the same kind of fact, and the one that moves most
            // often: every rebuild of an asset re-hashes its URL, and the URL is inside
            // this unit's bytecode. Compared against the declaration rather than the
            // file's view, since an asset is the host's and no file retypes one.
            for reference in &unit.asset_refs {
                match host.asset(&reference.path) {
                    Some(url) if url == &*reference.url => {}
                    Some(url) => diagnostics.push(Diagnostic::error(
                        &unit.path,
                        reference.span,
                        format!(
                            "the manifest maps `{}` to `{url}` now, and this file was \
                             compiled against `{}`",
                            reference.path, reference.url
                        ),
                    )),
                    None => diagnostics.push(Diagnostic::error(
                        &unit.path,
                        reference.span,
                        format!("`{}` is no longer in the manifest", reference.path),
                    )),
                }
            }
            // A claim is the host's as a manifest entry is, and what it said was
            // answered while the file compiled: the warning at the word, the warning at
            // a key, and the record a payload was checked against.
            for reference in &unit.claim_refs {
                let now = host.answer(
                    &reference.name,
                    reference.key.as_ref().map(|(key, _)| &**key),
                );
                if now == reference.answer {
                    continue;
                }
                if let Some((message, span)) = stale_claim(reference, &now) {
                    diagnostics.push(Diagnostic::error(&unit.path, span, message));
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
    diagnostics.extend(check_host_fields(units, index, host));
    // An unresolved import is always an error, so past this point every entry of
    // `resolved` is `Some`; a warning alone builds the module like a clean link does.
    if diagnostics.iter().any(Diagnostic::is_error) {
        return Compiled::new(None, diagnostics);
    }

    // Every type the project declares, whichever unit it came from: a type test asks
    // the table at run time and the loader validates against it, so the table is the
    // module's and not any unit's.
    let declarations = index
        .types()
        .map(|export| Declared {
            file: index_u32(positions[export.path.as_str()]),
            decl: export.decl.clone(),
        })
        .collect();
    let (records, bindings) = plans(
        units,
        index,
        host,
        &positions,
        &function_offsets,
        &mut diagnostics,
    );
    // A cycle among what the loader runs is as fatal as an unresolved import: the
    // program could never load.
    if diagnostics.iter().any(Diagnostic::is_error) {
        return Compiled::new(None, diagnostics);
    }
    let exports = index
        .exports()
        .map(|export| {
            let owner = positions[export.path.as_str()];
            Exported {
                name: export.signature.name.clone(),
                func: add_u32(function_offsets[owner], export.signature.func),
                ty: export.signature.ty(),
            }
        })
        .collect();
    let mut module = Module {
        declarations,
        exports,
        records,
        bindings,
        ..Module::default()
    };
    let mut payload_ids: HashSet<Rc<str>> = HashSet::new();
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
        module.documents.push(unit.page_shaped.then(|| Document {
            record: unit.record.as_deref().map(Rc::from),
            record_span: unit.record_span,
            body: unit.body.clone(),
        }));
        // Two documents writing the same block report one payload between them: the id
        // is the content, and the host keys what its producer made on it. Kept by a set,
        // since a relink walks every unit's payloads and must stay cheaper than a compile.
        for payload in &unit.payloads {
            if payload_ids.insert(payload.id.clone()) {
                module.payloads.push(payload.clone());
            }
        }
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
                    | Op::TypeTest { name }
                    | Op::Collect { name } => {
                        *name = add_u32(constant_offset, *name);
                    }
                    // Three operands rather than one: a mark's key and its payload name
                    // this file's constants as its name does.
                    Op::Mark { name, key, payload } => {
                        *name = add_u32(constant_offset, *name);
                        match key {
                            MarkKey::Const { index } | MarkKey::Field { index } => {
                                *index = add_u32(constant_offset, *index);
                            }
                            MarkKey::None => {}
                        }
                        match payload {
                            MarkPayload::Reported { index } => {
                                *index = add_u32(constant_offset, *index);
                            }
                            MarkPayload::None | MarkPayload::Popped => {}
                        }
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

/// The two fields the host declaration names, against what the project declares
/// (D43): the document type declares the url field as `url`, and the record of every
/// page declares the entry field as a layout of it. Neither name is a fact of any unit,
/// so a compile that changes only the declaration relinks every unit as it is, and
/// only here — with the index and the host in hand on every link — is the new name
/// seen. Reported where the compiler would report the same thing: at the type that
/// lacks the field, or at the field declared with the wrong type.
fn check_host_fields(units: &[Unit], index: &ExportIndex, host: &Host) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    // A document type no file declares is the compiler's to report, at the first read
    // of the value (D30).
    let Some((document, declared)) = host
        .document_type()
        .and_then(|document| index.lookup_type(&document).map(|d| (document, d)))
    else {
        return diagnostics;
    };
    let file_of = |decl: &Declaration| {
        index
            .lookup_type(&decl.name)
            .expect("a chain walks declared types")
            .path
            .clone()
    };
    if let Some(url) = host.url() {
        match index.field_in(&document, url) {
            None => diagnostics.push(Diagnostic::error(
                &declared.path,
                declared.decl.name_span,
                format!(
                    "`{document}` is the host's document type, so it declares where a \
                     page goes: `{url}: url`"
                ),
            )),
            Some((owner, field)) if field.ty != Type::Url => {
                diagnostics.push(Diagnostic::error(
                    file_of(owner),
                    field.span,
                    format!("`{url}` is where a page goes: declare it `{url}: url`"),
                ));
            }
            Some(_) => {}
        }
    }
    if let Some(entry) = host.entry() {
        let mut records: Vec<&str> = Vec::new();
        for unit in units {
            let Some(record) = unit.record.as_deref().filter(|_| unit.page_shaped) else {
                continue;
            };
            if records.contains(&record) {
                continue;
            }
            records.push(record);
            let Some(export) = index.lookup_type(record) else {
                continue;
            };
            match entry_fault(index, record, entry) {
                Some(EntryFault::Undeclared) => diagnostics.push(Diagnostic::error(
                    &export.path,
                    export.decl.name_span,
                    no_entry_field(record, &document, entry),
                )),
                Some(EntryFault::Misshaped(owner, field)) => {
                    diagnostics.push(Diagnostic::error(
                        file_of(owner),
                        field.span,
                        misshaped_entry_field(&owner.name, entry),
                    ));
                }
                None => {}
            }
        }
    }
    diagnostics
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

/// Why `reference` is stale under the answer the declaration gives `now`, and where to
/// say so: at the key where the key's coverage moved, at the name otherwise.
///
/// The comparison is ordered as a reader would ask it — whether the name is handled at
/// all, then whether this key is covered, then what the payload is — and the first
/// difference is the one reported, so one changed claim costs one diagnostic. `None`
/// where the two answers are equal, which the caller has already ruled out.
fn stale_claim(reference: &ClaimRef, now: &ClaimAnswer) -> Option<(String, Span)> {
    let name = &reference.name;
    let at_name = |message: String| Some((message, reference.span));
    let was = &reference.answer;
    match (was, now) {
        (ClaimAnswer::Claimed { .. }, ClaimAnswer::Claimed { .. }) => {}
        (ClaimAnswer::Claimed { .. }, _) => {
            return at_name(format!(
                "no handler claims `{name}` now, and this file was compiled where one did"
            ));
        }
        (_, ClaimAnswer::Claimed { .. }) => {
            return at_name(format!(
                "a handler claims `{name}` now, and this file was compiled where none did"
            ));
        }
        (ClaimAnswer::Undeclared, _) => {
            return at_name(format!(
                "the host declares handlers now, and this file was compiled against \
                 none, so nothing was said about `{name}`"
            ));
        }
        _ => {
            return at_name(format!(
                "the host declares no handlers now, and this file was compiled against \
                 the ones it did, which left `{name}` unclaimed"
            ));
        }
    }
    let (
        ClaimAnswer::Claimed {
            key: was_key,
            payload: was_payload,
            keyed_by: was_keyed_by,
        },
        ClaimAnswer::Claimed {
            key: now_key,
            payload: now_payload,
            keyed_by: now_keyed_by,
        },
    ) = (was, now)
    else {
        unreachable!("both answers are claims here");
    };
    if was_key != now_key {
        // A key is only asked about where the file wrote one, so there is a span for it.
        let (key, span) = reference
            .key
            .as_ref()
            .expect("a key answer comes of asking about a key");
        let message = match (was_key, now_key) {
            (_, Some(true)) => format!(
                "a producer claims `{key}` for `{name}` now, and this file was compiled \
                 where none did"
            ),
            (_, Some(false)) => format!(
                "no producer claims `{key}` for `{name}` now, and this file was compiled \
                 where one did"
            ),
            _ => format!(
                "the handlers for `{name}` claim no keys now, and this file was compiled \
                 against the keys they did"
            ),
        };
        return Some((message, *span));
    }
    if was_payload != now_payload {
        return at_name(match (was_payload, now_payload) {
            (Some(was), Some(now)) => format!(
                "the handler for `{name}` takes a `{now}` payload now, and this file was \
                 compiled against `{was}`"
            ),
            (None, Some(now)) => format!(
                "the handler for `{name}` takes a `{now}` payload now, and this file was \
                 compiled where it took none"
            ),
            (Some(was), None) => format!(
                "the handler for `{name}` takes no payload now, and this file was \
                 compiled against a `{was}`"
            ),
            (None, None) => unreachable!("the payloads differ"),
        });
    }
    at_name(match (was_keyed_by, now_keyed_by) {
        (Some(was), Some(now)) => format!(
            "the handler for `{name}` keys its payload by `{now}` now, and this file was \
             compiled against `{was}`"
        ),
        (None, Some(now)) => format!(
            "the handler for `{name}` keys its payload by `{now}` now, and this file was \
             compiled where it keyed by nothing"
        ),
        (Some(was), None) => format!(
            "the handler for `{name}` keys its payload by nothing now, and this file was \
             compiled against `{was}`"
        ),
        (None, None) => unreachable!("an answer that differs in nothing is not stale"),
    })
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
    use crate::host::{Ambient, Claim};
    use crate::testing::{
        AMBIENT, POST, addressed_host, at, linked, shell_host, shown, unit_against,
        unit_of, untyped_host,
    };
    use crate::{SourceFile, Value, render};

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
        const PAGE: &str = "<lem.pair(b: \"B\", a: \"A\")>";
        const LIB: &str = "<lem-fn pair(b: string, a: string): string = a + b />";
        let page = unit_of(
            "page.lem",
            &[
                (
                    "lib.lem",
                    "<lem-fn pair(a: string, b: string): string = a + b />",
                ),
                ("page.lem", PAGE),
            ],
        );
        let lib = unit_of("lib.lem", &[("lib.lem", LIB)]);
        let errors = link_errors(&[lib, page]);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "page.lem");
        assert_eq!(
            errors[0].to_string(),
            format!(
                "page.lem:{}: `pair` is now `pair(b: string, a: string): string`, but \
                 this file was compiled against `pair(a: string, b: string): string` — \
                 recompile it\n  lib.lem:{}: now defined here",
                at(PAGE, "pair(b: \"B\", a: \"A\")"),
                at(LIB, "pair")
            )
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
            render(&module, "page.lem", &[("page", Value::Null)])
                .unwrap()
                .html,
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
            render(&module, "page.lem", &[("page", Value::Null)])
                .unwrap()
                .html,
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
            render(&module, "page.lem", &[("page", Value::Null)])
                .unwrap()
                .html,
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
            .declarations
            .iter()
            .map(|declared| {
                let name = &declared.decl.name;
                let chain: Vec<&str> =
                    module.chain(name).iter().map(|d| &*d.name).collect();
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
                "Placement: Placement",
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
            render(&module, "page.lem", &[("page", Value::Null)])
                .unwrap()
                .html,
            "a42b"
        );
    }

    const ARTICLE: &str =
        "<lem-type Article>\n  title: string\n  date: datetime\n</lem-type>";

    #[test]
    fn refuses_to_link_a_unit_whose_type_changed_shape_under_it() {
        // page.lem read `a.title` as a `string` and printed it; with `title` now an
        // `int`, or gone, that check no longer holds and the unit must be recompiled.
        const PAGE: &str = "<lem-fn card(a: Article): string = a.title />";
        const NARROWED: &str = "<lem-type Article>\n  title: int\n</lem-type>";
        let page = unit_of("page.lem", &[("types.lem", ARTICLE), ("page.lem", PAGE)]);
        for (edit, expect) in [
            (
                NARROWED,
                format!(
                    "page.lem:{}: `Article` is now `Article {{title: int}}`, but this \
                     file was compiled against `Article {{title: string, date: \
                     datetime}}` — recompile it\n  types.lem:{}: now declared here",
                    at(PAGE, "Article"),
                    at(NARROWED, "Article")
                ),
            ),
            (
                "<lem-type Photo>\n  alt: string\n</lem-type>",
                format!("page.lem:{}: unknown type `Article`", at(PAGE, "Article")),
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
        assert_eq!(
            module.functions[article.steps[0].func as usize].name,
            "Article.title"
        );
        assert_eq!(&module.files[article.steps[0].file as usize], "types.lem");
    }

    /// `ambient` and `POST` with a document of `Post`, compiled as units against
    /// `host` — what a relink under another declaration links again as they are.
    fn post_units(host: &Host, ambient: &str) -> Vec<Unit> {
        let files = [
            SourceFile::new("ambient.lem", ambient),
            SourceFile::new("site.lem", POST),
            SourceFile::new("d.md", "---\ntitle: D\n---\nprose").with_record("Post"),
        ];
        let prepared: Vec<_> = files
            .iter()
            .map(|file| prepare(file, host).unwrap())
            .collect();
        let mut index = ExportIndex::new();
        for p in &prepared {
            index.offer(p.path(), p.exports());
            index.offer_types(p.path(), p.types());
        }
        prepared
            .iter()
            .map(|p| compile_unit(p, &index, host).expect_output())
            .collect()
    }

    /// The units of [`post_units`], compiled against `compiled_with` and linked
    /// against `linked_with`, as strings.
    fn relinked(compiled_with: &Host, linked_with: &Host, ambient: &str) -> Vec<String> {
        let units = post_units(compiled_with, ambient);
        shown(&link(&units, &ExportIndex::of(&units), linked_with).diagnostics)
    }

    #[test]
    fn refuses_a_document_type_that_declares_no_url_field() {
        const NO_URL: &str = "<lem-type Page>\n  draft: bool = false\n</lem-type>";
        assert_eq!(
            relinked(&addressed_host(), &addressed_host(), NO_URL),
            [format!(
                "ambient.lem:{}: `Page` is the host's document type, so it declares \
                 where a page goes: `url: url`",
                at(NO_URL, "Page")
            )]
        );
    }

    #[test]
    fn refuses_a_url_field_that_is_no_url() {
        const STRING_URL: &str = "<lem-type Page>\n  url: string\n</lem-type>";
        assert_eq!(
            relinked(&addressed_host(), &addressed_host(), STRING_URL),
            [format!(
                "ambient.lem:{}: `url` is where a page goes: declare it `url: url`",
                at(STRING_URL, "«url»: string")
            )]
        );
    }

    #[test]
    fn checks_no_url_field_for_a_host_that_names_none() {
        // A host that places no pages has nothing to read a url from, so a document
        // type without one is what it declares.
        const NO_URL: &str = "<lem-type Page>\n  draft: bool = false\n</lem-type>";
        let unplaced = Host::new()
            .bind("page", Ambient::named("Page"))
            .bind("site", Ambient::bindings("site.lem"))
            .documents("page")
            .rendered_through("layout");
        assert_eq!(relinked(&unplaced, &unplaced, NO_URL), Vec::<String>::new());
    }

    #[test]
    fn catches_a_renamed_field_on_a_relink_that_recompiled_nothing() {
        // The units compiled clean against a host whose entry is `layout` and whose
        // url field is `url`. The host now names `template` and `permalink`, and the
        // compile installing that declaration relinks every unit as it is: neither
        // name is a fact of any unit, so only the link step can see the change.
        let renamed = shell_host()
            .rendered_through("template")
            .addressed_by("permalink");
        assert_eq!(
            relinked(&addressed_host(), &renamed, AMBIENT),
            [
                format!(
                    "ambient.lem:{}: `Page` is the host's document type, so it \
                     declares where a page goes: `permalink: url`",
                    at(AMBIENT, "Page")
                ),
                format!(
                    "site.lem:{}: `Post` extends `Page`, so it declares how a page \
                     renders: `template: (Post, html): html`",
                    at(POST, "lem-type «Post»")
                ),
            ]
        );
        // An entry field the record has, of the wrong shape: refused where it is
        // declared, in the compiler's words.
        assert_eq!(
            relinked(
                &addressed_host(),
                &addressed_host().rendered_through("title"),
                AMBIENT,
            ),
            [format!(
                "site.lem:{}: `title` renders a `Post`: declare it \
                 `title: (Post, html): html`",
                at(POST, "«title»: string")
            )]
        );
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
        let names: Vec<(&str, &str, String)> = module
            .exports
            .iter()
            .map(|export| {
                (
                    &*export.name,
                    module.functions[export.func as usize].name.as_str(),
                    export.ty.to_string(),
                )
            })
            .collect();
        assert_eq!(
            names,
            [
                ("f", "f", "(): int".to_owned()),
                ("g", "g", "(): int".to_owned()),
                ("h", "h", "(): int".to_owned()),
            ]
        );
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
        const PAGE: &str = "<lem-fn card(a: Article): string = f(a.title) />\n\
                            <lem-fn other(): string = f(\"x\") />";
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
                ("page.lem", PAGE),
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
        assert_eq!(
            starts,
            [at(PAGE, "Article").start, at(PAGE, "f(a.title)").start]
        );
    }

    /// A host with one asset built, which is what the asset tests compile against.
    fn built() -> Host {
        Host::new().with_assets([("styles/site.css", "/site.abc123.css")])
    }

    #[test]
    fn compiles_a_unit_that_names_an_asset_to_the_same_bytes_under_the_same_manifest() {
        // Invariant 11's first half: the fact is only worth recording if consuming it
        // is stable. Two compiles against one manifest are one unit.
        const TEMPLATE: &str = "<lem.asset(\"styles/site.css\")>";
        let files = [("page.lem", TEMPLATE)];
        let once = unit_against(&built(), "page.lem", &files);
        let twice = unit_against(&built(), "page.lem", &files);
        assert_eq!(once.constants, twice.constants);
        assert_eq!(once.functions, twice.functions);
        assert_eq!(
            once.asset_refs
                .iter()
                .map(|reference| (&*reference.path, &*reference.url))
                .collect::<Vec<_>>(),
            [("styles/site.css", "/site.abc123.css")]
        );
    }

    #[test]
    fn refuses_to_link_a_unit_the_manifest_has_moved_the_asset_under() {
        // The other half, and the one that earns the fact: the URL is inside this
        // unit's bytecode, so a rebuild that re-hashes the asset leaves it stale. A
        // file that named no asset is untouched by the same rebuild.
        const TEMPLATE: &str = "<lem.asset(\"styles/site.css\")>";
        let files = [("page.lem", TEMPLATE)];
        let unit = unit_against(&built(), "page.lem", &files);
        for (rebuilt, expect) in [
            (
                Host::new().with_assets([("styles/site.css", "/site.def456.css")]),
                format!(
                    "page.lem:{}: the manifest maps `styles/site.css` to \
                     `/site.def456.css` now, and this file was compiled against \
                     `/site.abc123.css`",
                    at(TEMPLATE, "\"styles/site.css\"")
                ),
            ),
            (
                Host::new(),
                format!(
                    "page.lem:{}: `styles/site.css` is no longer in the manifest",
                    at(TEMPLATE, "\"styles/site.css\"")
                ),
            ),
        ] {
            let units = [unit.clone()];
            let linked = link(&units, &ExportIndex::of(&units), &rebuilt);
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
    fn refuses_to_link_a_file_whose_marks_the_host_claims_differently_now() {
        // What the claims said was answered while the file compiled — the warning at the
        // word, and with ∆7wK a payload's type — so any change to what they say about a
        // name this file marks leaves it stale, at that name.
        const SRC: &str = "<lem.mark(\"styles\")>";
        let files = [("page.lem", SRC)];
        let claimed = Host::untyped(["page"]).with_marks(["styles"]);
        let other = Host::untyped(["page"]).with_marks(["head"]);
        let none = Host::untyped(["page"]);
        for (compiled, now, message) in [
            (
                &claimed,
                &other,
                "no handler claims `styles` now, and this file was compiled where one did",
            ),
            (
                &other,
                &claimed,
                "a handler claims `styles` now, and this file was compiled where none did",
            ),
            (
                &none,
                &other,
                "the host declares handlers now, and this file was compiled against none, \
                 so nothing was said about `styles`",
            ),
            (
                &other,
                &none,
                "the host declares no handlers now, and this file was compiled against \
                 the ones it did, which left `styles` unclaimed",
            ),
        ] {
            let units = [unit_against(compiled, "page.lem", &files)];
            let linked = link(&units, &ExportIndex::of(&units), now);
            assert_eq!(
                linked
                    .diagnostics
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
                [format!("page.lem:{}: {message}", at(SRC, "\"styles\""))]
            );
        }
        // Unchanged claims link, and one name marked twice is one fact.
        const TWICE: &str = "<lem.mark(\"styles\")><lem.mark(\"styles\")>";
        let unit = unit_against(&claimed, "page.lem", &[("page.lem", TWICE)]);
        assert_eq!(unit.claim_refs.len(), 1);
        let units = [unit];
        assert!(
            link(&units, &ExportIndex::of(&units), &claimed)
                .output
                .is_some()
        );
    }

    /// A host whose handlers claim `code` for `keys`, with the payload record a mark of
    /// that name carries and the field it is keyed by.
    fn claiming(keys: &[&str], payload: &str) -> Host {
        Host::untyped(["site", "page", "build"])
            .with_claims([(
                "code",
                Claim::new()
                    .with_keys(keys.iter().copied())
                    .with_payload(payload, Some("language")),
            )])
            .expect("claims that agree")
    }

    /// A file declaring the payload record and marking `code` under one written key.
    const KEYED: &str = "<lem-type Code>\n  language: string?\n</lem-type>\n\
                         <lem.mark(\"code\", {language: \"bash\"})>";

    #[test]
    fn refuses_to_link_a_file_whose_key_the_host_claims_differently_now() {
        // What a per-key fact buys: the answer at `bash` was consumed while the file
        // compiled — it decided the warning — so a producer arriving for it, or dropping
        // it, leaves this file stale, and the message names the key.
        let host = claiming(&["css"], "Code");
        let unit = unit_against(&host, "page.lem", &[("page.lem", KEYED)]);
        for (now, expect) in [
            (
                claiming(&["bash", "css"], "Code"),
                "a producer claims `bash` for `code` now, and this file was compiled \
                 where none did",
            ),
            (
                claiming(&["css"], "Snippet"),
                "the handler for `code` takes a `Snippet` payload now, and this file was \
                 compiled against `Code`",
            ),
        ] {
            let units = [unit.clone()];
            let linked = link(&units, &ExportIndex::of(&units), &now);
            assert_eq!(
                linked
                    .diagnostics
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
                [format!(
                    "page.lem:{}: {expect}",
                    if expect.starts_with("a producer") {
                        at(KEYED, "\"bash\"")
                    } else {
                        at(KEYED, "mark(«\"code\"»")
                    }
                )],
                "{expect}"
            );
        }
        // The converse, and the half a digest could not give: a producer arriving for
        // another language leaves this file alone.
        let units = [unit];
        assert!(
            link(
                &units,
                &ExportIndex::of(&units),
                &claiming(&["css", "rust"], "Code")
            )
            .diagnostics
            .is_empty()
        );
    }

    #[test]
    fn records_one_claim_fact_for_each_name_and_key_a_file_asks_about() {
        // One fact per pair, however often the file asks: two marks of one name and key
        // are one question, and a second key is a second one.
        const TWICE: &str = "<lem-type Code>\n  language: string?\n</lem-type>\n\
                             <lem.mark(\"code\", {language: \"bash\"})>\
                             <lem.mark(\"code\", {language: \"bash\"})>\
                             <lem.mark(\"code\", {language: \"css\"})>";
        let unit = unit_against(
            &claiming(&["bash", "css"], "Code"),
            "page.lem",
            &[("page.lem", TWICE)],
        );
        assert_eq!(
            unit.claim_refs
                .iter()
                .map(|reference| (
                    &*reference.name,
                    reference.key.as_ref().map(|(key, _)| &**key)
                ))
                .collect::<Vec<_>>(),
            [("code", Some("bash")), ("code", Some("css"))]
        );
    }

    #[test]
    fn refuses_to_link_a_document_whose_language_a_producer_claims_now() {
        // A fence asks what a `mark()` with a written key asks, so a highlighter that
        // starts covering its language leaves exactly that document stale, the message
        // at the word — as does a host that starts declaring handlers at all. A producer
        // arriving for another language leaves it alone.
        const DOC: &str = "```sh\necho\n```\n";
        for (compiled, message) in [
            (
                claiming(&["css"], "Code"),
                "a producer claims `sh` for `code` now, and this file was compiled where \
                 none did",
            ),
            (
                Host::new(),
                "a handler claims `code` now, and this file was compiled where none did",
            ),
        ] {
            let units = [unit_against(&compiled, "post.md", &[("post.md", DOC)])];
            let linked = link(
                &units,
                &ExportIndex::of(&units),
                &claiming(&["css", "sh"], "Code"),
            );
            assert_eq!(
                linked
                    .diagnostics
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
                [format!("post.md:{}: {message}", at(DOC, "sh"))]
            );
        }
        let units = [unit_against(
            &claiming(&["css"], "Code"),
            "post.md",
            &[("post.md", DOC)],
        )];
        assert!(
            link(
                &units,
                &ExportIndex::of(&units),
                &claiming(&["css", "rust"], "Code")
            )
            .diagnostics
            .is_empty()
        );
    }

    #[test]
    fn compiles_a_document_whose_code_names_no_language_the_same_under_any_claims() {
        // Invariant 11 for claims: a block with no language asks no producer anything,
        // so no change to the claims touches its unit or its link — and it is still a
        // mark, with its payload reported, either way.
        const DOC: &str = "Plain:\n\n```\necho\n```\n\n    indented\n";
        let files = [("post.md", DOC)];
        let without = unit_against(&Host::new(), "post.md", &files);
        let with = unit_against(&claiming(&["sh"], "Code"), "post.md", &files);
        assert_eq!(without, with);
        assert!(with.claim_refs.is_empty());
        assert_eq!(with.payloads.len(), 2);
        let units = [with];
        assert!(
            link(&units, &ExportIndex::of(&units), &Host::new())
                .output
                .is_some()
        );
    }

    #[test]
    fn reports_one_payload_for_the_same_block_in_two_documents() {
        // The id is the content, so the module keeps one payload for a block two
        // documents both write, and each document's unit keeps its own.
        const DOC: &str = "```sh\necho\n```\n";
        let host = claiming(&["sh"], "Code");
        let units = [
            unit_against(&host, "a.md", &[("a.md", DOC)]),
            unit_against(&host, "b.md", &[("b.md", DOC)]),
        ];
        assert_eq!(units[0].payloads, units[1].payloads);
        let module = link(&units, &ExportIndex::of(&units), &host).expect_output();
        assert_eq!(module.payloads, units[0].payloads);
    }

    #[test]
    fn records_one_fact_however_many_times_a_file_names_the_same_asset() {
        const TEMPLATE: &str =
            "<lem.asset(\"styles/site.css\")><lem.asset(\"styles/site.css\")>";
        let unit = unit_against(&built(), "page.lem", &[("page.lem", TEMPLATE)]);
        assert_eq!(unit.asset_refs.len(), 1);
    }

    #[test]
    fn refuses_to_link_a_unit_the_host_rebound_an_ambient_name_under() {
        const TEMPLATE: &str = "<lem.page.url>";
        // page.lem read `page.url` as a `Page`'s field; a host that now calls `page`
        // something else checked that read against a declaration this unit never saw.
        let files = [("ambient.lem", AMBIENT), ("page.lem", TEMPLATE)];
        let host = Host::new().bind("page", Ambient::named("Page"));
        let unit = unit_against(&host, "page.lem", &files);
        let ambient = unit_against(&host, "ambient.lem", &files);
        for (rebound, expect) in [
            (
                Host::new().bind("page", Ambient::named("Photo")),
                format!(
                    "page.lem:{}: the host binds `page` as a `Photo` now, and this file \
                     was compiled against `Page`",
                    at(TEMPLATE, "«page».url")
                ),
            ),
            (
                Host::new(),
                format!(
                    "page.lem:{}: the host no longer binds `page`",
                    at(TEMPLATE, "«page».url")
                ),
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
            ("ambient.lem", AMBIENT),
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

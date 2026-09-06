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
use crate::exports::ExportIndex;

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
    /// The compile-time facts this unit consumed: the foreign signatures its calls were
    /// compiled against. With the file's text, these are the whole cache key (D25) — an
    /// edit that changes neither leaves this unit byte-identical.
    pub facts: Vec<ImportedSignature>,
}

/// A symbolic reference to a function another file defines.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportRef {
    /// The function's project-flat name.
    pub name: Rc<str>,
    /// Where this file first uses the name — where an unresolved import points.
    pub span: Span,
}

/// A top-level `lem-fn` a unit defines.
#[derive(Clone, Debug, PartialEq)]
pub struct ExportedSignature {
    /// The function's project-flat name.
    pub name: Rc<str>,
    /// Parameter names, in declaration order — what named arguments reorder against.
    pub params: Vec<Rc<str>>,
    /// The function's index in its unit.
    pub func: u32,
    /// The name token in the definition — where a duplicate of it is reported, and what
    /// a duplicate's label points at.
    pub name_span: Span,
}

/// A foreign signature a unit's calls were compiled against: the callable shape only,
/// never the body.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportedSignature {
    /// The function's project-flat name.
    pub name: Rc<str>,
    /// Parameter names, in declaration order.
    pub params: Vec<Rc<str>>,
}

/// Joins units into the [`Module`] the VM runs: constant pools concatenated, function
/// indices offset, imports resolved to the functions `index` says define them.
///
/// The index is the project's one answer to what a name means (`∆e5i`), so linking
/// derives nothing about names itself: it decides only where each unit's functions land.
/// An import the index has no definition for is an error pointing at the source that
/// names it, in the order the units were given; with any error there is no module.
///
/// # Panics
///
/// If the index names a file that is not among `units` — the caller is expected to link
/// every unit the index was offered.
pub fn link(units: &[Unit], index: &ExportIndex) -> Compiled<Module> {
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
                            // reordered against the old parameter list — so this unit
                            // has to be recompiled, not relinked. Reporting it here is
                            // what tells the shell which file that is.
                            let fact = unit
                                .facts
                                .iter()
                                .find(|fact| fact.name == import.name)
                                .expect("an import records the signature it consumed");
                            if fact.params != export.params {
                                diagnostics.push(
                                    Diagnostic::error(
                                        &unit.path,
                                        import.span,
                                        stale_signature_message(
                                            &import.name,
                                            &fact.params,
                                            &export.params,
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

    let mut module = Module::default();
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
                    | Op::OptField { name } => {
                        *name = add_u32(constant_offset, *name);
                    }
                    Op::Global { func: target }
                    | Op::Call { func: target, .. }
                    | Op::MakeClosure { func: target, .. } => *target = func(*target),
                    _ => {}
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

/// What a caller is told when the function it was compiled against has changed shape
/// under it: both signatures, so the difference is visible without opening the callee.
fn stale_signature_message(
    name: &str,
    compiled: &[Rc<str>],
    defined: &[Rc<str>],
) -> String {
    let shape = |params: &[Rc<str>]| {
        let params: Vec<&str> = params.iter().map(|p| &**p).collect();
        format!("{name}({})", params.join(", "))
    };
    format!(
        "`{name}` is now `{}`, but this file was compiled against `{}` — recompile it",
        shape(defined),
        shape(compiled)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::{compile_unit, prepare};
    use crate::{SourceFile, Value, render};

    /// Compiles the file at `path` into a unit against an index of every given file's
    /// exports.
    fn unit_of(path: &str, index_of: &[(&str, &str)]) -> Unit {
        let prepared: Vec<_> = index_of
            .iter()
            .map(|(path, text)| prepare(&SourceFile::new(*path, *text)).unwrap())
            .collect();
        let mut index = ExportIndex::new();
        for p in &prepared {
            index.offer(p.path(), p.exports());
        }
        let this = prepared
            .iter()
            .find(|p| p.path() == path)
            .expect("the file compiles against an index that includes it");
        compile_unit(this, &index).expect_output()
    }

    /// Links `units` against the index they make, asserting there is no module and
    /// returning what was reported.
    fn link_errors(units: &[Unit]) -> Vec<Diagnostic> {
        let linked = link(units, &ExportIndex::of(units));
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
            "page.lem:5..25: `pair` is now `pair(b, a)`, but this file was compiled against \
             `pair(a, b)` — recompile it\n  lib.lem:8..12: now defined here"
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
            errors[0].message.contains("`greet(who, loudly)`"),
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
        let module = link(&units, &ExportIndex::of(&units)).expect_output();
        assert_eq!(
            render(&module, "page.lem", &[("page", Value::Null)]).unwrap(),
            "HELLO you"
        );
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
        let module = link(&units, &index).expect_output();
        assert_eq!(module, link(&units, &index).expect_output());
        assert_eq!(
            render(&module, "page.lem", &[("page", Value::Null)]).unwrap(),
            "a42b"
        );
    }
}

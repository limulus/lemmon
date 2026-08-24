//! Per-file compiled units and the link step that joins them (D25, invariant 11).
//!
//! A [`Unit`] is one file's compiled output: its own constant pool (document prose
//! included) and its own functions, with every reference to another file's function left
//! symbolic in [`Unit::imports`]. [`link`] is a pass over plain data that concatenates
//! the pools, rewrites the indices, and resolves the symbols — producing the one
//! [`Module`] the VM runs, unchanged in shape. That split is what makes a unit a
//! function of its file's text plus the signatures it consumed ([`Unit::facts`]) and
//! nothing else: no file's bytecode depends on another file's body.

use std::rc::Rc;

use lemmon_syntax::Span;

use crate::Value;
use crate::bytecode::{Function, Module, Op};
use crate::compile::{CompileError, duplicate_message, index_u32};

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
    /// The name token in the definition — where a duplicate of it is reported.
    pub name_span: Span,
    /// The whole definition — what a duplicate's message cites.
    pub span: Span,
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
/// indices offset, imports resolved to the functions the other units export.
///
/// # Errors
///
/// An import no unit exports, or a name two units export — each pointing at the source
/// that names it, in the order the units were given.
pub fn link(units: &[Unit]) -> Result<Module, Vec<CompileError>> {
    let mut errors = Vec::new();
    let mut constant_offsets = Vec::with_capacity(units.len());
    let mut function_offsets = Vec::with_capacity(units.len());
    let (mut constants, mut functions) = (0u32, 0u32);
    for unit in units {
        constant_offsets.push(constants);
        function_offsets.push(functions);
        constants = add_u32(constants, index_u32(unit.constants.len()));
        functions = add_u32(functions, index_u32(unit.functions.len()));
    }

    // The export map, first definition kept — a later unit exporting the same name is
    // an error, as it is when the same files are compiled together.
    let mut exports: Vec<(&Rc<str>, usize, &ExportedSignature)> = Vec::new();
    for (index, unit) in units.iter().enumerate() {
        for export in &unit.exports {
            match exports.iter().find(|(name, _, _)| *name == &export.name) {
                Some(&(_, first, existing)) => errors.push(CompileError {
                    path: unit.path.clone(),
                    span: export.name_span,
                    message: duplicate_message(
                        &export.name,
                        &units[first].path,
                        existing.span,
                    ),
                }),
                None => exports.push((&export.name, index, export)),
            }
        }
    }

    let resolved: Vec<Vec<Option<u32>>> = units
        .iter()
        .map(|unit| {
            unit.imports
                .iter()
                .map(|import| {
                    let found = exports.iter().find(|(name, _, _)| *name == &import.name);
                    match found {
                        Some(&(_, index, export)) => {
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
                                errors.push(CompileError {
                                    path: unit.path.clone(),
                                    span: import.span,
                                    message: stale_signature_message(
                                        &import.name,
                                        &fact.params,
                                        &export.params,
                                    ),
                                });
                            }
                            Some(add_u32(function_offsets[index], export.func))
                        }
                        None => {
                            errors.push(CompileError {
                                path: unit.path.clone(),
                                span: import.span,
                                message: format!("undefined name `{}`", import.name),
                            });
                            None
                        }
                    }
                })
                .collect()
        })
        .collect();
    if !errors.is_empty() {
        return Err(errors);
    }

    let mut module = Module::default();
    for (index, unit) in units.iter().enumerate() {
        let (constant_offset, function_offset) =
            (constant_offsets[index], function_offsets[index]);
        let locals = index_u32(unit.functions.len());
        let func = |func: u32| {
            if func < locals {
                add_u32(function_offset, func)
            } else {
                resolved[index][(func - locals) as usize].expect("resolved above")
            }
        };
        module.files.push(unit.path.clone());
        module.entries.push(function_offset);
        module.constants.extend(unit.constants.iter().cloned());
        for function in &unit.functions {
            let mut function = function.clone();
            function.file = index_u32(index);
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
    Ok(module)
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
    use crate::compile::{SignatureTable, compile_unit, prepare};
    use crate::{SourceFile, Value, render};

    /// Compiles the file at `path` into a unit against a table of every given file's
    /// exports.
    fn unit_of(path: &str, table_of: &[(&str, &str)]) -> Unit {
        let prepared: Vec<_> = table_of
            .iter()
            .map(|(path, text)| prepare(&SourceFile::new(*path, *text)).unwrap())
            .collect();
        let (table, errors) =
            SignatureTable::build(prepared.iter().map(|p| (p.path(), p.exports())));
        assert_eq!(errors, Vec::new());
        let this = prepared
            .iter()
            .find(|p| p.path() == path)
            .expect("the file compiles against a table that includes it");
        compile_unit(this, &table).unwrap()
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
        let errors = link(&[page]).unwrap_err();
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
        let errors = link(&[lib, page]).unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "page.lem");
        assert_eq!(
            errors[0].message,
            "`pair` is now `pair(b, a)`, but this file was compiled against \
             `pair(a, b)` — recompile it"
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
        let errors = link(&[lib, page]).unwrap_err();
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
        let module = link(&[lib, page]).unwrap();
        assert_eq!(
            render(&module, "page.lem", &[("page", Value::Null)]).unwrap(),
            "HELLO you"
        );
    }

    #[test]
    fn refuses_to_link_two_units_exporting_the_same_name() {
        // Each compiles clean on its own; the collision exists only between them, so
        // only link can see it — and says what compiling them together would have.
        let a = unit_of("a.lem", &[("a.lem", "<lem-fn f(): int = 1 />")]);
        let b = unit_of("b.lem", &[("b.lem", "<lem-fn f(): int = 2 />")]);
        let errors = link(&[a, b]).unwrap_err();
        assert_eq!(
            errors[0].to_string(),
            "b.lem:8..9: `f` is already defined in a.lem at 0..23"
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
        let module = link(&[lib.clone(), page.clone()]).unwrap();
        assert_eq!(module, link(&[lib, page]).unwrap());
        assert_eq!(
            render(&module, "page.lem", &[("page", Value::Null)]).unwrap(),
            "a42b"
        );
    }
}

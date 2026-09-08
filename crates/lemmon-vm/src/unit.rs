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
use crate::decls::{Declaration, Decls, Shape, TypeFact};
use crate::exports::ExportIndex;
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
}

/// A foreign type a unit was compiled against: where this file first reached it, and
/// the declaration it consumed there.
#[derive(Clone, Debug, PartialEq)]
pub struct TypeRef {
    pub name: Rc<str>,
    pub span: Span,
    pub fact: TypeFact,
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
    let mut module = Module {
        types,
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
            index.offer_types(p.path(), p.types());
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
        let module = link(&units, &ExportIndex::of(&units)).expect_output();
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
        let module = link(&units, &ExportIndex::of(&units)).expect_output();
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
        let module = link(&units, &ExportIndex::of(&units)).expect_output();
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
        let module = link(&units, &ExportIndex::of(&units)).expect_output();
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
        let module = link(&units, &index).expect_output();
        assert_eq!(module, link(&units, &index).expect_output());
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
        // A default runs at load, never at compile (∆iCW), so it is not part of the
        // shape a consumer depends on — the other half of the rule, and invariant 11
        // for declarations: the consumer's unit is byte-identical either way.
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
        let types = unit_of("types.lem", &[("types.lem", defaulted)]);
        let units = [types, page];
        assert!(link(&units, &ExportIndex::of(&units)).output.is_some());
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
}

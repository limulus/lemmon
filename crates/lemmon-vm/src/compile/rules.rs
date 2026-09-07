//! The checker's rules that need no emitting: pure functions from types (and names) to
//! a type, or to the message of the diagnostic that refuses them.
//!
//! The walk in [`expr`](super::expr) and [`items`](super::items) lowers a construct,
//! asks the rule for its type, and reports what the rule refused at the construct's
//! span. The construct then takes whatever type keeps the rest of the file checkable —
//! the type that was expected where one was, `any` where there is nothing better — so
//! that one mistake costs one diagnostic (`docs/language.md` §Checking). Nothing here
//! sees the compiler: a rule takes the types and names it judges and nothing else,
//! which is what lets it be tested without compiling a template, and what gives
//! records' field rules (∆cea) one place to go. The relations the rules are built on —
//! [`assignable`], [`join`] — live with [`Type`] itself.
//!
//! What stays in the walk needs more than types and names: `??`, whose right side is
//! lowered under an expectation the left side decides; the `is` placeholder; and the
//! checks on a call's or a literal's shape — named arguments passed to something with
//! no parameter names, a key given twice in a map literal.

use std::rc::Rc;

use lemmon_syntax::ast::{BinaryOp, UnaryOp};

use crate::builtins::Builtin;
use crate::types::{Type, assignable, join};
use crate::unit::Param;

/// What a rule says: the type (or other fact) it grants, or the message refusing it.
pub(super) type Rule<T = Type> = Result<T, String>;

/// The type an expectation asks its literal for: a `T?` wanted is a `T` written.
fn expectation(expected: Option<&Type>) -> Option<&Type> {
    expected.map(Type::strip_optional)
}

/// Whether a value of type `actual` may stand where `expected` is wanted: the
/// inferred-then-compared half of bidirectional checking.
pub(super) fn expected(actual: Type, expected: &Type) -> Rule {
    if assignable(&actual, expected) {
        Ok(actual)
    } else {
        Err(format!("expected `{expected}`, got `{actual}`"))
    }
}

/// The element type a list or map literal is checked against, if the expectation says:
/// `pick` reads it from the expected container, and an expected `any` opts the whole
/// literal out of checking — its elements are `any` too.
pub(super) fn literal_expectation(
    expected: Option<&Type>,
    pick: fn(&Type) -> Option<&Rc<Type>>,
) -> Option<Type> {
    match expectation(expected) {
        Some(Type::Any) => Some(Type::Any),
        Some(ty) => pick(ty).map(|element| (**element).clone()),
        None => None,
    }
}

/// The type a literal's elements share once `next` joins those `so_far`: a `null` makes
/// it optional, and an element that fits neither is refused — a literal's elements
/// share one type, never `any`.
pub(super) fn element(so_far: &Type, next: &Type, what: &str) -> Rule {
    join(so_far, next).ok_or_else(|| {
        format!("expected `{so_far}`, got `{next}`: a {what} share one type")
    })
}

/// The type of a `?:` with nothing expected of it: the one its branches share.
pub(super) fn branches(then: &Type, otherwise: &Type) -> Rule {
    join(then, otherwise).ok_or_else(|| {
        format!("the branches have different types: `{then}` and `{otherwise}`")
    })
}

/// The type a unary operator's result has: `-` keeps a number's type, `!` yields a
/// `bool`; `any` is the VM's to check.
pub(super) fn unary(op: UnaryOp, operand: &Type) -> Rule {
    match (op, operand) {
        (UnaryOp::Neg, Type::Int | Type::Float | Type::Any) => Ok(operand.clone()),
        (UnaryOp::Neg, other) => Err(format!("cannot negate a `{other}`")),
        (UnaryOp::Not, Type::Bool | Type::Any) => Ok(Type::Bool),
        (UnaryOp::Not, other) => Err(format!("`!` needs a `bool`, got `{other}`")),
    }
}

/// The operand rule of `&&` and `||`: a `bool`, or `any`.
pub(super) fn logical(symbol: &str, operand: &Type) -> Rule<()> {
    match operand {
        Type::Bool | Type::Any => Ok(()),
        other => Err(format!("`{symbol}` needs a `bool`, got `{other}`")),
    }
}

/// The type a binary operator's result has, or why its operands do not fit it
/// (`docs/language.md` §Expressions). An `any` on either side is the VM's to check:
/// the result is `any` where the operator yields its operands' type, and `bool` where
/// it yields one. The short-circuit operators are not among these: `&&` and `||` are
/// answered by [`logical`] one operand at a time, and `??` by the walk.
pub(super) fn operator(op: BinaryOp, symbol: &str, lhs: &Type, rhs: &Type) -> Rule {
    let mismatch = || format!("cannot apply `{symbol}` to `{lhs}` and `{rhs}`");
    let any = *lhs == Type::Any || *rhs == Type::Any;
    match op {
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => {
            match (lhs, rhs) {
                _ if any => Ok(Type::Any),
                (Type::Int, Type::Int) => Ok(Type::Int),
                (a, b) if a.is_number() && b.is_number() => Ok(Type::Float),
                (Type::String, Type::String) if op == BinaryOp::Add => Ok(Type::String),
                _ => Err(mismatch()),
            }
        }
        BinaryOp::Lt | BinaryOp::Lte | BinaryOp::Gt | BinaryOp::Gte => {
            let ordered = any
                || (lhs.is_number() && rhs.is_number())
                || matches!(
                    (lhs, rhs),
                    (Type::String, Type::String) | (Type::Datetime, Type::Datetime)
                );
            if ordered {
                Ok(Type::Bool)
            } else {
                Err(mismatch())
            }
        }
        BinaryOp::Eq | BinaryOp::NotEq => {
            // Numbers compare numerically whatever their type, as the VM does.
            let numbers = lhs.is_number() && rhs.is_number();
            if matches!(lhs, Type::Fn(..)) || matches!(rhs, Type::Fn(..)) {
                Err("functions cannot be compared".to_owned())
            } else if numbers || assignable(lhs, rhs) || assignable(rhs, lhs) {
                Ok(Type::Bool)
            } else {
                Err(format!("cannot compare `{lhs}` with `{rhs}`"))
            }
        }
        BinaryOp::In => match rhs {
            Type::Any => Ok(Type::Bool),
            Type::List(element) if assignable(lhs, element) => Ok(Type::Bool),
            Type::List(element) => Err(format!(
                "`in` on a `{rhs}` needs `{element}` on its left, got `{lhs}`"
            )),
            Type::Map(_) if assignable(lhs, &Type::String) => Ok(Type::Bool),
            Type::Map(_) => Err(format!(
                "`in` on a map needs a `string` key on its left, got `{lhs}`"
            )),
            _ => Err(mismatch()),
        },
        BinaryOp::And | BinaryOp::Or | BinaryOp::Coalesce => {
            unreachable!("short-circuit operators are lowered by the walk")
        }
    }
}

/// The type of `object.name` (or `object?.name` when `optional`). Only `any` has fields
/// until records arrive (∆cea); `?.` needs an optional on its left and yields an
/// optional, and a value known to be null is an optional with nothing inside, so `?.`
/// on it is null again.
pub(super) fn member(object: &Type, name: &str, optional: bool) -> Rule {
    match (object, optional) {
        (Type::Any, _) => Ok(Type::Any),
        (Type::Null, true) => Ok(Type::Null),
        (Type::Optional(inner), true) => field(inner, name).map(Type::optional),
        (Type::Optional(_), false) => Err(may_be_null(object, ", or use `?.`")),
        (other, true) => Err(format!("`?.` needs an optional, got `{other}`")),
        (other, false) => field(other, name),
    }
}

/// The type of the field `name` of a value of type `ty`, or why it has none.
fn field(ty: &Type, name: &str) -> Rule {
    match ty {
        Type::Any => Ok(Type::Any),
        Type::Map(_) => Err(format!(
            "a `{ty}` has no fields: index a map as `[\"{name}\"]`"
        )),
        Type::Null => Err(format!("cannot read field `{name}` of null")),
        other => Err(format!("a `{other}` has no field `{name}`")),
    }
}

/// The message for reading through a `T?` as if it were a `T`.
fn may_be_null(ty: &Type, alternative: &str) -> String {
    format!("a `{ty}` may be null: narrow it with `lem-if`{alternative}")
}

/// The type of `object[index]`: a list's element under an `int`, a map's value under a
/// `string`; `any` is the VM's to index.
pub(super) fn index(object: &Type, index: &Type) -> Rule {
    let (element, key) = match object {
        Type::Any => return Ok(Type::Any),
        Type::Optional(_) => return Err(may_be_null(object, "")),
        Type::List(element) => ((**element).clone(), Type::Int),
        Type::Map(value) => ((**value).clone(), Type::String),
        other => return Err(format!("cannot index a `{other}`")),
    };
    if assignable(index, &key) {
        Ok(element)
    } else {
        let what = if key == Type::Int { "list" } else { "map" };
        Err(format!("a {what} is indexed by `{key}`, got `{index}`"))
    }
}

/// The condition rule: a `lem-if` or `?:` tests a `bool` or an optional — `any` is the
/// VM's to test, and `null` is an optional with nothing inside. An optional grants the
/// type it narrows to where the condition holds, for the walk to open on a binding.
pub(super) fn condition(ty: &Type) -> Rule<Option<Type>> {
    match ty {
        Type::Bool | Type::Any | Type::Null => Ok(None),
        Type::Optional(inner) => Ok(Some((**inner).clone())),
        other => Err(format!(
            "a condition must be a `bool` or an optional, got `{other}`"
        )),
    }
}

/// The print rule (`docs/language.md` §Escaping): what `<lem.…>` may print. A `bool`,
/// `null`, a list, a map, or a function has no printed form; an optional has one once
/// it is known not to be null; `any` is the VM's to print.
pub(super) fn printable(ty: &Type) -> Rule<()> {
    match ty {
        Type::String
        | Type::Html
        | Type::Int
        | Type::Float
        | Type::Datetime
        | Type::Url
        | Type::Asset
        | Type::Any => Ok(()),
        Type::Optional(_) => Err(format!(
            "cannot print a `{ty}`: narrow it with `lem-if`, or give it a default with `??`"
        )),
        Type::Fn(..) => Err(format!(
            "cannot print a `{ty}`: call it with `(…)` to print its result"
        )),
        other => Err(format!("cannot print a `{other}`")),
    }
}

/// What a `lem-for` over `ty` binds: a list's element and its `int` index, or a map's
/// `string` key and its value; two `any`s over `any`.
pub(super) fn iterable(ty: &Type) -> Rule<(Type, Type)> {
    match ty {
        Type::Any => Ok((Type::Any, Type::Any)),
        Type::List(element) => Ok(((**element).clone(), Type::Int)),
        Type::Map(value) => Ok((Type::String, (**value).clone())),
        other => Err(format!("cannot iterate over a `{other}`")),
    }
}

/// "1 argument", "2 arguments".
fn count(n: usize, noun: &str) -> String {
    format!("{n} {noun}{}", if n == 1 { "" } else { "s" })
}

/// "`f` takes 2 arguments, got 1"
fn arity(what: &str, wanted: usize, got: usize) -> String {
    format!("{what} takes {}, got {got}", count(wanted, "argument"))
}

/// Whether `argc` arguments are what a builtin takes.
pub(super) fn builtin(builtin: &Builtin, argc: usize) -> Rule<()> {
    if argc == builtin.arity {
        Ok(())
    } else {
        Err(arity(&format!("`{}`", builtin.name), builtin.arity, argc))
    }
}

/// What a lambda with `params` parameters takes from the type expected of it: its
/// parameter types and the return type its body is checked against, when a function
/// type of the same arity is expected; nothing when nothing is, or something else is
/// (the mismatch is then reported against the whole lambda); a refusal when a function
/// type of another arity is.
pub(super) fn lambda_shape(
    expected: Option<&Type>,
    params: usize,
) -> Rule<Option<(Rc<[Type]>, Type)>> {
    match expectation(expected) {
        Some(Type::Fn(param_tys, ret)) if param_tys.len() == params => {
            Ok(Some((param_tys.clone(), (**ret).clone())))
        }
        Some(want @ Type::Fn(param_tys, _)) => Err(format!(
            "this lambda takes {}, but a `{want}` takes {}",
            count(params, "parameter"),
            param_tys.len()
        )),
        _ => Ok(None),
    }
}

/// What calling a value of type `callee` with `argc` arguments checks them against and
/// yields: a function type's parameters (positionally) and its return; nothing and
/// `any` for an `any`, which the VM checks.
pub(super) fn callee(
    callee: &Type,
    name: Option<&str>,
    argc: usize,
) -> Rule<(Option<Rc<[Type]>>, Type)> {
    match callee {
        Type::Fn(params, _) if params.len() != argc => {
            let what = name.map_or("this function".to_owned(), |n| format!("`{n}`"));
            Err(arity(&what, params.len(), argc))
        }
        Type::Fn(params, ret) => Ok((Some(params.clone()), (**ret).clone())),
        Type::Any => Ok((None, Type::Any)),
        other => Err(format!("cannot call a `{other}`")),
    }
}

/// An argument of a call, as given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Arg {
    /// The `n`th positional argument (a UFCS receiver is the first).
    Positional(usize),
    /// The `n`th named argument.
    Named(usize),
}

/// The order a call to `fn_name` passes its arguments in: one source per parameter,
/// named arguments reordered into declaration order. A refusal names the argument to
/// blame — `None` blames the call itself, as a missing argument does — and only the
/// first rule broken is reported.
///
/// `positional` is how many positional arguments the call gives and `named` the names
/// it gives, in order: every `Arg` returned or blamed indexes those, and the argument
/// blamed for a surplus is the first one past the parameters, which the surplus
/// guarantees exists.
pub(super) fn arguments(
    fn_name: &str,
    params: &[Param],
    positional: usize,
    named: &[&str],
) -> Result<Vec<Arg>, (Option<Arg>, String)> {
    let mut slots: Vec<Option<Arg>> = vec![None; params.len()];
    if positional > params.len() {
        let extra = Arg::Positional(params.len());
        let message = arity(
            &format!("`{fn_name}`"),
            params.len(),
            positional + named.len(),
        );
        return Err((Some(extra), message));
    }
    for (i, slot) in slots.iter_mut().take(positional).enumerate() {
        *slot = Some(Arg::Positional(i));
    }
    for (n, name) in named.iter().enumerate() {
        let arg = Arg::Named(n);
        let Some(i) = params.iter().position(|p| &*p.name == *name) else {
            let message = format!("`{fn_name}` has no parameter `{name}`");
            return Err((Some(arg), message));
        };
        if slots[i].is_some() {
            return Err((Some(arg), format!("`{name}` is already given")));
        }
        slots[i] = Some(arg);
    }
    params
        .iter()
        .zip(slots)
        .map(|(param, slot)| {
            slot.ok_or_else(|| {
                let message =
                    format!("missing argument `{}` in call to `{fn_name}`", param.name);
                (None, message)
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opt(inner: Type) -> Type {
        Type::optional(inner)
    }

    fn func(params: Vec<Type>, ret: Type) -> Type {
        Type::function(params, ret)
    }

    fn param(name: &str) -> Param {
        Param {
            name: Rc::from(name),
            ty: Type::Int,
        }
    }

    #[test]
    fn strips_one_optional_from_an_expectation() {
        assert_eq!(expectation(Some(&opt(Type::Int))), Some(&Type::Int));
        assert_eq!(expectation(Some(&Type::Int)), Some(&Type::Int));
        assert_eq!(expectation(None), None);
    }

    #[test]
    fn reads_a_literals_element_type_from_the_container_expected() {
        let pick: fn(&Type) -> Option<&Rc<Type>> = |ty| match ty {
            Type::List(element) => Some(element),
            _ => None,
        };
        assert_eq!(
            literal_expectation(Some(&opt(Type::list(Type::Int))), pick),
            Some(Type::Int)
        );
        assert_eq!(literal_expectation(Some(&Type::Any), pick), Some(Type::Any));
        assert_eq!(literal_expectation(Some(&Type::Int), pick), None);
        assert_eq!(literal_expectation(None, pick), None);
    }

    #[test]
    fn joins_a_literals_elements_and_refuses_one_that_fits_neither() {
        assert_eq!(
            element(&Type::Int, &Type::Null, "list's elements"),
            Ok(opt(Type::Int))
        );
        assert_eq!(
            element(&Type::Int, &Type::String, "map's values"),
            Err("expected `int`, got `string`: a map's values share one type".to_owned())
        );
    }

    #[test]
    fn joins_a_ternarys_branches_and_refuses_two_that_share_nothing() {
        assert_eq!(branches(&Type::Int, &Type::Null), Ok(opt(Type::Int)));
        assert_eq!(
            branches(&Type::Int, &Type::String),
            Err("the branches have different types: `int` and `string`".to_owned())
        );
    }

    #[test]
    fn negates_numbers_and_any_keeping_their_type() {
        assert_eq!(unary(UnaryOp::Neg, &Type::Int), Ok(Type::Int));
        assert_eq!(unary(UnaryOp::Neg, &Type::Float), Ok(Type::Float));
        assert_eq!(unary(UnaryOp::Neg, &Type::Any), Ok(Type::Any));
        assert_eq!(
            unary(UnaryOp::Neg, &Type::String),
            Err("cannot negate a `string`".to_owned())
        );
    }

    #[test]
    fn nots_a_bool_or_any_into_a_bool() {
        assert_eq!(unary(UnaryOp::Not, &Type::Bool), Ok(Type::Bool));
        assert_eq!(unary(UnaryOp::Not, &Type::Any), Ok(Type::Bool));
        assert_eq!(
            unary(UnaryOp::Not, &Type::Int),
            Err("`!` needs a `bool`, got `int`".to_owned())
        );
    }

    #[test]
    fn admits_only_a_bool_or_any_beside_and_and_or() {
        assert_eq!(logical("&&", &Type::Bool), Ok(()));
        assert_eq!(logical("||", &Type::Any), Ok(()));
        assert_eq!(
            logical("&&", &Type::Int),
            Err("`&&` needs a `bool`, got `int`".to_owned())
        );
    }

    #[test]
    fn types_arithmetic_by_its_operands() {
        assert_eq!(
            operator(BinaryOp::Add, "+", &Type::Int, &Type::Int),
            Ok(Type::Int)
        );
        assert_eq!(
            operator(BinaryOp::Mul, "*", &Type::Int, &Type::Float),
            Ok(Type::Float)
        );
        assert_eq!(
            operator(BinaryOp::Rem, "%", &Type::Float, &Type::Float),
            Ok(Type::Float)
        );
        assert_eq!(
            operator(BinaryOp::Sub, "-", &Type::Any, &Type::Bool),
            Ok(Type::Any)
        );
    }

    #[test]
    fn concatenates_strings_with_plus_and_nothing_else() {
        assert_eq!(
            operator(BinaryOp::Add, "+", &Type::String, &Type::String),
            Ok(Type::String)
        );
        assert_eq!(
            operator(BinaryOp::Sub, "-", &Type::String, &Type::String),
            Err("cannot apply `-` to `string` and `string`".to_owned())
        );
        assert_eq!(
            operator(BinaryOp::Add, "+", &Type::String, &Type::Int),
            Err("cannot apply `+` to `string` and `int`".to_owned())
        );
    }

    #[test]
    fn orders_numbers_strings_and_datetimes_among_themselves() {
        assert_eq!(
            operator(BinaryOp::Lt, "lt", &Type::Int, &Type::Float),
            Ok(Type::Bool)
        );
        assert_eq!(
            operator(BinaryOp::Gte, "gte", &Type::String, &Type::String),
            Ok(Type::Bool)
        );
        assert_eq!(
            operator(BinaryOp::Gt, "gt", &Type::Datetime, &Type::Datetime),
            Ok(Type::Bool)
        );
        assert_eq!(
            operator(BinaryOp::Lte, "lte", &Type::Any, &Type::Bool),
            Ok(Type::Bool)
        );
        assert_eq!(
            operator(BinaryOp::Lt, "lt", &Type::Int, &Type::String),
            Err("cannot apply `lt` to `int` and `string`".to_owned())
        );
        assert_eq!(
            operator(BinaryOp::Gt, "gt", &Type::Bool, &Type::Bool),
            Err("cannot apply `gt` to `bool` and `bool`".to_owned())
        );
    }

    #[test]
    fn compares_numbers_and_assignable_types_but_never_functions() {
        assert_eq!(
            operator(BinaryOp::Eq, "==", &Type::Int, &Type::Float),
            Ok(Type::Bool)
        );
        assert_eq!(
            operator(BinaryOp::NotEq, "!=", &Type::Int, &opt(Type::Int)),
            Ok(Type::Bool)
        );
        assert_eq!(
            operator(BinaryOp::Eq, "==", &opt(Type::Int), &Type::Int),
            Ok(Type::Bool)
        );
        assert_eq!(
            operator(BinaryOp::Eq, "==", &Type::Int, &Type::String),
            Err("cannot compare `int` with `string`".to_owned())
        );
        let f = func(vec![], Type::Int);
        assert_eq!(
            operator(BinaryOp::Eq, "==", &f, &f),
            Err("functions cannot be compared".to_owned())
        );
    }

    #[test]
    fn checks_the_left_of_in_against_the_lists_element_or_a_maps_key() {
        assert_eq!(
            operator(BinaryOp::In, "in", &Type::Int, &Type::list(Type::Int)),
            Ok(Type::Bool)
        );
        assert_eq!(
            operator(BinaryOp::In, "in", &Type::String, &Type::map(Type::Int)),
            Ok(Type::Bool)
        );
        assert_eq!(
            operator(BinaryOp::In, "in", &Type::Bool, &Type::Any),
            Ok(Type::Bool)
        );
        assert_eq!(
            operator(BinaryOp::In, "in", &Type::String, &Type::list(Type::Int)),
            Err("`in` on a `[int]` needs `int` on its left, got `string`".to_owned())
        );
        assert_eq!(
            operator(BinaryOp::In, "in", &Type::Int, &Type::map(Type::Int)),
            Err("`in` on a map needs a `string` key on its left, got `int`".to_owned())
        );
        assert_eq!(
            operator(BinaryOp::In, "in", &Type::Int, &Type::Int),
            Err("cannot apply `in` to `int` and `int`".to_owned())
        );
    }

    #[test]
    fn reads_a_field_of_any_and_of_nothing_else_yet() {
        assert_eq!(member(&Type::Any, "f", false), Ok(Type::Any));
        assert_eq!(member(&Type::Any, "f", true), Ok(Type::Any));
        assert_eq!(member(&opt(Type::Any), "f", true), Ok(Type::Any));
        assert_eq!(
            member(&Type::String, "length", false),
            Err("a `string` has no field `length`".to_owned())
        );
        assert_eq!(
            member(&Type::map(Type::Int), "a", false),
            Err("a `{string: int}` has no fields: index a map as `[\"a\"]`".to_owned())
        );
        assert_eq!(
            member(&Type::Null, "f", false),
            Err("cannot read field `f` of null".to_owned())
        );
    }

    #[test]
    fn reads_through_an_optional_only_with_the_optional_chain() {
        assert_eq!(
            member(&opt(Type::String), "length", false),
            Err("a `string?` may be null: narrow it with `lem-if`, or use `?.`".to_owned())
        );
        assert_eq!(
            member(&opt(Type::String), "length", true),
            Err("a `string` has no field `length`".to_owned())
        );
        assert_eq!(member(&Type::Null, "f", true), Ok(Type::Null));
        assert_eq!(
            member(&Type::String, "length", true),
            Err("`?.` needs an optional, got `string`".to_owned())
        );
    }

    #[test]
    fn indexes_a_list_by_int_and_a_map_by_string() {
        assert_eq!(index(&Type::list(Type::Bool), &Type::Int), Ok(Type::Bool));
        assert_eq!(index(&Type::map(Type::Bool), &Type::String), Ok(Type::Bool));
        assert_eq!(index(&Type::list(Type::Bool), &Type::Any), Ok(Type::Bool));
        assert_eq!(index(&Type::Any, &Type::Bool), Ok(Type::Any));
        assert_eq!(
            index(&Type::list(Type::Int), &Type::String),
            Err("a list is indexed by `int`, got `string`".to_owned())
        );
        assert_eq!(
            index(&Type::map(Type::Int), &Type::Int),
            Err("a map is indexed by `string`, got `int`".to_owned())
        );
    }

    #[test]
    fn refuses_to_index_an_optional_or_a_scalar() {
        assert_eq!(
            index(&opt(Type::map(Type::Int)), &Type::String),
            Err("a `{string: int}?` may be null: narrow it with `lem-if`".to_owned())
        );
        assert_eq!(
            index(&Type::Int, &Type::Int),
            Err("cannot index a `int`".to_owned())
        );
    }

    #[test]
    fn tests_a_bool_or_an_optional_and_narrows_the_optional() {
        assert_eq!(condition(&Type::Bool), Ok(None));
        assert_eq!(condition(&Type::Any), Ok(None));
        assert_eq!(condition(&Type::Null), Ok(None));
        assert_eq!(condition(&opt(Type::String)), Ok(Some(Type::String)));
        assert_eq!(
            condition(&Type::Int),
            Err("a condition must be a `bool` or an optional, got `int`".to_owned())
        );
    }

    #[test]
    fn prints_scalars_html_and_any() {
        for ty in [
            Type::String,
            Type::Html,
            Type::Int,
            Type::Float,
            Type::Datetime,
            Type::Url,
            Type::Asset,
            Type::Any,
        ] {
            assert_eq!(printable(&ty), Ok(()), "{ty}");
        }
    }

    #[test]
    fn refuses_to_print_what_has_no_printed_form() {
        assert_eq!(
            printable(&Type::Bool),
            Err("cannot print a `bool`".to_owned())
        );
        assert_eq!(
            printable(&Type::Null),
            Err("cannot print a `null`".to_owned())
        );
        assert_eq!(
            printable(&Type::list(Type::Int)),
            Err("cannot print a `[int]`".to_owned())
        );
        assert_eq!(
            printable(&Type::map(Type::Int)),
            Err("cannot print a `{string: int}`".to_owned())
        );
        assert_eq!(
            printable(&opt(Type::String)),
            Err(
                "cannot print a `string?`: narrow it with `lem-if`, or give it a default \
                 with `??`"
                    .to_owned()
            )
        );
        assert_eq!(
            printable(&func(vec![Type::Int], Type::Int)),
            Err(
                "cannot print a `(int): int`: call it with `(…)` to print its result"
                    .to_owned()
            )
        );
    }

    #[test]
    fn iterates_a_list_with_its_index_and_a_map_with_its_key() {
        assert_eq!(
            iterable(&Type::list(Type::String)),
            Ok((Type::String, Type::Int))
        );
        assert_eq!(
            iterable(&Type::map(Type::Bool)),
            Ok((Type::String, Type::Bool))
        );
        assert_eq!(iterable(&Type::Any), Ok((Type::Any, Type::Any)));
        assert_eq!(
            iterable(&Type::Int),
            Err("cannot iterate over a `int`".to_owned())
        );
    }

    #[test]
    fn counts_with_the_plural_the_number_wants() {
        assert_eq!(count(1, "argument"), "1 argument");
        assert_eq!(count(0, "parameter"), "0 parameters");
    }

    #[test]
    fn admits_what_fits_the_expectation_and_names_both_types_otherwise() {
        assert_eq!(expected(Type::Int, &opt(Type::Int)), Ok(Type::Int));
        assert_eq!(expected(Type::Any, &Type::Int), Ok(Type::Any));
        assert_eq!(
            expected(Type::String, &Type::Int),
            Err("expected `int`, got `string`".to_owned())
        );
    }

    #[test]
    fn holds_a_builtin_to_its_arity() {
        let raw = &crate::builtins::BUILTINS[0];
        assert_eq!(builtin(raw, 1), Ok(()));
        assert_eq!(
            builtin(raw, 2),
            Err("`raw` takes 1 argument, got 2".to_owned())
        );
    }

    #[test]
    fn shapes_a_lambda_by_the_function_type_expected_of_it() {
        let want = func(vec![Type::Int, Type::String], Type::Bool);
        assert_eq!(
            lambda_shape(Some(&opt(want.clone())), 2),
            Ok(Some((Rc::from(vec![Type::Int, Type::String]), Type::Bool)))
        );
        assert_eq!(lambda_shape(Some(&Type::Int), 2), Ok(None));
        assert_eq!(lambda_shape(None, 0), Ok(None));
        assert_eq!(
            lambda_shape(Some(&want), 1),
            Err(
                "this lambda takes 1 parameter, but a `(int, string): bool` takes 2"
                    .to_owned()
            )
        );
    }

    #[test]
    fn calls_a_function_value_by_its_signature_and_any_unchecked() {
        assert_eq!(
            callee(&func(vec![Type::Int], Type::String), Some("f"), 1),
            Ok((Some(Rc::from(vec![Type::Int])), Type::String))
        );
        assert_eq!(callee(&Type::Any, None, 3), Ok((None, Type::Any)));
    }

    #[test]
    fn refuses_a_call_of_the_wrong_arity_or_of_a_non_function() {
        let f = func(vec![Type::Int, Type::Int], Type::Int);
        assert_eq!(
            callee(&f, Some("f"), 1),
            Err("`f` takes 2 arguments, got 1".to_owned())
        );
        assert_eq!(
            callee(&f, None, 0),
            Err("this function takes 2 arguments, got 0".to_owned())
        );
        assert_eq!(
            callee(&Type::String, Some("s"), 1),
            Err("cannot call a `string`".to_owned())
        );
    }

    #[test]
    fn orders_named_arguments_after_the_positional_ones_by_declaration() {
        let params = [param("a"), param("b"), param("c")];
        assert_eq!(
            arguments("f", &params, 1, &["c", "b"]),
            Ok(vec![Arg::Positional(0), Arg::Named(1), Arg::Named(0)])
        );
        assert_eq!(arguments("f", &[], 0, &[]), Ok(vec![]));
    }

    #[test]
    fn blames_the_first_extra_positional_argument_with_the_whole_count() {
        let params = [param("a")];
        assert_eq!(
            arguments("f", &params, 3, &["a"]),
            Err((
                Some(Arg::Positional(1)),
                "`f` takes 1 argument, got 4".to_owned()
            ))
        );
    }

    #[test]
    fn blames_a_named_argument_no_parameter_answers_to() {
        let params = [param("a")];
        assert_eq!(
            arguments("f", &params, 0, &["a", "z"]),
            Err((Some(Arg::Named(1)), "`f` has no parameter `z`".to_owned()))
        );
    }

    #[test]
    fn blames_a_named_argument_given_twice_or_already_positional() {
        let params = [param("a"), param("b")];
        assert_eq!(
            arguments("f", &params, 1, &["a"]),
            Err((Some(Arg::Named(0)), "`a` is already given".to_owned()))
        );
        assert_eq!(
            arguments("f", &params, 0, &["b", "b"]),
            Err((Some(Arg::Named(1)), "`b` is already given".to_owned()))
        );
    }

    #[test]
    fn blames_the_call_for_the_first_parameter_left_without_an_argument() {
        let params = [param("a"), param("b"), param("c")];
        assert_eq!(
            arguments("f", &params, 1, &["c"]),
            Err((None, "missing argument `b` in call to `f`".to_owned()))
        );
    }
}

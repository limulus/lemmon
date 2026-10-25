//! The checker's rules that need no emitting: pure functions from types (and names) to
//! a type, or to the message of the diagnostic that refuses them.
//!
//! The walk in [`expr`](super::expr) and [`items`](super::items) lowers a construct,
//! asks the rule for its type, and reports what the rule refused at the construct's
//! span. The construct then takes whatever type keeps the rest of the file checkable —
//! the type that was expected where one was, `any` where there is nothing better — so
//! that one mistake costs one diagnostic (`docs/language.md` §Checking). Nothing here
//! sees the compiler: a rule takes the types and names it judges — and the declaration
//! table, where a record's fields or ancestry decide — and nothing else, which is what
//! lets it be tested without compiling a template. The relations the rules are built on
//! — [`assignable`], [`join`] — live with [`Type`] itself.
//!
//! What stays in the walk needs more than types and names: `??`, whose right side is
//! lowered under an expectation the left side decides; and the checks on a call's or a
//! literal's shape — named arguments passed to something with no parameter names, a key
//! given twice in a map literal, a `lem-case` pattern spelled twice.

use std::rc::Rc;

use lemmon_syntax::ast::{BinaryOp, UnaryOp};
use lemmon_syntax::{Span, nearest};

use crate::builtins::{self, Builtin, ReceiverKind};
use crate::decls::{DeclKind, Decls, Shape, no_field};
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
pub(super) fn expected(actual: Type, expected: &Type, decls: &dyn Decls) -> Rule {
    if assignable(&actual, expected, decls) {
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
pub(super) fn element(so_far: &Type, next: &Type, what: &str, decls: &dyn Decls) -> Rule {
    join(so_far, next, decls).ok_or_else(|| {
        format!("expected `{so_far}`, got `{next}`: a {what} share one type")
    })
}

/// The type of a `?:` with nothing expected of it: the one its branches share.
pub(super) fn branches(then: &Type, otherwise: &Type, decls: &dyn Decls) -> Rule {
    join(then, otherwise, decls).ok_or_else(|| {
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
pub(super) fn operator(
    op: BinaryOp,
    symbol: &str,
    lhs: &Type,
    rhs: &Type,
    decls: &dyn Decls,
) -> Rule {
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
            } else if numbers || assignable(lhs, rhs, decls) || assignable(rhs, lhs, decls)
            {
                Ok(Type::Bool)
            } else {
                Err(format!("cannot compare `{lhs}` with `{rhs}`"))
            }
        }
        BinaryOp::In => match rhs {
            Type::Any => Ok(Type::Bool),
            Type::List(element) if assignable(lhs, element, decls) => Ok(Type::Bool),
            Type::List(element) => Err(format!(
                "`in` on a `{rhs}` needs `{element}` on its left, got `{lhs}`"
            )),
            Type::Map(_) if assignable(lhs, &Type::String, decls) => Ok(Type::Bool),
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

/// The type of `object.name` (or `object?.name` when `optional`). A record's fields are
/// its declaration's and its ancestors'; `any` has every field; `?.` needs an optional
/// on its left and yields an optional, and a value known to be null is an optional with
/// nothing inside, so `?.` on it is null again.
pub(super) fn member(object: &Type, name: &str, optional: bool, decls: &dyn Decls) -> Rule {
    match (object, optional) {
        (Type::Any, _) => Ok(Type::Any),
        (Type::Null, true) => Ok(Type::Null),
        (Type::Optional(inner), true) => field(inner, name, decls).map(Type::optional),
        (Type::Optional(_), false) => Err(may_be_null(object, ", or use `?.`")),
        (other, true) => Err(format!("`?.` needs an optional, got `{other}`")),
        (other, false) => field(other, name, decls),
    }
}

/// The type of the field `name` of a value of type `ty`, or why it has none. A builtin
/// type's method is named here too: it is not a field, and reading one without calling
/// it is the mistake worth naming, since a method is called and never read (D34).
fn field(ty: &Type, name: &str, decls: &dyn Decls) -> Rule {
    if let Some(kind) = ReceiverKind::of(ty)
        && builtins::method_id(kind, name).is_some()
    {
        return Err(format!(
            "`{name}` is a method of `{ty}`: call it as `.{name}()`"
        ));
    }
    match ty {
        Type::Any => Ok(Type::Any),
        Type::Map(_) => Err(format!(
            "a `{ty}` has no fields: index a map as `[\"{name}\"]`"
        )),
        Type::Null => Err(format!("cannot read field `{name}` of null")),
        Type::Named(record) => match decls.kind(record) {
            Some(DeclKind::Record) => decls
                .field(record, name)
                .ok_or_else(|| no_field(record, name, decls)),
            Some(DeclKind::Enum) => Err(format!("a `{ty}` is an enum and has no fields")),
            None => unreachable!("an undeclared name is read as `any` before any rule"),
        },
        other => Err(no_field_but_maybe_a_method(other, name)),
    }
}

/// "a `string` has no field `slugfy`", and — when a method of that type is one typo
/// away — the call form it was reaching for. A builtin type has fields in neither
/// spelling, so the only reading of `s.slugfy` worth offering is a method (`∆eHR`).
fn no_field_but_maybe_a_method(ty: &Type, name: &str) -> String {
    let mut out = format!("a `{ty}` has no field `{name}`");
    if let Some(kind) = ReceiverKind::of(ty)
        && let Some(near) = nearest(name, builtins::methods_of(kind))
    {
        out.push_str(&format!("; did you mean `.{near}()`?"));
    }
    out
}

/// What `object.name(…)` calls (or `object?.name(…)` when `optional`), given the
/// receiver's type: the method `name` of that builtin type, or the field of that name
/// called as the value it holds (D34). `.` asks the receiver and nothing else, so a
/// name that is neither is refused here — naming the methods the receiver does carry,
/// when it carries any. `?.` wants an optional and reads through it, as it does for a
/// field; the walk is what jumps the call when the receiver turns out to be null. A
/// receiver known to be null is refused exactly as reading its field is — `null` is not
/// `any`, and a call is no reason to hold a knowable mistake back to render time.
pub(super) fn callable_member(
    object: &Type,
    name: &str,
    optional: bool,
    decls: &dyn Decls,
) -> Rule<Callable> {
    match (object, optional) {
        (Type::Any, _) => Ok(Callable::Dynamic),
        (Type::Null, true) => Ok(Callable::Dynamic),
        (Type::Optional(inner), true) => callable(inner, name, decls),
        (Type::Optional(_), false) => Err(may_be_null(object, ", or use `?.`")),
        (other, true) => Err(format!("`?.` needs an optional, got `{other}`")),
        (other, false) => callable(other, name, decls),
    }
}

/// [`callable_member`] with the optionality settled: what a receiver of exactly this
/// type carries under `name`.
fn callable(ty: &Type, name: &str, decls: &dyn Decls) -> Rule<Callable> {
    match ReceiverKind::of(ty) {
        Some(kind) => match builtins::method_id(kind, name) {
            Some(id) => Ok(Callable::Method(id)),
            None => Err(no_method(ty, kind, name)),
        },
        None => field(ty, name, decls).map(Callable::Field),
    }
}

/// What `object.name(…)` resolved to.
#[derive(Debug, PartialEq)]
pub(super) enum Callable {
    /// `BUILTINS[id]`, whose first argument is the receiver.
    Method(u32),
    /// A field holding the function, of this type.
    Field(Type),
    /// Nothing decidable: the receiver is `any`, or a `null` the `?.` jump steps over
    /// before the call is reached, so the VM resolves the name against the value it
    /// finds (D34).
    Dynamic,
}

/// "a `[int]` has no method `frobnicate`", plus what it does have — and, for a map,
/// the indexing its field read would have suggested, since `m.foo()` for `m["foo"]` is
/// the mistake a map receiver invites.
fn no_method(ty: &Type, kind: ReceiverKind, name: &str) -> String {
    let mut out = format!("a `{ty}` has no method `{name}`");
    // One name when one is close, the inventory when none is: a type carrying ten
    // methods would otherwise answer a typo with a list to read rather than a fix.
    if let Some(near) = nearest(name, builtins::methods_of(kind)) {
        out.push_str(&format!("; did you mean `.{near}()`?"));
    } else {
        let mut have: Vec<&str> = builtins::methods_of(kind).collect();
        have.sort_unstable();
        if !have.is_empty() {
            let list: Vec<String> = have.iter().map(|m| format!("`.{m}()`")).collect();
            out.push_str(&format!("; it has {}", list.join(", ")));
        }
    }
    if matches!(ty, Type::Map(_)) {
        out.push_str(&format!("; an entry is `[\"{name}\"]`"));
    }
    out
}

/// The enum `expected` asks for, if it asks for one: a `T?` wanted is a `T` written,
/// as it is for a literal.
fn expected_enum<'a>(expected: Option<&'a Type>, decls: &dyn Decls) -> Option<&'a str> {
    match expectation(expected) {
        Some(Type::Named(name)) if decls.kind(name) == Some(DeclKind::Enum) => Some(name),
        _ => None,
    }
}

/// The record `expected` asks for, if it asks for one: a `T?` wanted is a `T` written,
/// as it is for a literal. What makes a `{…}` a record literal (D35).
pub(super) fn expected_record<'a>(
    expected: Option<&'a Type>,
    decls: &dyn Decls,
) -> Option<&'a str> {
    match expectation(expected) {
        Some(Type::Named(name)) if decls.kind(name) == Some(DeclKind::Record) => Some(name),
        _ => None,
    }
}

/// The type a record literal's entry `key` is checked against: the field of that name
/// the record or an ancestor declares. A key that names no field is refused — a record
/// has exactly its declared fields (D35).
pub(super) fn record_entry(record: &str, key: &str, decls: &dyn Decls) -> Rule {
    decls
        .field(record, key)
        .ok_or_else(|| no_field(record, key, decls))
}

/// Whether a record field of type `ty` may be left out, by a literal or by loaded data:
/// one whose type admits `null` — a `T?`, or `any`, which is what `any?` folds to.
pub(crate) fn omittable(ty: &Type) -> bool {
    matches!(ty, Type::Optional(_) | Type::Any)
}

/// What a record literal that gave the fields `given` leaves out: the fields it may
/// omit — those whose type admits `null`, which the literal fills with null so the
/// record is complete — or the refusal naming the ones it must give. A field's default
/// is not consulted: defaults are the loader's, evaluated where `body` and `site` exist
/// (D35). The loader fills an omitted field by the same rule, so the two producers of a
/// record agree.
pub(super) fn record_omissions(
    record: &str,
    given: &[String],
    decls: &dyn Decls,
) -> Rule<Vec<Rc<str>>> {
    let mut optional = Vec::new();
    let mut missing = Vec::new();
    for (name, ty) in decls.fields(record) {
        if given.iter().any(|key| **key == *name) {
            continue;
        }
        if omittable(&ty) {
            optional.push(name);
        } else {
            missing.push(format!("`{name}`"));
        }
    }
    match missing.len() {
        0 => Ok(optional),
        1 => Err(format!("missing field {} of `{record}`", missing[0])),
        _ => Err(format!(
            "missing fields {} of `{record}`",
            missing.join(", ")
        )),
    }
}

/// Which construct is testing a type, which decides only what an enum's name is told
/// instead: `is` compares an enum, a `lem-case` matches its variants (D32).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TestForm {
    Is,
    Case,
}

/// What a type test may name (D35): a declared record. An enum is compared or matched,
/// not tested; a builtin type's name (or a spelling the hints answer for) is no record;
/// an unknown name is the unknown type it is.
pub(super) fn tested_type(name: &str, form: TestForm, decls: &dyn Decls) -> Rule<()> {
    match decls.kind(name) {
        Some(DeclKind::Record) => Ok(()),
        Some(DeclKind::Enum) if form == TestForm::Case => {
            let hint = decls
                .variants(name)
                .and_then(<[(Rc<str>, Span)]>::first)
                .map_or(String::new(), |(variant, _)| format!(" as `.{variant}`"));
            Err(format!("`{name}` is an enum: match its variants{hint}"))
        }
        Some(DeclKind::Enum) => Err(format!("`{name}` is an enum: compare it with `==`")),
        None if super::annotations::reserved(name).is_some() => {
            Err(format!("a type test names a record type, got `{name}`"))
        }
        None => Err(format!("unknown type `{name}`")),
    }
}

/// The `null` pattern of a `lem-match` (D35): its subject must be able to be null —
/// an optional, or `any`.
pub(super) fn null_pattern(subject: &Type) -> Rule<()> {
    match subject {
        Type::Optional(_) | Type::Any | Type::Null => Ok(()),
        other => Err(format!("a `{other}` is never null")),
    }
}

/// The `.hero` pattern of a `lem-match`: a variant of the subject's enum, which a `T?`
/// subject still names. Nothing else has variants, and the qualified spelling is not a
/// pattern, so the refusal names the subject rather than a respelling.
pub(super) fn variant_pattern(subject: &Type, name: &str, decls: &dyn Decls) -> Rule<()> {
    match expected_enum(Some(subject), decls) {
        Some(enum_name) => qualified_variant(enum_name, name, decls).map(|_| ()),
        None => Err(format!(
            "a `.{name}` pattern needs an enum subject, got `{subject}`"
        )),
    }
}

/// The exhaustiveness rule, on the provisional reading (`docs/language.md` §Checking;
/// the exact rules are a deferred question): a `lem-match` with no `lem-default` must
/// cover its subject, and only an enum can be covered — every variant, and `null` when
/// the subject is optional. A record is matched by its subtypes, and that set is open:
/// another file may declare one. `covered` is the variants the cases name.
pub(super) fn exhaustive(
    subject: &Type,
    covered: &[Rc<str>],
    null_covered: bool,
    has_default: bool,
    decls: &dyn Decls,
) -> Rule<()> {
    if has_default {
        return Ok(());
    }
    let optional = matches!(subject, Type::Optional(_));
    match subject.strip_optional() {
        Type::Named(name) if decls.kind(name) == Some(DeclKind::Enum) => {
            let variants = decls.variants(name).expect("an enum has variants");
            let mut missing: Vec<String> = variants
                .iter()
                .filter(|(variant, _)| !covered.contains(variant))
                .map(|(variant, _)| format!("`.{variant}`"))
                .collect();
            if optional && !null_covered {
                missing.push("`null`".to_owned());
            }
            if missing.is_empty() {
                Ok(())
            } else {
                Err(format!(
                    "`lem-match` over `{subject}` misses {}",
                    missing.join(", ")
                ))
            }
        }
        Type::Named(name) if decls.kind(name) == Some(DeclKind::Record) => Err(format!(
            "`lem-match` over `{subject}` needs a `lem-default`: a record is matched by \
             its subtypes, and that set is open"
        )),
        _ => Err(format!(
            "`lem-match` over `{subject}` needs a `lem-default`: only an enum is matched \
             exhaustively"
        )),
    }
}

/// The type test rule (D35): what `subject is Name` narrows its subject to, for a
/// `name` [`tested_type`] admitted. An `any` may be anything; a record may be a
/// subtype of itself, or — when it is optional — itself, since that is a null test.
/// A test that could not fail, or could not hold, is a mistake and is refused: the
/// subject's own type, an ancestor of it, or a record unrelated to it. Nothing but a
/// record carries a type to test.
pub(super) fn type_test(subject: &Type, name: &str, decls: &dyn Decls) -> Rule {
    let narrowed = Type::Named(Rc::from(name));
    let optional = matches!(subject, Type::Optional(_));
    match subject.strip_optional() {
        Type::Any => Ok(narrowed),
        Type::Named(record) if decls.kind(record) == Some(DeclKind::Record) => {
            if **record == *name {
                if optional {
                    Ok(narrowed)
                } else {
                    Err(format!(
                        "`{name}` is `{name}` already: the test always holds"
                    ))
                }
            } else if decls.extends(name, record) {
                Ok(narrowed)
            } else if decls.extends(record, name) {
                Err(format!(
                    "`{record}` extends `{name}`: the test always holds"
                ))
            } else {
                Err(format!(
                    "`{record}` and `{name}` are unrelated: the test never holds"
                ))
            }
        }
        other => Err(format!("a type test needs a record, got `{other}`")),
    }
}

/// The dotted variant rule (D32, revised by `∆EN6`): `.hero` is a variant of whichever
/// enum is expected where it stands, and of nothing else. Where no enum is expected it
/// has none to belong to — the refusal names the qualified spelling, which needs no
/// expectation and so works wherever the dotted one does not.
pub(super) fn dotted_variant(
    expected: Option<&Type>,
    name: &str,
    decls: &dyn Decls,
) -> Rule {
    match expected_enum(expected, decls) {
        Some(enum_name) => qualified_variant(enum_name, name, decls),
        None => Err(format!(
            "no enum is expected here: qualify the variant as `Enum.{name}`"
        )),
    }
}

/// The qualified variant rule: `Placement.hero`, which says which enum it belongs to
/// and so stands anywhere. `enum_name` is a declared enum — the walk checks that
/// before it decides the expression is this and not a field access.
pub(super) fn qualified_variant(enum_name: &str, name: &str, decls: &dyn Decls) -> Rule {
    let variants = decls
        .variants(enum_name)
        .expect("a qualified variant is only read off a declared enum");
    if variants.iter().any(|(v, _)| &**v == name) {
        Ok(Type::Named(Rc::from(enum_name)))
    } else {
        Err(format!("`{enum_name}` has no variant `{name}`"))
    }
}

/// What the right operand of `op` is lowered expecting when the left is an enum, so a
/// dotted variant may stand there: the enum itself under `==`/`!=`, and a list of it
/// under `in`. The expectation only shapes the operand — a literal, a variant — and
/// the operator rule then judges the two types, so an optional on either side is
/// still compared as the operator allows.
pub(super) fn operand_expectation(
    op: BinaryOp,
    lhs: &Type,
    decls: &dyn Decls,
) -> Option<Type> {
    let enum_lhs = lhs
        .strip_optional()
        .named()
        .is_some_and(|name| decls.kind(name) == Some(DeclKind::Enum));
    if !enum_lhs {
        return None;
    }
    match op {
        BinaryOp::Eq | BinaryOp::NotEq => Some(lhs.clone()),
        BinaryOp::In => Some(Type::list(Type::optional(lhs.clone()))),
        _ => None,
    }
}

/// The message for reading through a `T?` as if it were a `T`.
fn may_be_null(ty: &Type, alternative: &str) -> String {
    format!("a `{ty}` may be null: narrow it with `lem-if`{alternative}")
}

/// The type of `object[index]`: a list's element under an `int`, a map's value under a
/// `string`; `any` is the VM's to index.
pub(super) fn index(object: &Type, index: &Type, decls: &dyn Decls) -> Rule {
    let (element, key) = match object {
        Type::Any => return Ok(Type::Any),
        Type::Optional(_) => return Err(may_be_null(object, "")),
        Type::List(element) => ((**element).clone(), Type::Int),
        Type::Map(value) => ((**value).clone(), Type::String),
        other => return Err(format!("cannot index a `{other}`")),
    };
    if assignable(index, &key, decls) {
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
/// `null`, a list, a map, a record, or a function has no printed form; an enum prints
/// its variant's name; an optional has a printed form once it is known not to be null;
/// `any` is the VM's to print.
pub(super) fn printable(ty: &Type, decls: &dyn Decls) -> Rule<()> {
    match ty {
        Type::String
        | Type::Html
        | Type::Int
        | Type::Float
        | Type::Datetime
        | Type::Url
        | Type::Asset
        | Type::Any => Ok(()),
        Type::Named(name) if decls.kind(name) == Some(DeclKind::Enum) => Ok(()),
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

/// Whether `argc` arguments are what a builtin takes, the receiver of a method among
/// them. The message counts as the author wrote it: a receiver stands left of the dot,
/// so `xs.first(1)` is one argument given to something that takes none.
pub(super) fn builtin(builtin: &Builtin, argc: usize) -> Rule<()> {
    if argc == builtin.arity {
        Ok(())
    } else {
        let written = argc - (builtin.arity - builtin.written_arity());
        Err(arity(&builtin.written(), builtin.written_arity(), written))
    }
}

/// Whether `argc` arguments are what `mark()` takes: a name, and the payload the host's
/// claim names where it names one (D42). The one builtin with an optional positional,
/// so the one that cannot be counted by [`builtin`].
pub(super) fn mark_arity(argc: usize) -> Rule<()> {
    if (1..=2).contains(&argc) {
        Ok(())
    } else {
        Err(format!("`mark` takes one or two arguments, got {argc}"))
    }
}

/// Whether `ty` is something a mark can carry to a handler where no claim types it: a
/// record the program declares, a map — which is the record an author writes where
/// nothing typed one — or `any`, which the VM checks where the value is made.
///
/// A payload is what a handler is dispatched on and reads fields of, so a value with no
/// fields is a mistake worth catching: `default` is a named argument exactly so that the
/// positional slot means the payload and nothing else (D42). The VM refuses the same
/// three kinds where a value reaches it through `any`, so the two agree.
pub(super) fn mark_payload(ty: &Type, decls: &dyn Decls) -> Rule<()> {
    match ty {
        Type::Any | Type::Map(_) => Ok(()),
        Type::Named(name) if decls.kind(name) == Some(DeclKind::Record) => Ok(()),
        _ => Err(format!("a mark's payload is a record, got {ty}")),
    }
}

/// Whether a mark of `name` can carry the declared record `record`: a payload crosses to
/// the host as data, so neither its fields nor those of a record they hold may be a
/// function, which has no form outside the program that made it.
///
/// The refusal names the path that reaches the field, since the record a claim names is
/// often not the one that declares it.
pub(super) fn payload_record(name: &str, record: &str, decls: &dyn Decls) -> Rule<()> {
    let mut pending: Vec<(Rc<str>, String)> = vec![(Rc::from(record), String::new())];
    let mut seen: Vec<Rc<str>> = Vec::new();
    while let Some((declared, path)) = pending.pop() {
        if seen.contains(&declared) {
            continue;
        }
        seen.push(declared.clone());
        for decl in decls.chain(&declared) {
            let Shape::Record { fields, .. } = &decl.shape else {
                continue;
            };
            for field in fields {
                let at = if path.is_empty() {
                    field.name.to_string()
                } else {
                    format!("{path}.{}", field.name)
                };
                match carried(&field.ty) {
                    Carried::Data => {}
                    Carried::Record(held) => pending.push((held, at)),
                    Carried::Function => {
                        return Err(format!(
                            "a mark's payload crosses to the handler as data, and \
                             `{record}.{at}` is a function, so `{name}` cannot carry it"
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

/// What carrying a value of `ty` to a handler needs: nothing, a walk into the record it
/// holds, or a refusal. A list, map or optional is followed into its element, which is
/// where a function would hide.
enum Carried {
    Data,
    Record(Rc<str>),
    Function,
}

fn carried(ty: &Type) -> Carried {
    match ty {
        Type::Fn(_, _) => Carried::Function,
        Type::List(inner) | Type::Map(inner) | Type::Optional(inner) => carried(inner),
        Type::Named(name) => Carried::Record(name.clone()),
        _ => Carried::Data,
    }
}

/// What a lambda is lowered under: its parameter types, and the return type its body is
/// checked against — `None` leaves the body to inference, as a builtin's function
/// argument does (D34).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct LambdaShape {
    pub(super) params: Rc<[Type]>,
    pub(super) ret: Option<Type>,
}

/// What a lambda with `params` parameters takes from the type expected of it: its
/// parameter types and the return type its body is checked against, when a function
/// type of the same arity is expected; nothing when nothing is, or something else is
/// (the mismatch is then reported against the whole lambda); a refusal when a function
/// type of another arity is.
pub(super) fn lambda_shape(
    expected: Option<&Type>,
    params: usize,
) -> Rule<Option<LambdaShape>> {
    match expectation(expected) {
        Some(Type::Fn(param_tys, ret)) if param_tys.len() == params => {
            Ok(Some(LambdaShape {
                params: param_tys.clone(),
                ret: Some((**ret).clone()),
            }))
        }
        Some(want @ Type::Fn(param_tys, _)) => Err(format!(
            "this lambda takes {}, but a `{want}` takes {}",
            count(params, "parameter"),
            param_tys.len()
        )),
        _ => Ok(None),
    }
}

/// The shape of a lambda with `lambda_params` parameters written where `written` expects
/// a function of `params` and leaves the return to it; a refusal when the arities differ.
pub(super) fn lambda_of(
    written: &str,
    params: Vec<Type>,
    lambda_params: usize,
) -> Rule<LambdaShape> {
    if params.len() == lambda_params {
        Ok(LambdaShape {
            params: Rc::from(params),
            ret: None,
        })
    } else {
        Err(format!(
            "this lambda takes {}, but {written} passes {}",
            count(lambda_params, "parameter"),
            count(params.len(), "argument")
        ))
    }
}

/// Whether a value of type `actual` may stand where `written` expects a function of
/// `params`: a function whose parameters each accept what is passed — the contravariance
/// a function type's assignability already has — or an `any`, which the VM checks.
pub(super) fn function_of(
    written: &str,
    params: &[Type],
    actual: &Type,
    decls: &dyn Decls,
) -> Rule {
    let fits = match actual {
        Type::Any => true,
        Type::Fn(wanted, _) => {
            wanted.len() == params.len()
                && params
                    .iter()
                    .zip(wanted.iter())
                    .all(|(given, wanted)| assignable(given, wanted, decls))
        }
        _ => false,
    };
    if fits {
        Ok(actual.clone())
    } else {
        let params: Vec<String> = params.iter().map(ToString::to_string).collect();
        Err(format!(
            "{written} takes a function of `({})`, got `{actual}`",
            params.join(", ")
        ))
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
    /// The `n`th positional argument.
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
    use crate::builtins::BUILTINS;
    use crate::decls::tests::{Table, enumeration, record};

    fn named(name: &str) -> Type {
        Type::Named(Rc::from(name))
    }

    /// `Article extends Page`, and an enum, for the rules that read declarations.
    fn table() -> Table {
        Table(vec![
            record("Page", None, &[("url", Type::Url)]),
            record(
                "Article",
                Some("Page"),
                &[
                    ("title", Type::String),
                    ("layout", Type::function(vec![], Type::Html)),
                ],
            ),
            enumeration("Placement", &["hero", "left"]),
        ])
    }

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
            element(&Type::Int, &Type::Null, "list's elements", &()),
            Ok(opt(Type::Int))
        );
        assert_eq!(
            element(&Type::Int, &Type::String, "map's values", &()),
            Err("expected `int`, got `string`: a map's values share one type".to_owned())
        );
    }

    #[test]
    fn joins_a_ternarys_branches_and_refuses_two_that_share_nothing() {
        assert_eq!(branches(&Type::Int, &Type::Null, &()), Ok(opt(Type::Int)));
        assert_eq!(
            branches(&Type::Int, &Type::String, &()),
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
            operator(BinaryOp::Add, "+", &Type::Int, &Type::Int, &()),
            Ok(Type::Int)
        );
        assert_eq!(
            operator(BinaryOp::Mul, "*", &Type::Int, &Type::Float, &()),
            Ok(Type::Float)
        );
        assert_eq!(
            operator(BinaryOp::Rem, "%", &Type::Float, &Type::Float, &()),
            Ok(Type::Float)
        );
        assert_eq!(
            operator(BinaryOp::Sub, "-", &Type::Any, &Type::Bool, &()),
            Ok(Type::Any)
        );
    }

    #[test]
    fn concatenates_strings_with_plus_and_nothing_else() {
        assert_eq!(
            operator(BinaryOp::Add, "+", &Type::String, &Type::String, &()),
            Ok(Type::String)
        );
        assert_eq!(
            operator(BinaryOp::Sub, "-", &Type::String, &Type::String, &()),
            Err("cannot apply `-` to `string` and `string`".to_owned())
        );
        assert_eq!(
            operator(BinaryOp::Add, "+", &Type::String, &Type::Int, &()),
            Err("cannot apply `+` to `string` and `int`".to_owned())
        );
    }

    #[test]
    fn orders_numbers_strings_and_datetimes_among_themselves() {
        assert_eq!(
            operator(BinaryOp::Lt, "lt", &Type::Int, &Type::Float, &()),
            Ok(Type::Bool)
        );
        assert_eq!(
            operator(BinaryOp::Gte, "gte", &Type::String, &Type::String, &()),
            Ok(Type::Bool)
        );
        assert_eq!(
            operator(BinaryOp::Gt, "gt", &Type::Datetime, &Type::Datetime, &()),
            Ok(Type::Bool)
        );
        assert_eq!(
            operator(BinaryOp::Lte, "lte", &Type::Any, &Type::Bool, &()),
            Ok(Type::Bool)
        );
        assert_eq!(
            operator(BinaryOp::Lt, "lt", &Type::Int, &Type::String, &()),
            Err("cannot apply `lt` to `int` and `string`".to_owned())
        );
        assert_eq!(
            operator(BinaryOp::Gt, "gt", &Type::Bool, &Type::Bool, &()),
            Err("cannot apply `gt` to `bool` and `bool`".to_owned())
        );
    }

    #[test]
    fn compares_numbers_and_assignable_types_but_never_functions() {
        assert_eq!(
            operator(BinaryOp::Eq, "==", &Type::Int, &Type::Float, &()),
            Ok(Type::Bool)
        );
        assert_eq!(
            operator(BinaryOp::NotEq, "!=", &Type::Int, &opt(Type::Int), &()),
            Ok(Type::Bool)
        );
        assert_eq!(
            operator(BinaryOp::Eq, "==", &opt(Type::Int), &Type::Int, &()),
            Ok(Type::Bool)
        );
        assert_eq!(
            operator(BinaryOp::Eq, "==", &Type::Int, &Type::String, &()),
            Err("cannot compare `int` with `string`".to_owned())
        );
        let f = func(vec![], Type::Int);
        assert_eq!(
            operator(BinaryOp::Eq, "==", &f, &f, &()),
            Err("functions cannot be compared".to_owned())
        );
    }

    #[test]
    fn checks_the_left_of_in_against_the_lists_element_or_a_maps_key() {
        assert_eq!(
            operator(BinaryOp::In, "in", &Type::Int, &Type::list(Type::Int), &()),
            Ok(Type::Bool)
        );
        assert_eq!(
            operator(
                BinaryOp::In,
                "in",
                &Type::String,
                &Type::map(Type::Int),
                &()
            ),
            Ok(Type::Bool)
        );
        assert_eq!(
            operator(BinaryOp::In, "in", &Type::Bool, &Type::Any, &()),
            Ok(Type::Bool)
        );
        assert_eq!(
            operator(
                BinaryOp::In,
                "in",
                &Type::String,
                &Type::list(Type::Int),
                &()
            ),
            Err("`in` on a `[int]` needs `int` on its left, got `string`".to_owned())
        );
        assert_eq!(
            operator(BinaryOp::In, "in", &Type::Int, &Type::map(Type::Int), &()),
            Err("`in` on a map needs a `string` key on its left, got `int`".to_owned())
        );
        assert_eq!(
            operator(BinaryOp::In, "in", &Type::Int, &Type::Int, &()),
            Err("cannot apply `in` to `int` and `int`".to_owned())
        );
    }

    #[test]
    fn reads_a_field_of_a_record_by_its_declaration_and_its_ancestors() {
        let table = table();
        assert_eq!(
            member(&named("Article"), "title", false, &table),
            Ok(Type::String)
        );
        assert_eq!(
            member(&named("Article"), "url", false, &table),
            Ok(Type::Url)
        );
        assert_eq!(
            member(&opt(named("Article")), "url", true, &table),
            Ok(opt(Type::Url))
        );
        assert_eq!(
            member(&named("Article"), "nope", false, &table),
            Err("`Article` has no field `nope`".to_owned())
        );
        assert_eq!(
            member(&named("Placement"), "hero", false, &table),
            Err("a `Placement` is an enum and has no fields".to_owned())
        );
    }

    #[test]
    fn names_a_method_read_without_its_call_rather_than_calling_it_no_field() {
        assert_eq!(
            member(&Type::String, "slugify", false, &()),
            Err("`slugify` is a method of `string`: call it as `.slugify()`".to_owned())
        );
        assert_eq!(
            member(&Type::list(Type::Int), "first", false, &()),
            Err("`first` is a method of `[int]`: call it as `.first()`".to_owned())
        );
    }

    /// The id of the method `kind` carries under `name`, so a test names the entry it
    /// is about and the table's order stays the table's business.
    fn method(kind: ReceiverKind, name: &str) -> u32 {
        builtins::method_id(kind, name).expect("a method")
    }

    /// The entry an ambient builtin or a method is, for a rule that takes one.
    fn entry(reach: Option<ReceiverKind>, name: &str) -> &'static Builtin {
        let id = match reach {
            Some(kind) => method(kind, name),
            None => builtins::lookup(name).expect("an ambient function"),
        };
        &BUILTINS[id as usize]
    }

    #[test]
    fn calls_a_builtin_types_method_and_a_records_field() {
        let table = table();
        assert_eq!(
            callable_member(&Type::String, "slugify", false, &()),
            Ok(Callable::Method(method(ReceiverKind::String, "slugify")))
        );
        assert_eq!(
            callable_member(&Type::list(Type::Int), "first", false, &()),
            Ok(Callable::Method(method(ReceiverKind::List, "first")))
        );
        assert_eq!(
            callable_member(&Type::map(Type::Int), "keys", false, &()),
            Ok(Callable::Method(method(ReceiverKind::Map, "keys")))
        );
        assert_eq!(
            callable_member(&named("Article"), "layout", false, &table),
            Ok(Callable::Field(Type::function(vec![], Type::Html)))
        );
    }

    #[test]
    fn leaves_a_call_on_any_to_the_vm_and_reads_through_an_optional_receiver() {
        assert_eq!(
            callable_member(&Type::Any, "whatever", false, &()),
            Ok(Callable::Dynamic)
        );
        assert_eq!(
            callable_member(&Type::Null, "whatever", true, &()),
            Ok(Callable::Dynamic)
        );
        assert_eq!(
            callable_member(&Type::optional(Type::String), "slugify", true, &()),
            Ok(Callable::Method(method(ReceiverKind::String, "slugify")))
        );
        assert_eq!(
            callable_member(&Type::optional(Type::String), "slugify", false, &()),
            Err("a `string?` may be null: narrow it with `lem-if`, or use `?.`".to_owned())
        );
        assert_eq!(
            callable_member(&Type::String, "slugify", true, &()),
            Err("`?.` needs an optional, got `string`".to_owned())
        );
    }

    #[test]
    fn suggests_the_method_a_misspelling_meant_whether_it_was_read_or_called() {
        // Read and called are two paths to one mistake, and used to answer differently:
        // the call form listed what the type had, the read form offered nothing (∆eHR).
        assert_eq!(
            member(&Type::String, "slugfy", false, &()),
            Err("a `string` has no field `slugfy`; did you mean `.slugify()`?".to_owned())
        );
        assert_eq!(
            callable_member(&Type::String, "slugfy", false, &()),
            Err("a `string` has no method `slugfy`; did you mean `.slugify()`?".to_owned())
        );
        assert_eq!(
            member(&Type::list(Type::Int), "lenght", false, &()),
            Err("a `[int]` has no field `lenght`; did you mean `.length()`?".to_owned())
        );
    }

    #[test]
    fn suggests_nothing_for_a_name_near_no_method_it_carries() {
        // The other half: a guess offered for anything would be a guess worth nothing.
        assert_eq!(
            member(&Type::String, "byline", false, &()),
            Err("a `string` has no field `byline`".to_owned())
        );
        assert_eq!(
            member(&Type::Bool, "slugfy", false, &()),
            Err("a `bool` has no field `slugfy`".to_owned())
        );
    }

    #[test]
    fn refuses_a_method_a_builtin_type_has_not_got_and_lists_the_ones_it_has() {
        assert_eq!(
            callable_member(&Type::String, "nope", false, &()),
            Err(
                "a `string` has no method `nope`; it has `.contains()`, `.endsWith()`, \
                 `.length()`, `.lower()`, `.replace()`, `.slugify()`, `.split()`, \
                 `.startsWith()`, `.trim()`, `.upper()`"
                    .to_owned()
            )
        );
        assert_eq!(
            callable_member(&Type::Int, "nope", false, &()),
            Err("a `int` has no method `nope`; it has `.abs()`".to_owned())
        );
    }

    #[test]
    fn counts_a_methods_receiver_out_of_the_arity_it_reports() {
        let raw = entry(None, "raw");
        assert_eq!(builtin(raw, 1), Ok(()));
        assert_eq!(
            builtin(raw, 2),
            Err("`raw` takes 1 argument, got 2".to_owned())
        );
        let slugify = entry(Some(ReceiverKind::String), "slugify");
        assert_eq!(builtin(slugify, 1), Ok(()));
        assert_eq!(
            builtin(slugify, 2),
            Err("`.slugify` takes 0 arguments, got 1".to_owned())
        );
    }

    #[test]
    fn reads_a_record_literal_only_under_an_expected_record() {
        let table = table();
        assert_eq!(
            expected_record(Some(&named("Article")), &table),
            Some("Article")
        );
        assert_eq!(
            expected_record(Some(&opt(named("Article"))), &table),
            Some("Article")
        );
        assert_eq!(expected_record(Some(&named("Placement")), &table), None);
        assert_eq!(expected_record(Some(&Type::map(Type::Any)), &table), None);
        assert_eq!(expected_record(Some(&Type::Any), &table), None);
        assert_eq!(expected_record(None, &table), None);
    }

    #[test]
    fn checks_a_record_literals_entries_against_the_declared_fields() {
        let table = table();
        assert_eq!(record_entry("Article", "title", &table), Ok(Type::String));
        assert_eq!(record_entry("Article", "url", &table), Ok(Type::Url));
        assert_eq!(
            record_entry("Article", "body", &table),
            Err("`Article` has no field `body`".to_owned())
        );
    }

    #[test]
    fn fills_omitted_optionals_and_refuses_omitted_required_fields() {
        let table = Table(vec![
            record(
                "Page",
                None,
                &[("url", Type::Url), ("draft", opt(Type::Bool))],
            ),
            record(
                "Article",
                Some("Page"),
                &[
                    ("title", Type::String),
                    ("subhead", opt(Type::String)),
                    ("date", Type::Datetime),
                ],
            ),
        ]);
        let given = |keys: &[&str]| -> Vec<String> {
            keys.iter().map(|k| (*k).to_owned()).collect()
        };
        assert_eq!(
            record_omissions("Article", &given(&["title", "date", "url"]), &table),
            Ok(vec![Rc::from("subhead"), Rc::from("draft")])
        );
        assert_eq!(
            record_omissions(
                "Article",
                &given(&["title", "date", "url", "subhead", "draft"]),
                &table
            ),
            Ok(vec![])
        );
        assert_eq!(
            record_omissions("Article", &given(&["date", "url"]), &table),
            Err("missing field `title` of `Article`".to_owned())
        );
        assert_eq!(
            record_omissions("Article", &given(&["subhead"]), &table),
            Err("missing fields `title`, `date`, `url` of `Article`".to_owned())
        );
        // `any` admits `null`, and is what `any?` folds to, so it may be left out.
        assert!(omittable(&Type::Any));
        assert!(omittable(&opt(Type::Int)));
        assert!(!omittable(&Type::Int));
    }

    #[test]
    fn lets_a_type_test_name_only_a_declared_record() {
        let table = table();
        assert_eq!(tested_type("Article", TestForm::Is, &table), Ok(()));
        assert_eq!(
            tested_type("Placement", TestForm::Is, &table),
            Err("`Placement` is an enum: compare it with `==`".to_owned())
        );
        assert_eq!(
            tested_type("string", TestForm::Is, &table),
            Err("a type test names a record type, got `string`".to_owned())
        );
        assert_eq!(
            tested_type("map", TestForm::Is, &table),
            Err("a type test names a record type, got `map`".to_owned())
        );
        assert_eq!(
            tested_type("Nope", TestForm::Is, &table),
            Err("unknown type `Nope`".to_owned())
        );
    }

    #[test]
    fn tells_a_case_pattern_to_match_an_enums_variants_rather_than_compare_it() {
        let table = table();
        assert_eq!(
            tested_type("Placement", TestForm::Case, &table),
            Err("`Placement` is an enum: match its variants as `.hero`".to_owned())
        );
        // Every other verdict is the same whichever construct asked.
        assert_eq!(tested_type("Article", TestForm::Case, &table), Ok(()));
        assert_eq!(
            tested_type("Nope", TestForm::Case, &table),
            Err("unknown type `Nope`".to_owned())
        );
    }

    #[test]
    fn narrows_a_type_test_to_a_subtype_and_refuses_one_that_cannot_fail_or_hold() {
        let table = Table(vec![
            record("Page", None, &[]),
            record("Article", Some("Page"), &[]),
            record("LinkPost", Some("Article"), &[]),
            record("Photo", None, &[]),
            enumeration("Placement", &["hero"]),
        ]);
        assert_eq!(
            type_test(&named("Article"), "LinkPost", &table),
            Ok(named("LinkPost"))
        );
        assert_eq!(
            type_test(&opt(named("Article")), "LinkPost", &table),
            Ok(named("LinkPost"))
        );
        assert_eq!(
            type_test(&Type::Any, "Article", &table),
            Ok(named("Article"))
        );
        assert_eq!(
            type_test(&opt(named("Article")), "Article", &table),
            Ok(named("Article"))
        );
        assert_eq!(
            type_test(&named("Article"), "Article", &table),
            Err("`Article` is `Article` already: the test always holds".to_owned())
        );
        assert_eq!(
            type_test(&named("LinkPost"), "Page", &table),
            Err("`LinkPost` extends `Page`: the test always holds".to_owned())
        );
        assert_eq!(
            type_test(&named("Article"), "Photo", &table),
            Err("`Article` and `Photo` are unrelated: the test never holds".to_owned())
        );
        assert_eq!(
            type_test(&opt(Type::list(Type::Int)), "Photo", &table),
            Err("a type test needs a record, got `[int]`".to_owned())
        );
        assert_eq!(
            type_test(&named("Placement"), "Photo", &table),
            Err("a type test needs a record, got `Placement`".to_owned())
        );
    }

    #[test]
    fn matches_null_only_against_a_subject_that_may_be_null() {
        assert_eq!(null_pattern(&opt(Type::Int)), Ok(()));
        assert_eq!(null_pattern(&Type::Any), Ok(()));
        assert_eq!(null_pattern(&Type::Null), Ok(()));
        assert_eq!(
            null_pattern(&named("Article")),
            Err("a `Article` is never null".to_owned())
        );
    }

    #[test]
    fn matches_a_variant_pattern_against_the_subjects_enum() {
        let table = table();
        assert_eq!(variant_pattern(&named("Placement"), "hero", &table), Ok(()));
        assert_eq!(
            variant_pattern(&opt(named("Placement")), "left", &table),
            Ok(())
        );
        assert_eq!(
            variant_pattern(&named("Placement"), "top", &table),
            Err("`Placement` has no variant `top`".to_owned())
        );
        assert_eq!(
            variant_pattern(&Type::Any, "hero", &table),
            Err("a `.hero` pattern needs an enum subject, got `any`".to_owned())
        );
        assert_eq!(
            variant_pattern(&named("Article"), "hero", &table),
            Err("a `.hero` pattern needs an enum subject, got `Article`".to_owned())
        );
    }

    #[test]
    fn requires_every_variant_or_a_default_and_a_default_for_anything_else() {
        let table = table();
        let hero = [Rc::from("hero")];
        let both = [Rc::from("hero"), Rc::from("left")];
        assert_eq!(
            exhaustive(&named("Placement"), &both, false, false, &table),
            Ok(())
        );
        assert_eq!(
            exhaustive(&named("Placement"), &hero, false, true, &table),
            Ok(())
        );
        assert_eq!(
            exhaustive(&named("Placement"), &hero, false, false, &table),
            Err("`lem-match` over `Placement` misses `.left`".to_owned())
        );
        assert_eq!(
            exhaustive(&opt(named("Placement")), &both, true, false, &table),
            Ok(())
        );
        assert_eq!(
            exhaustive(&opt(named("Placement")), &hero, false, false, &table),
            Err("`lem-match` over `Placement?` misses `.left`, `null`".to_owned())
        );
        assert_eq!(
            exhaustive(&named("Article"), &[], false, false, &table),
            Err(
                "`lem-match` over `Article` needs a `lem-default`: a record is matched \
                 by its subtypes, and that set is open"
                    .to_owned()
            )
        );
        assert_eq!(
            exhaustive(&named("Article"), &[], false, true, &table),
            Ok(())
        );
        assert_eq!(
            exhaustive(&opt(Type::String), &[], true, false, &table),
            Err(
                "`lem-match` over `string?` needs a `lem-default`: only an enum is \
                 matched exhaustively"
                    .to_owned()
            )
        );
        assert_eq!(
            exhaustive(&Type::Any, &[], false, false, &table),
            Err(
                "`lem-match` over `any` needs a `lem-default`: only an enum is matched \
                 exhaustively"
                    .to_owned()
            )
        );
    }

    #[test]
    fn admits_a_dotted_variant_only_where_an_enum_is_expected() {
        let table = table();
        let refused =
            Err("no enum is expected here: qualify the variant as `Enum.hero`".to_owned());
        assert_eq!(
            dotted_variant(Some(&named("Placement")), "hero", &table),
            Ok(named("Placement"))
        );
        // An optional expectation asks for the variant the same way a literal is asked.
        assert_eq!(
            dotted_variant(Some(&opt(named("Placement"))), "left", &table),
            Ok(named("Placement"))
        );
        assert_eq!(
            dotted_variant(Some(&named("Placement")), "hreo", &table),
            Err("`Placement` has no variant `hreo`".to_owned())
        );
        // Every expectation that is not an enum refuses the same way: a record, a
        // builtin type, `any`, and nothing expected at all.
        assert_eq!(
            dotted_variant(Some(&named("Article")), "hero", &table),
            refused
        );
        assert_eq!(dotted_variant(Some(&Type::String), "hero", &table), refused);
        assert_eq!(dotted_variant(Some(&Type::Any), "hero", &table), refused);
        assert_eq!(dotted_variant(None, "hero", &table), refused);
    }

    #[test]
    fn admits_a_qualified_variant_with_nothing_expected() {
        let table = table();
        assert_eq!(
            qualified_variant("Placement", "hero", &table),
            Ok(named("Placement"))
        );
        assert_eq!(
            qualified_variant("Placement", "hreo", &table),
            Err("`Placement` has no variant `hreo`".to_owned())
        );
    }

    #[test]
    fn reads_the_expected_enum_through_an_optional_and_past_everything_else() {
        let table = table();
        assert_eq!(
            expected_enum(Some(&named("Placement")), &table),
            Some("Placement")
        );
        assert_eq!(
            expected_enum(Some(&opt(named("Placement"))), &table),
            Some("Placement")
        );
        assert_eq!(expected_enum(Some(&named("Article")), &table), None);
        assert_eq!(expected_enum(Some(&named("Nope")), &table), None);
        assert_eq!(expected_enum(Some(&Type::Any), &table), None);
        assert_eq!(expected_enum(None, &table), None);
    }

    #[test]
    fn expects_an_enum_of_the_right_operand_only_beside_an_enum() {
        let table = table();
        let placement = named("Placement");
        assert_eq!(
            operand_expectation(BinaryOp::Eq, &placement, &table),
            Some(placement.clone())
        );
        assert_eq!(
            operand_expectation(BinaryOp::NotEq, &opt(placement.clone()), &table),
            Some(opt(placement.clone()))
        );
        assert_eq!(
            operand_expectation(BinaryOp::In, &placement, &table),
            Some(Type::list(opt(placement.clone())))
        );
        assert_eq!(operand_expectation(BinaryOp::Lt, &placement, &table), None);
        assert_eq!(
            operand_expectation(BinaryOp::Eq, &named("Article"), &table),
            None
        );
        assert_eq!(operand_expectation(BinaryOp::Eq, &Type::Int, &table), None);
    }

    #[test]
    fn compares_records_along_their_chain() {
        let table = table();
        assert_eq!(
            operator(
                BinaryOp::Eq,
                "==",
                &named("Article"),
                &named("Page"),
                &table
            ),
            Ok(Type::Bool)
        );
        assert_eq!(
            operator(
                BinaryOp::Eq,
                "==",
                &named("Article"),
                &named("Placement"),
                &table
            ),
            Err("cannot compare `Article` with `Placement`".to_owned())
        );
        assert_eq!(
            operator(
                BinaryOp::In,
                "in",
                &named("Article"),
                &Type::list(named("Page")),
                &table
            ),
            Ok(Type::Bool)
        );
    }

    #[test]
    fn assigns_a_record_to_its_base_and_joins_them_to_the_base() {
        let table = table();
        assert_eq!(
            expected(named("Article"), &named("Page"), &table),
            Ok(named("Article"))
        );
        assert_eq!(
            expected(named("Page"), &named("Article"), &table),
            Err("expected `Article`, got `Page`".to_owned())
        );
        assert_eq!(
            branches(&named("Article"), &named("Page"), &table),
            Ok(named("Page"))
        );
    }

    #[test]
    fn prints_an_enum_and_never_a_record() {
        let table = table();
        assert_eq!(printable(&named("Placement"), &table), Ok(()));
        assert_eq!(
            printable(&named("Article"), &table),
            Err("cannot print a `Article`".to_owned())
        );
    }

    #[test]
    fn reads_a_field_of_any_and_of_nothing_else_undeclared() {
        assert_eq!(member(&Type::Any, "f", false, &()), Ok(Type::Any));
        assert_eq!(member(&Type::Any, "f", true, &()), Ok(Type::Any));
        assert_eq!(member(&opt(Type::Any), "f", true, &()), Ok(Type::Any));
        assert_eq!(
            member(&Type::String, "length", false, &()),
            Err("`length` is a method of `string`: call it as `.length()`".to_owned())
        );
        assert_eq!(
            member(&Type::map(Type::Int), "a", false, &()),
            Err("a `{string: int}` has no fields: index a map as `[\"a\"]`".to_owned())
        );
        assert_eq!(
            member(&Type::Null, "f", false, &()),
            Err("cannot read field `f` of null".to_owned())
        );
    }

    #[test]
    fn reads_through_an_optional_only_with_the_optional_chain() {
        assert_eq!(
            member(&opt(Type::String), "length", false, &()),
            Err("a `string?` may be null: narrow it with `lem-if`, or use `?.`".to_owned())
        );
        assert_eq!(
            member(&opt(Type::String), "length", true, &()),
            Err("`length` is a method of `string`: call it as `.length()`".to_owned())
        );
        assert_eq!(member(&Type::Null, "f", true, &()), Ok(Type::Null));
        assert_eq!(
            member(&Type::String, "length", true, &()),
            Err("`?.` needs an optional, got `string`".to_owned())
        );
    }

    #[test]
    fn indexes_a_list_by_int_and_a_map_by_string() {
        assert_eq!(
            index(&Type::list(Type::Bool), &Type::Int, &()),
            Ok(Type::Bool)
        );
        assert_eq!(
            index(&Type::map(Type::Bool), &Type::String, &()),
            Ok(Type::Bool)
        );
        assert_eq!(
            index(&Type::list(Type::Bool), &Type::Any, &()),
            Ok(Type::Bool)
        );
        assert_eq!(index(&Type::Any, &Type::Bool, &()), Ok(Type::Any));
        assert_eq!(
            index(&Type::list(Type::Int), &Type::String, &()),
            Err("a list is indexed by `int`, got `string`".to_owned())
        );
        assert_eq!(
            index(&Type::map(Type::Int), &Type::Int, &()),
            Err("a map is indexed by `string`, got `int`".to_owned())
        );
    }

    #[test]
    fn refuses_to_index_an_optional_or_a_scalar() {
        assert_eq!(
            index(&opt(Type::map(Type::Int)), &Type::String, &()),
            Err("a `{string: int}?` may be null: narrow it with `lem-if`".to_owned())
        );
        assert_eq!(
            index(&Type::Int, &Type::Int, &()),
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
            assert_eq!(printable(&ty, &()), Ok(()), "{ty}");
        }
    }

    #[test]
    fn refuses_to_print_what_has_no_printed_form() {
        assert_eq!(
            printable(&Type::Bool, &()),
            Err("cannot print a `bool`".to_owned())
        );
        assert_eq!(
            printable(&Type::Null, &()),
            Err("cannot print a `null`".to_owned())
        );
        assert_eq!(
            printable(&Type::list(Type::Int), &()),
            Err("cannot print a `[int]`".to_owned())
        );
        assert_eq!(
            printable(&Type::map(Type::Int), &()),
            Err("cannot print a `{string: int}`".to_owned())
        );
        assert_eq!(
            printable(&opt(Type::String), &()),
            Err(
                "cannot print a `string?`: narrow it with `lem-if`, or give it a default \
                 with `??`"
                    .to_owned()
            )
        );
        assert_eq!(
            printable(&func(vec![Type::Int], Type::Int), &()),
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
        assert_eq!(expected(Type::Int, &opt(Type::Int), &()), Ok(Type::Int));
        assert_eq!(expected(Type::Any, &Type::Int, &()), Ok(Type::Any));
        assert_eq!(
            expected(Type::String, &Type::Int, &()),
            Err("expected `int`, got `string`".to_owned())
        );
    }

    #[test]
    fn holds_a_builtin_to_its_arity() {
        let raw = entry(None, "raw");
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
            Ok(Some(LambdaShape {
                params: Rc::from(vec![Type::Int, Type::String]),
                ret: Some(Type::Bool),
            }))
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
    fn shapes_a_lambda_a_builtin_passes_arguments_to_with_its_return_left_open() {
        assert_eq!(
            lambda_of("`.map`", vec![Type::Int], 1),
            Ok(LambdaShape {
                params: Rc::from(vec![Type::Int]),
                ret: None,
            })
        );
        assert_eq!(
            lambda_of("`.map`", vec![Type::Int], 2),
            Err("this lambda takes 2 parameters, but `.map` passes 1 argument".to_owned())
        );
    }

    #[test]
    fn admits_a_function_value_whose_parameters_accept_what_a_builtin_passes() {
        let decls = &table();
        let f = func(vec![Type::Any], Type::String);
        assert_eq!(
            function_of("`.map`", &[Type::Int], &f, decls),
            Ok(f.clone())
        );
        assert_eq!(
            function_of("`.map`", &[Type::Int], &Type::Any, decls),
            Ok(Type::Any)
        );
        let g = func(vec![Type::String], Type::String);
        assert_eq!(
            function_of("`.map`", &[Type::Int], &g, decls),
            Err("`.map` takes a function of `(int)`, got `(string): string`".to_owned())
        );
        assert_eq!(
            function_of("`.map`", &[Type::Int, Type::Int], &f, decls),
            Err("`.map` takes a function of `(int, int)`, got `(any): string`".to_owned())
        );
        assert_eq!(
            function_of("`.map`", &[Type::Int], &Type::Int, decls),
            Err("`.map` takes a function of `(int)`, got `int`".to_owned())
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

//! What an `int` and a `float` carry.
//!
//! Only what a page actually shows: a magnitude, the two ways of turning a `float` into
//! an `int`, and the fixed-decimal formatting an aperture or a duration needs. The
//! arithmetic itself is the operators' (`docs/language.md` §Expressions), not a method's.
//!
//! Nothing here rounds silently past what an `int` holds. A `float` too large to be one,
//! or a NaN, is a render error naming the value — the alternative is Rust's saturating
//! cast, which would put `9223372036854775807` on a page and call it a number.

use crate::Value;

use super::{Fault, Host, method_receiver};

/// The receiver as a float, or the refusal for a value only an `any` call could have
/// handed a float method.
fn float(name: &str, args: &[Value]) -> Result<f64, Fault> {
    match &args[0] {
        Value::Float(f) => Ok(*f),
        other => Err(method_receiver(name, "a float", other)),
    }
}

/// A rounded float as an `int`, or why it is not one.
fn whole(name: &str, rounded: f64) -> Result<Value, Fault> {
    // The bound is stated as a float because that is what the comparison is in: every
    // integer this admits is one an `i64` holds exactly.
    const LIMIT: f64 = 9_223_372_036_854_775_808.0;
    if rounded.is_finite() && (-LIMIT..LIMIT).contains(&rounded) {
        #[allow(
            clippy::cast_possible_truncation,
            reason = "bounded just above, and already whole"
        )]
        Ok(Value::Int(rounded as i64))
    } else {
        Err(format!("`.{name}()` cannot make an int of {rounded}").into())
    }
}

/// `n.abs()` on an `int`: its magnitude.
pub(super) fn int_abs(_host: &mut dyn Host, args: &[Value]) -> Result<Value, Fault> {
    match &args[0] {
        Value::Int(n) => n
            .checked_abs()
            .map(Value::Int)
            .ok_or_else(|| Fault::from("integer overflow")),
        other => Err(method_receiver("abs", "an int", other)),
    }
}

/// `x.abs()` on a `float`: its magnitude.
pub(super) fn float_abs(_host: &mut dyn Host, args: &[Value]) -> Result<Value, Fault> {
    Ok(Value::Float(float("abs", args)?.abs()))
}

/// `x.round()`: the nearest whole number, halves away from zero.
pub(super) fn round(_host: &mut dyn Host, args: &[Value]) -> Result<Value, Fault> {
    whole("round", float("round", args)?.round())
}

/// `x.floor()`: the whole number at or below.
pub(super) fn floor(_host: &mut dyn Host, args: &[Value]) -> Result<Value, Fault> {
    whole("floor", float("floor", args)?.floor())
}

/// `x.fixed(n)`: the number written with exactly `n` digits after the point — `ƒ/2.80`
/// before a site trims the zero off it.
///
/// A half rounds to the even digit (`0.5` written with no decimals is `0`, `1.5` is
/// `2`), which is what Rust's formatter does and what IEEE 754 calls the default. It is
/// not what JavaScript's `toFixed` does, so a page that shows a number landing exactly
/// on a half can differ from the Eleventy build by one in the last digit.
pub(super) fn fixed(_host: &mut dyn Host, args: &[Value]) -> Result<Value, Fault> {
    let value = float("fixed", args)?;
    let places = match &args[1] {
        Value::Int(n) if (0..=17).contains(n) => {
            usize::try_from(*n).expect("a count between 0 and 17")
        }
        Value::Int(n) => {
            return Err(format!(
                "`.fixed()` needs between 0 and 17 decimal places, got {n}"
            )
            .into());
        }
        other => {
            return Err(
                format!("`.fixed()` needs an int, got {}", other.type_name()).into(),
            );
        }
    };
    Ok(Value::str(format!("{value:.places$}")))
}

#[cfg(test)]
mod tests {
    use super::super::tests::{call, of};
    use super::*;
    use crate::builtins::Kind;

    fn on(kind: Kind, name: &'static str, args: &[Value]) -> Value {
        call(of(kind, name), args).expect("a number method")
    }

    #[test]
    fn takes_the_magnitude_of_an_int_and_of_a_float() {
        assert_eq!(on(Kind::Int, "abs", &[Value::Int(-42)]), Value::Int(42));
        assert_eq!(on(Kind::Int, "abs", &[Value::Int(42)]), Value::Int(42));
        assert_eq!(
            on(Kind::Float, "abs", &[Value::Float(-1.5)]),
            Value::Float(1.5)
        );
    }

    #[test]
    fn refuses_the_one_int_with_no_magnitude() {
        assert_eq!(
            call(of(Kind::Int, "abs"), &[Value::Int(i64::MIN)]).unwrap_err(),
            Fault::from("integer overflow")
        );
    }

    #[test]
    fn rounds_halves_away_from_zero_and_floors_downward() {
        assert_eq!(
            on(Kind::Float, "round", &[Value::Float(2.5)]),
            Value::Int(3)
        );
        assert_eq!(
            on(Kind::Float, "round", &[Value::Float(2.4)]),
            Value::Int(2)
        );
        assert_eq!(
            on(Kind::Float, "round", &[Value::Float(-2.5)]),
            Value::Int(-3)
        );
        assert_eq!(
            on(Kind::Float, "floor", &[Value::Float(2.9)]),
            Value::Int(2)
        );
        assert_eq!(
            on(Kind::Float, "floor", &[Value::Float(-2.1)]),
            Value::Int(-3)
        );
    }

    #[test]
    fn refuses_to_make_an_int_of_what_is_not_one() {
        for (name, value, shown) in [
            ("round", f64::NAN, "NaN"),
            ("round", f64::INFINITY, "inf"),
            ("floor", -f64::INFINITY, "-inf"),
            ("round", 1e30, "1000000000000000000000000000000"),
        ] {
            assert_eq!(
                call(of(Kind::Float, name), &[Value::Float(value)]).unwrap_err(),
                Fault::from(format!("`.{name}()` cannot make an int of {shown}")),
                "{name} {value}"
            );
        }
    }

    #[test]
    fn writes_a_float_with_exactly_the_places_asked_for() {
        let fixed = of(Kind::Float, "fixed");
        assert_eq!(
            call(fixed, &[Value::Float(2.8), Value::Int(2)]).unwrap(),
            Value::str("2.80")
        );
        // A half goes to the even digit, which is the formatter's rule, not a choice
        // made here — and the one place this can differ from the Eleventy build.
        assert_eq!(
            call(fixed, &[Value::Float(0.5), Value::Int(0)]).unwrap(),
            Value::str("0")
        );
        assert_eq!(
            call(fixed, &[Value::Float(1.5), Value::Int(0)]).unwrap(),
            Value::str("2")
        );
        assert_eq!(
            call(fixed, &[Value::Float(1.0 / 3.0), Value::Int(4)]).unwrap(),
            Value::str("0.3333")
        );
    }

    #[test]
    fn refuses_a_number_of_places_it_cannot_write() {
        let fixed = of(Kind::Float, "fixed");
        assert_eq!(
            call(fixed, &[Value::Float(1.0), Value::Int(-1)]).unwrap_err(),
            Fault::from("`.fixed()` needs between 0 and 17 decimal places, got -1")
        );
        assert_eq!(
            call(fixed, &[Value::Float(1.0), Value::Int(18)]).unwrap_err(),
            Fault::from("`.fixed()` needs between 0 and 17 decimal places, got 18")
        );
        assert_eq!(
            call(fixed, &[Value::Float(1.0), Value::str("2")]).unwrap_err(),
            Fault::from("`.fixed()` needs an int, got string")
        );
    }

    #[test]
    fn refuses_a_receiver_only_an_any_call_could_have_handed_it() {
        assert_eq!(
            call(of(Kind::Int, "abs"), &[Value::str("s")]).unwrap_err(),
            Fault::from("`.abs()` needs an int, got string")
        );
        for name in ["abs", "round", "floor", "fixed"] {
            let entry = of(Kind::Float, name);
            let args = [Value::Int(1), Value::Int(2)];
            assert_eq!(
                call(entry, &args[..entry.arity]).unwrap_err(),
                Fault::from(format!("`.{name}()` needs a float, got int")),
                "{name}"
            );
        }
    }
}

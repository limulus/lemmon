//! What an arithmetic, comparison, membership, or index operator means for a pair of
//! values.
//!
//! Free functions over a pair of [`Value`]s, and for all but the index an [`Op`]: no
//! stack, no frame, nothing the machine holds. `Vm::step` pops the operands and pushes what these return; equality
//! and ordering it leaves to [`Value`], which a sort asks as well.

use crate::Value;
use crate::bytecode::Op;

pub(super) fn index_into(object: &Value, index: &Value) -> Result<Value, String> {
    match (object, index) {
        (Value::List(items), Value::Int(i)) => usize::try_from(*i)
            .ok()
            .and_then(|i| items.get(i))
            .cloned()
            .ok_or_else(|| {
                format!("index {i} is out of range for a list of {}", items.len())
            }),
        (Value::Map(map), Value::Str(key)) => map
            .get(key)
            .cloned()
            .ok_or_else(|| format!("no key \"{key}\"")),
        (object, index) => Err(format!(
            "cannot index a {} with a {}",
            object.type_name(),
            index.type_name()
        )),
    }
}

#[allow(
    clippy::cast_precision_loss,
    reason = "int/float promotion is the language rule"
)]
pub(super) fn binary(op: Op, lhs: &Value, rhs: &Value) -> Result<Value, String> {
    use Value::{Bool, Float, Int, List, Map, Str};
    let mismatch = || {
        format!(
            "cannot apply `{}` to a {} and a {}",
            symbol(op),
            lhs.type_name(),
            rhs.type_name()
        )
    };
    Ok(match (op, lhs, rhs) {
        (Op::Eq, _, _) => Bool(lhs.equals(rhs)?),
        (Op::Ne, _, _) => Bool(!lhs.equals(rhs)?),
        (Op::In, _, List(items)) => {
            for item in items.iter() {
                if item.equals(lhs)? {
                    return Ok(Bool(true));
                }
            }
            Bool(false)
        }
        (Op::In, Str(key), Map(map)) => Bool(map.get(key).is_some()),
        (Op::In, _, _) => return Err(mismatch()),
        (Op::Add, Str(a), Str(b)) => Value::str(format!("{a}{b}")),
        // Ordering is `Value::order`'s and not this table's, so that `lt` and a sort
        // cannot come to disagree about what has an order.
        (Op::Lt | Op::Le | Op::Gt | Op::Ge, _, _) => match lhs.order(rhs)? {
            Some(ordering) => Bool(ordered(op, ordering)),
            None => return Err(mismatch()),
        },
        (_, Int(a), Int(b)) => int_arith(op, *a, *b)?,
        (_, Int(a), Float(b)) => float_arith(op, *a as f64, *b)?,
        (_, Float(a), Int(b)) => float_arith(op, *a, *b as f64)?,
        (_, Float(a), Float(b)) => float_arith(op, *a, *b)?,
        _ => return Err(mismatch()),
    })
}

fn int_arith(op: Op, a: i64, b: i64) -> Result<Value, String> {
    let checked = |r: Option<i64>| r.ok_or_else(|| "integer overflow".to_owned());
    Ok(match op {
        Op::Add => Value::Int(checked(a.checked_add(b))?),
        Op::Sub => Value::Int(checked(a.checked_sub(b))?),
        Op::Mul => Value::Int(checked(a.checked_mul(b))?),
        Op::Div | Op::Rem if b == 0 => return Err("division by zero".to_owned()),
        Op::Div => Value::Int(checked(a.checked_div(b))?),
        Op::Rem => Value::Int(checked(a.checked_rem(b))?),
        _ => unreachable!("{op:?} is not arithmetic"),
    })
}

fn float_arith(op: Op, a: f64, b: f64) -> Result<Value, String> {
    Ok(match op {
        Op::Add => Value::Float(a + b),
        Op::Sub => Value::Float(a - b),
        Op::Mul => Value::Float(a * b),
        Op::Div | Op::Rem if b == 0.0 => return Err("division by zero".to_owned()),
        Op::Div => Value::Float(a / b),
        Op::Rem => Value::Float(a % b),
        _ => unreachable!("{op:?} is not arithmetic"),
    })
}

fn ordered(op: Op, ordering: std::cmp::Ordering) -> bool {
    match op {
        Op::Lt => ordering.is_lt(),
        Op::Le => ordering.is_le(),
        Op::Gt => ordering.is_gt(),
        Op::Ge => ordering.is_ge(),
        _ => unreachable!("{op:?} is not an ordering"),
    }
}

const fn symbol(op: Op) -> &'static str {
    match op {
        Op::Add => "+",
        Op::Sub => "-",
        Op::Mul => "*",
        Op::Div => "/",
        Op::Rem => "%",
        Op::Lt => "lt",
        Op::Le => "lte",
        Op::Gt => "gt",
        Op::Ge => "gte",
        Op::In => "in",
        _ => unreachable!(),
    }
}

//! What a `datetime` and a `duration` carry.
//!
//! A datetime's methods all read it through the offset it was written in (`datetime.rs`),
//! never through UTC: a site shows the day its author meant, and the last half-hour of
//! a day on the west coast is already tomorrow in UTC.
//!
//! There is no arithmetic here — no `d + duration`, no difference of two datetimes. The
//! port does not need it, and it would settle the number model (`docs/language.md`
//! §Deferred questions) by accident.

use crate::Value;
use crate::datetime::Datetime;

use super::{Fault, Host, method_receiver};

/// The receiver as a datetime, or the refusal for a value only an `any` call could have
/// handed a datetime method.
fn moment(name: &str, args: &[Value]) -> Result<Datetime, Fault> {
    match &args[0] {
        Value::Datetime(d) => Ok(*d),
        other => Err(method_receiver(name, "a datetime", other)),
    }
}

/// The receiver as a span of seconds, or the refusal for a value only an `any` call
/// could have handed a duration method.
fn span(name: &str, args: &[Value]) -> Result<i64, Fault> {
    match &args[0] {
        Value::Duration(seconds) => Ok(*seconds),
        other => Err(method_receiver(name, "a duration", other)),
    }
}

/// `d.rfc3339()`: the moment as `<lem.d>` prints it, as a string a template can pass
/// on. An Atom `<updated>` wants this, and so does a `<time datetime="…">` attribute
/// built by interpolation rather than by printing.
pub(super) fn rfc3339(_host: &mut dyn Host, args: &[Value]) -> Result<Value, Fault> {
    Ok(Value::str(moment("rfc3339", args)?.to_string()))
}

/// `d.rfc822()`: the date an RSS `pubDate` wants — `Sat, 4 Apr 2026 10:30:00 -0700`.
pub(super) fn rfc822(_host: &mut dyn Host, args: &[Value]) -> Result<Value, Fault> {
    moment("rfc822", args)?
        .rfc2822()
        .map(Value::str)
        .map_err(Fault::from)
}

/// `d.format(pattern)`: strftime, so that a site words a date the way it words one —
/// `format("%B %-d, %Y")` is `April 4, 2026`.
///
/// A literal pattern is read at compile time too (`compile/calls.rs`), so a typo in a
/// template is a compile error rather than one page's render error.
pub(super) fn format(_host: &mut dyn Host, args: &[Value]) -> Result<Value, Fault> {
    let moment = moment("format", args)?;
    match &args[1] {
        Value::Str(pattern) => moment.format(pattern).map(Value::str).map_err(Fault::from),
        other => {
            Err(format!("`.format()` needs a string, got {}", other.type_name()).into())
        }
    }
}

/// `d.year()`, `d.month()`, `d.day()`: the civil calendar in the written offset — a
/// copyright line's year, a permalink's month.
pub(super) fn year(_host: &mut dyn Host, args: &[Value]) -> Result<Value, Fault> {
    Ok(Value::Int(moment("year", args)?.year()))
}

pub(super) fn month(_host: &mut dyn Host, args: &[Value]) -> Result<Value, Fault> {
    Ok(Value::Int(moment("month", args)?.month()))
}

pub(super) fn day(_host: &mut dyn Host, args: &[Value]) -> Result<Value, Fault> {
    Ok(Value::Int(moment("day", args)?.day()))
}

/// `span.seconds()`: the whole span, which is what a duration is held as.
pub(super) fn seconds(_host: &mut dyn Host, args: &[Value]) -> Result<Value, Fault> {
    Ok(Value::Int(span("seconds", args)?))
}

/// `span.minutes()`: whole minutes, the remainder dropped — 90 seconds is one minute.
/// A site that wants the remainder counted reaches for `seconds()` and words the rule
/// itself; there is no rounding hidden here for it to fight.
pub(super) fn minutes(_host: &mut dyn Host, args: &[Value]) -> Result<Value, Fault> {
    Ok(Value::Int(span("minutes", args)? / 60))
}

#[cfg(test)]
mod tests {
    use super::super::tests::{call, of};
    use super::*;
    use crate::builtins::Kind;

    fn published_moment() -> Datetime {
        Datetime::parse("2026-04-04T10:30:00-07:00").expect("a datetime")
    }

    fn published() -> Value {
        Value::Datetime(published_moment())
    }

    fn on(kind: Kind, name: &'static str, args: &[Value]) -> Value {
        call(of(kind, name), args).expect("a time method")
    }

    #[test]
    fn writes_a_moment_in_the_two_forms_a_feed_asks_for() {
        assert_eq!(
            on(Kind::Datetime, "rfc3339", &[published()]),
            Value::str("2026-04-04T10:30:00-07:00")
        );
        assert_eq!(
            on(Kind::Datetime, "rfc822", &[published()]),
            Value::str("Sat, 4 Apr 2026 10:30:00 -0700")
        );
    }

    #[test]
    fn passes_the_refusal_of_a_year_rfc_822_cannot_write_through() {
        let ancient = Datetime::parse("-000500-01-01").expect("a datetime");
        assert_eq!(
            call(of(Kind::Datetime, "rfc822"), &[Value::Datetime(ancient)]).unwrap_err(),
            Fault::from(ancient.rfc2822().unwrap_err())
        );
    }

    #[test]
    fn fills_in_the_pattern_a_site_words_its_dates_with() {
        assert_eq!(
            on(
                Kind::Datetime,
                "format",
                &[published(), Value::str("%B %-d, %Y")]
            ),
            Value::str("April 4, 2026")
        );
    }

    #[test]
    fn refuses_a_pattern_that_is_no_pattern_and_one_that_is_no_string() {
        let format = of(Kind::Datetime, "format");
        assert_eq!(
            call(format, &[published(), Value::str("%Q%")]).unwrap_err(),
            Fault::from(published_moment().format("%Q%").unwrap_err())
        );
        assert_eq!(
            call(format, &[published(), Value::Int(1)]).unwrap_err(),
            Fault::from("`.format()` needs a string, got int")
        );
    }

    #[test]
    fn reads_the_calendar_the_author_wrote_in_and_not_the_one_utc_had_turned_to() {
        // Half past eleven at night in Arizona is already the fifth in UTC.
        let evening = Value::Datetime(
            Datetime::parse("2026-04-04T23:30:00-07:00").expect("a datetime"),
        );
        assert_eq!(
            (
                on(Kind::Datetime, "year", std::slice::from_ref(&evening)),
                on(Kind::Datetime, "month", std::slice::from_ref(&evening)),
                on(Kind::Datetime, "day", &[evening]),
            ),
            (Value::Int(2026), Value::Int(4), Value::Int(4))
        );
    }

    #[test]
    fn counts_a_span_in_seconds_and_in_whole_minutes() {
        assert_eq!(
            on(Kind::Duration, "seconds", &[Value::Duration(90)]),
            Value::Int(90)
        );
        assert_eq!(
            on(Kind::Duration, "minutes", &[Value::Duration(90)]),
            Value::Int(1)
        );
        assert_eq!(
            on(Kind::Duration, "minutes", &[Value::Duration(59)]),
            Value::Int(0)
        );
    }

    #[test]
    fn refuses_a_receiver_only_an_any_call_could_have_handed_it() {
        for name in ["rfc3339", "rfc822", "format", "year", "month", "day"] {
            let entry = of(Kind::Datetime, name);
            let args = [Value::str("2026-04-04"), Value::str("%Y")];
            assert_eq!(
                call(entry, &args[..entry.arity]).unwrap_err(),
                Fault::from(format!("`.{name}()` needs a datetime, got string")),
                "{name}"
            );
        }
        for name in ["seconds", "minutes"] {
            assert_eq!(
                call(of(Kind::Duration, name), &[Value::Int(90)]).unwrap_err(),
                Fault::from(format!("`.{name}()` needs a duration, got int")),
                "{name}"
            );
        }
    }
}

//! The `datetime` value: an instant, and the UTC offset it is written in (D38).
//!
//! Two datetimes are equal and ordered by their instant alone, so
//! `2026-04-04T10:30:00-07:00` and `2026-04-04T17:30:00Z` are one moment written twice.
//! The offset is kept beside the instant because a site displays the time an author
//! meant: printing gives back the offset that was written, and the civil accessors
//! answer in it rather than in UTC. [`Datetime::utc`] is the one way out of the written
//! offset: the same instant at offset zero.
//!
//! No time zone, only an offset. The core has no time-zone database — bundling one
//! would put four hundred kilobytes of data in the `.wasm` and a yearly update in the
//! release process — so a string naming a zone is refused rather than silently read for
//! its offset and its DST rules ignored.
//!
//! Every refusal here is worded by this module and never by `jiff`, so that a corpus
//! expectation survives a patch release of the dependency.

use std::borrow::Cow;
use std::cmp::Ordering;
use std::fmt;

use jiff::fmt::temporal::Pieces;
use jiff::fmt::{rfc2822, strtime};
use jiff::tz::Offset;
use jiff::{Timestamp, Zoned, civil};

/// A moment, and the offset it is written in: the one the source wrote, or zero once
/// [`Datetime::utc`] has rewritten it.
///
/// Held decomposed rather than as a [`Timestamp`] beside an [`Offset`], which would be
/// twenty-four bytes and make every [`Value`](crate::Value) in the VM eight bytes wider
/// than the sixteen a pointer-and-length variant needs. The parts only ever come from a
/// timestamp this module built, so putting them back together cannot fail.
#[derive(Clone, Copy, Debug)]
pub struct Datetime {
    seconds: i64,
    nanoseconds: i32,
    offset_seconds: i32,
}

impl Datetime {
    /// `text` as a datetime, or why it is not one.
    ///
    /// Three spellings are a datetime: a date alone (`2024-11-24`), which is midnight
    /// UTC; a date and a time with an offset (`2024-11-24T18:35:00Z`,
    /// `2026-04-04T10:30:00-07:00`), whose separator may be a `T` or a space; and the
    /// same with a space before the offset (`2026-04-04 10:30:00 -07:00`), which is
    /// what a YAML timestamp looks like when a host hands its text through.
    ///
    /// # Errors
    ///
    /// When `text` is not one of those: a time with no offset names the offset it
    /// wants, a zone annotation says why a name cannot stand for one, and anything else
    /// is shown the two shapes.
    pub fn parse(text: &str) -> Result<Self, String> {
        let normalized = unspaced(text);
        let pieces = Pieces::parse(normalized.as_bytes())
            .map_err(|_| format!("`{text}` is not a datetime: {SHAPES}"))?;
        if pieces.time_zone_annotation().is_some() {
            return Err(format!(
                "`{text}` names a time zone, and the core has no time-zone database: \
                 write the UTC offset the time is in"
            ));
        }
        let Some(time) = pieces.time() else {
            return Self::at(
                pieces.date().to_datetime(civil::Time::midnight()),
                Offset::UTC,
            )
            .ok_or_else(|| format!("`{text}` is not a datetime: {SHAPES}"));
        };
        let Some(offset) = pieces.to_numeric_offset() else {
            return Err(format!(
                "`{text}` has a time and no UTC offset: write `Z`, or an offset like \
                 `-07:00`, after it — a datetime is a moment, not a wall clock"
            ));
        };
        Self::at(pieces.date().to_datetime(time), offset)
            .ok_or_else(|| format!("`{text}` is not a datetime: {SHAPES}"))
    }

    /// The instant `civil` names in `offset`, unless it falls outside the range a
    /// timestamp holds.
    fn at(civil: civil::DateTime, offset: Offset) -> Option<Self> {
        let timestamp = offset.to_timestamp(civil).ok()?;
        Some(Self {
            seconds: timestamp.as_second(),
            nanoseconds: timestamp.subsec_nanosecond(),
            offset_seconds: offset.seconds(),
        })
    }

    /// The year the civil calendar shows, read through the offset this is written in.
    #[must_use]
    pub fn year(&self) -> i64 {
        i64::from(self.civil().year())
    }

    /// The month, `1` through `12`.
    #[must_use]
    pub fn month(&self) -> i64 {
        i64::from(self.civil().month())
    }

    /// The day of the month, `1` through `31`.
    #[must_use]
    pub fn day(&self) -> i64 {
        i64::from(self.civil().day())
    }

    /// The same instant written at offset zero, which every method then reads it
    /// through: what a feed or a sitemap, whose reader is a program, spells its dates in.
    /// Offset zero is fixed, as every offset is, and the instant does not move, so this
    /// needs no time-zone database and is no arithmetic (module docs).
    #[must_use]
    pub fn utc(self) -> Self {
        Self {
            offset_seconds: 0,
            ..self
        }
    }

    /// The RFC 2822 date an RSS `pubDate` wants — `Sat, 4 Apr 2026 10:30:00 -0700` —
    /// in the offset this is written in. RFC 822 itself allows a two-digit year and
    /// RFC 2822 narrowed it to four, which is what a feed reader expects today.
    ///
    /// # Errors
    ///
    /// When the year is before 1. A timestamp reaches back to -9999 and the format has
    /// no way to write those years, so the refusal is a real one rather than an
    /// assertion — a `datetime("-000500-01-01")` is a string a template can hold.
    pub fn rfc2822(&self) -> Result<String, String> {
        let mut out = String::new();
        rfc2822::DateTimePrinter::new()
            .print_zoned(&self.zoned(), &mut out)
            .map_err(|_| format!("`{self}` is before year 1 and has no RFC 822 form"))?;
        Ok(out)
    }

    /// `pattern` filled in from this datetime, read through the offset it is written in.
    ///
    /// # Errors
    ///
    /// When `pattern` holds something strftime has no conversion for. The refusal
    /// names the pattern rather than the character, because jiff words which character
    /// and this module words its own refusals.
    pub fn format(&self, pattern: &str) -> Result<String, String> {
        strtime::format(pattern, &self.zoned()).map_err(|_| {
            format!(
                "`{pattern}` is not a datetime pattern: write one strftime understands, \
                 like `%B %-d, %Y` or `%Y-%m-%d`"
            )
        })
    }

    /// Whether `pattern` is one strftime can fill in, asked with no datetime to fill it
    /// from: what the compiler holds a literal pattern to (`compile/calls.rs`).
    ///
    /// A pattern either has a conversion for every `%` in it or has not, whatever
    /// moment it is filled in from, so any moment answers for all of them.
    ///
    /// # Errors
    ///
    /// The same refusal [`format`](Self::format) would give at render.
    pub fn check_pattern(pattern: &str) -> Result<(), String> {
        Self::parse("2026-01-01T00:00:00Z")
            .expect("a datetime written by this module")
            .format(pattern)
            .map(drop)
    }

    /// The offset this is written in, as the civil calendar sees the instant through
    /// it: what a year, a month, or a day is to the site that wrote it.
    fn civil(&self) -> civil::DateTime {
        self.offset().to_datetime(self.timestamp())
    }

    /// The instant in a time zone fixed at the offset this is written in, which is the
    /// shape jiff's printers and `strftime` take. Fixed because there is no zone to name
    /// (module docs): the offset is all there is.
    fn zoned(&self) -> Zoned {
        self.timestamp().to_zoned(self.offset().to_time_zone())
    }

    /// The offset this is written in.
    fn offset(&self) -> Offset {
        Offset::from_seconds(self.offset_seconds).expect("an offset this was built from")
    }

    /// The instant, which is what equality and ordering read.
    fn timestamp(&self) -> Timestamp {
        Timestamp::new(self.seconds, self.nanoseconds)
            .expect("a timestamp this was built from")
    }
}

/// The two shapes every refusal that is not more specific shows.
const SHAPES: &str = "write a date (`2024-11-24`), or a date and time with an offset \
     (`2024-11-24T18:35:00Z`)";

/// `text` with the space a YAML timestamp puts before its offset removed, which RFC
/// 3339 does not allow: `2026-04-04 10:30:00 -07:00` is what front matter carries, and
/// a host that hands the scalar through rather than normalising it is the ordinary
/// case. The space between the date and the time is RFC 9557's, and stands.
fn unspaced(text: &str) -> Cow<'_, str> {
    match text.rsplit_once(' ') {
        Some((head, tail))
            if head.contains(':') && (tail.starts_with('+') || tail.starts_with('-')) =>
        {
            Cow::Owned(format!("{head}{tail}"))
        }
        _ => Cow::Borrowed(text),
    }
}

/// RFC 3339, in the offset it is written in — `Z` where that is UTC, whichever of `Z`
/// and `+00:00` the source spelled, and once [`Datetime::utc`] has rewritten it.
impl fmt::Display for Datetime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (timestamp, offset) = (self.timestamp(), self.offset());
        if offset == Offset::UTC {
            write!(f, "{timestamp}")
        } else {
            write!(f, "{}", timestamp.display_with_offset(offset))
        }
    }
}

/// By the instant, never the offset: the same moment written in two offsets is one
/// value to `==`, as it is to `lt`.
impl PartialEq for Datetime {
    fn eq(&self, other: &Self) -> bool {
        (self.seconds, self.nanoseconds) == (other.seconds, other.nanoseconds)
    }
}

impl Eq for Datetime {}

impl Ord for Datetime {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.seconds, self.nanoseconds).cmp(&(other.seconds, other.nanoseconds))
    }
}

impl PartialOrd for Datetime {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Datetime {
        Datetime::parse(text).expect("a datetime")
    }

    fn refusal(text: &str) -> String {
        Datetime::parse(text).expect_err("not a datetime")
    }

    #[test]
    fn reads_a_date_alone_as_midnight_utc() {
        assert_eq!(parse("2024-11-24").to_string(), "2024-11-24T00:00:00Z");
    }

    #[test]
    fn keeps_the_offset_a_datetime_was_written_in() {
        assert_eq!(
            parse("2026-04-04T10:30:00-07:00").to_string(),
            "2026-04-04T10:30:00-07:00"
        );
        assert_eq!(
            parse("2026-09-08T12:00:00Z").to_string(),
            "2026-09-08T12:00:00Z"
        );
    }

    #[test]
    fn prints_utc_as_z_however_the_source_spelled_it() {
        assert_eq!(
            parse("2026-09-08T12:00:00+00:00").to_string(),
            "2026-09-08T12:00:00Z"
        );
    }

    #[test]
    fn reads_a_space_between_the_date_and_the_time_and_before_the_offset() {
        let written = "2026-04-04T10:30:00-07:00";
        assert_eq!(parse("2026-04-04 10:30:00-07:00").to_string(), written);
        assert_eq!(parse("2026-04-04 10:30:00 -07:00").to_string(), written);
    }

    #[test]
    fn puts_a_moment_before_the_epoch_back_together() {
        // The parts are held decomposed and recombined by an `expect`, which rests on
        // jiff giving a negative timestamp a negative subsecond. A moment before 1970
        // is the only thing that exercises that, and a build that got it wrong would
        // panic rather than report (invariant 8).
        for (written, printed) in [
            ("1969-07-20T20:17:00Z", "1969-07-20T20:17:00Z"),
            ("1969-07-20", "1969-07-20T00:00:00Z"),
            (
                "1969-07-20T20:17:00.25-04:00",
                "1969-07-20T20:17:00.25-04:00",
            ),
            ("1969-12-31T23:59:59.5Z", "1969-12-31T23:59:59.5Z"),
            ("1900-01-01T00:00:00-07:00", "1900-01-01T00:00:00-07:00"),
        ] {
            let moment = parse(written);
            assert_eq!(moment.to_string(), printed, "{written}");
            assert!(moment < parse("1970-01-01"), "{written}");
            assert_eq!(moment.civil().year().to_string(), written[..4], "{written}");
        }
    }

    #[test]
    fn keeps_a_fraction_of_a_second() {
        assert_eq!(
            parse("2026-04-04T10:30:00.5-07:00").to_string(),
            "2026-04-04T10:30:00.5-07:00"
        );
    }

    #[test]
    fn refuses_a_time_with_no_offset_by_naming_the_offset_it_wants() {
        assert_eq!(
            refusal("2026-04-04T10:30:00"),
            "`2026-04-04T10:30:00` has a time and no UTC offset: write `Z`, or an \
             offset like `-07:00`, after it — a datetime is a moment, not a wall clock"
        );
    }

    #[test]
    fn refuses_a_named_time_zone_and_says_why() {
        assert_eq!(
            refusal("2024-11-24T10:00:00-05:00[America/New_York]"),
            "`2024-11-24T10:00:00-05:00[America/New_York]` names a time zone, and the \
             core has no time-zone database: write the UTC offset the time is in"
        );
    }

    #[test]
    fn refuses_what_is_no_datetime_at_all_by_showing_both_shapes() {
        for text in ["2024-13-01", "2024-11-31", "tomorrow", "", "  2024-11-24  "] {
            assert_eq!(
                refusal(text),
                format!("`{text}` is not a datetime: {SHAPES}"),
                "{text}"
            );
        }
    }

    #[test]
    fn refuses_a_year_no_timestamp_can_hold() {
        assert!(refusal("9999-12-31T23:59:59+00:00").contains("is not a datetime"));
        assert!(refusal("9999-12-31").contains("is not a datetime"));
    }

    #[test]
    fn compares_by_the_instant_and_never_by_the_offset() {
        let here = parse("2026-04-04T10:30:00-07:00");
        let there = parse("2026-04-04T17:30:00Z");
        assert_eq!(here, there);
        assert_eq!(here.cmp(&there), Ordering::Equal);
        assert!(parse("2024-11-24") < here);
        assert!(here > parse("2024-11-24T00:00:00Z"));
    }

    #[test]
    fn reads_the_year_month_and_day_in_the_written_offset() {
        // Late enough in the evening that UTC has turned the page and the site has not:
        // the accessors answer for the calendar the author wrote in.
        let evening = parse("2026-04-04T23:30:00-07:00");
        assert_eq!(
            (evening.year(), evening.month(), evening.day()),
            (2026, 4, 4)
        );
        assert_eq!(evening.timestamp().to_string(), "2026-04-05T06:30:00Z");
        assert_eq!(evening.offset(), Offset::from_hours(-7).unwrap());
        assert_eq!(parse("1969-07-20").year(), 1969);
    }

    #[test]
    fn writes_the_same_instant_at_offset_zero_in_utc() {
        let evening = parse("2026-04-04T23:30:00-07:00");
        assert_eq!(evening.utc().to_string(), "2026-04-05T06:30:00Z");
    }

    #[test]
    fn reads_the_calendar_utc_has_turned_to_once_in_utc() {
        let evening = parse("2026-04-04T23:30:00-07:00").utc();
        assert_eq!(
            (evening.year(), evening.month(), evening.day()),
            (2026, 4, 5)
        );
    }

    #[test]
    fn writes_a_moment_before_the_epoch_and_its_fraction_in_utc() {
        assert_eq!(
            parse("1969-07-20T20:17:00.25-04:00").utc().to_string(),
            "1969-07-21T00:17:00.25Z"
        );
    }

    #[test]
    fn writes_the_rfc_822_date_a_feed_wants() {
        assert_eq!(
            parse("2026-04-04T10:30:00-07:00").rfc2822(),
            Ok("Sat, 4 Apr 2026 10:30:00 -0700".to_owned())
        );
        assert_eq!(
            parse("2026-01-01").rfc2822(),
            Ok("Thu, 1 Jan 2026 00:00:00 +0000".to_owned())
        );
    }

    #[test]
    fn refuses_an_rfc_822_date_for_a_year_the_format_cannot_write() {
        // Reachable from a template — `datetime("-000500-01-01")` parses — so the
        // format's one limit is a refusal with a span rather than a panic (invariant 8).
        assert_eq!(
            parse("-000500-01-01").rfc2822(),
            Err(
                "`-000500-01-01T00:00:00Z` is before year 1 and has no RFC 822 form"
                    .to_owned()
            )
        );
    }

    #[test]
    fn fills_in_a_strftime_pattern_from_the_written_offset() {
        let published = parse("2026-04-04T10:30:00-07:00");
        assert_eq!(
            published.format("%B %-d, %Y"),
            Ok("April 4, 2026".to_owned())
        );
        assert_eq!(
            published.format("%Y-%m-%dT%H:%M:%S%z"),
            Ok("2026-04-04T10:30:00-0700".to_owned())
        );
        assert_eq!(published.format(""), Ok(String::new()));
    }

    #[test]
    fn refuses_a_pattern_strftime_has_no_conversion_for() {
        for pattern in ["%Q%", "%", "100%"] {
            assert_eq!(
                parse("2026-04-04").format(pattern),
                Err(format!(
                    "`{pattern}` is not a datetime pattern: write one strftime \
                     understands, like `%B %-d, %Y` or `%Y-%m-%d`"
                )),
                "{pattern}"
            );
        }
    }

    #[test]
    fn answers_for_a_pattern_with_no_datetime_to_fill_it_from() {
        assert_eq!(Datetime::check_pattern("%B %-d, %Y"), Ok(()));
        assert_eq!(
            Datetime::check_pattern("%Q%"),
            Err(parse("2026-04-04").format("%Q%").unwrap_err())
        );
    }

    #[test]
    fn is_no_wider_than_a_pointer_and_a_length() {
        // What keeps `Value` sixteen bytes plus its tag, which every value in the VM
        // pays for; see the struct's own note.
        assert_eq!(size_of::<Datetime>(), 16);
    }

    #[test]
    fn leaves_a_string_with_no_offset_to_remove_alone() {
        assert_eq!(unspaced("2024-11-24"), "2024-11-24");
        assert_eq!(unspaced("2026-04-04 10:30:00"), "2026-04-04 10:30:00");
        assert_eq!(unspaced("a b"), "a b");
    }
}

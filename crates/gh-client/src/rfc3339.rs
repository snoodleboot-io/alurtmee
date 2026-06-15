//! A minimal, dependency-free RFC3339 → epoch-seconds parser.
//!
//! GitHub Actions timestamps (`run_started_at`, `updated_at`) arrive as RFC3339 UTC strings such as
//! `2026-06-15T00:00:10Z`. CI-timing analysis only needs the *difference* between two such stamps
//! (a wall-clock duration), so a full date/time crate (chrono/time) would be overkill — and the
//! workspace forbids new deps. This module parses the narrow shape GitHub emits into epoch seconds
//! using Howard Hinnant's branch-free `days_from_civil` algorithm, which is exact for all dates in
//! range without any lookup tables.
//!
//! Tolerated input: `YYYY-MM-DDTHH:MM:SS` with an optional fractional `.sss` part (ignored — second
//! resolution is enough for run durations) followed by either `Z` or a `+00:00` UTC offset. Only UTC
//! is supported because that is all GitHub returns; any other zone or malformed input yields `None`.

/// Convert a civil date (proleptic Gregorian) to days since 1970-01-01 (which is day 0).
///
/// Branch-free algorithm from Howard Hinnant's `chrono`-compatible date library. Valid for any
/// year in `i32` range; for our inputs the result comfortably fits the later `i64` math. `m` is
/// `1..=12`, `d` is `1..=31`; out-of-range components are rejected before this is called.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    // Shift the year so that March is the first month: leap days then land at the end of the
    // "year", which removes the February special-case from the arithmetic.
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146097 + doe - 719468
}

/// Parse an RFC3339 UTC timestamp into epoch seconds, or `None` if malformed.
///
/// Accepts `YYYY-MM-DDTHH:MM:SS` with an optional fractional-seconds part and a `Z` or `+00:00`
/// zone; see the module docs for the exact contract. The fractional part is parsed only for
/// validation and then discarded (second resolution).
pub(crate) fn parse_rfc3339_to_epoch(s: &str) -> Option<i64> {
    let s = s.trim();
    let bytes = s.as_bytes();
    // Minimum: "YYYY-MM-DDTHH:MM:SS" plus at least a one-char zone = 20 chars.
    if bytes.len() < 20 {
        return None;
    }
    // The date/time skeleton must have its separators in fixed positions.
    if bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    // GitHub uses 'T'; tolerate a space separator too (still valid RFC3339).
    if bytes[10] != b'T' && bytes[10] != b't' && bytes[10] != b' ' {
        return None;
    }
    if bytes[13] != b':' || bytes[16] != b':' {
        return None;
    }

    let year = parse_uint(&s[0..4])?;
    let month = parse_uint(&s[5..7])?;
    let day = parse_uint(&s[8..10])?;
    let hour = parse_uint(&s[11..13])?;
    let minute = parse_uint(&s[14..16])?;
    let second = parse_uint(&s[17..19])?;

    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    if hour > 23 || minute > 59 || second > 60 {
        // second == 60 tolerated for leap seconds; it is clamped into the minute below.
        return None;
    }

    // Whatever follows the seconds is the (optional) fractional part and the zone designator.
    let rest = &s[19..];
    if !valid_fraction_and_zone(rest) {
        return None;
    }

    let days = days_from_civil(year as i64, month as i64, day as i64);
    // A leap-second `:60` is folded back to `:59` so the value stays monotonic for diffs.
    let second = second.min(59);
    Some(days * 86_400 + hour as i64 * 3_600 + minute as i64 * 60 + second as i64)
}

/// Validate the trailing `[.fraction]<zone>` segment. The fraction (if present) must be a dot
/// followed by one or more digits; the zone must be `Z`/`z` or a `+00:00`/`-00:00` UTC offset.
fn valid_fraction_and_zone(rest: &str) -> bool {
    let mut rest = rest;
    if let Some(stripped) = rest.strip_prefix('.') {
        // Consume the run of fractional digits; there must be at least one.
        let frac_len = stripped.bytes().take_while(u8::is_ascii_digit).count();
        if frac_len == 0 {
            return false;
        }
        rest = &stripped[frac_len..];
    }
    matches!(rest, "Z" | "z" | "+00:00" | "-00:00" | "+0000" | "-0000")
}

/// Parse an all-ASCII-digit string into a `u32`, rejecting empty or non-digit input.
fn parse_uint(s: &str) -> Option<u32> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse::<u32>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_zero() {
        assert_eq!(parse_rfc3339_to_epoch("1970-01-01T00:00:00Z"), Some(0));
    }

    #[test]
    fn ten_second_diff() {
        let a = parse_rfc3339_to_epoch("2026-06-15T00:00:10Z").unwrap();
        let b = parse_rfc3339_to_epoch("2026-06-15T00:00:00Z").unwrap();
        assert_eq!(a - b, 10);
    }

    #[test]
    fn known_epoch_value() {
        // 2001-09-09T01:46:40Z is the classic 1_000_000_000 epoch second.
        assert_eq!(
            parse_rfc3339_to_epoch("2001-09-09T01:46:40Z"),
            Some(1_000_000_000)
        );
    }

    #[test]
    fn crosses_month_boundary() {
        // 23:59:50 on Jan 31 to 00:00:10 on Feb 1 is 20 seconds across a month boundary.
        let end_of_jan = parse_rfc3339_to_epoch("2026-01-31T23:59:50Z").unwrap();
        let start_of_feb = parse_rfc3339_to_epoch("2026-02-01T00:00:10Z").unwrap();
        assert_eq!(start_of_feb - end_of_jan, 20);
    }

    #[test]
    fn crosses_year_boundary() {
        let end_of_year = parse_rfc3339_to_epoch("2025-12-31T23:59:00Z").unwrap();
        let start_of_year = parse_rfc3339_to_epoch("2026-01-01T00:00:00Z").unwrap();
        assert_eq!(start_of_year - end_of_year, 60);
    }

    #[test]
    fn leap_day_is_handled() {
        // 2024 is a leap year: Feb 29 exists and Mar 1 is one day later.
        let feb29 = parse_rfc3339_to_epoch("2024-02-29T00:00:00Z").unwrap();
        let mar1 = parse_rfc3339_to_epoch("2024-03-01T00:00:00Z").unwrap();
        assert_eq!(mar1 - feb29, 86_400);
    }

    #[test]
    fn tolerates_fractional_seconds() {
        assert_eq!(
            parse_rfc3339_to_epoch("2026-06-15T00:00:10.123Z"),
            Some(parse_rfc3339_to_epoch("2026-06-15T00:00:10Z").unwrap())
        );
    }

    #[test]
    fn tolerates_explicit_utc_offset() {
        assert_eq!(
            parse_rfc3339_to_epoch("2026-06-15T00:00:10+00:00"),
            parse_rfc3339_to_epoch("2026-06-15T00:00:10Z")
        );
    }

    #[test]
    fn rejects_malformed_input() {
        assert_eq!(parse_rfc3339_to_epoch(""), None);
        assert_eq!(parse_rfc3339_to_epoch("not-a-timestamp"), None);
        assert_eq!(parse_rfc3339_to_epoch("2026-06-15"), None);
        assert_eq!(parse_rfc3339_to_epoch("2026-13-01T00:00:00Z"), None); // bad month
        assert_eq!(parse_rfc3339_to_epoch("2026-06-15T25:00:00Z"), None); // bad hour
        assert_eq!(parse_rfc3339_to_epoch("2026-06-15T00:00:00"), None); // no zone
        assert_eq!(parse_rfc3339_to_epoch("2026-06-15T00:00:00.Z"), None); // empty fraction
        assert_eq!(parse_rfc3339_to_epoch("2026/06/15T00:00:00Z"), None); // wrong sep
    }
}

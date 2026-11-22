/// Times in the format of RFC 3339, section 5.6 (`2021-07-04T02:18:03Z`,
/// `2019-10-12T07:20:00.00+09:00`), read as milliseconds since the Unix epoch.
pub struct Rfc3339;

impl Rfc3339 {
    /// The time of `text` in milliseconds since 1970-01-01T00:00:00Z, or `None` when `text` is
    /// not a valid time.
    ///
    /// Accepted, besides the strict `date-time` of RFC 3339: `t` and a space for `T`, `z` for
    /// `Z` (both allowed by the RFC), and a time without an offset, which is read as UTC (HERE
    /// Traffic API v7 allows it and states that its times are UTC). Fractions of a second are
    /// truncated to milliseconds. A leap second (`:60`) counts as the first second of the next
    /// minute.
    pub fn epoch_ms(text: &str) -> Option<i64> {
        let bytes = text.as_bytes();
        let number = |from: usize, len: usize| -> Option<i64> {
            let digits = bytes.get(from..from + len)?;
            digits.iter().try_fold(0i64, |n, &b| b.is_ascii_digit().then(|| n * 10 + i64::from(b - b'0')))
        };
        let separator = |at: usize, allowed: &[u8]| bytes.get(at).is_some_and(|b| allowed.contains(b));
        if !(separator(4, b"-") && separator(7, b"-") && separator(10, b"Tt ") && separator(13, b":") && separator(16, b":")) {
            return None;
        }
        let (year, month, day) = (number(0, 4)?, number(5, 2)?, number(8, 2)?);
        let (hour, minute, second) = (number(11, 2)?, number(14, 2)?, number(17, 2)?);
        if hour > 23 || minute > 59 || second > 60 {
            return None;
        }
        let mut at = 19;
        let mut millis = 0;
        if separator(at, b".") {
            at += 1;
            let start = at;
            while bytes.get(at).is_some_and(u8::is_ascii_digit) {
                if at - start < 3 {
                    millis = millis * 10 + i64::from(bytes[at] - b'0');
                }
                at += 1;
            }
            let digits = at - start;
            if digits == 0 {
                return None;
            }
            for _ in digits..3 {
                millis *= 10;
            }
        }
        let offset_minutes = match bytes.get(at) {
            None => 0,
            Some(b'Z' | b'z') if at + 1 == bytes.len() => 0,
            Some(&sign @ (b'+' | b'-')) if at + 6 == bytes.len() && separator(at + 3, b":") => {
                let (h, m) = (number(at + 1, 2)?, number(at + 4, 2)?);
                if h > 23 || m > 59 {
                    return None;
                }
                if sign == b'+' { h * 60 + m } else { -(h * 60 + m) }
            }
            _ => return None,
        };
        let days = Self::days_from_civil(year, month, day)?;
        let seconds = days * 86_400 + hour * 3_600 + minute * 60 + second - offset_minutes * 60;
        Some(seconds * 1_000 + millis)
    }

    /// The number of days in `month` (1 to 12) of `year` of the proleptic Gregorian calendar.
    pub fn days_in_month(year: i64, month: i64) -> i64 {
        match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
            2 => 28,
            _ => 0,
        }
    }

    /// The number of days from 1970-01-01 to the date (negative before it), in the proleptic
    /// Gregorian calendar (the algorithm `days_from_civil` of Howard Hinnant,
    /// <https://howardhinnant.github.io/date_algorithms.html#days_from_civil>). `None` for an
    /// invalid date, or a year beyond ±1 000 000 000 (kept away from overflow).
    pub fn days_from_civil(year: i64, month: i64, day: i64) -> Option<i64> {
        if !(-1_000_000_000..=1_000_000_000).contains(&year) || !(1..=12).contains(&month) || day < 1 || day > Self::days_in_month(year, month) {
            return None;
        }
        let year = if month <= 2 { year - 1 } else { year };
        let era = year.div_euclid(400);
        let year_of_era = year - era * 400;
        let month_index = (month + 9) % 12;
        let day_of_year = (153 * month_index + 2) / 5 + day - 1;
        let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
        Some(era * 146_097 + day_of_era - 719_468)
    }
}

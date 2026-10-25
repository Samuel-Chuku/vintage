//! UTC dates for block times, without a date library.

/// `YYYY-MM-DD HH:MM UTC` for unix seconds (civil-from-days, Howard Hinnant).
pub fn utc(unix: u32) -> String {
    let secs = i64::from(unix);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02} UTC", rem / 3600, rem % 3600 / 60)
}

#[cfg(test)]
mod tests {
    #[test]
    fn known_dates() {
        assert_eq!(super::utc(0), "1970-01-01 00:00 UTC");
        // 2026-07-28 00:00:00 UTC
        assert_eq!(super::utc(1_785_196_800), "2026-07-28 00:00 UTC");
        // leap day
        assert_eq!(super::utc(1_709_208_000), "2024-02-29 12:00 UTC");
    }
}

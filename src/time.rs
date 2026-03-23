use std::time::{SystemTime, UNIX_EPOCH};

/// Returns current time as epoch milliseconds.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Format milliseconds as a human-readable duration string.
/// - hours > 0: "1h02m"
/// - minutes > 0: "5m03s"
/// - otherwise: "42s" or "0s"
pub fn format_duration(ms: u64) -> String {
    let total_secs = ms / 1000;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    if hours > 0 {
        format!("{}h{:02}m", hours, minutes)
    } else if minutes > 0 {
        format!("{}m{:02}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

/// Format a reset epoch (in ms) as a countdown from now.
/// Returns empty string if in the past.
/// - hours > 0: "(~2h)"
/// - otherwise: "(15m)"
pub fn format_reset_time(reset_epoch_ms: u64) -> String {
    let now = now_ms();
    if reset_epoch_ms <= now {
        return String::new();
    }
    let diff_ms = reset_epoch_ms - now;
    let total_secs = diff_ms / 1000;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;

    if hours > 0 {
        format!("(~{}h)", hours)
    } else {
        format!("({}m)", minutes)
    }
}

/// Format a token count.
/// - >= 1_000_000: "2.5M"
/// - >= 1_000: "1.5k"
/// - otherwise: "500"
pub fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        let val = n as f64 / 1_000_000.0;
        // Trim trailing zero after decimal if it's a whole number of tenths
        let s = format!("{:.1}", val);
        format!("{}M", s)
    } else if n >= 1_000 {
        let val = n as f64 / 1_000.0;
        let s = format!("{:.1}", val);
        format!("{}k", s)
    } else {
        format!("{}", n)
    }
}

/// Parse `count` ASCII decimal digits from byte slice `b` starting at `start`.
/// Returns None if any byte is not an ASCII digit.
pub fn parse_digits(b: &[u8], start: usize, count: usize) -> Option<u32> {
    let end = start.checked_add(count)?;
    if end > b.len() {
        return None;
    }
    let mut value: u32 = 0;
    for &byte in &b[start..end] {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value * 10 + (byte - b'0') as u32;
    }
    Some(value)
}

/// Compute the number of days since the Unix epoch (1970-01-01) for a civil date.
/// Algorithm from http://howardhinnant.github.io/date_algorithms.html
/// Returns None if the date is invalid (month out of 1..=12 range).
pub fn days_from_civil(y: i64, m: i64, d: i64) -> Option<i64> {
    if m < 1 || m > 12 {
        return None;
    }
    // Shift so that March is month 0 (makes leap-year math simpler)
    let (y, m) = if m <= 2 {
        (y - 1, m + 9)
    } else {
        (y, m - 3)
    };
    let era: i64 = if y >= 0 { y } else { y - 399 } / 400;
    let yoe: i64 = y - era * 400; // [0, 399]
    let doy: i64 = (153 * m + 2) / 5 + d - 1; // [0, 365]
    let doe: i64 = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    Some(era * 146097 + doe - 719468)
}

/// Parse an ISO 8601 datetime string to epoch milliseconds.
///
/// Accepted format: `YYYY-MM-DDTHH:MM:SS[.sss][Z|+HH:MM|-HH:MM]`
/// Returns None on any parse failure.
pub fn parse_iso8601(s: &str) -> Option<u64> {
    let b = s.as_bytes();

    // Minimum: "YYYY-MM-DDTHH:MM:SS" = 19 bytes
    if b.len() < 19 {
        return None;
    }

    let year = parse_digits(b, 0, 4)? as i64;
    if b.get(4) != Some(&b'-') {
        return None;
    }
    let month = parse_digits(b, 5, 2)? as i64;
    if b.get(7) != Some(&b'-') {
        return None;
    }
    let day = parse_digits(b, 8, 2)? as i64;
    if b.get(10) != Some(&b'T') {
        return None;
    }
    let hour = parse_digits(b, 11, 2)? as i64;
    if b.get(13) != Some(&b':') {
        return None;
    }
    let minute = parse_digits(b, 14, 2)? as i64;
    if b.get(16) != Some(&b':') {
        return None;
    }
    let second = parse_digits(b, 17, 2)? as i64;

    // Optional fractional seconds (.sss)
    let mut pos = 19;
    let mut frac_ms: u64 = 0;
    if b.get(pos) == Some(&b'.') {
        pos += 1;
        // Read up to 3 digits of fractional seconds
        let frac_start = pos;
        let mut frac_digits = 0u32;
        let mut frac_value = 0u64;
        while pos < b.len() && b[pos].is_ascii_digit() && frac_digits < 3 {
            frac_value = frac_value * 10 + (b[pos] - b'0') as u64;
            frac_digits += 1;
            pos += 1;
        }
        // Skip any remaining fractional digits beyond 3
        while pos < b.len() && b[pos].is_ascii_digit() {
            pos += 1;
        }
        // Pad to milliseconds (3 digits)
        let _ = frac_start;
        frac_ms = frac_value * 10u64.pow(3 - frac_digits.min(3));
    }

    // Timezone offset in seconds
    let tz_offset_secs: i64 = if pos >= b.len() {
        // No timezone: assume UTC
        0
    } else {
        match b[pos] {
            b'Z' => {
                pos += 1;
                0
            }
            b'+' | b'-' => {
                let sign: i64 = if b[pos] == b'+' { 1 } else { -1 };
                pos += 1;
                if pos + 5 > b.len() {
                    return None;
                }
                let tz_hour = parse_digits(b, pos, 2)? as i64;
                if b.get(pos + 2) != Some(&b':') {
                    return None;
                }
                let tz_min = parse_digits(b, pos + 3, 2)? as i64;
                pos += 5;
                sign * (tz_hour * 3600 + tz_min * 60)
            }
            _ => return None,
        }
    };

    // Ensure nothing trailing
    if pos != b.len() {
        return None;
    }

    let days = days_from_civil(year, month, day)?;

    // Convert to epoch milliseconds
    let epoch_secs: i64 = days * 86400
        + hour * 3600
        + minute * 60
        + second
        - tz_offset_secs;

    if epoch_secs < 0 {
        return None;
    }

    Some(epoch_secs as u64 * 1000 + frac_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration_seconds() {
        assert_eq!(format_duration(5000), "5s");
        assert_eq!(format_duration(0), "0s");
    }

    #[test]
    fn test_format_duration_minutes() {
        assert_eq!(format_duration(65000), "1m05s");
        assert_eq!(format_duration(120000), "2m00s");
    }

    #[test]
    fn test_format_duration_hours() {
        assert_eq!(format_duration(3661000), "1h01m");
        assert_eq!(format_duration(7200000), "2h00m");
    }

    #[test]
    fn test_format_tokens() {
        assert_eq!(format_tokens(500), "500");
        assert_eq!(format_tokens(1500), "1.5k");
        assert_eq!(format_tokens(2500000), "2.5M");
    }

    #[test]
    fn test_parse_iso8601_utc() {
        assert_eq!(parse_iso8601("2025-01-01T00:00:00Z"), Some(1735689600000));
    }

    #[test]
    fn test_parse_iso8601_with_millis() {
        assert_eq!(
            parse_iso8601("2025-01-01T00:00:00.500Z"),
            Some(1735689600500)
        );
    }

    #[test]
    fn test_parse_iso8601_with_offset() {
        // 2025-01-01T05:00:00+05:00 == 2025-01-01T00:00:00Z == 1735689600000
        assert_eq!(
            parse_iso8601("2025-01-01T05:00:00+05:00"),
            Some(1735689600000)
        );
    }

    #[test]
    fn test_parse_iso8601_invalid() {
        assert_eq!(parse_iso8601("not-a-date"), None);
        assert_eq!(parse_iso8601("2025-13-01T00:00:00Z"), None);
        assert_eq!(parse_iso8601(""), None);
    }

    #[test]
    fn test_days_from_civil() {
        assert_eq!(days_from_civil(1970, 1, 1), Some(0));
        assert_eq!(days_from_civil(2025, 1, 1), Some(20089));
    }
}

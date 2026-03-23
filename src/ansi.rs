pub const RESET: &str = "\x1b[0m";
pub const DIM: &str = "\x1b[2m";
pub const BOLD: &str = "\x1b[1m";
pub const GREEN: &str = "\x1b[38;2;5;150;105m";       // Tailwind Emerald-600
pub const YELLOW: &str = "\x1b[38;2;217;119;6m";      // Tailwind Amber-600
pub const RED: &str = "\x1b[38;2;220;38;38m";          // Tailwind Red-600
pub const CYAN: &str = "\x1b[36m";
pub const BLUE: &str = "\x1b[34m";
pub const MAGENTA: &str = "\x1b[35m";
pub const WHITE: &str = "\x1b[37m";
pub const GRAY: &str = "\x1b[90m";
pub const SLATE600: &str = "\x1b[38;2;100;116;139m";   // Data values
pub const SLATE700: &str = "\x1b[38;2;51;65;85m";      // Labels
pub const SLATE700_BOLD: &str = "\x1b[1;38;2;51;65;85m";
pub const SLATE800: &str = "\x1b[38;2;51;65;85m";      // Separators
pub const SLATE800_BOLD: &str = "\x1b[1;38;2;51;65;85m";

/// Remove ANSI escape sequences from a string.
pub fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_escape = false;
    for ch in s.chars() {
        if in_escape {
            if ch == 'm' {
                in_escape = false;
            }
            // skip all chars inside the escape sequence
        } else if ch == '\x1b' {
            in_escape = true;
        } else {
            result.push(ch);
        }
    }
    result
}

/// Pad a string (which may contain ANSI codes) to the given visible width.
pub fn pad_ansi(s: &str, width: usize) -> String {
    let visible = strip_ansi(s);
    let visible_len = visible.chars().count();
    if visible_len >= width {
        s.to_string()
    } else {
        let padding = width - visible_len;
        let mut result = s.to_string();
        for _ in 0..padding {
            result.push(' ');
        }
        result
    }
}

/// Return GREEN, YELLOW, or RED based on the given percentage thresholds.
pub fn color_for_percent(pct: f64, warn_at: f64, crit_at: f64) -> &'static str {
    if pct >= crit_at {
        RED
    } else if pct >= warn_at {
        YELLOW
    } else {
        GREEN
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_ansi() {
        let colored = format!("{}Hello{}", GREEN, RESET);
        assert_eq!(strip_ansi(&colored), "Hello");
    }

    #[test]
    fn test_strip_ansi_plain() {
        let plain = "Hello, world!";
        assert_eq!(strip_ansi(plain), plain);
    }

    #[test]
    fn test_pad_ansi() {
        let colored = format!("{}Hi{}", GREEN, RESET);
        let padded = pad_ansi(&colored, 6);
        assert_eq!(strip_ansi(&padded).len(), 6);
        assert!(padded.starts_with(GREEN));
    }

    #[test]
    fn test_color_for_percent() {
        // warn_at=70, crit_at=85
        assert_eq!(color_for_percent(50.0, 70.0, 85.0), GREEN);
        assert_eq!(color_for_percent(70.0, 70.0, 85.0), YELLOW);
        assert_eq!(color_for_percent(85.0, 70.0, 85.0), RED);
    }
}

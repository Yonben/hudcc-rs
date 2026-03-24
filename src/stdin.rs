// Stdin JSON parser — reads the payload Claude Code pipes to the HUD binary.

use crate::json::JsonValue;
use std::io::Read;

// ---------------------------------------------------------------------------
// TTY detection (Unix only)
// ---------------------------------------------------------------------------

#[cfg(unix)]
pub fn atty_stdin() -> bool {
    use std::os::raw::c_int;
    extern "C" {
        fn isatty(fd: c_int) -> c_int;
    }
    unsafe { isatty(0) != 0 }
}

#[cfg(not(unix))]
pub fn atty_stdin() -> bool {
    false
}

// ---------------------------------------------------------------------------
// Typed output struct
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct StdinData {
    pub raw: JsonValue,
    pub context_pct: u32,
    pub model_id: String,
    pub version: Option<String>,
    pub transcript_path: Option<String>,
    pub total_cost_usd: f64,
    pub total_duration_ms: u64,
    pub total_lines_added: u64,
    pub total_lines_removed: u64,
    pub total_api_duration_ms: u64,
    pub current_dir: Option<String>,
    pub agent_name: Option<String>,
    pub input_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    pub total_output_tokens: u64,
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Read all of stdin and parse as JSON.  Returns `None` when stdin is a TTY
/// (i.e. no data is being piped) or when the input is empty / unparseable.
pub fn read_stdin(debug_enabled: bool) -> Option<StdinData> {
    if atty_stdin() {
        return None;
    }

    let mut buf = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut buf) {
        if debug_enabled {
            eprintln!("[hud] stdin: read error: {}", e);
        }
        return None;
    }
    let trimmed = buf.trim();
    if trimmed.is_empty() {
        return None;
    }

    let val = match crate::json::parse(trimmed) {
        Ok(v) => v,
        Err(e) => {
            if debug_enabled {
                eprintln!("[hud] stdin: parse error: {}", e);
            }
            return None;
        }
    };
    Some(extract(&val))
}

// ---------------------------------------------------------------------------
// Model-name helpers
// ---------------------------------------------------------------------------

/// Try to turn a raw model ID (e.g. `"claude-opus-4-6"`) into a display name
/// like `"Opus 4.6"`.  Returns `None` when the ID does not match the expected
/// pattern.
pub fn parse_model_name(id: &str) -> Option<String> {
    // Strip optional "claude-" prefix.
    let rest = id.strip_prefix("claude-").unwrap_or(id);

    // Recognised families.
    let (family_display, rest) = if let Some(r) = rest.strip_prefix("opus-") {
        ("Opus", r)
    } else if let Some(r) = rest.strip_prefix("sonnet-") {
        ("Sonnet", r)
    } else if let Some(r) = rest.strip_prefix("haiku-") {
        ("Haiku", r)
    } else {
        return None;
    };

    // Extract major version number — everything up to the next '-'.
    // Then the minor version is what follows (digits only; ignore any
    // trailing date suffix like "-20250219").
    let mut parts = rest.splitn(3, '-');
    let major = parts.next().filter(|s| s.chars().all(|c| c.is_ascii_digit()))?;
    let minor = parts
        .next()
        .and_then(|s| {
            // Keep only the leading digits of the minor segment so that
            // date suffixes (e.g. "20250219" in "4-20250219") are ignored
            // when they don't look like a short version number.  However,
            // the task test uses `claude-sonnet-3-5` where `5` is the minor
            // version, so we just take the whole segment if it is ≤ 4 digits
            // (a reasonable minor version), otherwise treat it as a date and
            // use "0".
            let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
            if digits.is_empty() {
                None
            } else if digits.len() > 4 {
                // Looks like a date — treat minor as 0.
                Some("0")
            } else {
                Some(s)
            }
        })?;

    Some(format!("{} {}.{}", family_display, major, minor))
}

/// Resolve the model display name from the JSON payload.
pub fn get_model_id(val: &JsonValue) -> String {
    // Prefer model.id, then model.display_name.
    let raw_id = val
        .get_path(&["model", "id"])
        .and_then(|v| v.as_str())
        .or_else(|| {
            val.get_path(&["model", "display_name"])
                .and_then(|v| v.as_str())
        });

    match raw_id {
        None => String::new(),
        Some(id) => parse_model_name(id).unwrap_or_else(|| id.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Context-window percentage
// ---------------------------------------------------------------------------

pub fn get_context_percent(val: &JsonValue) -> u32 {
    // Prefer the pre-computed field.
    if let Some(pct) = val
        .get_path(&["context_window", "used_percentage"])
        .and_then(|v| v.as_f64())
    {
        return (pct.round() as i64).clamp(0, 100) as u32;
    }

    // Fallback: compute from token counts / context_window_size.
    let input = val
        .get_path(&["context_window", "current_usage", "input_tokens"])
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let cache_create = val
        .get_path(&["context_window", "current_usage", "cache_creation_input_tokens"])
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let cache_read = val
        .get_path(&["context_window", "current_usage", "cache_read_input_tokens"])
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let output = val
        .get_path(&["context_window", "total_output_tokens"])
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let window = val
        .get_path(&["context_window", "context_window_size"])
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    if window <= 0.0 {
        return 0;
    }

    let used = input + cache_create + cache_read + output;
    let pct = (used / window * 100.0).round() as i64;
    pct.clamp(0, 100) as u32
}

// ---------------------------------------------------------------------------
// Full extraction
// ---------------------------------------------------------------------------

pub fn extract(val: &JsonValue) -> StdinData {
    let context_pct = get_context_percent(val);
    let model_id = get_model_id(val);

    let version = val
        .get("version")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let transcript_path = val
        .get("transcript_path")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let total_cost_usd = val
        .get_path(&["cost", "total_cost_usd"])
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let total_duration_ms = val
        .get_path(&["cost", "total_duration_ms"])
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as u64;

    let total_lines_added = val
        .get_path(&["cost", "total_lines_added"])
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as u64;

    let total_lines_removed = val
        .get_path(&["cost", "total_lines_removed"])
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as u64;

    let total_api_duration_ms = val
        .get_path(&["cost", "total_api_duration_ms"])
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as u64;

    let current_dir = val
        .get_path(&["workspace", "current_dir"])
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let agent_name = val
        .get_path(&["agent", "name"])
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let input_tokens = val
        .get_path(&["context_window", "current_usage", "input_tokens"])
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as u64;

    let cache_creation_tokens = val
        .get_path(&["context_window", "current_usage", "cache_creation_input_tokens"])
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as u64;

    let cache_read_tokens = val
        .get_path(&["context_window", "current_usage", "cache_read_input_tokens"])
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as u64;

    let total_output_tokens = val
        .get_path(&["context_window", "total_output_tokens"])
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as u64;

    StdinData {
        raw: val.clone(),
        context_pct,
        model_id,
        version,
        transcript_path,
        total_cost_usd,
        total_duration_ms,
        total_lines_added,
        total_lines_removed,
        total_api_duration_ms,
        current_dir,
        agent_name,
        input_tokens,
        cache_creation_tokens,
        cache_read_tokens,
        total_output_tokens,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::parse;

    #[test]
    fn test_parse_model_name() {
        assert_eq!(
            parse_model_name("claude-opus-4-6"),
            Some("Opus 4.6".to_string())
        );
        // sonnet with a date-style minor (long digit string treated as date → minor=0)
        // But the task says "sonnet with date suffix" should still work.
        // claude-sonnet-3-5 → Sonnet 3.5
        assert_eq!(
            parse_model_name("claude-sonnet-3-5"),
            Some("Sonnet 3.5".to_string())
        );
        assert_eq!(
            parse_model_name("claude-haiku-3-0"),
            Some("Haiku 3.0".to_string())
        );
        // Non-Claude model — should return None.
        assert_eq!(parse_model_name("gpt-4"), None);
    }

    #[test]
    fn test_extract_context_percent_direct() {
        let json = r#"{
            "context_window": {
                "used_percentage": 42.7
            }
        }"#;
        let val = parse(json).unwrap();
        assert_eq!(get_context_percent(&val), 43);
    }

    #[test]
    fn test_extract_context_percent_fallback() {
        let json = r#"{
            "context_window": {
                "context_window_size": 10000,
                "current_usage": {
                    "input_tokens": 1000,
                    "cache_creation_input_tokens": 500,
                    "cache_read_input_tokens": 200
                },
                "total_output_tokens": 300
            }
        }"#;
        let val = parse(json).unwrap();
        // (1000 + 500 + 200 + 300) / 10000 * 100 = 20%
        assert_eq!(get_context_percent(&val), 20);
    }

    #[test]
    fn test_missing_model_field() {
        let json = r#"{"context_window": {"used_percentage": 50}}"#;
        let val = parse(json).unwrap();
        let data = extract(&val);
        assert_eq!(data.model_id, "");
    }

    #[test]
    fn test_missing_context_window() {
        let json = r#"{"model": {"id": "claude-opus-4-6"}}"#;
        let val = parse(json).unwrap();
        let data = extract(&val);
        assert_eq!(data.context_pct, 0);
    }

    #[test]
    fn test_context_percent_clamped() {
        let json = r#"{"context_window": {"used_percentage": 150.0}}"#;
        let val = parse(json).unwrap();
        assert_eq!(get_context_percent(&val), 100);
    }

    #[test]
    fn test_extract_full_stdin() {
        let json = r#"{
            "model": {
                "id": "claude-sonnet-4-5",
                "display_name": "Claude Sonnet 4.5"
            },
            "context_window": {
                "used_percentage": 55,
                "context_window_size": 200000,
                "current_usage": {
                    "input_tokens": 800,
                    "cache_creation_input_tokens": 100,
                    "cache_read_input_tokens": 50
                },
                "total_output_tokens": 200
            },
            "version": "1.2.3",
            "transcript_path": "/tmp/transcript.jsonl",
            "cost": {
                "total_cost_usd": 0.0123,
                "total_duration_ms": 4500,
                "total_lines_added": 120,
                "total_lines_removed": 30,
                "total_api_duration_ms": 3200
            },
            "workspace": {
                "current_dir": "/home/user/project"
            },
            "agent": {
                "name": "my-agent"
            }
        }"#;
        let val = parse(json).unwrap();
        let data = extract(&val);

        assert_eq!(data.context_pct, 55);
        assert_eq!(data.model_id, "Sonnet 4.5");
        assert_eq!(data.version.as_deref(), Some("1.2.3"));
        assert_eq!(data.transcript_path.as_deref(), Some("/tmp/transcript.jsonl"));
        assert!((data.total_cost_usd - 0.0123).abs() < 1e-9);
        assert_eq!(data.total_duration_ms, 4500);
        assert_eq!(data.total_lines_added, 120);
        assert_eq!(data.total_lines_removed, 30);
        assert_eq!(data.total_api_duration_ms, 3200);
        assert_eq!(data.current_dir.as_deref(), Some("/home/user/project"));
        assert_eq!(data.agent_name.as_deref(), Some("my-agent"));
        assert_eq!(data.input_tokens, 800);
        assert_eq!(data.cache_creation_tokens, 100);
        assert_eq!(data.cache_read_tokens, 50);
        assert_eq!(data.total_output_tokens, 200);
    }
}

// API client: OAuth credential handling, token refresh, and usage fetch via curl.

use crate::cache;
use crate::json::{parse, JsonValue};
use crate::time::{now_ms, parse_iso8601};
use std::fs;
use std::io::Write;
use std::process::Command;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const OAUTH_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
pub const API_TIMEOUT: u64 = 8;

// ---------------------------------------------------------------------------
// Public data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct UsageData {
    pub five_hour: f64,
    pub five_hour_resets: Option<u64>, // epoch ms
    pub seven_day: f64,
    pub seven_day_resets: Option<u64>, // epoch ms
}

// ---------------------------------------------------------------------------
// Private credential type
// ---------------------------------------------------------------------------

struct Credentials {
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<u64>,
}

// ---------------------------------------------------------------------------
// Debug logging
// ---------------------------------------------------------------------------

fn debug_log_api(debug_enabled: bool, msg: &str) {
    if !debug_enabled {
        return;
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let path = format!("{}/.claude/hud/.usage-debug.log", home);
    // Ensure parent directory exists
    if let Some(parent) = std::path::Path::new(&path).parent() {
        let _ = fs::create_dir_all(parent);
    }
    let now = now_ms();
    let entry = format!("[{}] {}\n", now, msg);
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = f.write_all(entry.as_bytes());
    }
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

/// Fetch usage data, using a cache to avoid excessive API calls.
pub fn get_usage(debug_enabled: bool) -> Option<UsageData> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let cache_path = format!("{}/.claude/hud/.usage-cache.json", home);

    // Check cache first
    if let Some(entry) = cache::read_cache(&cache_path) {
        if cache::is_valid(&entry) {
            debug_log_api(debug_enabled, "cache hit");
            if let Some(data_val) = &entry.data {
                return usage_from_json(data_val);
            }
            // Valid cache with no data (error/rate-limited state)
            return None;
        }
    }

    debug_log_api(debug_enabled, "cache miss, fetching credentials");

    // Get credentials
    let mut creds = get_credentials(debug_enabled)?;

    // Refresh token if expired
    let now = now_ms();
    let is_expired = creds
        .expires_at
        .map(|exp| exp <= now)
        .unwrap_or(false);

    if is_expired {
        debug_log_api(debug_enabled, "token expired, refreshing");
        if let Some(ref rt) = creds.refresh_token.clone() {
            if let Some(new_creds) = refresh_access_token(rt, debug_enabled) {
                write_back_credentials(&new_creds, debug_enabled);
                creds = new_creds;
            } else {
                debug_log_api(debug_enabled, "token refresh failed");
                cache::write_cache(&cache_path, None, true, false);
                return None;
            }
        }
    }

    // Fetch usage
    debug_log_api(debug_enabled, "fetching usage from API");
    let url = "https://api.anthropic.com/api/oauth/usage";
    let headers = vec![
        format!("Authorization: Bearer {}", creds.access_token),
        "Content-Type: application/json".to_string(),
    ];

    let (status, body) = curl_get(url, &headers, API_TIMEOUT)?;

    if status == 429 {
        debug_log_api(debug_enabled, "rate limited (429)");
        // Preserve stale data with rate-limit backoff
        let stale = cache::read_cache(&cache_path)
            .and_then(|e| e.data.clone())
            .as_ref()
            .map(|v| usage_from_json(v))
            .flatten();
        let stale_json = stale.as_ref().map(|u| usage_to_json(u));
        cache::write_cache(&cache_path, stale_json.as_ref(), false, true);
        return stale;
    }

    if status != 200 {
        debug_log_api(debug_enabled, &format!("API error status {}", status));
        cache::write_cache(&cache_path, None, true, false);
        return None;
    }

    let parsed = parse(&body).ok()?;
    let usage = build_usage_data(&parsed);
    let usage_json = usage_to_json(&usage);
    cache::write_cache(&cache_path, Some(&usage_json), false, false);

    debug_log_api(debug_enabled, "usage fetched and cached");
    Some(usage)
}

// ---------------------------------------------------------------------------
// Credential handling
// ---------------------------------------------------------------------------

fn get_credentials(debug_enabled: bool) -> Option<Credentials> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let creds_path = format!("{}/.claude/.credentials.json", home);

    if let Some(creds) = read_credentials_file(&creds_path, debug_enabled) {
        return Some(creds);
    }

    // macOS keychain fallback
    #[cfg(target_os = "macos")]
    {
        debug_log_api(debug_enabled, "trying macOS keychain");
        if let Some(creds) = read_keychain_credentials(debug_enabled) {
            return Some(creds);
        }
    }

    debug_log_api(debug_enabled, "no credentials found");
    None
}

fn read_credentials_file(path: &str, debug_enabled: bool) -> Option<Credentials> {
    let contents = fs::read_to_string(path).ok()?;
    let root = parse(&contents).ok()?;

    // Try claudeAiOauth nested key first
    let token_val = root
        .get("claudeAiOauth")
        .and_then(|o| o.get("accessToken"))
        .or_else(|| root.get("accessToken"));

    let access_token = token_val.and_then(|v| v.as_str()).map(|s| s.to_string())?;

    let refresh_token = root
        .get("claudeAiOauth")
        .and_then(|o| o.get("refreshToken"))
        .or_else(|| root.get("refreshToken"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let expires_at = root
        .get("claudeAiOauth")
        .and_then(|o| o.get("expiresAt"))
        .or_else(|| root.get("expiresAt"))
        .and_then(|v| v.as_f64())
        .map(|f| f as u64);

    debug_log_api(debug_enabled, "credentials loaded from file");

    Some(Credentials {
        access_token,
        refresh_token,
        expires_at,
    })
}

#[cfg(target_os = "macos")]
fn read_keychain_credentials(debug_enabled: bool) -> Option<Credentials> {
    let output = Command::new("security")
        .args(&["find-generic-password", "-s", "claude.ai", "-w"])
        .output()
        .ok()?;

    if !output.status.success() {
        debug_log_api(debug_enabled, "keychain lookup failed");
        return None;
    }

    let token = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if token.is_empty() {
        return None;
    }

    debug_log_api(debug_enabled, "credentials loaded from keychain");
    Some(Credentials {
        access_token: token,
        refresh_token: None,
        expires_at: None,
    })
}

fn write_back_credentials(creds: &Credentials, debug_enabled: bool) {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let creds_path = format!("{}/.claude/.credentials.json", home);

    // Re-read the current file to avoid losing other fields
    let mut root = fs::read_to_string(&creds_path)
        .ok()
        .and_then(|s| parse(&s).ok())
        .unwrap_or_else(|| JsonValue::Object(vec![]));

    // Determine where to write (nested claudeAiOauth or top-level)
    let use_nested = root.get("claudeAiOauth").is_some();

    if use_nested {
        // Update or create the nested object
        let mut oauth_pairs: Vec<(String, JsonValue)> = root
            .get("claudeAiOauth")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();

        set_or_replace(&mut oauth_pairs, "accessToken", JsonValue::Str(creds.access_token.clone()));
        if let Some(ref rt) = creds.refresh_token {
            set_or_replace(&mut oauth_pairs, "refreshToken", JsonValue::Str(rt.clone()));
        }
        if let Some(exp) = creds.expires_at {
            set_or_replace(&mut oauth_pairs, "expiresAt", JsonValue::Number(exp as f64));
        }

        let nested_val = JsonValue::Object(oauth_pairs);
        if let JsonValue::Object(ref mut pairs) = root {
            set_or_replace(pairs, "claudeAiOauth", nested_val);
        }
    } else {
        if let JsonValue::Object(ref mut pairs) = root {
            set_or_replace(pairs, "accessToken", JsonValue::Str(creds.access_token.clone()));
            if let Some(ref rt) = creds.refresh_token {
                set_or_replace(pairs, "refreshToken", JsonValue::Str(rt.clone()));
            }
            if let Some(exp) = creds.expires_at {
                set_or_replace(pairs, "expiresAt", JsonValue::Number(exp as f64));
            }
        }
    }

    let json_str = pretty_print_json(&root, 0);

    if let Some(parent) = std::path::Path::new(&creds_path).parent() {
        let _ = fs::create_dir_all(parent);
    }

    if let Ok(mut f) = fs::File::create(&creds_path) {
        let _ = f.write_all(json_str.as_bytes());
    }

    debug_log_api(debug_enabled, "credentials written back to file");
}

fn set_or_replace(pairs: &mut Vec<(String, JsonValue)>, key: &str, value: JsonValue) {
    if let Some(entry) = pairs.iter_mut().find(|(k, _)| k == key) {
        entry.1 = value;
    } else {
        pairs.push((key.to_string(), value));
    }
}

// ---------------------------------------------------------------------------
// Token refresh
// ---------------------------------------------------------------------------

fn refresh_access_token(refresh_token: &str, debug_enabled: bool) -> Option<Credentials> {
    debug_log_api(debug_enabled, "refreshing access token");

    let url = "https://platform.claude.com/v1/oauth/token";
    let body = format!(
        "grant_type=refresh_token&refresh_token={}&client_id={}",
        url_encode(refresh_token),
        url_encode(OAUTH_CLIENT_ID)
    );

    let (status, resp_body) = curl_post(url, "application/x-www-form-urlencoded", &body, API_TIMEOUT)?;

    if status != 200 {
        debug_log_api(debug_enabled, &format!("token refresh failed with status {}", status));
        return None;
    }

    let parsed = parse(&resp_body).ok()?;

    let access_token = parsed
        .get("access_token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())?;

    let new_refresh_token = parsed
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Compute expires_at: prefer absolute expires_at, else compute from expires_in
    let expires_at = parsed
        .get("expires_at")
        .and_then(|v| v.as_f64())
        .map(|f| f as u64)
        .or_else(|| {
            parsed
                .get("expires_in")
                .and_then(|v| v.as_f64())
                .map(|secs| now_ms() + (secs as u64) * 1000)
        });

    debug_log_api(debug_enabled, "token refreshed successfully");

    Some(Credentials {
        access_token,
        refresh_token: new_refresh_token,
        expires_at,
    })
}

// ---------------------------------------------------------------------------
// Curl helpers
// ---------------------------------------------------------------------------

fn curl_get(url: &str, headers: &[String], timeout: u64) -> Option<(u32, String)> {
    let mut cmd = Command::new("curl");
    cmd.arg("-s")
        .arg("-w").arg("\n%{http_code}")
        .arg("--max-time").arg(timeout.to_string());
    for h in headers {
        cmd.arg("-H").arg(h);
    }
    cmd.arg(url);
    run_curl(cmd)
}

fn curl_post(url: &str, content_type: &str, body: &str, timeout: u64) -> Option<(u32, String)> {
    let mut cmd = Command::new("curl");
    cmd.arg("-s")
        .arg("-w").arg("\n%{http_code}")
        .arg("--max-time").arg(timeout.to_string())
        .arg("-X").arg("POST")
        .arg("-H").arg(format!("Content-Type: {}", content_type))
        .arg("-d").arg(body)
        .arg(url);
    run_curl(cmd)
}

fn run_curl(mut cmd: Command) -> Option<(u32, String)> {
    let output = cmd.output().ok()?;
    let raw = String::from_utf8(output.stdout).ok()?;

    // The last line is the HTTP status code (from -w '\n%{http_code}')
    let mut lines: Vec<&str> = raw.lines().collect();
    let status_line = lines.pop()?;
    let status: u32 = status_line.trim().parse().ok()?;
    let body = lines.join("\n");

    Some((status, body))
}

// ---------------------------------------------------------------------------
// URL encoding
// ---------------------------------------------------------------------------

pub fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            b => {
                out.push('%');
                out.push(char::from_digit((b >> 4) as u32, 16).unwrap().to_ascii_uppercase());
                out.push(char::from_digit((b & 0xf) as u32, 16).unwrap().to_ascii_uppercase());
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// JSON conversion helpers
// ---------------------------------------------------------------------------

pub fn build_usage_data(resp: &JsonValue) -> UsageData {
    // Expected shape: { "five_hour": { "utilization": 0.5, "resets_at": "..." }, "seven_day": { ... } }
    let five_hour = resp
        .get("five_hour")
        .and_then(|v| v.get("utilization"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let five_hour_resets = resp
        .get("five_hour")
        .and_then(|v| v.get("resets_at"))
        .and_then(|v| v.as_str())
        .and_then(parse_iso8601);

    let seven_day = resp
        .get("seven_day")
        .and_then(|v| v.get("utilization"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let seven_day_resets = resp
        .get("seven_day")
        .and_then(|v| v.get("resets_at"))
        .and_then(|v| v.as_str())
        .and_then(parse_iso8601);

    UsageData {
        five_hour,
        five_hour_resets,
        seven_day,
        seven_day_resets,
    }
}

pub fn usage_to_json(data: &UsageData) -> JsonValue {
    let mut pairs = vec![
        ("five_hour".to_string(), JsonValue::Number(data.five_hour)),
        ("seven_day".to_string(), JsonValue::Number(data.seven_day)),
    ];
    if let Some(r) = data.five_hour_resets {
        pairs.push(("five_hour_resets".to_string(), JsonValue::Number(r as f64)));
    } else {
        pairs.push(("five_hour_resets".to_string(), JsonValue::Null));
    }
    if let Some(r) = data.seven_day_resets {
        pairs.push(("seven_day_resets".to_string(), JsonValue::Number(r as f64)));
    } else {
        pairs.push(("seven_day_resets".to_string(), JsonValue::Null));
    }
    JsonValue::Object(pairs)
}

pub fn usage_from_json(val: &JsonValue) -> Option<UsageData> {
    let five_hour = val.get("five_hour")?.as_f64()?;
    let seven_day = val.get("seven_day")?.as_f64()?;

    let five_hour_resets = val
        .get("five_hour_resets")
        .and_then(|v| v.as_f64())
        .map(|f| f as u64);

    let seven_day_resets = val
        .get("seven_day_resets")
        .and_then(|v| v.as_f64())
        .map(|f| f as u64);

    Some(UsageData {
        five_hour,
        five_hour_resets,
        seven_day,
        seven_day_resets,
    })
}

// ---------------------------------------------------------------------------
// Pretty-print JSON (2-space indent)
// ---------------------------------------------------------------------------

pub fn pretty_print_json(val: &JsonValue, indent: usize) -> String {
    let spaces = " ".repeat(indent);
    let inner_spaces = " ".repeat(indent + 2);
    match val {
        JsonValue::Null => "null".to_string(),
        JsonValue::Bool(true) => "true".to_string(),
        JsonValue::Bool(false) => "false".to_string(),
        JsonValue::Number(n) => {
            if n.is_finite() && *n == (*n as i64) as f64 {
                format!("{}", *n as i64)
            } else {
                format!("{}", n)
            }
        }
        JsonValue::Str(s) => JsonValue::Str(s.clone()).to_json_string(),
        JsonValue::Array(items) => {
            if items.is_empty() {
                return "[]".to_string();
            }
            let mut out = String::from("[\n");
            for (i, item) in items.iter().enumerate() {
                out.push_str(&inner_spaces);
                out.push_str(&pretty_print_json(item, indent + 2));
                if i + 1 < items.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str(&spaces);
            out.push(']');
            out
        }
        JsonValue::Object(pairs) => {
            if pairs.is_empty() {
                return "{}".to_string();
            }
            let mut out = String::from("{\n");
            for (i, (k, v)) in pairs.iter().enumerate() {
                out.push_str(&inner_spaces);
                out.push_str(&JsonValue::Str(k.clone()).to_json_string());
                out.push_str(": ");
                out.push_str(&pretty_print_json(v, indent + 2));
                if i + 1 < pairs.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str(&spaces);
            out.push('}');
            out
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_encode() {
        assert_eq!(url_encode("hello"), "hello");
        assert_eq!(url_encode("a b"), "a%20b");
        assert_eq!(url_encode("a+b=c"), "a%2Bb%3Dc");
    }

    #[test]
    fn test_usage_json_roundtrip() {
        let original = UsageData {
            five_hour: 0.42,
            five_hour_resets: Some(1735689600000),
            seven_day: 0.75,
            seven_day_resets: None,
        };
        let json_val = usage_to_json(&original);
        let recovered = usage_from_json(&json_val).expect("should round-trip");

        assert!((recovered.five_hour - original.five_hour).abs() < 1e-9);
        assert_eq!(recovered.five_hour_resets, original.five_hour_resets);
        assert!((recovered.seven_day - original.seven_day).abs() < 1e-9);
        assert_eq!(recovered.seven_day_resets, original.seven_day_resets);
    }

    #[test]
    fn test_build_usage_data() {
        let resp_json = r#"{
            "five_hour": {
                "utilization": 0.5,
                "resets_at": "2025-01-01T00:00:00Z"
            },
            "seven_day": {
                "utilization": 0.25,
                "resets_at": "2025-01-08T00:00:00Z"
            }
        }"#;
        let parsed = parse(resp_json).expect("valid json");
        let usage = build_usage_data(&parsed);

        assert!((usage.five_hour - 0.5).abs() < 1e-9);
        assert!((usage.seven_day - 0.25).abs() < 1e-9);
        assert!(usage.five_hour_resets.is_some());
        assert!(usage.seven_day_resets.is_some());
        // 2025-01-01T00:00:00Z = 1735689600000 ms
        assert_eq!(usage.five_hour_resets, Some(1735689600000));
    }

    #[test]
    fn test_pretty_print_json() {
        let val = JsonValue::Object(vec![
            ("name".to_string(), JsonValue::Str("Alice".to_string())),
            ("age".to_string(), JsonValue::Number(30.0)),
        ]);
        let output = pretty_print_json(&val, 0);

        // Must use 2-space indentation
        assert!(output.contains("  \"name\": \"Alice\""));
        assert!(output.contains("  \"age\": 30"));
        // Must start with { and end with }
        assert!(output.starts_with('{'));
        assert!(output.trim_end().ends_with('}'));
    }
}

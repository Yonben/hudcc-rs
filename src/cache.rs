// File-based JSON cache with TTL logic.

use crate::json::{parse, JsonValue};
use crate::time::now_ms;
use std::fs;
use std::io::Write;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const CACHE_TTL_MS: u64 = 60_000;
pub const CACHE_TTL_FAILURE_MS: u64 = 15_000;
pub const CACHE_TTL_RATELIMIT_MS: u64 = 120_000;
pub const VERSION_CACHE_TTL_MS: u64 = 3_600_000;

// ---------------------------------------------------------------------------
// CacheEntry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub timestamp: u64,
    pub data: Option<JsonValue>,
    pub error: bool,
    pub rate_limited: bool,
}

// ---------------------------------------------------------------------------
// Public functions
// ---------------------------------------------------------------------------

/// Read a JSON cache file and parse into a CacheEntry.
/// Returns None if the file doesn't exist, can't be read, or is malformed.
pub fn read_cache(path: &str) -> Option<CacheEntry> {
    let contents = fs::read_to_string(path).ok()?;
    let root = parse(&contents).ok()?;

    let timestamp = root
        .get("timestamp")
        .and_then(|v| v.as_f64())
        .map(|f| f as u64)?;

    let data = root.get("data").cloned();
    // Treat an explicit null as no data
    let data = match data {
        Some(JsonValue::Null) | None => None,
        Some(v) => Some(v),
    };

    let error = root
        .get("error")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let rate_limited = root
        .get("rateLimited")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    Some(CacheEntry {
        timestamp,
        data,
        error,
        rate_limited,
    })
}

/// Check whether a CacheEntry is still within its TTL.
pub fn is_valid(entry: &CacheEntry) -> bool {
    let now = now_ms();
    let ttl = if entry.rate_limited {
        CACHE_TTL_RATELIMIT_MS
    } else if entry.error {
        CACHE_TTL_FAILURE_MS
    } else {
        CACHE_TTL_MS
    };
    now.saturating_sub(entry.timestamp) < ttl
}

/// Write a JSON cache file with the current timestamp.
/// Creates parent directories as needed.
pub fn write_cache(path: &str, data: Option<&JsonValue>, error: bool, rate_limited: bool) {
    let timestamp = now_ms();
    let data_val = data
        .map(|v| v.to_json_string())
        .unwrap_or_else(|| "null".to_string());
    let json = format!(
        "{{\"timestamp\":{},\"data\":{},\"error\":{},\"rateLimited\":{}}}",
        timestamp, data_val, error, rate_limited
    );
    if ensure_parent_dir(path).is_err() {
        return;
    }
    if let Ok(mut f) = fs::File::create(path) {
        let _ = f.write_all(json.as_bytes());
    }
}

/// Read a version string from a cache file, respecting VERSION_CACHE_TTL_MS.
/// Returns None if file is missing, malformed, or expired.
pub fn read_version_cache(path: &str) -> Option<String> {
    let contents = fs::read_to_string(path).ok()?;
    let root = parse(&contents).ok()?;

    let timestamp = root
        .get("timestamp")
        .and_then(|v| v.as_f64())
        .map(|f| f as u64)?;

    let now = now_ms();
    if now.saturating_sub(timestamp) >= VERSION_CACHE_TTL_MS {
        return None;
    }

    root.get("version").and_then(|v| v.as_str()).map(|s| s.to_string())
}

/// Write a version string to a cache file with the current timestamp.
pub fn write_version_cache(path: &str, version: &str) {
    let timestamp = now_ms();
    // Escape version string via JsonValue
    let version_json = JsonValue::Str(version.to_string()).to_json_string();
    let json = format!(
        "{{\"timestamp\":{},\"version\":{}}}",
        timestamp, version_json
    );
    if ensure_parent_dir(path).is_err() {
        return;
    }
    if let Ok(mut f) = fs::File::create(path) {
        let _ = f.write_all(json.as_bytes());
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn ensure_parent_dir(path: &str) -> std::io::Result<()> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> String {
        let mut p = std::env::temp_dir();
        p.push(name);
        p.to_string_lossy().to_string()
    }

    #[test]
    fn test_write_and_read_cache() {
        let path = temp_path("hud_rs_test_cache_basic.json");
        let data = JsonValue::Object(vec![
            ("key".to_string(), JsonValue::Str("value".to_string())),
        ]);
        write_cache(&path, Some(&data), false, false);

        let entry = read_cache(&path).expect("cache should be readable");
        assert!(!entry.error);
        assert!(!entry.rate_limited);
        assert!(entry.data.is_some());

        // Entry written just now should be valid
        assert!(is_valid(&entry));

        // Cleanup
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_expired_cache() {
        let path = temp_path("hud_rs_test_cache_expired.json");
        // Write a cache file with a very old timestamp (1000 ms since epoch)
        let json = r#"{"timestamp":1000,"data":null,"error":false,"rateLimited":false}"#;
        if let Ok(mut f) = fs::File::create(&path) {
            let _ = f.write_all(json.as_bytes());
        }

        let entry = read_cache(&path).expect("cache should be readable");
        assert_eq!(entry.timestamp, 1000);
        assert!(!is_valid(&entry), "old cache entry should not be valid");

        // Cleanup
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_version_cache() {
        let path = temp_path("hud_rs_test_version_cache.json");
        write_version_cache(&path, "1.2.3");

        let version = read_version_cache(&path).expect("version cache should be readable");
        assert_eq!(version, "1.2.3");

        // Cleanup
        let _ = fs::remove_file(&path);
    }
}

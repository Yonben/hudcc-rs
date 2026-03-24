// Self-update logic: check GitHub Releases, compare versions, download & replace.

use crate::cache;
use crate::json::{self, JsonValue};
use crate::time::now_ms;
use std::fs;
use std::io::Write;
use std::process::Command;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const GITHUB_REPO: &str = "yonben/hudcc-rs";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const ASSET_NAME: &str = "hudcc-rs-x86_64-unknown-linux-musl";

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
const ASSET_NAME: &str = "hudcc-rs-x86_64-apple-darwin";

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const ASSET_NAME: &str = "hudcc-rs-aarch64-apple-darwin";

#[cfg(not(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
)))]
const ASSET_NAME: &str = "";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum UpdateStatus {
    Updated(String),   // Successfully updated to this version
    Available(String), // Update available but auto-update disabled
}

// ---------------------------------------------------------------------------
// Version comparison
// ---------------------------------------------------------------------------

fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    let s = s.strip_prefix('v').unwrap_or(s);
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let major = parts[0].parse().ok()?;
    let minor = parts[1].parse().ok()?;
    let patch = parts[2].parse().ok()?;
    Some((major, minor, patch))
}

fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Cache helpers
// ---------------------------------------------------------------------------

fn home_dir() -> String {
    std::env::var("HOME").unwrap_or_else(|_| "/tmp".into())
}

fn update_cache_path() -> String {
    format!("{}/.claude/hud/.hud-update-cache.json", home_dir())
}

fn read_update_cache() -> Option<String> {
    let path = update_cache_path();
    let contents = fs::read_to_string(&path).ok()?;
    let root = json::parse(&contents).ok()?;

    let timestamp = root.get("timestamp")?.as_f64()? as u64;
    let now = now_ms();
    if now.saturating_sub(timestamp) >= cache::UPDATE_CACHE_TTL_MS {
        return None;
    }

    root.get("latest_version")?.as_str().map(|s| s.to_string())
}

fn write_update_cache(version: &str) {
    let path = update_cache_path();
    let timestamp = now_ms();
    let version_json = JsonValue::Str(version.to_string()).to_json_string();
    let json = format!(
        "{{\"timestamp\":{},\"latest_version\":{}}}",
        timestamp, version_json
    );
    if let Some(parent) = std::path::Path::new(&path).parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut f) = fs::File::create(&path) {
        let _ = f.write_all(json.as_bytes());
    }
}

// ---------------------------------------------------------------------------
// GitHub API
// ---------------------------------------------------------------------------

fn fetch_latest_tag() -> Option<String> {
    let url = format!(
        "https://api.github.com/repos/{}/releases/latest",
        GITHUB_REPO
    );
    let output = Command::new("curl")
        .args([
            "-s",
            "--max-time", "5",
            "-H", "Accept: application/vnd.github+json",
        ])
        .arg(&url)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let body = String::from_utf8_lossy(&output.stdout);
    let parsed = json::parse(&body).ok()?;
    parsed.get("tag_name")?.as_str().map(|s| s.to_string())
}

// ---------------------------------------------------------------------------
// Download & replace
// ---------------------------------------------------------------------------

fn download_and_replace(tag: &str) -> bool {
    if ASSET_NAME.is_empty() {
        return false;
    }

    let exe_path = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return false,
    };

    let tmp_path = format!("{}.tmp", exe_path.display());

    let url = format!(
        "https://github.com/{}/releases/download/{}/{}",
        GITHUB_REPO, tag, ASSET_NAME
    );

    let output = Command::new("curl")
        .args([
            "-fsSL",
            "--max-time", "30",
            "-o",
        ])
        .arg(&tmp_path)
        .arg(&url)
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => {
            let _ = fs::remove_file(&tmp_path);
            return false;
        }
    };
    let _ = output; // used above

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o755));
    }

    // Atomic rename
    if fs::rename(&tmp_path, &exe_path).is_err() {
        let _ = fs::remove_file(&tmp_path);
        return false;
    }

    true
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Check for updates and optionally self-update.
/// Returns `None` if already on latest or check fails.
pub fn check_for_update() -> Option<UpdateStatus> {
    if ASSET_NAME.is_empty() {
        return None;
    }

    // Check cache first
    let latest_tag = match read_update_cache() {
        Some(cached) => cached,
        None => {
            let tag = fetch_latest_tag()?;
            write_update_cache(&tag);
            tag
        }
    };

    if !is_newer(&latest_tag, CURRENT_VERSION) {
        return None;
    }

    let version_display = latest_tag.strip_prefix('v').unwrap_or(&latest_tag).to_string();

    let auto_update_disabled = std::env::var("HUD_NO_AUTO_UPDATE")
        .ok()
        .map(|v| v == "1")
        .unwrap_or(false);

    if auto_update_disabled {
        return Some(UpdateStatus::Available(version_display));
    }

    if download_and_replace(&latest_tag) {
        Some(UpdateStatus::Updated(version_display))
    } else {
        // Download failed — show as available instead of hiding
        Some(UpdateStatus::Available(version_display))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version_basic() {
        assert_eq!(parse_version("1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("0.1.0"), Some((0, 1, 0)));
    }

    #[test]
    fn test_parse_version_invalid() {
        assert_eq!(parse_version("1.2"), None);
        assert_eq!(parse_version("abc"), None);
        assert_eq!(parse_version(""), None);
        assert_eq!(parse_version("1.2.3.4"), None);
    }

    #[test]
    fn test_is_newer() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.0.9", "0.1.0"));
    }

    #[test]
    fn test_is_newer_major_bump() {
        assert!(is_newer("2.0.0", "1.9.9"));
        assert!(is_newer("1.0.0", "0.99.99"));
    }
}

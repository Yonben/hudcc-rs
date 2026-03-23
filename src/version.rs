use std::process::Command;
use crate::json;
use crate::cache;

fn home_dir() -> String {
    std::env::var("HOME").unwrap_or_else(|_| "/tmp".into())
}

fn version_cache_path() -> String {
    format!("{}/.claude/hud/.version-cache.json", home_dir())
}

/// Get the latest Claude Code version from npm. Uses 1hr cache.
pub fn get_latest_version() -> Option<String> {
    let cp = version_cache_path();

    // Check cache
    if let Some(cached) = cache::read_version_cache(&cp) {
        return Some(cached);
    }

    // Fetch from npm
    let output = Command::new("curl")
        .args([
            "-s",
            "--max-time", "3",
            "-H", "Accept: application/json",
            "https://registry.npmjs.org/@anthropic-ai/claude-code/latest",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let body = String::from_utf8_lossy(&output.stdout);
    let parsed = json::parse(&body).ok()?;
    let version = parsed.get("version")?.as_str()?.to_string();

    cache::write_version_cache(&cp, &version);
    Some(version)
}

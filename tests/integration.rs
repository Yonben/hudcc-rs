use hudcc_rs::ansi::strip_ansi;
use hudcc_rs::api::UsageData;
use hudcc_rs::config::{Config, Layout};
use hudcc_rs::render::render;
use hudcc_rs::stdin::StdinData;
use hudcc_rs::transcript::TranscriptData;
use hudcc_rs::json::JsonValue;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a default StdinData, then apply overrides via `f`.
fn make_stdin(f: impl FnOnce(&mut StdinData)) -> StdinData {
    let mut data = StdinData {
        raw: JsonValue::Null,
        context_pct: 42,
        model_id: "claude-sonnet-4-20250514".to_string(),
        version: Some("1.0.30".to_string()),
        transcript_path: None,
        total_cost_usd: 0.125,
        total_duration_ms: 65_000,
        total_lines_added: 120,
        total_lines_removed: 30,
        total_api_duration_ms: 12_000,
        current_dir: Some("/home/user/project".to_string()),
        agent_name: None,
        input_tokens: 50_000,
        cache_creation_tokens: 10_000,
        cache_read_tokens: 25_000,
        total_output_tokens: 8_000,
    };
    f(&mut data);
    data
}

/// Strip ANSI escape codes and replace non-breaking spaces with regular spaces.
fn plain(s: &str) -> String {
    strip_ansi(s).replace('\u{00A0}', " ")
}

fn default_transcript() -> TranscriptData {
    TranscriptData {
        session_start: None,
        agents: vec![],
        todos: vec![],
    }
}

fn all_columns_config(layout: Layout) -> Config {
    Config {
        columns: vec![
            "5h Usage".into(),
            "7d Usage".into(),
            "Context".into(),
            "Model".into(),
            "Version".into(),
            "Session".into(),
            "Changes".into(),
            "Directory".into(),
            "Cost".into(),
            "Tokens".into(),
            "Output Tokens".into(),
            "Cache".into(),
            "API Time".into(),
            "5h Reset".into(),
            "7d Reset".into(),
        ],
        layout,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_full_pipeline_all_columns() {
    let stdin = make_stdin(|_| {});
    let usage = UsageData {
        five_hour: 35.0,
        five_hour_resets: None,
        seven_day: 12.5,
        seven_day_resets: None,
    };
    let transcript = default_transcript();
    let config = all_columns_config(Layout::Vertical);

    let output = render(
        Some(&usage),
        &transcript,
        &stdin,
        Some("1.0.31"),
        &config,
    );
    let text = plain(&output);

    // Key labels present
    assert!(text.contains("5h Usage"), "missing '5h Usage' label in: {}", text);
    assert!(text.contains("7d Usage"), "missing '7d Usage' label in: {}", text);
    assert!(text.contains("Context"), "missing 'Context' label in: {}", text);
    assert!(text.contains("Model"), "missing 'Model' label in: {}", text);
    assert!(text.contains("Version"), "missing 'Version' label in: {}", text);

    // Key values present
    assert!(text.contains("35%"), "missing '35%' in: {}", text);
    assert!(text.contains("12%"), "missing '12%' (7d) in: {}", text);
    assert!(text.contains("42%"), "missing '42%' context in: {}", text);
    assert!(text.contains("claude-sonnet"), "missing 'claude-sonnet' model in: {}", text);
    assert!(text.contains("1.0.30"), "missing version '1.0.30' in: {}", text);
}

#[test]
fn test_full_pipeline_no_usage() {
    let stdin = make_stdin(|d| {
        d.version = None;
    });
    let transcript = default_transcript();
    let config = all_columns_config(Layout::Vertical);

    let output = render(
        None,
        &transcript,
        &stdin,
        None,
        &config,
    );
    let text = plain(&output);

    assert!(text.contains("N/A"), "expected 'N/A' when no usage data, got: {}", text);
}

#[test]
fn test_vertical_layout_has_two_rows() {
    let stdin = make_stdin(|_| {});
    let transcript = default_transcript();
    let config = all_columns_config(Layout::Vertical);

    let output = render(
        None,
        &transcript,
        &stdin,
        None,
        &config,
    );

    let non_empty_lines: Vec<&str> = output
        .lines()
        .filter(|l: &&str| !l.trim().is_empty())
        .collect();

    assert!(
        non_empty_lines.len() >= 2,
        "vertical layout should produce at least 2 non-empty lines, got {}: {:?}",
        non_empty_lines.len(),
        non_empty_lines
    );
}

#[test]
fn test_horizontal_layout_single_row() {
    let stdin = make_stdin(|_| {});
    let transcript = default_transcript();
    let config = all_columns_config(Layout::Horizontal);

    let output = render(
        None,
        &transcript,
        &stdin,
        None,
        &config,
    );
    let text = plain(&output);

    let non_empty_lines: Vec<&str> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect();

    // Horizontal layout should have label+value pairs on one line
    assert!(
        non_empty_lines.len() >= 1,
        "horizontal layout should produce at least 1 line"
    );

    // The first non-empty line should contain both a label and a value
    let first = non_empty_lines[0];
    assert!(
        first.contains("5h Usage") && first.contains("Context"),
        "horizontal line should contain multiple label+value pairs, got: {}",
        first
    );
}

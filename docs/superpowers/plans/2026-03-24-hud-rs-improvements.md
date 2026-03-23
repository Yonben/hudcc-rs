# HUD RS Improvements Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Improve the hud_rs Claude Code status line with render deduplication, UX polish, error observability, test coverage, and JSON parser performance.

**Architecture:** Five independent improvement areas applied sequentially to `render.rs`, `main.rs`, `api.rs`, `config.rs`, `stdin.rs`, and `json.rs`. Step 2 (UX) depends on step 1 (render refactor). All others are independent.

**Tech Stack:** Rust (2021 edition), zero external dependencies, `std` only.

**Spec:** `docs/superpowers/specs/2026-03-24-hud-rs-improvements-design.md`

---

### Task 1: Refactor render.rs column building to declarative registry

**Files:**
- Modify: `src/render.rs:46-271` (column building section)

- [ ] **Step 1: Write a test that captures current render output for regression**

Add to `src/render.rs` in the `#[cfg(test)] mod tests` block:

```rust
#[test]
fn test_render_all_columns_regression() {
    let usage = UsageData {
        five_hour: 42.0,
        five_hour_resets: None,
        seven_day: 15.0,
        seven_day_resets: None,
    };
    let transcript = TranscriptData {
        session_start: None,
        agents: vec![],
        todos: vec![],
    };
    let stdin_data = StdinData {
        raw: crate::json::JsonValue::Null,
        context_pct: 30,
        model_id: "Opus 4.6".to_string(),
        version: Some("1.0.0".to_string()),
        transcript_path: None,
        total_cost_usd: 0.50,
        total_duration_ms: 120000,
        total_lines_added: 42,
        total_lines_removed: 7,
        total_api_duration_ms: 5000,
        current_dir: Some("/home/user/project".to_string()),
        agent_name: None,
        input_tokens: 5000,
        cache_creation_tokens: 1000,
        cache_read_tokens: 3000,
        total_output_tokens: 800,
    };
    let config = Config {
        columns: vec![
            "5h Usage".into(), "7d Usage".into(), "Context".into(),
            "Model".into(), "Version".into(), "Session".into(),
            "Changes".into(), "Directory".into(), "Cost".into(),
            "Tokens".into(), "Output Tokens".into(), "Cache".into(),
            "API Time".into(), "5h Reset".into(), "7d Reset".into(),
        ],
        layout: Layout::Vertical,
    };
    let out = render(Some(&usage), &transcript, &stdin_data, Some("1.0.0"), &config);
    let plain = crate::ansi::strip_ansi(&out).replace('\u{00A0}', " ");

    // All columns present
    assert!(plain.contains("5h Usage:"), "missing 5h Usage");
    assert!(plain.contains("42%"), "missing 5h value");
    assert!(plain.contains("7d Usage:"), "missing 7d Usage");
    assert!(plain.contains("15%"), "missing 7d value");
    assert!(plain.contains("Context:"), "missing Context");
    assert!(plain.contains("30%"), "missing context value");
    assert!(plain.contains("Model:"), "missing Model");
    assert!(plain.contains("Opus 4.6"), "missing model value");
    assert!(plain.contains("Version:"), "missing Version");
    assert!(plain.contains("v1.0.0"), "missing version value");
    assert!(plain.contains("Session:"), "missing Session");
    assert!(plain.contains("Changes:"), "missing Changes");
    assert!(plain.contains("+42"), "missing added");
    assert!(plain.contains("-7"), "missing removed");
    assert!(plain.contains("Directory:"), "missing Directory");
    assert!(plain.contains("/home/user/project"), "missing dir value");
    assert!(plain.contains("Cost:"), "missing Cost");
    assert!(plain.contains("$0.50"), "missing cost value");
    assert!(plain.contains("Tokens:"), "missing Tokens");
    assert!(plain.contains("Out Tokens:"), "missing Out Tokens");
    assert!(plain.contains("Cache:"), "missing Cache");
    assert!(plain.contains("API Time:"), "missing API Time");
    assert!(plain.contains("5h Reset:"), "missing 5h Reset");
    assert!(plain.contains("7d Reset:"), "missing 7d Reset");
}
```

- [ ] **Step 2: Run test to verify it passes (captures current behavior)**

Run: `cargo test test_render_all_columns_regression -- --nocapture`
Expected: PASS

- [ ] **Step 3: Replace 15 if-blocks with declarative column registry**

Replace lines 46-271 in `src/render.rs` (the `// Build columns` section) with:

```rust
    // -----------------------------------------------------------------------
    // Build columns
    // -----------------------------------------------------------------------
    let defs: Vec<(&str, Box<dyn Fn() -> Column + '_>)> = vec![
        ("5h Usage", Box::new(|| {
            let label = format!("{}5h Usage:{}", SLATE800_BOLD, RESET);
            let value = if let Some(u) = usage {
                let pct = u.five_hour;
                let color = color_for_percent(pct, 60.0, 80.0);
                let mut v = format!("{}{:.0}%{}", color, pct, RESET);
                if let Some(resets) = u.five_hour_resets {
                    let rt = format_reset_time(resets);
                    if !rt.is_empty() {
                        v.push_str(&format!(" {}{}{}", SLATE600, rt, RESET));
                    }
                }
                v
            } else {
                format!("{}N/A{}", SLATE600, RESET)
            };
            Column { label, value }
        })),
        ("7d Usage", Box::new(|| {
            let label = format!("{}7d Usage:{}", SLATE800_BOLD, RESET);
            let value = if let Some(u) = usage {
                let pct = u.seven_day;
                let color = color_for_percent(pct, 60.0, 80.0);
                let mut v = format!("{}{:.0}%{}", color, pct, RESET);
                if let Some(resets) = u.seven_day_resets {
                    let rt = format_reset_time(resets);
                    if !rt.is_empty() {
                        v.push_str(&format!(" {}{}{}", SLATE600, rt, RESET));
                    }
                }
                v
            } else {
                format!("{}N/A{}", SLATE600, RESET)
            };
            Column { label, value }
        })),
        ("Context", Box::new(|| {
            let label = format!("{}Context:{}", SLATE800_BOLD, RESET);
            let pct = stdin.context_pct as f64;
            let color = color_for_percent(pct, 70.0, 85.0);
            let value = format!("{}{:.0}% {}{}Used{}", color, pct, RESET, SLATE600, RESET);
            Column { label, value }
        })),
        ("Model", Box::new(|| {
            let label = "Model:".to_string();
            let value = format!("{}{}{}", SLATE600, stdin.model_id, RESET);
            Column { label, value }
        })),
        ("Version", Box::new(|| {
            let label = "Version:".to_string();
            let value = if let Some(ref ver) = stdin.version {
                let dot = match latest_version {
                    None => format!("{}●{} ", GREEN, RESET),
                    Some(lv) => {
                        if ver == lv {
                            format!("{}●{} ", GREEN, RESET)
                        } else {
                            format!("{}●{} ", YELLOW, RESET)
                        }
                    }
                };
                format!("{}{}v{}{}", dot, SLATE600, ver, RESET)
            } else {
                format!("{}N/A{}", SLATE600, RESET)
            };
            Column { label, value }
        })),
        ("Session", Box::new(|| {
            let label = "Session:".to_string();
            let value = if stdin.total_duration_ms > 0 {
                format!("{}{}{}", SLATE600, format_duration(stdin.total_duration_ms), RESET)
            } else {
                format!("{}N/A{}", SLATE600, RESET)
            };
            Column { label, value }
        })),
        ("Changes", Box::new(|| {
            let label = "Changes:".to_string();
            let added = stdin.total_lines_added;
            let removed = stdin.total_lines_removed;
            let value = if added == 0 && removed == 0 {
                format!("{}+0/-0{}", SLATE600, RESET)
            } else {
                format!("{}+{}{}/{}{}{}",
                    GREEN, added, RESET, RED, format!("-{}", removed), RESET)
            };
            Column { label, value }
        })),
        ("Directory", Box::new(|| {
            let label = "Directory:".to_string();
            let value = match &stdin.current_dir {
                Some(d) => format!("{}{}{}", SLATE600, d, RESET),
                None => format!("{}N/A{}", SLATE600, RESET),
            };
            Column { label, value }
        })),
        ("Cost", Box::new(|| {
            let label = "Cost:".to_string();
            let cost = stdin.total_cost_usd;
            let color = if cost >= 1.0 {
                RED
            } else if cost >= 0.25 {
                YELLOW
            } else {
                GREEN
            };
            let value = format!("{}${:.2}{}", color, cost, RESET);
            Column { label, value }
        })),
        ("Tokens", Box::new(|| {
            let label = "Tokens:".to_string();
            let total = stdin.input_tokens + stdin.cache_creation_tokens + stdin.cache_read_tokens;
            let value = format!("{}{}{}", SLATE600, format_tokens(total), RESET);
            Column { label, value }
        })),
        ("Output Tokens", Box::new(|| {
            let label = "Out Tokens:".to_string();
            let value = format!("{}{}{}", SLATE600, format_tokens(stdin.total_output_tokens), RESET);
            Column { label, value }
        })),
        ("Cache", Box::new(|| {
            let label = "Cache:".to_string();
            let total_in = stdin.input_tokens + stdin.cache_creation_tokens + stdin.cache_read_tokens;
            let cache_pct = if total_in > 0 {
                (stdin.cache_read_tokens as f64 / total_in as f64) * 100.0
            } else {
                0.0
            };
            let color = if cache_pct >= 50.0 {
                GREEN
            } else if cache_pct >= 20.0 {
                YELLOW
            } else {
                SLATE600
            };
            let value = format!("{}{:.0}%{}{} hit{}", color, cache_pct, RESET, SLATE600, RESET);
            Column { label, value }
        })),
        ("API Time", Box::new(|| {
            let label = "API Time:".to_string();
            let value = if stdin.total_api_duration_ms > 0 {
                format!("{}{}{}", SLATE600, format_duration(stdin.total_api_duration_ms), RESET)
            } else {
                format!("{}N/A{}", SLATE600, RESET)
            };
            Column { label, value }
        })),
        ("5h Reset", Box::new(|| {
            let label = "5h Reset:".to_string();
            let value = if let Some(u) = usage {
                if let Some(resets) = u.five_hour_resets {
                    let rt = format_reset_time(resets);
                    if rt.is_empty() {
                        format!("{}N/A{}", SLATE600, RESET)
                    } else {
                        format!("{}{}{}", SLATE600, rt, RESET)
                    }
                } else {
                    format!("{}N/A{}", SLATE600, RESET)
                }
            } else {
                format!("{}N/A{}", SLATE600, RESET)
            };
            Column { label, value }
        })),
        ("7d Reset", Box::new(|| {
            let label = "7d Reset:".to_string();
            let value = if let Some(u) = usage {
                if let Some(resets) = u.seven_day_resets {
                    let rt = format_reset_time(resets);
                    if rt.is_empty() {
                        format!("{}N/A{}", SLATE600, RESET)
                    } else {
                        format!("{}{}{}", SLATE600, rt, RESET)
                    }
                } else {
                    format!("{}N/A{}", SLATE600, RESET)
                }
            } else {
                format!("{}N/A{}", SLATE600, RESET)
            };
            Column { label, value }
        })),
    ];

    let columns: Vec<Column> = defs.into_iter()
        .filter(|(name, _)| config.columns.contains(&name.to_string()))
        .map(|(_, build)| build())
        .collect();
```

- [ ] **Step 4: Run all tests to verify refactor is behavior-preserving**

Run: `cargo test`
Expected: ALL PASS (including the regression test from step 1)

- [ ] **Step 5: Commit**

```bash
git add src/render.rs
git commit -m "refactor: replace column if-blocks with declarative registry"
```

---

### Task 2: Update truncate() to append ellipsis

**Files:**
- Modify: `src/render.rs:28-30` (truncate function)
- Test: `src/render.rs` (existing test module)

- [ ] **Step 1: Update the existing truncate test to expect ellipsis**

In `src/render.rs`, replace the `test_truncate` test:

```rust
#[test]
fn test_truncate() {
    // No truncation needed
    assert_eq!(truncate("hello", 10), "hello");
    // Exact length — no truncation
    assert_eq!(truncate("hello", 5), "hello");
    // Truncation with ellipsis
    assert_eq!(truncate("hello world", 5), "hell…");
    // Unicode: 5 chars input, max 5 — no truncation
    assert_eq!(truncate("héllo", 5), "héllo");
    // Unicode: 11 chars input, max 5 — truncation
    assert_eq!(truncate("héllo wörld", 5), "héll…");
    // max=1 edge case
    assert_eq!(truncate("hello", 1), "…");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test test_truncate -- --nocapture`
Expected: FAIL — current truncate doesn't add ellipsis

- [ ] **Step 3: Update truncate function**

Replace `truncate` function in `src/render.rs`:

```rust
/// Unicode-safe truncation: takes up to `max` chars, appending `…` if truncated.
fn truncate(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test test_truncate -- --nocapture`
Expected: PASS

- [ ] **Step 5: Run all tests to verify no regressions**

Run: `cargo test`
Expected: ALL PASS. Note: `test_render_all_columns_regression` should still pass since no column values use agent descriptions that get truncated.

- [ ] **Step 6: Commit**

```bash
git add src/render.rs
git commit -m "feat: add ellipsis to truncated text in HUD display"
```

---

### Task 3: Add agent overflow indicator (+N more)

**Files:**
- Modify: `src/render.rs:370-405` (agent tree section)
- Test: `src/render.rs` (test module)

- [ ] **Step 1: Write test for overflow indicator**

Add to `src/render.rs` test module:

```rust
#[test]
fn test_agent_overflow_indicator() {
    let usage = UsageData {
        five_hour: 10.0,
        five_hour_resets: None,
        seven_day: 5.0,
        seven_day_resets: None,
    };
    let agents: Vec<crate::transcript::Agent> = (0..7).map(|i| {
        crate::transcript::Agent {
            id: format!("agent_{}", i),
            agent_type: "Task".to_string(),
            model: Some("claude-sonnet-4-5".to_string()),
            description: format!("Agent task {}", i),
            status: AgentStatus::Running,
            start_time: crate::time::now_ms(),
        }
    }).collect();
    let transcript = TranscriptData {
        session_start: None,
        agents,
        todos: vec![],
    };
    let stdin_data = StdinData {
        raw: crate::json::JsonValue::Null,
        context_pct: 30,
        model_id: "Opus 4.6".to_string(),
        version: Some("1.0.0".to_string()),
        transcript_path: None,
        total_cost_usd: 0.0,
        total_duration_ms: 0,
        total_lines_added: 0,
        total_lines_removed: 0,
        total_api_duration_ms: 0,
        current_dir: None,
        agent_name: None,
        input_tokens: 0,
        cache_creation_tokens: 0,
        cache_read_tokens: 0,
        total_output_tokens: 0,
    };
    let config = Config {
        columns: vec!["5h Usage".into()],
        layout: Layout::Vertical,
    };
    let out = render(Some(&usage), &transcript, &stdin_data, None, &config);
    let plain = crate::ansi::strip_ansi(&out).replace('\u{00A0}', " ");

    // Should show 4 agents + overflow line
    assert!(plain.contains("and 3 more"), "missing overflow indicator, got:\n{}", plain);
    // Should NOT show agent_4, agent_5, agent_6 descriptions
    assert!(!plain.contains("Agent task 4"), "agent 4 should be hidden");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test test_agent_overflow_indicator -- --nocapture`
Expected: FAIL — current code shows 5 agents with no overflow line

- [ ] **Step 3: Implement overflow logic in agent tree section**

Replace the agent tree section in `src/render.rs` (the `if running_count > 0` block after line 3) with:

```rust
    if running_count > 0 {
        let now = now_ms();
        let has_overflow = running_count > 5;
        let display_limit = if has_overflow { 4 } else { 5 };
        let display_agents: Vec<&&crate::transcript::Agent> = running_agents.iter()
            .take(display_limit)
            .collect();

        for (i, agent) in display_agents.iter().enumerate() {
            let is_last = !has_overflow && i == display_agents.len().saturating_sub(1);
            let prefix = if is_last { "└─" } else { "├─" };
            let type_trunc = truncate(&agent.agent_type, 14);
            let model_label = match agent.model.as_deref() {
                Some(m) if m.to_lowercase().contains("opus") => {
                    format!("{}Opus{}", MAGENTA, RESET)
                }
                Some(m) if m.to_lowercase().contains("haiku") => {
                    format!("{}Haiku{}", GREEN, RESET)
                }
                _ => format!("{}Sonnet{}", CYAN, RESET),
            };
            let elapsed_ms = now.saturating_sub(agent.start_time);
            let elapsed_str = format_duration(elapsed_ms);
            let elapsed_padded = format!("{:>5}", elapsed_str);
            let desc_trunc = truncate(&agent.description, 45);

            output.push('\n');
            output.push_str(&format!(
                "{}{}{}{} {}{}{} {} {}{}{} {}  {}{}{}",
                RESET, SLATE800, prefix, RESET,
                WHITE, type_trunc, RESET,
                model_label,
                SLATE600, elapsed_padded, RESET,
                "  ",
                SLATE600, desc_trunc, RESET
            ));
        }

        if has_overflow {
            let remaining = running_count - 4;
            output.push('\n');
            output.push_str(&format!(
                "{}{}└─{} {}… and {} more{}",
                RESET, SLATE800, RESET, SLATE600, remaining, RESET
            ));
        }
    }
```

- [ ] **Step 4: Run tests to verify**

Run: `cargo test`
Expected: ALL PASS

- [ ] **Step 5: Commit**

```bash
git add src/render.rs
git commit -m "feat: show '+N more' when running agents exceed 5"
```

---

### Task 4: Add sub-penny cost display

**Files:**
- Modify: `src/render.rs` (Cost column in the registry)
- Test: `src/render.rs` (test module)

- [ ] **Step 1: Write test for sub-penny cost**

Add to `src/render.rs` test module:

```rust
#[test]
fn test_sub_penny_cost() {
    let transcript = TranscriptData {
        session_start: None,
        agents: vec![],
        todos: vec![],
    };
    let stdin_data = StdinData {
        raw: crate::json::JsonValue::Null,
        context_pct: 30,
        model_id: "Opus 4.6".to_string(),
        version: None,
        transcript_path: None,
        total_cost_usd: 0.005,
        total_duration_ms: 0,
        total_lines_added: 0,
        total_lines_removed: 0,
        total_api_duration_ms: 0,
        current_dir: None,
        agent_name: None,
        input_tokens: 0,
        cache_creation_tokens: 0,
        cache_read_tokens: 0,
        total_output_tokens: 0,
    };
    let config = Config {
        columns: vec!["Cost".into()],
        layout: Layout::Horizontal,
    };
    let out = render(None, &transcript, &stdin_data, None, &config);
    let plain = crate::ansi::strip_ansi(&out).replace('\u{00A0}', " ");

    assert!(plain.contains("<$0.01"), "sub-penny cost should show <$0.01, got: {}", plain);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test test_sub_penny_cost -- --nocapture`
Expected: FAIL — current code shows `$0.01` or `$0.00`

- [ ] **Step 3: Update Cost column in the registry**

In `src/render.rs`, find the `("Cost", Box::new(||` closure and replace its body with:

```rust
            let label = "Cost:".to_string();
            let cost = stdin.total_cost_usd;
            let value = if cost > 0.0 && cost < 0.01 {
                format!("{}<$0.01{}", GREEN, RESET)
            } else {
                let color = if cost >= 1.0 {
                    RED
                } else if cost >= 0.25 {
                    YELLOW
                } else {
                    GREEN
                };
                format!("{}${:.2}{}", color, cost, RESET)
            };
            Column { label, value }
```

- [ ] **Step 4: Run tests to verify**

Run: `cargo test`
Expected: ALL PASS

- [ ] **Step 5: Commit**

```bash
git add src/render.rs
git commit -m "feat: display <\$0.01 for sub-penny costs"
```

---

### Task 5: Add truncation for Directory and Model columns

**Files:**
- Modify: `src/render.rs` (Directory and Model columns in the registry)
- Test: `src/render.rs` (test module)

- [ ] **Step 1: Write test for long directory and model truncation**

Add to `src/render.rs` test module:

```rust
#[test]
fn test_long_directory_truncated() {
    let transcript = TranscriptData {
        session_start: None,
        agents: vec![],
        todos: vec![],
    };
    let stdin_data = StdinData {
        raw: crate::json::JsonValue::Null,
        context_pct: 30,
        model_id: "Some Very Long Model Name Here".to_string(),
        version: None,
        transcript_path: None,
        total_cost_usd: 0.0,
        total_duration_ms: 0,
        total_lines_added: 0,
        total_lines_removed: 0,
        total_api_duration_ms: 0,
        current_dir: Some("/home/user/very/long/nested/directory/path/here".to_string()),
        agent_name: None,
        input_tokens: 0,
        cache_creation_tokens: 0,
        cache_read_tokens: 0,
        total_output_tokens: 0,
    };
    let config = Config {
        columns: vec!["Directory".into(), "Model".into()],
        layout: Layout::Horizontal,
    };
    let out = render(None, &transcript, &stdin_data, None, &config);
    let plain = crate::ansi::strip_ansi(&out).replace('\u{00A0}', " ");

    // Directory should be truncated to 20 chars with ellipsis
    assert!(plain.contains("…"), "should have ellipsis for truncation");
    assert!(!plain.contains("/home/user/very/long/nested/directory/path/here"),
        "full directory should not appear");
    // Model should be truncated to 15 chars with ellipsis
    assert!(!plain.contains("Some Very Long Model Name Here"),
        "full model name should not appear");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test test_long_directory_truncated -- --nocapture`
Expected: FAIL — current code doesn't truncate directory or model

- [ ] **Step 3: Update Directory column in registry**

In `src/render.rs`, find the `("Directory", Box::new(||` closure and update to apply truncation before ANSI wrapping:

```rust
        ("Directory", Box::new(|| {
            let label = "Directory:".to_string();
            let value = match &stdin.current_dir {
                Some(d) => format!("{}{}{}", SLATE600, truncate(d, 20), RESET),
                None => format!("{}N/A{}", SLATE600, RESET),
            };
            Column { label, value }
        })),
```

- [ ] **Step 4: Update Model column in registry**

In `src/render.rs`, find the `("Model", Box::new(||` closure and update:

```rust
        ("Model", Box::new(|| {
            let label = "Model:".to_string();
            let value = format!("{}{}{}", SLATE600, truncate(&stdin.model_id, 15), RESET);
            Column { label, value }
        })),
```

- [ ] **Step 5: Run tests to verify**

Run: `cargo test`
Expected: ALL PASS. Note: `test_render_all_columns_regression` checks for "Opus 4.6" (8 chars, under 15 limit) so it still passes. `test_render_contains_labels` also checks "Opus 4.6" — still passes.

- [ ] **Step 6: Commit**

```bash
git add src/render.rs
git commit -m "feat: truncate long directory and model names with ellipsis"
```

---

### Task 6: Add HUD_DEBUG error observability to main.rs

**Files:**
- Modify: `src/main.rs:42-45` (replace DEBUG_USAGE with HUD_DEBUG)
- Modify: `src/main.rs:71-80` (thread join error handling)

- [ ] **Step 1: Replace DEBUG_USAGE with HUD_DEBUG in main.rs**

In `src/main.rs`, replace:

```rust
    let debug_enabled = std::env::var("DEBUG_USAGE")
        .ok()
        .map(|v| v == "1")
        .unwrap_or(false);
```

with:

```rust
    let debug_enabled = std::env::var("HUD_DEBUG").as_deref() == Ok("1");
```

- [ ] **Step 2: Add debug logging for thread panics**

In `src/main.rs`, replace the thread join section:

```rust
    let usage = usage_handle.join().unwrap_or(None);
    let transcript_data =
        transcript_handle
            .join()
            .unwrap_or_else(|_| transcript::TranscriptData {
                session_start: None,
                agents: vec![],
                todos: vec![],
            });
    let latest_version = version_handle.join().unwrap_or(None);
```

with:

```rust
    let usage = match usage_handle.join() {
        Ok(v) => v,
        Err(e) => {
            if debug_enabled {
                let msg = if let Some(s) = e.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = e.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown panic".to_string()
                };
                eprintln!("[hud] usage thread panicked: {}", msg);
            }
            None
        }
    };
    let transcript_data = match transcript_handle.join() {
        Ok(v) => v,
        Err(e) => {
            if debug_enabled {
                let msg = if let Some(s) = e.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = e.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown panic".to_string()
                };
                eprintln!("[hud] transcript thread panicked: {}", msg);
            }
            transcript::TranscriptData {
                session_start: None,
                agents: vec![],
                todos: vec![],
            }
        }
    };
    let latest_version = match version_handle.join() {
        Ok(v) => v,
        Err(e) => {
            if debug_enabled {
                let msg = if let Some(s) = e.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = e.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown panic".to_string()
                };
                eprintln!("[hud] version thread panicked: {}", msg);
            }
            None
        }
    };
```

- [ ] **Step 3: Build to verify compilation**

Run: `cargo build`
Expected: Compiles successfully

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: add HUD_DEBUG=1 mode with thread panic logging"
```

---

### Task 7: Add HUD_DEBUG to api.rs (replace file-based logging)

**Files:**
- Modify: `src/api.rs:43-58` (debug_log_api function)
- Modify: `src/api.rs:65` (get_usage signature — no change needed, already takes `debug_enabled: bool`)

- [ ] **Step 1: Replace file-based debug logging with stderr**

In `src/api.rs`, replace the `debug_log_api` function:

```rust
fn debug_log_api(debug_enabled: bool, msg: &str) {
    if !debug_enabled {
        return;
    }
    eprintln!("[hud] api: {}", msg);
}
```

- [ ] **Step 2: No other changes needed in api.rs**

The existing `run_curl`, `get_usage`, and all `debug_log_api` call sites already log specific failure info (status codes at lines 118 and 131, token refresh failures at line 313, etc.). Step 1's replacement of `debug_log_api` to use stderr is the only change needed — all existing call sites now automatically go to stderr instead of the file.

- [ ] **Step 3: Build to verify compilation**

Run: `cargo build`
Expected: Compiles successfully

- [ ] **Step 4: Run existing tests**

Run: `cargo test`
Expected: ALL PASS

- [ ] **Step 5: Commit**

```bash
git add src/api.rs
git commit -m "refactor: replace file-based debug logging with stderr in api.rs"
```

---

### Task 8: Add HUD_DEBUG to config.rs and stdin.rs

**Files:**
- Modify: `src/config.rs:141` (read_config function signature)
- Modify: `src/stdin.rs:54` (read_stdin function)
- Modify: `src/main.rs` (pass debug_enabled to read_config and read_stdin)

- [ ] **Step 1: Add debug parameter to config.rs**

In `src/config.rs`, update `read_config` to accept and use a debug parameter:

```rust
pub fn read_config(debug_enabled: bool) -> Config {
```

Add debug logging at the parse error point (line 167):

```rust
    let parsed = match json::parse(&stripped) {
        Ok(v) => v,
        Err(e) => {
            if debug_enabled {
                eprintln!("[hud] config: parse error: {}", e);
            }
            return default_config;
        }
    };
```

- [ ] **Step 2: Add debug logging to stdin.rs**

In `src/stdin.rs`, update `read_stdin` to accept a debug parameter:

```rust
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
```

- [ ] **Step 3: Update main.rs to pass debug_enabled to both functions**

In `src/main.rs`, the `debug_enabled` variable is computed on line 42 (updated in Task 6). Move it before `read_stdin` and `read_config` calls:

Reorder `run()` so `debug_enabled` is computed first, then passed:

```rust
fn run() {
    let debug_enabled = std::env::var("HUD_DEBUG").as_deref() == Ok("1");

    let stdin_data = match stdin::read_stdin(debug_enabled) {
        Some(d) => d,
        None => {
            println!("{}[HUD] waiting for data...{}", ansi::DIM, ansi::RESET);
            return;
        }
    };

    let config = config::read_config(debug_enabled);

    let no_network = std::env::var("HUD_NO_NETWORK")
        .ok()
        .map(|v| v == "1")
        .unwrap_or(false);
    // ... rest unchanged
```

- [ ] **Step 4: Build and test**

Run: `cargo build && cargo test`
Expected: Compiles and ALL PASS

- [ ] **Step 5: Commit**

```bash
git add src/config.rs src/stdin.rs src/main.rs
git commit -m "feat: add HUD_DEBUG logging to config.rs and stdin.rs"
```

---

### Task 9: Add integration tests

**Files:**
- Create: `tests/integration.rs`

- [ ] **Step 1: Create integration test file**

Create `tests/integration.rs`:

```rust
//! Integration tests: full stdin → render → output pipeline.

use hud_rs::ansi::strip_ansi;
use hud_rs::api::UsageData;
use hud_rs::config::{Config, Layout};
use hud_rs::render::render;
use hud_rs::stdin::StdinData;
use hud_rs::transcript::{TranscriptData, AgentStatus, Agent};

fn make_stdin(overrides: impl FnOnce(&mut StdinData)) -> StdinData {
    let mut data = StdinData {
        raw: hud_rs::json::JsonValue::Null,
        context_pct: 50,
        model_id: "Opus 4.6".to_string(),
        version: Some("1.2.3".to_string()),
        transcript_path: None,
        total_cost_usd: 0.42,
        total_duration_ms: 300000,
        total_lines_added: 100,
        total_lines_removed: 20,
        total_api_duration_ms: 8000,
        current_dir: Some("/home/user/project".to_string()),
        agent_name: None,
        input_tokens: 10000,
        cache_creation_tokens: 2000,
        cache_read_tokens: 5000,
        total_output_tokens: 3000,
    };
    overrides(&mut data);
    data
}

fn make_usage() -> UsageData {
    UsageData {
        five_hour: 35.0,
        five_hour_resets: None,
        seven_day: 12.0,
        seven_day_resets: None,
    }
}

fn empty_transcript() -> TranscriptData {
    TranscriptData {
        session_start: None,
        agents: vec![],
        todos: vec![],
    }
}

fn plain(output: &str) -> String {
    strip_ansi(output).replace('\u{00A0}', " ")
}

#[test]
fn test_full_pipeline_all_columns() {
    let usage = make_usage();
    let transcript = empty_transcript();
    let stdin = make_stdin(|_| {});
    let config = Config {
        columns: vec![
            "5h Usage".into(), "7d Usage".into(), "Context".into(),
            "Model".into(), "Version".into(), "Session".into(),
            "Changes".into(), "Directory".into(), "Cost".into(),
            "Tokens".into(), "Output Tokens".into(), "Cache".into(),
            "API Time".into(),
        ],
        layout: Layout::Vertical,
    };
    let out = render(Some(&usage), &transcript, &stdin, Some("1.2.3"), &config);
    let p = plain(&out);

    assert!(p.contains("5h Usage:"));
    assert!(p.contains("35%"));
    assert!(p.contains("Context:"));
    assert!(p.contains("50%"));
    assert!(p.contains("$0.42"));
    assert!(p.contains("+100"));
    assert!(p.contains("-20"));
}

#[test]
fn test_full_pipeline_no_usage() {
    let transcript = empty_transcript();
    let stdin = make_stdin(|d| { d.version = None; });
    let config = Config {
        columns: vec!["5h Usage".into(), "Version".into()],
        layout: Layout::Vertical,
    };
    let out = render(None, &transcript, &stdin, None, &config);
    let p = plain(&out);

    assert!(p.contains("N/A"));
}

#[test]
fn test_vertical_layout_has_two_rows() {
    let usage = make_usage();
    let transcript = empty_transcript();
    let stdin = make_stdin(|_| {});
    let config = Config {
        columns: vec!["5h Usage".into(), "Context".into()],
        layout: Layout::Vertical,
    };
    let out = render(Some(&usage), &transcript, &stdin, None, &config);
    let p = plain(&out);

    // Vertical layout: labels on one line, values on the next
    let lines: Vec<&str> = p.lines().filter(|l| !l.trim().is_empty()).collect();
    assert!(lines.len() >= 2, "vertical layout should have at least 2 lines, got {}", lines.len());
    assert!(lines[0].contains("5h Usage:"));
    assert!(lines[1].contains("35%"));
}

#[test]
fn test_horizontal_layout_single_row() {
    let usage = make_usage();
    let transcript = empty_transcript();
    let stdin = make_stdin(|_| {});
    let config = Config {
        columns: vec!["5h Usage".into(), "Context".into()],
        layout: Layout::Horizontal,
    };
    let out = render(Some(&usage), &transcript, &stdin, None, &config);
    let p = plain(&out);

    let content_lines: Vec<&str> = p.lines().filter(|l| !l.trim().is_empty()).collect();
    // Horizontal: one line with both label+value pairs
    assert!(content_lines[0].contains("5h Usage:"));
    assert!(content_lines[0].contains("Context:"));
}
```

- [ ] **Step 2: Create lib.rs and update main.rs for integration test access**

Integration tests in `tests/` can only access `pub` items from the crate root. The current `src/main.rs` declares modules privately (`mod json;` etc.). We need to:
1. Create `src/lib.rs` with `pub mod` declarations (the library crate)
2. Remove all `mod` declarations from `src/main.rs` and use `use hud_rs::*;` instead (the binary crate re-uses the library)

**Important:** Do NOT add `pub mod` to `main.rs` — that would compile modules twice and cause duplicate symbol errors. Only `lib.rs` should have the `mod` declarations.

Create `src/lib.rs`:

```rust
pub mod json;
pub mod ansi;
pub mod time;
pub mod cache;
pub mod config;
pub mod stdin;
pub mod api;
pub mod version;
pub mod transcript;
pub mod render;
```

Replace the entirety of `src/main.rs` with the following (note: all `mod` declarations are removed, replaced by `use hud_rs::*;`):

```rust
use hud_rs::*;

fn main() {
    let result = std::panic::catch_unwind(run);
    match result {
        Ok(()) => {}
        Err(e) => {
            let msg = if let Some(s) = e.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = e.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown error".to_string()
            };
            println!("[HUD] error: {}", msg);
        }
    }
}

fn run() {
    let debug_enabled = std::env::var("HUD_DEBUG").as_deref() == Ok("1");

    let stdin_data = match stdin::read_stdin(debug_enabled) {
        Some(d) => d,
        None => {
            println!("{}[HUD] waiting for data...{}", ansi::DIM, ansi::RESET);
            return;
        }
    };

    let config = config::read_config(debug_enabled);

    let no_network = std::env::var("HUD_NO_NETWORK")
        .ok()
        .map(|v| v == "1")
        .unwrap_or(false);

    let transcript_path = stdin_data.transcript_path.clone();

    let usage_handle = std::thread::spawn(move || {
        if no_network { None } else { api::get_usage(debug_enabled) }
    });

    let transcript_handle = std::thread::spawn(move || match transcript_path {
        Some(ref p) => transcript::parse_transcript(p),
        None => transcript::TranscriptData {
            session_start: None,
            agents: vec![],
            todos: vec![],
        },
    });

    let version_handle = std::thread::spawn(move || {
        if no_network { None } else { version::get_latest_version() }
    });

    let usage = match usage_handle.join() {
        Ok(v) => v,
        Err(e) => {
            if debug_enabled {
                let msg = if let Some(s) = e.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = e.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown panic".to_string()
                };
                eprintln!("[hud] usage thread panicked: {}", msg);
            }
            None
        }
    };
    let transcript_data = match transcript_handle.join() {
        Ok(v) => v,
        Err(e) => {
            if debug_enabled {
                let msg = if let Some(s) = e.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = e.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown panic".to_string()
                };
                eprintln!("[hud] transcript thread panicked: {}", msg);
            }
            transcript::TranscriptData {
                session_start: None,
                agents: vec![],
                todos: vec![],
            }
        }
    };
    let latest_version = match version_handle.join() {
        Ok(v) => v,
        Err(e) => {
            if debug_enabled {
                let msg = if let Some(s) = e.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = e.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown panic".to_string()
                };
                eprintln!("[hud] version thread panicked: {}", msg);
            }
            None
        }
    };

    let output = render::render(
        usage.as_ref(),
        &transcript_data,
        &stdin_data,
        latest_version.as_deref(),
        &config,
    );

    print!("{}", output);
}
```

- [ ] **Step 3: Run integration tests**

Run: `cargo test --test integration`
Expected: ALL PASS

- [ ] **Step 4: Run full test suite**

Run: `cargo test`
Expected: ALL PASS

- [ ] **Step 5: Commit**

```bash
git add src/lib.rs src/main.rs tests/integration.rs
git commit -m "test: add integration tests and lib.rs for test access"
```

---

### Task 10: Add edge-case unit tests

**Files:**
- Modify: `src/render.rs` (test module)
- Modify: `src/transcript.rs` (test module)
- Modify: `src/stdin.rs` (test module)
- Modify: `src/json.rs` (test module)

- [ ] **Step 1: Add render.rs edge-case tests**

Add to `src/render.rs` test module:

```rust
#[test]
fn test_empty_columns_config() {
    let transcript = TranscriptData {
        session_start: None,
        agents: vec![],
        todos: vec![],
    };
    let stdin_data = StdinData {
        raw: crate::json::JsonValue::Null,
        context_pct: 50,
        model_id: "Opus 4.6".to_string(),
        version: None,
        transcript_path: None,
        total_cost_usd: 0.0,
        total_duration_ms: 0,
        total_lines_added: 0,
        total_lines_removed: 0,
        total_api_duration_ms: 0,
        current_dir: None,
        agent_name: None,
        input_tokens: 0,
        cache_creation_tokens: 0,
        cache_read_tokens: 0,
        total_output_tokens: 0,
    };
    let config = Config {
        columns: vec![],
        layout: Layout::Vertical,
    };
    let out = render(None, &transcript, &stdin_data, None, &config);
    let plain = crate::ansi::strip_ansi(&out).replace('\u{00A0}', " ");
    // Should not contain any column labels
    assert!(!plain.contains("Usage:"));
    assert!(!plain.contains("Context:"));
}
```

- [ ] **Step 2: Add transcript.rs edge-case tests**

Add to `src/transcript.rs` test module:

```rust
#[test]
fn test_malformed_jsonl_skipped() {
    let mut state = ParseState::new();
    // Valid line
    process_line(r#"{"timestamp":"2025-01-01T00:00:00Z","type":"assistant","content":[]}"#, &mut state);
    // Malformed line — should be skipped
    process_line("this is not json {{{", &mut state);
    // Empty line — should be skipped
    process_line("", &mut state);
    // Session start should still be extracted from the valid line
    assert!(state.session_start.is_some());
}

#[test]
fn test_stale_agent_marked_completed() {
    let mut state = ParseState::new();
    // Manually add an agent with a start_time more than 30 minutes ago
    let old_time = now_ms().saturating_sub(STALE_AGENT_MS + 1000);
    state.agent_map.push(AgentEntry {
        tool_use_id: "tu_stale".to_string(),
        agent_id: None,
        agent_type: "Task".to_string(),
        model: None,
        description: "old task".to_string(),
        status: AgentStatus::Running,
        start_time: old_time,
    });

    let result = build_result(state);
    // The stale agent should be marked completed
    assert_eq!(result.agents.len(), 1);
    assert_eq!(result.agents[0].status, AgentStatus::Completed);
}
```

- [ ] **Step 3: Add stdin.rs edge-case tests**

Add to `src/stdin.rs` test module:

```rust
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
```

- [ ] **Step 4: Add json.rs edge-case tests**

Add to `src/json.rs` test module:

```rust
#[test]
fn test_deeply_nested() {
    // 15 levels of nesting
    let mut json = String::new();
    for _ in 0..15 {
        json.push_str(r#"{"a":"#);
    }
    json.push_str("1");
    for _ in 0..15 {
        json.push('}');
    }
    let val = parse(&json).unwrap();
    // Navigate to the deepest value
    let mut keys = vec!["a"; 14];
    let deepest = val.get_path(&keys).unwrap();
    assert_eq!(deepest.get("a"), Some(&JsonValue::Number(1.0)));
}

#[test]
fn test_very_long_string() {
    let long = "x".repeat(10_000);
    let json = format!(r#""{}""#, long);
    let val = parse(&json).unwrap();
    assert_eq!(val.as_str().unwrap().len(), 10_000);
}
```

- [ ] **Step 5: Make `build_result` and `AgentEntry` visible to tests**

In `src/transcript.rs`, ensure `build_result` and `AgentEntry` are accessible from the test module. They already are since the tests are in the same file. Verify the `test_stale_agent_marked_completed` test can construct `AgentEntry` — it's defined as a private struct in the same module, so tests within `mod tests` can access it.

- [ ] **Step 6: Run all tests**

Run: `cargo test`
Expected: ALL PASS

- [ ] **Step 7: Commit**

```bash
git add src/render.rs src/transcript.rs src/stdin.rs src/json.rs
git commit -m "test: add edge-case unit tests for render, transcript, stdin, json"
```

---

### Task 11: Rewrite JSON parser to byte-based

**Files:**
- Modify: `src/json.rs:150-417` (Parser struct and methods)

- [ ] **Step 1: Rename existing parser for reference testing**

In `src/json.rs`, rename the existing `Parser` struct and its `parse` entry point to keep as reference:

Add before the `#[cfg(test)]` block at the end. Copy the *entire* current `Parser` struct definition (lines 150-153), the entire `impl Parser` block (lines 155-401), and the `parse` public function (lines 407-418) into a `#[cfg(test)] mod char_parser` module. The module wraps the old char-based parser so it can be called from tests for differential comparison.

Specifically, copy these items into the module:
- `struct Parser { input: Vec<char>, pos: usize }` (the original char-based struct)
- All methods: `new`, `skip_ws`, `peek`, `advance`, `expect`, `parse_value`, `parse_null`, `parse_bool`, `consume_literal`, `parse_number`, `parse_string`, `parse_array`, `parse_object`
- A `pub fn parse(input: &str)` entry point identical to the current public one

```rust
#[cfg(test)]
mod char_parser {
    use super::JsonValue;

    // Paste the entire original Parser struct and impl here (lines 150-401)
    // Then add the parse entry point:

    pub fn parse(input: &str) -> Result<JsonValue, String> {
        let mut p = Parser::new(input);
        let value = p.parse_value()?;
        p.skip_ws();
        if p.pos < p.input.len() {
            return Err(format!("trailing data after JSON value at pos {}", p.pos));
        }
        Ok(value)
    }
}
```

- [ ] **Step 2: Rewrite Parser to use `&[u8]`**

Replace the `Parser` struct and all methods (lines 150-401) with:

```rust
struct Parser<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Parser {
            input: input.as_bytes(),
            pos: 0,
        }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.input.len() {
            match self.input[self.pos] {
                b' ' | b'\t' | b'\n' | b'\r' => self.pos += 1,
                _ => break,
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<u8> {
        let b = self.input.get(self.pos).copied();
        if b.is_some() {
            self.pos += 1;
        }
        b
    }

    fn expect(&mut self, ch: u8) -> Result<(), String> {
        match self.advance() {
            Some(b) if b == ch => Ok(()),
            Some(b) => Err(format!(
                "expected '{}' but got '{}' at pos {}",
                ch as char, b as char, self.pos - 1
            )),
            None => Err(format!("expected '{}' but reached end of input", ch as char)),
        }
    }

    fn parse_value(&mut self) -> Result<JsonValue, String> {
        self.skip_ws();
        match self.peek() {
            Some(b'"') => self.parse_string().map(JsonValue::Str),
            Some(b't') | Some(b'f') => self.parse_bool(),
            Some(b'n') => self.parse_null(),
            Some(b'[') => self.parse_array(),
            Some(b'{') => self.parse_object(),
            Some(b) if b == b'-' || b.is_ascii_digit() => self.parse_number(),
            Some(b) => Err(format!("unexpected character '{}' at pos {}", b as char, self.pos)),
            None => Err("unexpected end of input".to_string()),
        }
    }

    fn parse_null(&mut self) -> Result<JsonValue, String> {
        self.consume_literal(b"null")?;
        Ok(JsonValue::Null)
    }

    fn parse_bool(&mut self) -> Result<JsonValue, String> {
        if self.peek() == Some(b't') {
            self.consume_literal(b"true")?;
            Ok(JsonValue::Bool(true))
        } else {
            self.consume_literal(b"false")?;
            Ok(JsonValue::Bool(false))
        }
    }

    fn consume_literal(&mut self, lit: &[u8]) -> Result<(), String> {
        for &ch in lit {
            match self.advance() {
                Some(b) if b == ch => {}
                Some(b) => {
                    return Err(format!(
                        "expected '{}' while parsing literal, got '{}' at pos {}",
                        ch as char, b as char, self.pos - 1
                    ))
                }
                None => {
                    return Err("unexpected end of input while parsing literal".to_string())
                }
            }
        }
        Ok(())
    }

    fn parse_number(&mut self) -> Result<JsonValue, String> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        if self.peek() == Some(b'0') {
            self.pos += 1;
        } else if matches!(self.peek(), Some(b'1'..=b'9')) {
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        } else {
            return Err(format!("invalid number at pos {}", self.pos));
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(format!("expected digit after '.' at pos {}", self.pos));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e') | Some(b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                self.pos += 1;
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(format!("expected digit in exponent at pos {}", self.pos));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        // Safe: number chars are always ASCII
        let raw = std::str::from_utf8(&self.input[start..self.pos])
            .map_err(|e| format!("invalid utf8 in number: {}", e))?;
        raw.parse::<f64>()
            .map(JsonValue::Number)
            .map_err(|e| format!("invalid number '{}': {}", raw, e))
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut s = String::new();
        loop {
            match self.advance() {
                None => return Err("unterminated string".to_string()),
                Some(b'"') => break,
                Some(b'\\') => {
                    match self.advance() {
                        Some(b'"') => s.push('"'),
                        Some(b'\\') => s.push('\\'),
                        Some(b'/') => s.push('/'),
                        Some(b'n') => s.push('\n'),
                        Some(b't') => s.push('\t'),
                        Some(b'r') => s.push('\r'),
                        Some(b'b') => s.push('\x08'),
                        Some(b'f') => s.push('\x0C'),
                        Some(b'u') => {
                            let mut hex = String::with_capacity(4);
                            for _ in 0..4 {
                                match self.advance() {
                                    Some(h) if (h as char).is_ascii_hexdigit() => {
                                        hex.push(h as char);
                                    }
                                    Some(c) => {
                                        return Err(format!(
                                            "invalid hex digit '{}' in \\uXXXX escape",
                                            c as char
                                        ))
                                    }
                                    None => {
                                        return Err(
                                            "unexpected end of input in \\uXXXX escape".to_string()
                                        )
                                    }
                                }
                            }
                            let code = u32::from_str_radix(&hex, 16)
                                .map_err(|e| format!("invalid unicode escape \\u{}: {}", hex, e))?;
                            let ch = char::from_u32(code).ok_or_else(|| {
                                format!("invalid unicode code point U+{:04X}", code)
                            })?;
                            s.push(ch);
                        }
                        Some(c) => return Err(format!("invalid escape sequence '\\{}'", c as char)),
                        None => return Err("unexpected end of input after '\\'".to_string()),
                    }
                }
                Some(b) => {
                    // UTF-8 safe: continuation bytes (0x80-0xBF) never equal
                    // structural chars (", \). Decode multi-byte sequences.
                    let start = self.pos - 1;
                    let byte_len = if b < 0x80 {
                        1
                    } else if b < 0xE0 {
                        2
                    } else if b < 0xF0 {
                        3
                    } else {
                        4
                    };
                    // Advance past remaining continuation bytes
                    for _ in 1..byte_len {
                        self.pos += 1;
                    }
                    let slice = &self.input[start..self.pos];
                    match std::str::from_utf8(slice) {
                        Ok(ch_str) => s.push_str(ch_str),
                        Err(_) => s.push(char::REPLACEMENT_CHARACTER),
                    }
                }
            }
        }
        Ok(s)
    }

    fn parse_array(&mut self) -> Result<JsonValue, String> {
        self.expect(b'[')?;
        self.skip_ws();
        let mut items = Vec::new();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(JsonValue::Array(items));
        }
        loop {
            items.push(self.parse_value()?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => { self.pos += 1; }
                Some(b']') => { self.pos += 1; break; }
                Some(b) => return Err(format!("expected ',' or ']' in array, got '{}'", b as char)),
                None => return Err("unexpected end of input in array".to_string()),
            }
        }
        Ok(JsonValue::Array(items))
    }

    fn parse_object(&mut self) -> Result<JsonValue, String> {
        self.expect(b'{')?;
        self.skip_ws();
        let mut pairs = Vec::new();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(JsonValue::Object(pairs));
        }
        loop {
            self.skip_ws();
            if self.peek() != Some(b'"') {
                return Err(format!(
                    "expected string key in object, got {:?} at pos {}",
                    self.peek().map(|b| b as char),
                    self.pos
                ));
            }
            let key = self.parse_string()?;
            self.skip_ws();
            self.expect(b':')?;
            let value = self.parse_value()?;
            pairs.push((key, value));
            self.skip_ws();
            match self.peek() {
                Some(b',') => { self.pos += 1; }
                Some(b'}') => { self.pos += 1; break; }
                Some(b) => return Err(format!("expected ',' or '}}' in object, got '{}'", b as char)),
                None => return Err("unexpected end of input in object".to_string()),
            }
        }
        Ok(JsonValue::Object(pairs))
    }
}
```

Also update the public `parse` function:

```rust
pub fn parse(input: &str) -> Result<JsonValue, String> {
    let mut p = Parser::new(input);
    let value = p.parse_value()?;
    p.skip_ws();
    if p.pos < p.input.len() {
        return Err(format!(
            "trailing data after JSON value at pos {}",
            p.pos
        ));
    }
    Ok(value)
}
```

- [ ] **Step 3: Add differential test**

Add to `src/json.rs` test module:

```rust
#[test]
fn test_byte_parser_matches_char_parser() {
    let cases = vec![
        "null",
        "true",
        "false",
        "42",
        "-3.14",
        "1e10",
        r#""hello""#,
        r#""escaped \"quotes\"""#,
        r#""unicode \u00e9""#,
        r#""multi-byte: café ñ 日本語""#,
        "[]",
        "{}",
        "[1, 2, 3]",
        r#"{"a": 1, "b": true, "c": null}"#,
        r#"{"nested": {"deep": {"value": [1, "two", false]}}}"#,
        r#"[{"id": 1}, {"id": 2}]"#,
        r#""line1\nline2\ttab""#,
        r#""back\\slash""#,
        "  { \"spaced\" :  42  }  ",
        // Large string
        &format!(r#""{}""#, "abcdef".repeat(1000)),
    ];

    for input in &cases {
        let byte_result = parse(input);
        let char_result = char_parser::parse(input);
        assert_eq!(
            byte_result, char_result,
            "parsers disagree on input: {}",
            if input.len() > 100 { &input[..100] } else { input }
        );
    }
}
```

- [ ] **Step 4: Run all tests**

Run: `cargo test`
Expected: ALL PASS — all 10 existing tests plus differential test

- [ ] **Step 5: Commit**

```bash
git add src/json.rs
git commit -m "perf: rewrite JSON parser to byte-based, eliminating Vec<char> allocation"
```

---

### Task 12: Final verification

- [ ] **Step 1: Run full test suite**

Run: `cargo test`
Expected: ALL PASS

- [ ] **Step 2: Build release binary**

Run: `cargo build --release`
Expected: Compiles with no errors

- [ ] **Step 3: Run benchmark if available**

Run: `bash benchmark.sh` (if it exists)
Expected: Binary runs and produces output

- [ ] **Step 4: Verify no compiler warnings**

Run: `cargo build 2>&1`
Expected: No warnings (or only pre-existing ones)

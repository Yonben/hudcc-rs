// ANSI-colored status line renderer for the Claude Code HUD.

use crate::ansi::{
    pad_ansi, color_for_percent, strip_ansi,
    RESET, GREEN, YELLOW, RED, CYAN, MAGENTA, WHITE,
    SLATE600, SLATE800, SLATE800_BOLD,
};
use crate::time::{format_duration, format_reset_time, format_tokens, now_ms};
use crate::api::UsageData;
use crate::config::{Config, Layout};
use crate::stdin::StdinData;
use crate::transcript::{TranscriptData, AgentStatus};

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

struct Column {
    label: String,
    value: String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Unicode-safe truncation: takes up to `max` chars.
fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

// ---------------------------------------------------------------------------
// Public render function
// ---------------------------------------------------------------------------

pub fn render(
    usage: Option<&UsageData>,
    transcript: &TranscriptData,
    stdin: &StdinData,
    latest_version: Option<&str>,
    config: &Config,
) -> String {
    // -----------------------------------------------------------------------
    // Build columns
    // -----------------------------------------------------------------------
    let mut columns: Vec<Column> = Vec::new();

    // "5h Usage"
    if config.columns.contains(&"5h Usage".to_string()) {
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
        columns.push(Column { label, value });
    }

    // "7d Usage"
    if config.columns.contains(&"7d Usage".to_string()) {
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
        columns.push(Column { label, value });
    }

    // "Context"
    if config.columns.contains(&"Context".to_string()) {
        let label = "Context:".to_string();
        let pct = stdin.context_pct as f64;
        let color = color_for_percent(pct, 70.0, 85.0);
        // Spec: color + "{pct}% " + RESET + SLATE600 + "Used" + RESET
        let value = format!(
            "{}{:.0}% {}{}Used{}",
            color, pct, RESET, SLATE600, RESET
        );
        columns.push(Column { label, value });
    }

    // "Model"
    if config.columns.contains(&"Model".to_string()) {
        let label = "Model:".to_string();
        let value = format!("{}{}{}", SLATE600, stdin.model_id, RESET);
        columns.push(Column { label, value });
    }

    // "Version"
    if config.columns.contains(&"Version".to_string()) {
        let label = "Version:".to_string();
        let value = if let Some(ref ver) = stdin.version {
            // Green dot if current==latest or no latest; yellow dot if update available
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
        columns.push(Column { label, value });
    }

    // "Session"
    if config.columns.contains(&"Session".to_string()) {
        let label = "Session:".to_string();
        let value = if stdin.total_duration_ms > 0 {
            format!("{}{}{}", SLATE600, format_duration(stdin.total_duration_ms), RESET)
        } else {
            format!("{}N/A{}", SLATE600, RESET)
        };
        columns.push(Column { label, value });
    }

    // "Changes"
    if config.columns.contains(&"Changes".to_string()) {
        let label = "Changes:".to_string();
        let added = stdin.total_lines_added;
        let removed = stdin.total_lines_removed;
        // Spec: GREEN + "+{added}" + RESET + SLATE600 + "/" + RESET + RED + "-{removed}"
        let value = if added == 0 && removed == 0 {
            format!("{}+0/-0{}", SLATE600, RESET)
        } else {
            format!(
                "{}+{}{}{}{}{}{}{}",
                GREEN, added, RESET, SLATE600, "/", RESET, RED, format!("-{}", removed)
            )
        };
        columns.push(Column { label, value });
    }

    // "Directory"
    if config.columns.contains(&"Directory".to_string()) {
        let label = "Directory:".to_string();
        let value = match &stdin.current_dir {
            Some(d) => format!("{}{}{}", SLATE600, d, RESET),
            None => format!("{}N/A{}", SLATE600, RESET),
        };
        columns.push(Column { label, value });
    }

    // "Cost"
    if config.columns.contains(&"Cost".to_string()) {
        let label = "Cost:".to_string();
        let cost = stdin.total_cost_usd;
        let color = if cost >= 1.0 {
            RED
        } else if cost >= 0.25 {
            YELLOW
        } else {
            GREEN
        };
        // Spec says: "${:.2}" format
        let value = format!("{}${:.2}{}", color, cost, RESET);
        columns.push(Column { label, value });
    }

    // "Tokens"
    if config.columns.contains(&"Tokens".to_string()) {
        let label = "Tokens:".to_string();
        let total = stdin.input_tokens + stdin.cache_creation_tokens + stdin.cache_read_tokens;
        let value = format!("{}{}{}", SLATE600, format_tokens(total), RESET);
        columns.push(Column { label, value });
    }

    // "Output Tokens"
    if config.columns.contains(&"Output Tokens".to_string()) {
        let label = "Out Tokens:".to_string();
        let value = format!("{}{}{}", SLATE600, format_tokens(stdin.total_output_tokens), RESET);
        columns.push(Column { label, value });
    }

    // "Cache"
    if config.columns.contains(&"Cache".to_string()) {
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
        let value = format!(
            "{}{:.0}%{}{} hit{}",
            color, cache_pct, RESET, SLATE600, RESET
        );
        columns.push(Column { label, value });
    }

    // "API Time"
    if config.columns.contains(&"API Time".to_string()) {
        let label = "API Time:".to_string();
        let value = if stdin.total_api_duration_ms > 0 {
            format!("{}{}{}", SLATE600, format_duration(stdin.total_api_duration_ms), RESET)
        } else {
            format!("{}N/A{}", SLATE600, RESET)
        };
        columns.push(Column { label, value });
    }

    // "5h Reset"
    if config.columns.contains(&"5h Reset".to_string()) {
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
        columns.push(Column { label, value });
    }

    // "7d Reset"
    if config.columns.contains(&"7d Reset".to_string()) {
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
        columns.push(Column { label, value });
    }

    // -----------------------------------------------------------------------
    // Layout rendering
    // -----------------------------------------------------------------------
    let blank_line = format!("\n{}\u{200B}", RESET);

    let main_section = if columns.is_empty() {
        String::new()
    } else {
        match config.layout {
            Layout::Vertical => {
                // Compute max widths for alignment
                let max_label = columns.iter()
                    .map(|c| strip_ansi(&c.label).chars().count())
                    .max()
                    .unwrap_or(0);
                let max_value = columns.iter()
                    .map(|c| strip_ansi(&c.value).chars().count())
                    .max()
                    .unwrap_or(0);

                let row1: Vec<String> = columns.iter()
                    .map(|c| pad_ansi(&c.label, max_label))
                    .collect();
                let row2: Vec<String> = columns.iter()
                    .map(|c| pad_ansi(&c.value, max_value))
                    .collect();

                let sep_colored = format!(" {}│{} ", SLATE800, RESET);
                let line1 = row1.join(&sep_colored);
                let line2 = row2.join(&sep_colored);
                format!("{}\n{}", line1, line2)
            }
            Layout::Horizontal => {
                let pairs: Vec<String> = columns.iter()
                    .map(|c| format!("{} {}", c.label, c.value))
                    .collect();
                pairs.join(" │ ")
            }
        }
    };

    // -----------------------------------------------------------------------
    // Line 3: agents/todos
    // -----------------------------------------------------------------------
    let running_agents: Vec<&crate::transcript::Agent> = transcript.agents.iter()
        .filter(|a| a.status == AgentStatus::Running)
        .collect();
    let running_count = running_agents.len();

    let total_todos = transcript.todos.len();
    let done_todos = transcript.todos.iter()
        .filter(|t| t.status == "completed")
        .count();

    let mut line3_parts: Vec<String> = Vec::new();

    // Running agents count
    if running_count > 0 {
        line3_parts.push(format!(
            "{}Agents:{} ${}",
            SLATE800_BOLD, RESET, running_count
        ));
    }

    // Agent name
    if let Some(ref name) = stdin.agent_name {
        line3_parts.push(format!(
            "{}Agent:{} {}{}{}",
            SLATE800_BOLD, RESET, MAGENTA, name, RESET
        ));
    }

    // Todo progress
    if total_todos > 0 {
        let color = if done_todos == total_todos { GREEN } else { YELLOW };
        line3_parts.push(format!(
            "{}Todos:{} {}{}/{}{} ",
            SLATE800_BOLD, RESET, color, done_todos, total_todos, RESET
        ));
    }

    let sep_colored = format!(" {}│{} ", SLATE800, RESET);

    let mut output = main_section;

    if !line3_parts.is_empty() {
        let line3 = line3_parts.join(&sep_colored);
        output.push_str(&blank_line);
        output.push('\n');
        output.push_str(RESET);
        output.push_str(&line3);
    }

    // -----------------------------------------------------------------------
    // Agent detail tree (if running agents, max 5)
    // -----------------------------------------------------------------------
    if running_count > 0 {
        let now = now_ms();
        let display_agents: Vec<&&crate::transcript::Agent> = running_agents.iter()
            .take(5)
            .collect();
        let last_idx = display_agents.len().saturating_sub(1);

        for (i, agent) in display_agents.iter().enumerate() {
            let prefix = if i == last_idx { "└─" } else { "├─" };
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
    }

    // -----------------------------------------------------------------------
    // Final: append blank_line + \n, then replace spaces with NBSP
    // -----------------------------------------------------------------------
    output.push_str(&blank_line);
    output.push('\n');

    // Replace all regular spaces with non-breaking spaces
    output.replace(' ', "\u{00A0}")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ansi;

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 5), "hello");
        assert_eq!(truncate("héllo wörld", 5), "héllo");
    }

    #[test]
    fn test_render_contains_labels() {
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
            columns: vec!["5h Usage".into(), "Context".into(), "Model".into()],
            layout: Layout::Vertical,
        };
        let out = render(Some(&usage), &transcript, &stdin_data, None, &config);
        let plain = ansi::strip_ansi(&out).replace('\u{00A0}', " ");
        assert!(plain.contains("5h Usage:"));
        assert!(plain.contains("42%"));
        assert!(plain.contains("Context:"));
        assert!(plain.contains("30%"));
        assert!(plain.contains("Model:"));
        assert!(plain.contains("Opus 4.6"));
    }
}

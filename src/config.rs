// Config reader for ~/.claude/hud/config.jsonc
// Parses JSONC (JSON with comments and trailing commas) to determine
// which columns to display and the layout mode.

use crate::json;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const ALL_COLUMNS: &[&str] = &[
    "5h Usage", "7d Usage", "Context", "Model", "Version",
    "Session", "Changes", "Directory", "Cost",
    "Tokens", "Output Tokens", "Cache", "API Time", "5h Reset", "7d Reset",
];

fn default_enabled(name: &str) -> bool {
    matches!(name, "5h Usage" | "7d Usage" | "Context" | "Model" | "Version")
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Layout {
    Vertical,
    Horizontal,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub columns: Vec<String>,
    pub layout: Layout,
}

// ---------------------------------------------------------------------------
// JSONC stripping
// ---------------------------------------------------------------------------

/// Strip `//` line comments (but not `//` inside strings) and trailing
/// commas before `}` or `]`.
pub fn strip_jsonc(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;

    // Phase 1: remove // comments while preserving strings
    let mut no_comments = String::with_capacity(input.len());
    while i < len {
        match chars[i] {
            '"' => {
                // consume the whole string literal
                no_comments.push('"');
                i += 1;
                while i < len {
                    let c = chars[i];
                    no_comments.push(c);
                    i += 1;
                    if c == '\\' {
                        // escaped character — push it verbatim and continue
                        if i < len {
                            no_comments.push(chars[i]);
                            i += 1;
                        }
                    } else if c == '"' {
                        break;
                    }
                }
            }
            '/' if i + 1 < len && chars[i + 1] == '/' => {
                // skip until end of line
                while i < len && chars[i] != '\n' {
                    i += 1;
                }
            }
            c => {
                no_comments.push(c);
                i += 1;
            }
        }
    }

    // Phase 2: remove trailing commas before } or ]
    // Walk through no_comments; when we see a comma, check whether the next
    // non-whitespace character is } or ] and drop the comma if so.
    let chars2: Vec<char> = no_comments.chars().collect();
    let len2 = chars2.len();
    let mut j = 0;
    while j < len2 {
        match chars2[j] {
            '"' => {
                // preserve strings verbatim
                result.push('"');
                j += 1;
                while j < len2 {
                    let c = chars2[j];
                    result.push(c);
                    j += 1;
                    if c == '\\' {
                        if j < len2 {
                            result.push(chars2[j]);
                            j += 1;
                        }
                    } else if c == '"' {
                        break;
                    }
                }
            }
            ',' => {
                // peek ahead to find the next non-whitespace char
                let mut k = j + 1;
                while k < len2 && chars2[k].is_ascii_whitespace() {
                    k += 1;
                }
                if k < len2 && (chars2[k] == '}' || chars2[k] == ']') {
                    // trailing comma — drop it
                    j += 1;
                } else {
                    result.push(',');
                    j += 1;
                }
            }
            c => {
                result.push(c);
                j += 1;
            }
        }
    }

    result
}

// ---------------------------------------------------------------------------
// Config reader
// ---------------------------------------------------------------------------

/// Read `~/.claude/hud/config.jsonc`, parse it, and return a `Config`.
/// Falls back to defaults on any error.
pub fn read_config() -> Config {
    let default_config = Config {
        columns: ALL_COLUMNS
            .iter()
            .filter(|&&name| default_enabled(name))
            .map(|&name| name.to_string())
            .collect(),
        layout: Layout::Vertical,
    };

    // Resolve the config file path
    let home = match std::env::var("HOME") {
        Ok(h) => h,
        Err(_) => return default_config,
    };
    let path = format!("{}/.claude/hud/config.jsonc", home);

    let raw = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return default_config,
    };

    let stripped = strip_jsonc(&raw);

    let parsed = match json::parse(&stripped) {
        Ok(v) => v,
        Err(_) => return default_config,
    };

    // Extract layout
    let layout = match parsed.get("layout").and_then(|v| v.as_str()) {
        Some("horizontal") => Layout::Horizontal,
        _ => Layout::Vertical,
    };

    // Extract columns: look for a "columns" object mapping column name -> bool
    let columns = if let Some(cols_val) = parsed.get("columns") {
        if let Some(pairs) = cols_val.as_object() {
            // Collect in the canonical order defined by ALL_COLUMNS
            ALL_COLUMNS
                .iter()
                .filter(|&&name| {
                    pairs
                        .iter()
                        .find(|(k, _)| k == name)
                        .and_then(|(_, v)| v.as_bool())
                        .unwrap_or(false)
                })
                .map(|&name| name.to_string())
                .collect()
        } else {
            default_config.columns.clone()
        }
    } else {
        default_config.columns.clone()
    };

    Config { columns, layout }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_jsonc_comments() {
        // Full-line comment
        let input = "// this is a comment\n{\"key\": 1}";
        let result = strip_jsonc(input);
        assert!(!result.contains("this is a comment"));
        assert!(result.contains("\"key\""));

        // Inline comment
        let input2 = "{\"key\": 1 // inline comment\n}";
        let result2 = strip_jsonc(input2);
        assert!(!result2.contains("inline comment"));
        assert!(result2.contains("\"key\""));
    }

    #[test]
    fn test_strip_jsonc_trailing_commas() {
        let input = "{\"a\": 1,}";
        let result = strip_jsonc(input);
        // The trailing comma before } should be removed
        assert!(!result.contains(",}"));
        // Parsing should succeed
        assert!(json::parse(&result).is_ok());
    }

    #[test]
    fn test_strip_jsonc_preserves_strings() {
        let input = r#"{"url": "http://example.com"}"#;
        let result = strip_jsonc(input);
        // The URL inside the string must be preserved unchanged
        assert!(result.contains("http://example.com"));
        assert!(json::parse(&result).is_ok());
    }

    #[test]
    fn test_default_columns() {
        let config = Config {
            columns: ALL_COLUMNS
                .iter()
                .filter(|&&name| default_enabled(name))
                .map(|&name| name.to_string())
                .collect(),
            layout: Layout::Vertical,
        };

        assert_eq!(config.columns.len(), 5);
        assert!(config.columns.contains(&"5h Usage".to_string()));
        assert!(config.columns.contains(&"7d Usage".to_string()));
        assert!(config.columns.contains(&"Context".to_string()));
        assert!(config.columns.contains(&"Model".to_string()));
        assert!(config.columns.contains(&"Version".to_string()));
        assert!(!config.columns.contains(&"Session".to_string()));
    }
}

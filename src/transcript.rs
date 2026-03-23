// JSONL transcript parser for Claude Code session files.
// Tracks running subagents and todo lists.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::fs::File;

use crate::json::{parse, JsonValue};
use crate::time::{parse_iso8601, now_ms};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const MAX_TAIL_BYTES: u64 = 512 * 1024;
pub const MAX_AGENT_MAP: usize = 100;
pub const STALE_AGENT_MS: u64 = 30 * 60_000;
pub const FIRST_LINE_BUFFER: usize = 4096;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct TranscriptData {
    pub session_start: Option<u64>, // epoch ms
    pub agents: Vec<Agent>,
    pub todos: Vec<Todo>,
}

#[derive(Debug, Clone)]
pub struct Agent {
    pub id: String,
    pub agent_type: String,
    pub model: Option<String>,
    pub description: String,
    pub status: AgentStatus,
    pub start_time: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentStatus {
    Running,
    Completed,
}

#[derive(Debug, Clone)]
pub struct Todo {
    pub content: String,
    pub status: String,
}

// ---------------------------------------------------------------------------
// Private types
// ---------------------------------------------------------------------------

/// Tracks an agent entry keyed by the tool_use id that spawned it.
#[derive(Debug)]
struct AgentEntry {
    tool_use_id: String,
    agent_id: Option<String>, // filled in when async launch confirmed
    agent_type: String,
    model: Option<String>,
    description: String,
    status: AgentStatus,
    start_time: u64,
}

/// Mutable state threaded through parse_line calls.
pub(crate) struct ParseState {
    session_start: Option<u64>,
    /// Agents keyed by tool_use_id.
    agent_map: Vec<AgentEntry>,
    /// bg_map: maps agent_id -> tool_use_id for async launched agents.
    bg_map: HashMap<String, String>,
    todos: Vec<Todo>,
}

impl ParseState {
    fn new() -> Self {
        ParseState {
            session_start: None,
            agent_map: Vec::new(),
            bg_map: HashMap::new(),
            todos: Vec::new(),
        }
    }

    /// Find agent entry index by tool_use_id.
    fn find_by_tool_use_id(&self, id: &str) -> Option<usize> {
        self.agent_map.iter().position(|e| e.tool_use_id == id)
    }

    /// Evict oldest completed entry to make room (called when at MAX_AGENT_MAP).
    fn evict_oldest_completed(&mut self) {
        if let Some(pos) = self.agent_map.iter().position(|e| e.status == AgentStatus::Completed) {
            self.agent_map.remove(pos);
        } else {
            // No completed entries; evict the oldest running one.
            if !self.agent_map.is_empty() {
                self.agent_map.remove(0);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

/// Parse a JSONL transcript file and return extracted data.
pub fn parse_transcript(path: &str) -> TranscriptData {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return empty_transcript(),
    };

    let file_size = meta.len();
    let mut state = ParseState::new();

    if file_size > MAX_TAIL_BYTES {
        // Large file: read first FIRST_LINE_BUFFER bytes for session_start,
        // then tail-read last MAX_TAIL_BYTES for agents.
        if let Ok(mut f) = File::open(path) {
            // Read first chunk for session_start only.
            let mut first_buf = vec![0u8; FIRST_LINE_BUFFER];
            let n = f.read(&mut first_buf).unwrap_or(0);
            first_buf.truncate(n);
            let first_text = String::from_utf8_lossy(&first_buf);
            if let Some(line) = first_text.lines().next() {
                process_line(line, &mut state);
            }

            // Seek to tail.
            let tail_offset = file_size.saturating_sub(MAX_TAIL_BYTES);
            if f.seek(SeekFrom::Start(tail_offset)).is_ok() {
                let mut tail_buf = Vec::new();
                let _ = f.read_to_end(&mut tail_buf);
                let tail_text = String::from_utf8_lossy(&tail_buf);
                let mut lines = tail_text.lines();
                // Discard potentially partial first line.
                if tail_offset > 0 {
                    lines.next();
                }
                for line in lines {
                    process_line(line, &mut state);
                }
            }
        }
    } else {
        // Small file: read entirely.
        if let Ok(contents) = std::fs::read_to_string(path) {
            for line in contents.lines() {
                process_line(line, &mut state);
            }
        }
    }

    build_result(state)
}

/// Construct the empty (default) transcript result.
fn empty_transcript() -> TranscriptData {
    TranscriptData {
        session_start: None,
        agents: Vec::new(),
        todos: Vec::new(),
    }
}

/// Convert ParseState into final TranscriptData, applying staleness and ordering.
fn build_result(mut state: ParseState) -> TranscriptData {
    let now = now_ms();

    // Mark stale running agents as completed.
    for entry in &mut state.agent_map {
        if entry.status == AgentStatus::Running {
            if now.saturating_sub(entry.start_time) > STALE_AGENT_MS {
                entry.status = AgentStatus::Completed;
            }
        }
    }

    // Separate running and completed.
    let mut running: Vec<&AgentEntry> = state.agent_map.iter()
        .filter(|e| e.status == AgentStatus::Running)
        .collect();
    let mut completed: Vec<&AgentEntry> = state.agent_map.iter()
        .filter(|e| e.status == AgentStatus::Completed)
        .collect();

    // Sort completed by start_time descending (most recent first).
    completed.sort_by(|a, b| b.start_time.cmp(&a.start_time));

    // Running agents first, then recent completed, max 10 total.
    running.append(&mut completed);
    let selected: Vec<Agent> = running.into_iter().take(10).map(|e| Agent {
        id: e.agent_id.clone().unwrap_or_else(|| e.tool_use_id.clone()),
        agent_type: e.agent_type.clone(),
        model: e.model.clone(),
        description: e.description.clone(),
        status: e.status.clone(),
        start_time: e.start_time,
    }).collect();

    TranscriptData {
        session_start: state.session_start,
        agents: selected,
        todos: state.todos,
    }
}

// ---------------------------------------------------------------------------
// Line processor
// ---------------------------------------------------------------------------

/// Parse one JSONL line and update state.
pub(crate) fn process_line(line: &str, state: &mut ParseState) {
    let line = line.trim();
    if line.is_empty() {
        return;
    }

    let val = match parse(line) {
        Ok(v) => v,
        Err(_) => return,
    };

    // Extract session_start from first timestamp seen.
    if state.session_start.is_none() {
        if let Some(ts_str) = val.get("timestamp").and_then(|v| v.as_str()) {
            if let Some(ms) = parse_iso8601(ts_str) {
                state.session_start = Some(ms);
            }
        }
    }

    // Determine message role/type.
    let msg_type = val.get("type").and_then(|v| v.as_str()).unwrap_or("");

    match msg_type {
        "assistant" => {
            process_assistant_message(&val, state);
        }
        "tool" => {
            process_tool_result(&val, state);
        }
        _ => {
            // Some transcripts have content blocks directly at top level.
            // Also handle messages with a "message" wrapper.
            if let Some(msg) = val.get("message") {
                let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");
                match role {
                    "assistant" => process_assistant_message(msg, state),
                    _ => {}
                }
            }
        }
    }
}

fn process_assistant_message(msg: &JsonValue, state: &mut ParseState) {
    // Get content array.
    let content = match msg.get("content").and_then(|v| v.as_array()) {
        Some(c) => c,
        None => return,
    };

    for block in content {
        let block_type = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if block_type != "tool_use" {
            continue;
        }

        let tool_name = block.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let tool_use_id = match block.get("id").and_then(|v| v.as_str()) {
            Some(id) => id.to_string(),
            None => continue,
        };

        match tool_name {
            "Task" | "proxy_Task" => {
                let input = block.get("input");
                let description = input
                    .and_then(|i| i.get("description").or_else(|| i.get("prompt")))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let model = input
                    .and_then(|i| i.get("model"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                // Evict if at capacity.
                if state.agent_map.len() >= MAX_AGENT_MAP {
                    state.evict_oldest_completed();
                }

                let start_time = state.session_start.unwrap_or_else(now_ms);
                state.agent_map.push(AgentEntry {
                    tool_use_id,
                    agent_id: None,
                    agent_type: tool_name.to_string(),
                    model,
                    description,
                    status: AgentStatus::Running,
                    start_time,
                });
            }
            "TaskCreate" | "TodoWrite" => {
                // Capture todos from input.todos array.
                if let Some(input) = block.get("input") {
                    if let Some(todos_arr) = input.get("todos").and_then(|v| v.as_array()) {
                        let mut new_todos = Vec::new();
                        for todo in todos_arr {
                            let content = todo.get("content")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let status = todo.get("status")
                                .and_then(|v| v.as_str())
                                .unwrap_or("pending")
                                .to_string();
                            new_todos.push(Todo { content, status });
                        }
                        state.todos = new_todos;
                    }
                }
            }
            _ => {}
        }
    }
}

fn process_tool_result(msg: &JsonValue, state: &mut ParseState) {
    // tool_result lines have tool_use_id at top level or in content.
    let tool_use_id = match msg.get("tool_use_id").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => return,
    };

    // Only process if tool_use_id matches a known agent.
    let entry_idx = match state.find_by_tool_use_id(&tool_use_id) {
        Some(idx) => idx,
        None => return,
    };

    // Get the text content of the result.
    let content_text = msg.get("content")
        .map(|c| extract_text(c))
        .unwrap_or_default();

    // Check for "Async agent launched" pattern.
    if content_text.contains("Async agent launched") {
        if let Some(agent_id) = extract_agent_id(&content_text) {
            state.agent_map[entry_idx].agent_id = Some(agent_id.clone());
            state.bg_map.insert(agent_id, tool_use_id);
        }
        // Agent is still running asynchronously.
    } else {
        // Check for TaskOutput completion tags.
        if let Some(task_id) = extract_task_id(&content_text) {
            if content_text.contains("<status>completed</status>") {
                // Find the agent by its agent_id.
                if let Some(pos) = state.agent_map.iter().position(|e| {
                    e.agent_id.as_deref() == Some(&task_id)
                }) {
                    state.agent_map[pos].status = AgentStatus::Completed;
                    return;
                }
            }
            // Look up bg_map for this task_id.
            if let Some(tuid) = state.bg_map.get(&task_id).cloned() {
                if content_text.contains("<status>completed</status>") {
                    if let Some(pos) = state.find_by_tool_use_id(&tuid) {
                        state.agent_map[pos].status = AgentStatus::Completed;
                    }
                }
            }
        } else {
            // Direct tool result — mark completed.
            state.agent_map[entry_idx].status = AgentStatus::Completed;
        }
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Extract text from a JSON value that may be a string or an array of content blocks.
pub fn extract_text(block: &JsonValue) -> String {
    match block {
        JsonValue::Str(s) => s.clone(),
        JsonValue::Array(items) => {
            let mut out = String::new();
            for item in items {
                match item {
                    JsonValue::Str(s) => out.push_str(s),
                    JsonValue::Object(_) => {
                        let t = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
                        if t == "text" {
                            if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                                out.push_str(text);
                            }
                        }
                    }
                    _ => {}
                }
            }
            out
        }
        _ => String::new(),
    }
}

/// Find `agentId:` marker and extract the following ASCII alphanumeric ID.
pub fn extract_agent_id(text: &str) -> Option<String> {
    let marker = "agentId:";
    let pos = text.find(marker)?;
    let after = text[pos + marker.len()..].trim_start();
    let id: String = after.chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if id.is_empty() {
        None
    } else {
        Some(id)
    }
}

/// Extract the content between `<task_id>` and `</task_id>` tags.
pub fn extract_task_id(text: &str) -> Option<String> {
    let open = "<task_id>";
    let close = "</task_id>";
    let start = text.find(open)? + open.len();
    let end = text[start..].find(close)?;
    Some(text[start..start + end].to_string())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_agent_id() {
        let text = "Async agent launched agentId: abc123 done";
        assert_eq!(extract_agent_id(text), Some("abc123".to_string()));
    }

    #[test]
    fn test_extract_task_id() {
        let text = "<task_id>xyz789</task_id>";
        assert_eq!(extract_task_id(text), Some("xyz789".to_string()));
    }

    #[test]
    fn test_process_agent_lifecycle() {
        let mut state = ParseState::new();

        // Create agent via tool_use.
        let tool_use_line = r#"{"type":"assistant","content":[{"type":"tool_use","id":"tu_001","name":"Task","input":{"description":"Do something","prompt":"test"}}]}"#;
        process_line(tool_use_line, &mut state);

        assert_eq!(state.agent_map.len(), 1);
        assert_eq!(state.agent_map[0].status, AgentStatus::Running);
        assert_eq!(state.agent_map[0].tool_use_id, "tu_001");

        // Complete the agent via tool_result.
        let tool_result_line = r#"{"type":"tool","tool_use_id":"tu_001","content":"Task completed successfully"}"#;
        process_line(tool_result_line, &mut state);

        assert_eq!(state.agent_map[0].status, AgentStatus::Completed);
    }

    #[test]
    fn test_process_todos() {
        let mut state = ParseState::new();

        let todo_line = r#"{"type":"assistant","content":[{"type":"tool_use","id":"tu_002","name":"TodoWrite","input":{"todos":[{"content":"Fix bug","status":"pending"},{"content":"Write tests","status":"in_progress"}]}}]}"#;
        process_line(todo_line, &mut state);

        assert_eq!(state.todos.len(), 2);
        assert_eq!(state.todos[0].content, "Fix bug");
        assert_eq!(state.todos[0].status, "pending");
        assert_eq!(state.todos[1].content, "Write tests");
        assert_eq!(state.todos[1].status, "in_progress");
    }

    #[test]
    fn test_empty_transcript() {
        let result = parse_transcript("/nonexistent/path/to/file.jsonl");
        assert!(result.session_start.is_none());
        assert!(result.agents.is_empty());
        assert!(result.todos.is_empty());
    }
}

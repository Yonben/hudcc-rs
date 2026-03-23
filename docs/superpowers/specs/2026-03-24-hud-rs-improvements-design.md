# HUD RS Improvements Design

**Date:** 2026-03-24
**Status:** Approved
**Scope:** Five improvement areas for the hud_rs Claude Code status line, ordered by implementation sequence.

## Overview

The hud_rs binary is feature-complete. These improvements focus on code quality, polish, observability, test coverage, and performance — in that order. The core data and column set remain unchanged; the work is about *how* information is presented and how maintainable the codebase is.

## 1. Render Deduplication

### Problem

`render.rs` lines 48–271 contain 15 column definitions, each following an identical pattern:

```rust
if config.columns.contains(&"X".to_string()) {
    let label = ...;
    let value = ...;
    columns.push(Column { label, value });
}
```

This is ~220 lines of structural repetition. Adding a new column requires copying the boilerplate.

### Design

Replace with a declarative column registry using closures:

```rust
let defs: Vec<(&str, Box<dyn Fn() -> Column>)> = vec![
    ("5h Usage", Box::new(|| Column { label: ..., value: ... })),
    ("Context", Box::new(|| Column { label: ..., value: ... })),
    // ...
];

let columns: Vec<Column> = defs.into_iter()
    .filter(|(name, _)| config.columns.contains(&name.to_string()))
    .map(|(_, build)| build())
    .collect();
```

Each column's rendering logic stays self-contained inside its closure. The filter/map loop replaces all 15 `if` blocks. Adding a new column becomes appending one tuple.

No macros. No trait objects beyond the closure. No change to the `Column` struct or the layout rendering code below it.

### Constraints

- The column order in the `defs` vec must match the current insertion order (which follows `config.columns` ordering).
- Filter uses `config.columns.contains()` as today — config drives which columns appear.

## 2. UX Polish

Built on top of the cleaned-up column registry. Four targeted changes.

### 2a. Truncation with Ellipsis

Update the `truncate()` function:

```rust
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max - 1).collect();
        t.push('…');
        t
    }
}
```

Applied to:
- Agent descriptions (45 chars) — shows `…` when truncated
- Agent types (14 chars) — shows `…` when truncated
- Directory column values (20 chars) — new truncation
- Model names (15 chars) — new truncation

### 2b. Agent Overflow Indicator

When `running_agents.len() > 5`, display 4 agents plus an overflow line:

```
├─ Task  Sonnet   2m  Implementing feature X…
├─ Task  Sonnet   1m  Running tests for modu…
├─ Task  Haiku   45s  Checking lint results
├─ Task  Sonnet  30s  Reviewing code changes…
└─ … and 3 more
```

Implementation: when `running_count > 5`, `take(4)` instead of `take(5)`, then append the overflow line with `└─` prefix.

### 2c. Sub-Penny Cost Display

In the "Cost" column:
- `cost == 0.0` → `$0.00` in green (unchanged)
- `cost > 0.0 && cost < 0.01` → `<$0.01` in green
- `cost >= 0.01` → `${:.2}` with existing color thresholds (green < $0.25, yellow < $1.00, red >= $1.00)

### 2d. Long Value Truncation

To prevent layout breakage with unexpectedly long values:
- Directory column: truncate to 20 chars with ellipsis
- Model name: truncate to 15 chars with ellipsis

These apply after the value is computed, before it's wrapped in ANSI codes.

## 3. Error Observability

### Design

A `HUD_DEBUG=1` environment variable that writes diagnostics to stderr.

**Debug helper (in `main.rs`):**

```rust
fn debug(msg: &str) {
    if std::env::var("HUD_DEBUG").as_deref() == Ok("1") {
        eprintln!("[hud] {}", msg);
    }
}
```

For efficiency, the env var check is done once at startup and stored as a `bool`, passed to functions that need it or accessed via a module-level approach.

**Instrumented failure points:**

| Module | What's logged | Current behavior |
|--------|--------------|-----------------|
| `api.rs` | curl timeout vs DNS failure vs bad JSON vs HTTP status code | Returns `None` |
| `config.rs` | Config file exists but failed to parse (with error detail) | Silent fallback to defaults |
| `main.rs` | Thread panic payload (from `join().err()`) | `unwrap_or(None)` |
| `stdin.rs` | Stdin read failure reason | Returns `None` |

**No behavior change when `HUD_DEBUG` is unset.** All existing code paths remain identical. The debug lines are additive only.

## 4. Test Coverage

### 4a. Integration Test

New file: `tests/integration.rs`

Tests the full pipeline: `stdin::extract()` → `render::render()` → strip ANSI → assert content.

Test cases:
- All standard columns populated with realistic values
- Empty/missing optional fields (no usage data, no version, no agents)
- Vertical layout output has two rows separated by newline
- Horizontal layout output is a single row

### 4b. Edge-Case Unit Tests

Added to existing test modules:

**`render.rs` tests:**
- Truncated agent description contains `…`
- 6+ agents produces "+N more" overflow line
- Cost of `0.005` displays as `<$0.01`
- Empty column config produces empty main section

**`transcript.rs` tests:**
- Malformed JSONL lines are skipped without panic
- Agent running for >30 minutes is marked completed (stale cutoff)
- Empty transcript file produces empty `TranscriptData`

**`stdin.rs` tests:**
- Missing `model` field → empty string model_id
- Missing `context_window` → 0% context
- `used_percentage` of 150.0 is clamped to 100

**`json.rs` tests:**
- Deeply nested objects (10+ levels)
- Very long string values (10K+ chars)

### 4c. No Mock Network Tests

The existing `HUD_NO_NETWORK=1` mode already exercises the no-network path. Adding mock HTTP would add complexity without proportional value.

## 5. JSON Parser Performance

### Problem

`json.rs` line 158 converts the entire input to `Vec<char>`, allocating 4 bytes per character. For a 512KB cached API response, this means ~2MB just for the character index.

### Design

Rewrite the `Parser` struct to operate on `&[u8]` instead of `&[char]`:

```rust
struct Parser<'a> {
    input: &'a [u8],
    pos: usize,
}
```

**Key invariants:**
- JSON structural characters (`{}[]:,"\\`) are all ASCII single-byte — safe to match at byte level
- String content between quotes is valid UTF-8 — slice directly with `std::str::from_utf8(&input[start..end])`
- `\uXXXX` escape handling remains the same (already operates on hex digits)
- `peek()` returns `Option<u8>` instead of `Option<char>`
- `advance()` increments `pos` by 1 for structural chars, by UTF-8 byte length inside strings

**Public API unchanged:**
```rust
pub fn parse(input: &str) -> Result<JsonValue, String>
```

The function converts `input` to `input.as_bytes()` internally. All callers are unaffected.

**Risk mitigation:** Keep the old `Vec<char>` parser available behind `#[cfg(test)]` as a reference implementation. Add a differential fuzz test that feeds random valid JSON to both parsers and asserts identical output.

### Constraints

- Zero external dependencies remains — no `simd-json`, no `serde`.
- All 10 existing `json.rs` tests must pass unchanged.
- The `JsonValue` enum and all accessor methods (`get()`, `get_path()`, `as_str()`, `as_f64()`, etc.) are unchanged.

## Implementation Order

```
1. Render deduplication  (render.rs)
2. UX polish             (render.rs, truncate helper)
3. Error observability   (main.rs, api.rs, config.rs, stdin.rs)
4. Test coverage         (tests/integration.rs, existing test modules)
5. JSON parser perf      (json.rs)
```

Each step is independently shippable and testable. Step 2 depends on step 1 (builds on the cleaned-up column registry). All others are independent.

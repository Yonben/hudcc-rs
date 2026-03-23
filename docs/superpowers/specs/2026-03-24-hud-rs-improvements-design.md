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

- The display order is always the hardcoded `defs` vec order. `config.columns` controls *which* columns appear, not their position. This matches the current behavior where column position is determined by the order of `if` blocks in the source.
- Filter uses `config.columns.contains()` as today.
- Closures capture borrowed references (`usage`, `stdin`, etc.) from the `render()` function. The closure type needs a lifetime annotation: `Box<dyn Fn() -> Column + 'a>` where `'a` is tied to the function parameters. Alternatively, the implementer may use a simpler approach (e.g., a helper function per column, or inline closures without boxing) — whichever compiles cleanest.

## 2. UX Polish

Built on top of the cleaned-up column registry. Four targeted changes.

### 2a. Truncation with Ellipsis

Modify the *existing* `truncate()` function (which already truncates agent descriptions at 45 chars and agent types at 14 chars) to append `…` when truncation occurs. This changes the truncation behavior at existing call sites, not their locations.

Updated `truncate()` function:

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

Implementation: when `running_count > 5`, `take(4)` instead of `take(5)`, then append the overflow line. The overflow count is `running_count - 4`.

**Tree prefix logic:** When overflow is active, all 4 agent lines use `├─` prefix; only the overflow summary line uses `└─`. This differs from the non-overflow case where the last agent line gets `└─`.

### 2c. Sub-Penny Cost Display

In the "Cost" column:
- `cost == 0.0` → `$0.00` in green (unchanged)
- `cost > 0.0 && cost < 0.01` → `<$0.01` in green
- `cost >= 0.01` → `${:.2}` with existing color thresholds (green < $0.25, yellow < $1.00, red >= $1.00)

### 2d. Long Value Truncation (New Call Sites)

Section 2a modifies the existing `truncate()` behavior. This section adds *new* call sites for truncation to prevent layout breakage with unexpectedly long values:
- Directory column: truncate to 20 chars with ellipsis
- Model name: truncate to 15 chars with ellipsis

These apply after the value is computed, before it's wrapped in ANSI codes.

## 3. Error Observability

### Design

A `HUD_DEBUG=1` environment variable that writes diagnostics to stderr.

**Relationship to existing `DEBUG_USAGE`:** The codebase already has a `DEBUG_USAGE` env var that writes API debug data to `~/.claude/hud/.usage-debug.log`. `HUD_DEBUG` supersedes it — when `HUD_DEBUG=1`, all debug output (including API diagnostics) goes to stderr. The file-based `DEBUG_USAGE` mechanism is removed and its call sites migrated to the new stderr approach.

**Debug bool — passed explicitly:** The env var is checked once at startup in `main.rs` and stored as a `bool`. This bool is passed as a parameter to functions that need it (matching the pattern already used by `api::get_usage()` which takes `debug_enabled: bool`). No global state, no atomics.

```rust
let debug = std::env::var("HUD_DEBUG").as_deref() == Ok("1");
```

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
- **UTF-8 safety:** UTF-8's self-synchronizing property guarantees that `"` (0x22) and `\` (0x5C) bytes never appear as continuation bytes in multi-byte sequences. This means byte-level scanning for string delimiters is safe without tracking UTF-8 state.
- String content between quotes is valid UTF-8 — slice directly with `std::str::from_utf8(&input[start..end])`
- `\uXXXX` escape handling remains the same (already operates on hex digits)
- `peek()` returns `Option<u8>` instead of `Option<char>`
- `advance()` increments `pos` by 1 for structural chars, by UTF-8 byte length inside strings

**Public API unchanged:**
```rust
pub fn parse(input: &str) -> Result<JsonValue, String>
```

The function converts `input` to `input.as_bytes()` internally. All callers are unaffected.

**Risk mitigation:** Keep the old `Vec<char>` parser available behind `#[cfg(test)]` as a reference implementation. Add a differential test with a curated set of edge-case JSON strings (deeply nested, unicode-heavy, large, empty, etc.) that feeds both parsers identical input and asserts identical output. This is a `#[test]`, not actual fuzzing — no external tools or dependencies needed.

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

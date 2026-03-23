#!/usr/bin/env bash
set -euo pipefail

RUNS=${1:-100}
RUST_BIN="./target/release/hud_rs"
JS_SCRIPT="./hud/metricc-cc-statusbar.mjs"

TEST_INPUT='{"context_window":{"used_percentage":72,"context_window_size":200000,"current_usage":{"input_tokens":100000,"cache_creation_input_tokens":20000,"cache_read_input_tokens":24000},"total_output_tokens":5000},"model":{"id":"claude-opus-4-6"},"version":"1.0.5","cost":{"total_cost_usd":0.42,"total_duration_ms":600000,"total_lines_added":150,"total_lines_removed":30,"total_api_duration_ms":25000},"workspace":{"current_dir":"/home/user/project"},"agent":{"name":"code-agent"}}'

# Build release if needed
if [ ! -f "$RUST_BIN" ]; then
    echo "Building release binary..."
    cargo build --release 2>/dev/null
fi

# Check JS exists
if [ ! -f "$JS_SCRIPT" ]; then
    echo "JS script not found at $JS_SCRIPT"
    exit 1
fi

echo "Benchmarking $RUNS runs each (network disabled via HUD_NO_NETWORK=1)..."
echo ""

# Warmup
echo "$TEST_INPUT" | HUD_NO_NETWORK=1 "$RUST_BIN" >/dev/null 2>&1 || true
echo "$TEST_INPUT" | HUD_NO_NETWORK=1 node "$JS_SCRIPT" >/dev/null 2>&1 || true

# Benchmark Rust
echo "Running Rust ($RUNS iterations)..."
rust_start=$(date +%s%N)
for ((i = 0; i < RUNS; i++)); do
    echo "$TEST_INPUT" | HUD_NO_NETWORK=1 "$RUST_BIN" >/dev/null 2>&1
done
rust_end=$(date +%s%N)
rust_total_ns=$((rust_end - rust_start))
rust_avg_ms=$(echo "scale=2; $rust_total_ns / $RUNS / 1000000" | bc)
rust_total_ms=$(echo "scale=2; $rust_total_ns / 1000000" | bc)

# Benchmark JS
echo "Running JS   ($RUNS iterations)..."
js_start=$(date +%s%N)
for ((i = 0; i < RUNS; i++)); do
    echo "$TEST_INPUT" | HUD_NO_NETWORK=1 node "$JS_SCRIPT" >/dev/null 2>&1
done
js_end=$(date +%s%N)
js_total_ns=$((js_end - js_start))
js_avg_ms=$(echo "scale=2; $js_total_ns / $RUNS / 1000000" | bc)
js_total_ms=$(echo "scale=2; $js_total_ns / 1000000" | bc)

speedup=$(echo "scale=1; $js_total_ns / $rust_total_ns" | bc)

echo ""
echo "═══════════════════════════════════════"
echo "  Results ($RUNS runs, no network)"
echo "═══════════════════════════════════════"
printf "  %-8s  %10s  %10s\n" "" "Total" "Avg/run"
printf "  %-8s  %8s ms  %8s ms\n" "Rust" "$rust_total_ms" "$rust_avg_ms"
printf "  %-8s  %8s ms  %8s ms\n" "JS" "$js_total_ms" "$js_avg_ms"
echo "───────────────────────────────────────"
echo "  Rust is ${speedup}x faster"
echo "═══════════════════════════════════════"

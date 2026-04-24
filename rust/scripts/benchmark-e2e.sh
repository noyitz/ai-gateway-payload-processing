#!/bin/bash
# End-to-end load test using ghz against all 3 ext_proc configurations.
#
# Prerequisites:
#   - ghz installed (go install github.com/bojand/ghz/cmd/ghz@latest)
#   - All 3 configs running (via docker-compose or Kind)
#
# Usage:
#   ./rust/scripts/benchmark-e2e.sh [config-a-port] [config-b-port] [config-c-port]
#
# Default ports: 9014 (Go), 9024 (Hybrid), 9034 (Rust)

set -euo pipefail

CONFIG_A_PORT="${1:-9014}"
CONFIG_B_PORT="${2:-9024}"
CONFIG_C_PORT="${3:-9034}"

PROTO_FILE="rust/proto/envoy/service/ext_proc/v3/external_processor.proto"
PROTO_INCLUDE="rust/proto:rust/proto/xds"
CALL="envoy.service.ext_proc.v3.ExternalProcessor/Process"

CONCURRENCY_LEVELS=(1 10 50 100)
DURATION="30s"
RESULTS_DIR="rust/testdata/e2e_results"
mkdir -p "$RESULTS_DIR"

echo "=== E2E Load Test ==="
echo "Config A (Go):     localhost:$CONFIG_A_PORT"
echo "Config B (Hybrid): localhost:$CONFIG_B_PORT"
echo "Config C (Rust):   localhost:$CONFIG_C_PORT"
echo "Duration per test: $DURATION"
echo "Concurrency levels: ${CONCURRENCY_LEVELS[*]}"
echo ""

run_ghz() {
    local name=$1
    local port=$2
    local concurrency=$3
    local output_file="$RESULTS_DIR/${name}_c${concurrency}.json"

    echo "  Running $name @ concurrency=$concurrency..."

    ghz --insecure \
        --proto "$PROTO_FILE" \
        --import-paths "$PROTO_INCLUDE" \
        --call "$CALL" \
        --concurrency "$concurrency" \
        --duration "$DURATION" \
        --format json \
        "localhost:$port" > "$output_file" 2>/dev/null || {
            echo "  WARNING: ghz failed for $name c=$concurrency (server may not be running)"
            return 1
        }

    # Extract key metrics
    local avg=$(jq -r '.average // "N/A"' "$output_file")
    local p50=$(jq -r '.latencyDistribution[] | select(.percentage == 50) | .latency // "N/A"' "$output_file" 2>/dev/null || echo "N/A")
    local p99=$(jq -r '.latencyDistribution[] | select(.percentage == 99) | .latency // "N/A"' "$output_file" 2>/dev/null || echo "N/A")
    local rps=$(jq -r '.rps // "N/A"' "$output_file")

    echo "    avg=$avg  p50=$p50  p99=$p99  rps=$rps"
}

for concurrency in "${CONCURRENCY_LEVELS[@]}"; do
    echo ""
    echo "--- Concurrency: $concurrency ---"
    run_ghz "config-a" "$CONFIG_A_PORT" "$concurrency" || true
    run_ghz "config-b" "$CONFIG_B_PORT" "$concurrency" || true
    run_ghz "config-c" "$CONFIG_C_PORT" "$concurrency" || true
done

echo ""
echo "=== Results saved to $RESULTS_DIR ==="
echo "View individual results: jq . $RESULTS_DIR/<config>_c<n>.json"

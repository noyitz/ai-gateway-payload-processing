#!/bin/bash
# Run both Go and Rust micro-benchmarks and display comparison.
#
# Usage: ./rust/scripts/benchmark-micro.sh
#
# Prerequisites: cargo, go, installed toolchains

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
RUST_DIR="$PROJECT_ROOT/rust"

echo "=== Rust Benchmarks (criterion) ==="
echo ""
cd "$RUST_DIR"
cargo bench -p ipp-translators 2>&1 | grep -E "(^[a-z_].*time:|^\s+time:)"
echo ""

echo "=== Go Benchmarks (testing.B) ==="
echo ""
cd "$PROJECT_ROOT"
go test -bench=. -benchmem ./pkg/plugins/api-translation/translator/ 2>&1 | grep -E "^Benchmark"
echo ""

echo "=== Done ==="
echo "Full criterion HTML report: $RUST_DIR/target/criterion/report/index.html"

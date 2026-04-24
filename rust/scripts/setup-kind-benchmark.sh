#!/bin/bash
# Set up a Kind cluster for benchmarking all 3 ext_proc configurations.
#
# This script:
#   1. Creates a Kind cluster (or reuses existing)
#   2. Builds and loads all 3 Docker images
#   3. Deploys Istio + Gateway
#   4. Deploys each config as a separate deployment
#
# Usage: ./rust/scripts/setup-kind-benchmark.sh
#
# Prerequisites: docker, kind, kubectl, helm

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

KIND_CLUSTER_NAME="${KIND_CLUSTER_NAME:-ipp-benchmark}"
PLATFORM="linux/amd64"

echo "=== IPP Benchmark Environment Setup ==="
echo "  Cluster: $KIND_CLUSTER_NAME"
echo ""

# --- Create Kind cluster ---
if kind get clusters 2>/dev/null | grep -q "^${KIND_CLUSTER_NAME}$"; then
    echo "Kind cluster '$KIND_CLUSTER_NAME' already exists"
else
    echo "Creating Kind cluster..."
    kind create cluster --name "$KIND_CLUSTER_NAME" --wait 120s
fi
kubectl config use-context "kind-${KIND_CLUSTER_NAME}"

# --- Build images ---
echo ""
echo "Building Config A (Go) image..."
docker build --platform "$PLATFORM" \
    -t ipp-go:latest \
    "$PROJECT_ROOT"

echo ""
echo "Building Config B (Hybrid) image..."
docker build --platform "$PLATFORM" \
    -f "$PROJECT_ROOT/docker/Dockerfile.hybrid" \
    -t ipp-hybrid:latest \
    "$PROJECT_ROOT"

echo ""
echo "Building Config C (Rust) image..."
docker build --platform "$PLATFORM" \
    -f "$PROJECT_ROOT/docker/Dockerfile.rust" \
    -t ipp-rust:latest \
    "$PROJECT_ROOT"

# --- Load images into Kind ---
echo ""
echo "Loading images into Kind..."
kind load docker-image ipp-go:latest --name "$KIND_CLUSTER_NAME"
kind load docker-image ipp-hybrid:latest --name "$KIND_CLUSTER_NAME"
kind load docker-image ipp-rust:latest --name "$KIND_CLUSTER_NAME"

echo ""
echo "=== Setup Complete ==="
echo ""
echo "Images loaded into Kind cluster '$KIND_CLUSTER_NAME'."
echo ""
echo "Next steps:"
echo "  1. Deploy Istio and Gateway (use test/e2e/scripts/setup-kind.sh as reference)"
echo "  2. Deploy each config as a separate Deployment"
echo "  3. Run: ./rust/scripts/benchmark-e2e.sh"

# Benchmark Results — Go vs Rust Translator Performance

**Machine**: Apple M4 Pro (arm64)
**Date**: 2026-04-23
**Go version**: 1.25.0
**Rust version**: 1.95.0

## Translator-Level Benchmarks

| Benchmark | Go (ns/op) | Go allocs | Rust (ns) | Ratio | Winner |
|-----------|-----------|-----------|-----------|-------|--------|
| OpenAI passthrough | 18.8 | 0 | 64 | 3.4x | Go |
| Anthropic request (basic) | 349 | 12 | 739 | 2.1x | Go |
| Anthropic request (complex) | 3,744 | 128 | 12,716 | 3.4x | Go |
| Anthropic response (text) | 467 | 17 | 1,465 | 3.1x | Go |
| Anthropic response (tool_use) | 1,392 | 46 | 3,923 | 2.8x | Go |
| Azure response strip | 22.4 | 0 | 1,488 | 66x | Go |
| JSON parse+serialize (complex) | 23,649 | 467 | 8,537 | 2.8x | **Rust** |

## Analysis

### Why Go is faster at the translator level

Go translators operate on `map[string]any` **in place** — they never serialize or deserialize
the body. A translator that doesn't mutate the body (OpenAI, Azure request) literally returns
`nil` with zero allocations. Even complex translators (Anthropic) build a new `map[string]any`
directly without going through any intermediate representation.

The Rust implementation uses **typed serde structs**: it deserializes `serde_json::Value` into
strongly-typed `OpenAiRequest` / `AnthropicRequest` structs, transforms them, then serializes
back to `serde_json::Value`. This gives compile-time safety but adds serialization cost.

### Where Rust wins

- **Raw JSON parsing**: serde_json is 2.8x faster than Go's encoding/json for the same payload
- **Binary size**: Rust binary is 10MB vs Go's ~50-70MB
- **No GC pauses**: Rust has deterministic latency (no GC)

### Where the comparison matters

The translator benchmark measures **plugin CPU cost only**. In the real request flow:
1. Envoy sends body over gRPC → JSON parsing (Rust wins ~3x)
2. Plugin chain processes → translation (Go wins 2-3x for Anthropic)
3. Response sent back → JSON serialization (Rust wins ~3x)

The **end-to-end latency** depends on all three steps. For providers that don't mutate the body
(OpenAI, Bedrock), the dominant cost is JSON parsing — where Rust wins. For Anthropic (the only
provider with full body transformation), Go wins at the translation step but Rust may win
overall due to faster JSON handling.

### Optimization opportunities for Rust

1. **In-place Value mutation**: Skip typed structs, work directly with `serde_json::Value`
   like Go does. This would eliminate the deserialize-transform-serialize overhead.
2. **Azure strip without clone**: The current Rust Azure translator clones the entire body
   before stripping. In-place mutation on `&mut Value` would match Go's performance.
3. **Pool allocations**: Reuse CycleState and HashMap allocations across requests.

## Build Metrics

| Metric | Go | Rust |
|--------|-----|------|
| Binary size (stripped) | ~50MB | 10MB |
| Docker image | ~30MB (UBI minimal) | ~20MB (distroless) |
| Full build time | ~15s | ~70s |
| Incremental build | ~3s | ~2s |

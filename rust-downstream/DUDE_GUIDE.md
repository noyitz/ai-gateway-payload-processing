# The Dude's Guide to the Rust IPP Codebase

*"This is a very complicated case, Maude. You know, a lotta ins, a lotta outs, a lotta what-have-yous."*

Hey Dude. Noy set up the Rust IPP codebase and it's split across two repos, two branches. I know, I know — that's just like, your opinion, man — but here's how it all ties together.

## The Two Branches, Man

### Repo 1: The Framework (upstream — the rug that ties the room together)

**Repo**: [`llm-d-incubation/inference-payload-processor-rs`](https://github.com/llm-d-incubation/inference-payload-processor-rs)
**PR**: [#5](https://github.com/llm-d-incubation/inference-payload-processor-rs/pull/5) (pending merge to main)

```
rust/
├── crates/
│   ├── ext-proc-proto/     ← Envoy gRPC types (tonic-build)
│   ├── framework/          ← Plugin traits, CycleState, InferenceMessage
│   ├── plugins/            ← ALL generic plugins live here
│   │   └── src/
│   │       ├── body_field_to_header.rs
│   │       ├── api_translation/        ← All 5 provider translators
│   │       │   ├── anthropic.rs
│   │       │   ├── openai.rs
│   │       │   ├── azure_openai.rs
│   │       │   ├── vertex_openai.rs
│   │       │   ├── bedrock_openai.rs
│   │       │   └── api_translation_plugin.rs
│   │       └── apikey_injection/       ← Secret-based auth injection
│   │           ├── mod.rs
│   │           ├── secret_store.rs
│   │           ├── reconciler.rs
│   │           └── auth.rs
│   └── server/             ← tonic ext_proc gRPC server
│       └── src/
│           ├── main.rs
│           ├── ext_proc_handler.rs
│           ├── health.rs
│           └── metrics.rs
```

This is the generic stuff — framework, server, plugins that any inference gateway could use. No MaaS-specific code here. 96 tests passing.

### Repo 2: The Product (downstream — that's where your PRs go, Dude)

**Repo**: [`opendatahub-io/ai-gateway-payload-processing`](https://github.com/opendatahub-io/ai-gateway-payload-processing)
**Branch**: `feat/rust-downstream-plugins`

The clean downstream structure is in `rust-downstream/`:

```
rust-downstream/
├── Cargo.toml              ← imports upstream via git dependency
├── crates/
│   └── model-provider-resolver/   ← YOUR TERRITORY, Dude
│       └── src/
│           ├── lib.rs              ← plugin impl (watches ExternalModel CRD)
│           ├── model_store.rs      ← in-memory model cache
│           └── reconciler.rs       ← kube-rs watcher for maas.opendatahub.io CRDs
└── cmd/
    └── src/
        └── main.rs                 ← registers upstream + downstream plugins, starts server
```

This is the product-specific stuff — ExternalModel CRD from `maas.opendatahub.io`, NeMo guardrails, anything MaaS-specific. It imports the upstream framework + generic plugins as git dependencies.

The old `rust/` directory is also there with the full monolithic codebase from the POC. It still works but `rust-downstream/` is the clean structure going forward.

## How Dependencies Flow

*"Obviously you're not a golfer."*

```
Upstream (llm-d-incubation/inference-payload-processor-rs)
  ├── framework          (defines plugin traits)
  ├── plugins            (generic plugins: translation, auth)
  └── server             (tonic ext_proc server)
        ↑
        │ git dependency
        │
Downstream (opendatahub-io/ai-gateway-payload-processing)
  ├── model-provider-resolver  (product-specific: ExternalModel CRD)
  └── cmd/main.rs              (wires everything together, starts server)
```

The downstream `Cargo.toml` has:
```toml
[workspace.dependencies]
ipp-framework = { git = "https://github.com/noyitz/gateway-api-inference-extension.git", branch = "feature/rust-full-stack" }
ipp-plugins = { git = "https://github.com/noyitz/gateway-api-inference-extension.git", branch = "feature/rust-full-stack" }
ipp-server = { git = "https://github.com/noyitz/gateway-api-inference-extension.git", branch = "feature/rust-full-stack" }
```

## Where Your PRs Fit In, Dude

*"New shit has come to light, man."*

Your PRs introduce new CRDs (`ExternalProvider`, `ExternalModel` v2) and change how model-to-provider resolution works. Here's how they map to the Rust codebase:

### PR #182 — CRD types + generated artifacts
**Where it goes**: Downstream `model-provider-resolver/`
- The Rust reconciler currently watches `maas.opendatahub.io/v1alpha1/ExternalModel`
- Your new CRDs (`inference.opendatahub.io/v1alpha1`) need new types in the reconciler
- **File to modify**: `rust-downstream/crates/model-provider-resolver/src/reconciler.rs`
- Add the new GVK constants and update `run_external_model_watcher` to watch the new CRD

### PR #183 — Reconcilers + controller binary
**Where it goes**: Downstream `model-provider-resolver/`
- The Rust equivalent already has a reconciler (`reconciler.rs`) using `kube-rs::runtime::watcher`
- You'll need to update the store (`model_store.rs`) to handle the new CRD fields
- The Go controller binary is separate — in Rust, the reconciler runs inside the ext_proc server process

### PR #184 — BBR plugin integration
**Where it goes**: Downstream `model-provider-resolver/src/lib.rs`
- The plugin's `process_request` reads model info from the store and writes to CycleState
- Update `ExternalModelInfo` struct to include new fields from your CRD changes
- CycleState keys are in upstream `framework/src/state_keys.rs`

### PR #212 — apiFormat explicit translation
**Where it goes**: Upstream `plugins/src/api_translation/api_translation_plugin.rs`
- Currently the api-translation plugin reads `provider` from CycleState to pick a translator
- Your `apiFormat` field on `ExternalProviderRef` tells which translator to use per-model
- **Option A**: Add `apiFormat` to CycleState in model-provider-resolver, read it in api-translation
- **Option B**: Add a new CycleState key (e.g., `state_keys::API_FORMAT`) that overrides the provider-based translator selection

### PR #213 — Multi-provider weights
**Where it goes**: Downstream `model-provider-resolver/`
- The current store maps one model → one provider
- Multi-provider needs the store to return a weighted list
- Update `ExternalModelInfo` and `ModelInfoStore` in `model_store.rs`

### PR #207 — E2E test coverage
**Where it goes**: Downstream `tests/`
- Tool calling, multimodal, JSON mode tests
- Add to `rust-downstream/tests/` or extend the existing `e2e_plugin_chain.rs` in the old `rust/` dir

## How to Build and Test

*"The Dude abides."*

### Upstream (framework + generic plugins):
```bash
cd gateway-api-inference-extension/rust
cargo test --workspace    # 96 tests
cargo clippy --workspace  # should be clean
```

### Downstream (product-specific):
```bash
cd ai-gateway-payload-processing/rust-downstream
cargo test --workspace    # 11 tests
cargo clippy --workspace
```

### Full monolithic (old POC, still works):
```bash
cd ai-gateway-payload-processing/rust
cargo test --workspace    # 131 tests
```

### Deploy to cluster:
The Dockerfile in `docker/Dockerfile.rust` builds from the old `rust/` directory. For the new split structure, you'd need a new Dockerfile that builds `rust-downstream/cmd` and pulls upstream crates from git.

## Key Interfaces You'll Touch

### The Plugin Trait (upstream: `framework/src/plugin.rs`)
```rust
pub trait RequestProcessor: Send + Sync {
    fn name(&self) -> &str;
    fn process_request(
        &self,
        cycle_state: &mut CycleState,
        request: &mut InferenceRequest,
    ) -> Result<(), PluginError>;
}
```

### CycleState Keys (upstream: `framework/src/state_keys.rs`)
```rust
pub const PROVIDER: &str = "provider";
pub const MODEL: &str = "model";
pub const CREDS_REF_NAME: &str = "credential-ref-name";
pub const CREDS_REF_NAMESPACE: &str = "credential-ref-namespace";
```
If you need new keys (like `API_FORMAT`), add them here.

### ModelInfoStore (downstream: `model-provider-resolver/src/model_store.rs`)
```rust
pub struct ExternalModelInfo {
    pub provider: String,
    pub target_model: String,
    pub secret_name: String,
    pub secret_namespace: String,
    // Your new fields go here (api_format, provider_weights, etc.)
}
```

### Translator Trait (upstream: `plugins/src/api_translation/translator.rs`)
```rust
pub trait Translator: Send + Sync {
    fn translate_request(&self, body: &Value) -> Result<TranslateRequestResult, PluginError>;
    fn translate_response(&self, body: &mut Value, model: &str) -> Result<bool, PluginError>;
}
```

## What Noy is Working On Tomorrow

Noy is continuing production hardening:
- Wiring metrics recording into ext_proc_handler (the Prometheus counters exist but don't count anything yet)
- Adding retry logic to the kube-rs reconciler
- Tracking spawned task JoinHandles for proper lifecycle management
- Removing the unsafe block in field_stripper.rs

Don't touch `ext_proc_handler.rs`, `metrics.rs`, `health.rs`, or `reconciler.rs` — Noy's got those. Focus on the model-provider-resolver and CRD integration.

## Quick Reference

| What | Where | Repo |
|------|-------|------|
| Plugin traits | `crates/framework/` | `llm-d-incubation/inference-payload-processor-rs` |
| Generic plugins | `crates/plugins/` | `llm-d-incubation/inference-payload-processor-rs` |
| ext_proc server | `crates/server/` | `llm-d-incubation/inference-payload-processor-rs` |
| Model resolver | `rust-downstream/crates/model-provider-resolver/` | `opendatahub-io/ai-gateway-payload-processing` |
| Binary entrypoint | `rust-downstream/cmd/` | `opendatahub-io/ai-gateway-payload-processing` |
| Benchmark report | `BENCHMARK.md` | `llm-d-incubation/inference-payload-processor-rs` |

*"That's just like, your opinion, man. But the tests pass."*

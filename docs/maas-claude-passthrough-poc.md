# MaaS AI Coding Tools PoC — Design, Implementation & Operations Manual

## Overview

This PoC routes **Claude Code** and **OpenAI Codex** through the MaaS (Models as a Service) gateway on OpenShift, providing:
- **Multi-provider support** — Anthropic (Claude) and OpenAI (GPT/Codex) through the same gateway
- **Per-user API keys** — each developer gets their own MaaS key, works for both Claude Code and Codex
- **Centralized credential management** — real provider API keys live in K8s Secrets, users never see them
- **Durable usage tracking** — every request recorded in PostgreSQL (user, model, tokens, cost)
- **Grafana analytics dashboard** — per-user, per-model with cost calculations
- **Streaming support** — full SSE streaming passthrough with usage extraction (Anthropic + OpenAI formats)
- **Transparent model swapping** — admin can switch the backend model without users knowing

**Repos:**
- IPP plugins: `ai-gateway-payload-processing` branch `feature/maas-claude-passthrough-poc`
- Framework SSE fix: `llm-d/llm-d-inference-payload-processor` [PR #138](https://github.com/llm-d/llm-d-inference-payload-processor/pull/138)

**Interactive Flow Visualization:** https://noyitz.github.io/ai-gateway-docs/claude-passthrough/

---

## Architecture

```
┌─────────────┐
│ Claude Code  │  x-api-key: sk-oai-<user-key>
│ (Anthropic)  │──┐
└─────────────┘   │
                  │    Same MaaS API key
┌─────────────┐   │
│ Codex        │──┤   Authorization: Bearer sk-oai-<user-key>
│ (OpenAI)     │  │
└─────────────┘   │
                  ▼
           ┌──────────────────────────────────────────────┐
           │              OpenShift Cluster                │
           │                                              │
           │  Istio Gateway ──▶ Kuadrant Auth ──▶ IPP     │
           │                   (validates key,    │       │
           │                    injects username)  │       │
           │                                      ▼       │
           │                              Plugin Chain:   │
           │                              1. body-field-to-header
           │                              2. external-metering ──▶ Metering Svc ──▶ PostgreSQL
           │                              3. model-provider-resolver
           │                              4. apikey-injection (swaps to real API key)
           │                                    │   │     │
           │                          ┌─────────┘   └──┐  │
           │                          ▼                ▼  │
           │                   Anthropic API    OpenAI API │
           │                                              │
           │  Grafana ◀── PostgreSQL (durable analytics)  │
           └──────────────────────────────────────────────┘
```

---

## Code Changes on Top of GA Baseline

### New Plugins

| Plugin | File | Purpose |
|--------|------|---------|
| `external-metering` | `pkg/plugins/external-metering/plugin.go` | Sends CloudEvents to metering service for durable PostgreSQL storage. Pre-request balance check. Parses SSE streaming responses for token usage (both Anthropic and OpenAI formats). |

### New Components

| Component | Location | Purpose |
|-----------|----------|---------|
| Metering Service | `metering-service/` | Go HTTP server: receives CloudEvents at `/api/v1/events`, writes to PostgreSQL. Balance check at `/api/v1/customers/{user}/entitlements/{key}/value`. |
| PostgreSQL Schema | `metering-service/internal/storage/postgres.go` | `usage_events` table + `model_pricing` table with cost lookup. Auto-migrates on startup. |

### Changes to Existing Code

| File | Change |
|------|--------|
| `pkg/plugins/model-provider-resolver/plugin.go` | Allow `/v1/messages` (Anthropic) and `/v1/responses` (OpenAI Codex) paths — was hardcoded to `/chat/completions` only. Also: transparent model override with parameter stripping when `targetModel` differs from request. |
| `pkg/plugins/common/state/state-keys.go` | Added metering CycleState keys |
| `pkg/plugins/plugins.go` | Registered `external-metering` plugin |
| `go.mod` | Added `replace` directive to framework fork with SSE streaming fix |

### Upstream Framework Fix (PR to `llm-d/llm-d-inference-payload-processor`)

| File | Change |
|------|--------|
| `pkg/handlers/response.go` | Parse SSE streaming responses — supports both Anthropic format (`usage` at top level) and OpenAI Responses API format (`usage` nested in `response.completed` event). Always respond to response headers so Envoy proceeds with body chunks. |
| `pkg/handlers/server.go` | Send immediate ack for non-EoS response body chunks so Envoy continues forwarding (was blocking after first chunk). |

---

## Cluster Configuration Changes

### IPP Deployment

| Setting | Value |
|---------|-------|
| Image | Built via OCP binary build to internal registry |
| Plugin chain | `body-field-to-header` → `external-metering` → `model-provider-resolver` → `apikey-injection` |
| Flags | `--streaming` |
| Removed | `api-translation` (passthrough mode — no request/response format translation) |

### EnvoyFilter

| Setting | Value |
|---------|-------|
| `cluster_name` | `payload-processing.openshift-ingress.svc.cluster.local` |
| `request_body_mode` | `FULL_DUPLEX_STREAMED` |
| `response_body_mode` | `STREAMED` |

### Kubernetes Resources

| Resource | Namespace | Purpose |
|----------|-----------|---------|
| ExternalModel `ext-claude-sonnet` | `llm` | provider: anthropic, targetModel: claude-opus-4-6, endpoint: api.anthropic.com |
| ExternalModel `ext-openai` | `llm` | provider: openai, targetModel: gpt-5.5, endpoint: api.openai.com |
| Secret `anthropic-api-key` | `llm` | Real Anthropic API key (labeled `bbr-managed`) |
| Secret `openai-api-key` | `llm` | Real OpenAI API key (labeled `bbr-managed`) |
| MaaSModelRef (per model) | `llm` | Links ExternalModel to MaaS |
| HTTPRoute (per model) | `llm` | URLRewrite (`ReplacePrefixMatch: /`): strips path prefix so provider APIs receive correct paths |
| MaaS Subscription | `models-as-a-service` | All models added to modelRefs |
| MaaS AuthPolicy | `models-as-a-service` | All models added to modelRefs |
| StatefulSet `metering-postgresql` | `openshift-ingress` | PostgreSQL 16, 1Gi PVC |
| Deployment `metering-service` | `openshift-ingress` | Go HTTP metering service |
| Deployment `grafana` | `openshift-ingress` | Analytics dashboard (queries PostgreSQL) |

### AuthConfig Patches (Authorino)

| Change | Why |
|--------|-----|
| Added `api-keys-via-xapikey` auth method | Claude Code sends key in `x-api-key` header, not `Authorization: Bearer` |
| Updated `apiKeyValidation` expression | Extract key from `x-api-key` or `Authorization` header |
| Added `X-MaaS-Username` response header | Injects username into request for IPP to read (all models) |
| MaaS controller + Kuadrant operator scaled to 0 | Prevent overwriting manual AuthConfig patches |

### Workarounds (PoC only)

| Workaround | Production Fix |
|------------|---------------|
| Operators scaled to 0 | MaaS AuthPolicy must support `x-api-key` header natively |
| Manual AuthConfig patches | Upstream MaaS controller change |
| Framework fork for SSE | [PR #138](https://github.com/llm-d/llm-d-inference-payload-processor/pull/138) to upstream |
| External metering + PostgreSQL | Can be replaced by native Limitador metrics with Tenant telemetry enabled (requires clean MaaS install with Kuadrant 1.4.2+) |

---

## User Setup Guide

### Claude Code

Open a **new terminal tab** (don't modify your existing config):

```bash
export ANTHROPIC_BASE_URL=<MAAS_CLAUDE_ENDPOINT_URL>
export ANTHROPIC_API_KEY=<YOUR_MAAS_API_KEY>
export NODE_TLS_REJECT_UNAUTHORIZED=0
unset CLAUDE_CODE_USE_VERTEX
unset ANTHROPIC_VERTEX_PROJECT_ID
claude
```

**To revert:** Close the tab. Or explicitly:
```bash
export CLAUDE_CODE_USE_VERTEX=1
export ANTHROPIC_VERTEX_PROJECT_ID=<your-project>
unset ANTHROPIC_BASE_URL
unset ANTHROPIC_API_KEY
claude
```

**How to verify you're back to normal:** When Claude Code starts, the status bar should NOT show "API Usage Billing".

### OpenAI Codex

**Install:**
```bash
npm install -g @openai/codex
```

**Configure `~/.codex/config.toml`:**
```toml
model = "gpt-5.5"
model_provider = "maas"

[model_providers.maas]
name = "MaaS Gateway"
base_url = "<MAAS_OPENAI_ENDPOINT_URL>/v1"
wire_api = "responses"
env_key = "MAAS_API_KEY"
```

**Run (new terminal tab):**
```bash
export MAAS_API_KEY=<YOUR_MAAS_API_KEY>
export NODE_TLS_REJECT_UNAUTHORIZED=0
codex
```

**To revert:** Remove or comment out the `model_provider` and `[model_providers.maas]` lines from `~/.codex/config.toml`.

### Testing with curl

**Anthropic (Claude):**
```bash
curl -s <MAAS_CLAUDE_ENDPOINT_URL>/v1/messages \
  --insecure \
  -H "Content-Type: application/json" \
  -H "anthropic-version: 2023-06-01" \
  -H "x-api-key: <YOUR_MAAS_API_KEY>" \
  -d '{"model": "claude-opus-4-6", "max_tokens": 100,
       "messages": [{"role": "user", "content": "What is MaaS?"}]}'
```

**OpenAI (Codex/GPT):**
```bash
curl -s <MAAS_OPENAI_ENDPOINT_URL>/v1/responses \
  --insecure \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <YOUR_MAAS_API_KEY>" \
  -d '{"model": "gpt-5.5", "input": "What is MaaS?"}'
```

---

## Admin Operations Guide

### Adding a New User

**In production**, users would authenticate to MaaS via the gateway (OpenShift SSO) and mint their own API key through a self-service portal or CLI call to `POST /v1/api-keys`. No admin intervention needed.

**In this PoC**, the MaaS API is only accessible internally (no external route), so an admin creates keys on behalf of users:

```bash
oc exec -n redhat-ods-applications deployment/maas-api -- curl -sk \
  -X POST "https://localhost:8443/v1/api-keys" \
  -H "Content-Type: application/json" \
  -H "X-MaaS-Username: <new-username>" \
  -H 'X-MaaS-Group: ["system:authenticated"]' \
  -d '{"name": "<new-username>-claude"}'
```

Response includes the API key (shown only once):
```json
{
  "key": "sk-oai-...",
  "name": "<new-username>-claude",
  "subscription": "external-models-subscription",
  "expiresAt": "..."
}
```

Send the key to the user securely. The same key works for both Claude Code and Codex. No other cluster changes needed.

### Revoking a User's Key

```bash
oc exec -n redhat-ods-applications deployment/maas-api -- curl -sk \
  -X DELETE "https://localhost:8443/v1/api-keys/<key-id>" \
  -H "X-MaaS-Username: <username>" \
  -H 'X-MaaS-Group: ["system:authenticated"]'
```

### Transparent Model Swapping

An admin can change the backend model without users knowing:

```bash
# Swap Claude users to Sonnet (cheaper)
oc patch externalmodel ext-claude-sonnet -n llm --type=merge \
  -p '{"spec":{"targetModel":"claude-sonnet-4-20250514"}}'

# Swap back to Opus
oc patch externalmodel ext-claude-sonnet -n llm --type=merge \
  -p '{"spec":{"targetModel":"claude-opus-4-6"}}'
```

The model-provider-resolver automatically rewrites the model field in the request body and strips incompatible parameters (`effort`, `thinking`, `output_config`) when overriding.

### Viewing Usage Data

**Grafana Dashboard:** Browse to the Grafana route URL → "MaaS Usage Analytics" dashboard.

Panels:
- **Company Overview:** Total requests, tokens, estimated cost ($), active users
- **User Breakdown:** Top 10 users, user spend over time, sortable user summary table
- **Model Breakdown:** Usage by model (pie), cost comparison, tokens over time
- **Detailed Log:** Full event table with timestamp, user, model, tokens, cost

### Updating Model Pricing

```bash
oc exec metering-postgresql-0 -n openshift-ingress -- \
  psql -U metering -c "
    INSERT INTO model_pricing (model, provider, prompt_cost_per_1k, completion_cost_per_1k)
    VALUES ('new-model-id', 'provider', 0.003, 0.015)
    ON CONFLICT (model) DO UPDATE SET
      prompt_cost_per_1k = EXCLUDED.prompt_cost_per_1k,
      completion_cost_per_1k = EXCLUDED.completion_cost_per_1k"
```

### Restoring Cluster After Demo

```bash
# Scale operators back up
oc scale deployment maas-controller -n redhat-ods-applications --replicas=1
oc scale deployment kuadrant-operator-controller-manager -n kuadrant-system --replicas=1
```

---

## Known Limitations

1. **Streaming token counts** — SSE streaming responses are parsed for usage data via a framework fix ([PR #138](https://github.com/llm-d/llm-d-inference-payload-processor/pull/138)). Supports both Anthropic (`message_delta` events) and OpenAI Responses API (`response.completed` events). Response body chunks are accumulated in-memory; bounded by `max_tokens`.

2. **AuthConfig patches** — The `x-api-key` auth method and `X-MaaS-Username` header injection are manual AuthConfig patches applied to ALL model AuthConfigs. MaaS controller and Kuadrant operator must be scaled to 0 to prevent overwriting. Production requires upstream MaaS controller changes.

3. **External metering stack** — The PostgreSQL + metering service can be eliminated once native Limitador metrics with per-user labels are available (requires Tenant telemetry feature on a clean MaaS install with Kuadrant 1.4.2+).

4. **Intermittent 503s** — TLS connection resets to provider APIs via Istio occasionally cause 503 errors. Both Claude Code and Codex have built-in retry that handles this transparently.

5. **Kuadrant Wasm SSE bug** — `authorized_hits` (token-level) metric not emitted for SSE streaming responses. The Wasm plugin can't parse SSE to extract `usage.total_tokens`. Bug filed: https://github.com/Kuadrant/wasm-shim/issues/373. Workaround: external-metering IPP plugin with SSE parsing.

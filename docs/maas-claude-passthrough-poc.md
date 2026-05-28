# MaaS Claude Passthrough PoC — Design, Implementation & Operations Manual

## Overview

This PoC routes Claude Code through the MaaS (Models as a Service) gateway on OpenShift, providing:
- **Per-user API keys** — each developer gets their own MaaS key
- **Centralized credential management** — the real Anthropic API key lives in a K8s Secret, users never see it
- **Durable usage tracking** — every request recorded in PostgreSQL (user, model, tokens, cost)
- **Grafana analytics dashboard** — per-org, per-user, per-model with cost calculations
- **Streaming support** — full SSE streaming passthrough with usage extraction
- **Prometheus real-time metrics** — live counters for operational monitoring

**Repos:**
- IPP plugins: `ai-gateway-payload-processing` branch `feature/maas-claude-passthrough-poc`
- Framework SSE fix: `llm-d/llm-d-inference-payload-processor` [PR #138](https://github.com/llm-d/llm-d-inference-payload-processor/pull/138)

---

## Architecture

```
┌─────────────┐
│ Claude Code  │  x-api-key: sk-oai-<user-key>
│ (User A/B)   │──────────────────────────────┐
└─────────────┘                               │
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
           │                              5. usage-tracking ──▶ Prometheus
           │                                      │       │
           │                                      ▼       │
           │                              Anthropic API   │
           │                              (api.anthropic.com)
           │                                              │
           │  Grafana ◀── PostgreSQL (durable analytics)  │
           │  Grafana ◀── Prometheus (real-time counters)  │
           └──────────────────────────────────────────────┘
```

---

## Code Changes on Top of GA Baseline

### New Plugins

| Plugin | File | Purpose |
|--------|------|---------|
| `usage-tracking` | `pkg/plugins/usage-tracking/plugin.go` | Prometheus counters (requests, prompt_tokens, completion_tokens) by provider/model/user. Reads `X-MaaS-Username` header. Strips `accept-encoding` to prevent gzip responses. |
| `external-metering` | `pkg/plugins/external-metering/plugin.go` | Sends CloudEvents to metering service for durable PostgreSQL storage. Pre-request balance check. Skips `stream_options` injection for Anthropic format. |

### New Components

| Component | Location | Purpose |
|-----------|----------|---------|
| Metering Service | `metering-service/` | Go HTTP server: receives CloudEvents at `/api/v1/events`, writes to PostgreSQL. Balance check at `/api/v1/customers/{user}/entitlements/{key}/value`. |
| PostgreSQL Schema | `metering-service/internal/storage/postgres.go` | `usage_events` table + `model_pricing` table with cost lookup. Auto-migrates on startup. |

### Changes to Existing Code

| File | Change |
|------|--------|
| `pkg/plugins/model-provider-resolver/plugin.go` | Allow `/v1/messages` path (was hardcoded to `/chat/completions` only) |
| `pkg/plugins/common/state/state-keys.go` | Added metering CycleState keys |
| `pkg/plugins/plugins.go` | Registered `usage-tracking` and `external-metering` plugins |
| `cmd/main.go` | Wired `WithCustomCollectors` for Prometheus export |
| `go.mod` | Added `replace` directive to framework fork with SSE streaming fix |

### Upstream Framework Fix (PR to `llm-d/llm-d-inference-payload-processor`)

| File | Change |
|------|--------|
| `pkg/handlers/response.go` | Parse SSE streaming responses (extract usage from `data:` lines). Always respond to response headers so Envoy proceeds with body chunks. |
| `pkg/handlers/server.go` | Send immediate ack for non-EoS response body chunks so Envoy continues forwarding (was blocking after first chunk). |

---

## Cluster Configuration Changes

### IPP Deployment

| Setting | Value |
|---------|-------|
| Image | Built via OCP binary build to internal registry |
| Plugin chain | `body-field-to-header` → `external-metering` → `model-provider-resolver` → `apikey-injection` → `usage-tracking` |
| Flags | `--streaming`, `--metrics-endpoint-auth=false`, `--tracing=false` |
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
| ExternalModel `ext-claude-sonnet` | `llm` | provider: anthropic, targetModel: claude-opus-4-6 |
| Secret `anthropic-api-key` | `llm` | Real Anthropic API key (labeled `bbr-managed`) |
| MaaSModelRef `ext-claude-sonnet` | `llm` | Links ExternalModel to MaaS |
| HTTPRoute `ext-claude-sonnet` | `llm` | URLRewrite: strips `/llm/ext-claude-sonnet` prefix |
| MaaS Subscription | `models-as-a-service` | `ext-claude-sonnet` added to modelRefs |
| MaaS AuthPolicy | `models-as-a-service` | `ext-claude-sonnet` added to modelRefs |
| Service `payload-processing` | `openshift-ingress` | Added `metrics` port 9090 |
| StatefulSet `metering-postgresql` | `openshift-ingress` | PostgreSQL 16, 1Gi PVC |
| Deployment `metering-service` | `openshift-ingress` | Go HTTP metering service |
| Deployment `prometheus` | `openshift-ingress` | Scrapes IPP metrics |
| Deployment `grafana` | `openshift-ingress` | Analytics dashboard |

### AuthConfig Patches (Authorino)

| Change | Why |
|--------|-----|
| Added `api-keys-via-xapikey` auth method | Claude Code sends key in `x-api-key` header, not `Authorization: Bearer` |
| Updated `apiKeyValidation` expression | Extract key from `x-api-key` when present |
| Added `X-MaaS-Username` response header | Injects username into request for IPP to read |
| MaaS controller + Kuadrant operator scaled to 0 | Prevent overwriting manual AuthConfig patches |

### Workarounds (PoC only)

| Workaround | Production Fix |
|------------|---------------|
| Operators scaled to 0 | MaaS AuthPolicy must support `x-api-key` header natively |
| Manual AuthConfig patches | Upstream MaaS controller change |
| Framework fork for SSE | [PR #138](https://github.com/llm-d/llm-d-inference-payload-processor/pull/138) to upstream |
| `metrics-endpoint-auth=false` | Use ServiceMonitor with RBAC |

---

## User Setup Guide

### For End Users (Developers)

You need: a MaaS API key (provided by your admin) and the MaaS endpoint URL.

**Step 1:** Open a **new terminal tab** (don't modify your existing config).

**Step 2:** Set environment variables:
```bash
export ANTHROPIC_BASE_URL=<MAAS_ENDPOINT_URL>
export ANTHROPIC_API_KEY=<YOUR_MAAS_API_KEY>
export NODE_TLS_REJECT_UNAUTHORIZED=0
unset CLAUDE_CODE_USE_VERTEX
unset ANTHROPIC_VERTEX_PROJECT_ID
claude
```

**Step 3:** Use Claude Code normally. All requests route through MaaS.

### Reverting to Normal Claude (IMPORTANT)

**After testing, you MUST revert.** The MaaS PoC uses a shared Anthropic API key — do not leave Claude Code pointed at MaaS for regular work.

**Option A (easiest):** Close the MaaS terminal tab. Open a fresh terminal — your shell profile already has the Vertex config.

**Option B (explicit):** In the same terminal:
```bash
export CLAUDE_CODE_USE_VERTEX=1
export ANTHROPIC_VERTEX_PROJECT_ID=<your-project>
unset ANTHROPIC_BASE_URL
unset ANTHROPIC_API_KEY
claude
```

**How to verify you're back to normal:** When Claude Code starts, the status bar should NOT show "API Usage Billing". If it does, you're still on MaaS — close and try again.

### Testing with curl

```bash
curl -s <MAAS_ENDPOINT_URL>/v1/messages \
  --insecure \
  -H "Content-Type: application/json" \
  -H "anthropic-version: 2023-06-01" \
  -H "x-api-key: <YOUR_MAAS_API_KEY>" \
  -d '{
    "model": "claude-opus-4-6",
    "max_tokens": 100,
    "messages": [{"role": "user", "content": "What is MaaS?"}]
  }'
```

---

## Admin Operations Guide

### Adding a New User

**Step 1:** Create a MaaS API key for the user:

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

**Step 2:** Send the key to the user securely (not via email/Slack in plaintext).

**Step 3:** No other cluster changes needed — the key automatically has access to all models in the subscription.

### Revoking a User's Key

```bash
oc exec -n redhat-ods-applications deployment/maas-api -- curl -sk \
  -X DELETE "https://localhost:8443/v1/api-keys/<key-id>" \
  -H "X-MaaS-Username: <username>" \
  -H 'X-MaaS-Group: ["system:authenticated"]'
```

### Viewing Usage Data

**Grafana Dashboard:** Browse to the Grafana route URL → "MaaS Usage Analytics" dashboard.

Panels:
- **Company Overview:** Total requests, tokens, estimated cost ($), active users
- **Organization Breakdown:** Top 10 orgs by token usage, org spend over time
- **User Breakdown:** Top 10 users, user spend over time, sortable user summary table
- **Model Breakdown:** Usage by model (pie), cost comparison, tokens over time
- **Detailed Log:** Full event table with timestamp, user, model, tokens, cost

**PostgreSQL direct query:**
```bash
oc exec metering-postgresql-0 -n openshift-ingress -- \
  psql -U metering -c "
    SELECT e.username, COUNT(*) as requests,
      SUM(e.total_tokens) as tokens,
      ROUND(SUM(e.prompt_tokens * COALESCE(p.prompt_cost_per_1k, 0.003)/1000.0 +
        e.completion_tokens * COALESCE(p.completion_cost_per_1k, 0.015)/1000.0)::numeric, 4) as cost_usd
    FROM usage_events e LEFT JOIN model_pricing p ON e.model = p.model
    GROUP BY e.username ORDER BY cost_usd DESC"
```

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

1. **Streaming token counts** — SSE streaming responses are parsed for usage data via a framework fix ([PR #138](https://github.com/llm-d/llm-d-inference-payload-processor/pull/138)). Response body chunks are accumulated in-memory for parsing; bounded by `max_tokens` (typically <100KB).

2. **AuthConfig patches** — The `x-api-key` auth method and `X-MaaS-Username` header injection are manual AuthConfig patches. MaaS controller and Kuadrant operator must be scaled to 0 to prevent overwriting. Production requires upstream MaaS controller changes.

3. **Intermittent 503s** — TLS connection resets to `api.anthropic.com` via Istio occasionally cause 503 errors. Claude Code's built-in retry handles this transparently.

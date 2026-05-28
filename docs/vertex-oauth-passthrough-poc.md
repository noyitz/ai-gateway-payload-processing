# MaaS Claude Passthrough PoC — Design & Manual

## Overview

This PoC demonstrates Claude Code (Anthropic's CLI) routing through the MaaS (Models as a Service) gateway on OpenShift, with per-user API keys, centralized credential management, and token usage tracking via Prometheus/Grafana.

**Branch:** `feature/vertex-oauth-passthrough` in `ai-gateway-payload-processing`

## Architecture

```
┌─────────────┐     ┌──────────────────────────────────────────────────┐
│ Claude Code  │     │              OpenShift Cluster                   │
│ (User A)     │────▶│                                                  │
│              │     │  ┌────────────┐  ┌──────────┐  ┌─────────────┐  │     ┌──────────────┐
│ Claude Code  │     │  │   Istio    │  │ Kuadrant │  │    IPP      │  │     │  Anthropic   │
│ (User B)     │────▶│  │  Gateway   │─▶│  Auth +  │─▶│ (ext_proc)  │──│────▶│    API       │
│              │     │  │            │  │ Rate Lim │  │             │  │     │              │
└─────────────┘     │  └────────────┘  └──────────┘  └─────────────┘  │     └──────────────┘
                    │                                  │               │
                    │                         ┌────────┴────────┐      │
                    │                         │  IPP Plugins    │      │
                    │                         │                 │      │
                    │                         │ • model-resolver│      │
                    │                         │ • apikey-inject │      │
                    │                         │ • usage-tracking│      │
                    │                         └─────────────────┘      │
                    │                                                  │
                    │  ┌──────────────┐  ┌──────────────┐             │
                    │  │  Prometheus  │  │   Grafana    │             │
                    │  │  (scrapes    │  │  (dashboard) │             │
                    │  │   IPP :9090) │  │              │             │
                    │  └──────────────┘  └──────────────┘             │
                    └──────────────────────────────────────────────────┘
```

### Request Flow

1. **Claude Code** sends Anthropic Messages API request (`POST /v1/messages`) with MaaS API key in `x-api-key` header
2. **Istio Gateway** receives request, matches HTTPRoute with URL rewrite (strips path prefix)
3. **Kuadrant (Authorino)** validates the MaaS API key via MaaS API, injects `X-MaaS-Username` header
4. **IPP ext_proc** processes the request through the plugin chain:
   - `body-field-to-header` — extracts model name from body
   - `model-provider-resolver` — resolves ExternalModel, writes provider/credentials to CycleState
   - `apikey-injection` — swaps MaaS API key for real Anthropic API key from K8s Secret
   - `usage-tracking` — captures user identity, strips accept-encoding
5. **Request forwards** to `api.anthropic.com` with real Anthropic API key
6. **Response returns** through IPP — usage-tracking plugin reads token counts and emits Prometheus metrics
7. **Grafana** displays per-user token usage from Prometheus

### Passthrough Mode

In this PoC, the `api-translation` plugin is **not used**. The request and response bodies pass through IPP unchanged in native Anthropic format. This is what enables Claude Code to work directly — it sends Anthropic Messages API format, and the backend is the Anthropic API.

## Code Changes (branch: `feature/vertex-oauth-passthrough`)

### Commits

| Commit | Description |
|--------|-------------|
| `2503339` | OAuthAuthGenerator for GCP Vertex AI token exchange |
| `522a083` | Register vertex-oauth provider + usage-tracking plugin |
| `b118ef3` | Anthropic passthrough support (/v1/messages path) + per-user tracking |
| `f350280` | Fix X-MaaS-Username header reading + strip accept-encoding |

### Files Changed

| File | Change |
|------|--------|
| `pkg/plugins/common/provider/provider.go` | Added `VertexOAuth = "vertex-oauth"` constant |
| `pkg/plugins/apikey-injection/plugin.go` | Registered `OAuthAuthGenerator` for vertex-oauth provider |
| `pkg/plugins/apikey-injection/auth/oauth_auth_generator.go` | New: OAuth token exchange from GCP SA key |
| `pkg/plugins/apikey-injection/auth/oauth_auth_generator_test.go` | New: unit tests |
| `pkg/plugins/api-translation/plugin.go` | Registered vertex-oauth with VertexOpenAI translator |
| `pkg/plugins/model-provider-resolver/plugin.go` | Allow `/v1/messages` path (was hardcoded to `/chat/completions`) |
| `pkg/plugins/usage-tracking/plugin.go` | New: Prometheus counters (requests, prompt tokens, completion tokens) by provider/model/user |
| `pkg/plugins/usage-tracking/plugin_test.go` | New: unit tests |
| `pkg/plugins/plugins.go` | Registered usage-tracking plugin |
| `cmd/main.go` | Wired `WithCustomCollectors` for Prometheus metrics export |

## Cluster Configuration

### Prerequisites

- OpenShift cluster with Istio, Kuadrant (Authorino + Limitador), MaaS controller
- `maas.opendatahub.io` ExternalModel CRD installed

### Kubernetes Resources Created

#### ExternalModel
```yaml
apiVersion: maas.opendatahub.io/v1alpha1
kind: ExternalModel
metadata:
  name: ext-claude-sonnet
  namespace: llm
spec:
  provider: anthropic
  targetModel: claude-opus-4-6         # Must match what Claude Code sends
  endpoint: api.anthropic.com
  credentialRef:
    name: anthropic-api-key            # Secret with real Anthropic API key
```

#### Secret (Anthropic API Key)
```yaml
apiVersion: v1
kind: Secret
metadata:
  name: anthropic-api-key
  namespace: llm
  labels:
    inference.networking.k8s.io/bbr-managed: "true"
type: Opaque
stringData:
  api-key: "<ANTHROPIC_API_KEY>"       # Real Anthropic API key (sk-ant-...)
```

#### MaaSModelRef
```yaml
apiVersion: maas.opendatahub.io/v1alpha1
kind: MaaSModelRef
metadata:
  name: ext-claude-sonnet
  namespace: llm
spec:
  modelRef:
    kind: ExternalModel
    name: ext-claude-sonnet
```

#### MaaS API Keys
Created via MaaS API internal endpoint for each user:
```bash
oc exec -n redhat-ods-applications deployment/maas-api -- curl -sk \
  -X POST "https://localhost:8443/v1/api-keys" \
  -H "Content-Type: application/json" \
  -H "X-MaaS-Username: <username>" \
  -H 'X-MaaS-Group: ["system:authenticated"]' \
  -d '{"name": "<username>-claude-demo"}'
```

### Cluster Config Changes

#### 1. EnvoyFilter (`payload-processing` in openshift-ingress)
- **cluster_name**: Changed to `payload-processing.openshift-ingress.svc.cluster.local`
- **processing_mode**: Changed to `FULL_DUPLEX_STREAMED` for both request and response body

#### 2. IPP Deployment (`payload-processing` in openshift-ingress)
- **Image**: Built from branch, pushed via OCP binary build to internal registry
- **Plugin chain** (passthrough mode — no api-translation):
  ```
  --plugin body-field-to-header:model-extractor:{...}
  --plugin model-provider-resolver:model-provider-resolver
  --plugin apikey-injection:apikey-injection
  --plugin usage-tracking:usage-tracking
  ```
- **Flags**: `--streaming`, `--metrics-endpoint-auth=false`

#### 3. Service (`payload-processing`)
- Added `metrics` port (9090) alongside `grpc` port (9004)

#### 4. HTTPRoute (`ext-claude-sonnet`)
- Added `URLRewrite` filter: `ReplacePrefixMatch: /` to strip `/llm/ext-claude-sonnet` prefix

#### 5. MaaS Subscription
- Added `ext-claude-sonnet` to `spec.modelRefs`

#### 6. MaaS AuthPolicy
- Added `ext-claude-sonnet` to `spec.modelRefs`

#### 7. Authorino AuthConfig (4 AuthConfigs in kuadrant-system)
- Added `api-keys-via-xapikey` authentication method (Claude Code sends key in `x-api-key` header, not `Authorization: Bearer`)
- Updated `apiKeyValidation` metadata to extract key from `x-api-key` header
- Added `X-MaaS-Username` response header injection from API key validation

#### 8. Prometheus + Grafana (standalone, in openshift-ingress)
- Prometheus: scrapes `payload-processing:9090/metrics` every 10s
- Grafana: dashboard "MaaS Claude Usage Dashboard" with per-user breakdown

### Workarounds (PoC only, not for production)

| Workaround | Reason | Production Fix |
|------------|--------|---------------|
| Anthropic API key instead of Vertex AI SA | GCP org policy blocks SA key creation | Obtain SA key from team admin or use Workload Identity Federation |
| MaaS controller scaled to 0 | Overwrites AuthConfig patches | Add `x-api-key` support to MaaS controller's AuthPolicy template |
| Kuadrant operator scaled to 0 | Overwrites AuthConfig patches | Same as above |
| Manual AuthConfig patches for `x-api-key` | MaaS AuthPolicy only checks `Authorization` header | Upstream fix in MaaS controller |
| `accept-encoding` stripped by IPP | Compressed responses can't be parsed by BBR framework | Framework fix to handle gzip/br decompression |
| Streaming responses skip usage tracking | BBR framework can't parse SSE as JSON | Framework fix for SSE response parsing, or aggregate from Anthropic usage API |
| `metrics-endpoint-auth=false` | Standalone Prometheus can't authenticate | Use ServiceMonitor with proper RBAC |

## Testing Manual

### Prerequisites
- Access to the OpenShift cluster (sandbox2228)
- Claude Code v2.1.153+ installed

### Setting Up Claude Code via MaaS

**User A (in a new terminal tab):**
```bash
export ANTHROPIC_BASE_URL=https://maas.apps.ocp.4fnz2.sandbox2228.opentlc.com/llm/ext-claude-sonnet
export ANTHROPIC_API_KEY=<USER_A_MAAS_API_KEY>
export NODE_TLS_REJECT_UNAUTHORIZED=0
unset CLAUDE_CODE_USE_VERTEX
unset ANTHROPIC_VERTEX_PROJECT_ID
claude
```

**User B (in a new terminal tab):**
```bash
export ANTHROPIC_BASE_URL=https://maas.apps.ocp.4fnz2.sandbox2228.opentlc.com/llm/ext-claude-sonnet
export ANTHROPIC_API_KEY=<USER_B_MAAS_API_KEY>
export NODE_TLS_REJECT_UNAUTHORIZED=0
unset CLAUDE_CODE_USE_VERTEX
unset ANTHROPIC_VERTEX_PROJECT_ID
claude
```

### Testing with curl

```bash
curl -s \
  "https://maas.apps.ocp.4fnz2.sandbox2228.opentlc.com/llm/ext-claude-sonnet/v1/messages" \
  --insecure \
  -H "Content-Type: application/json" \
  -H "anthropic-version: 2023-06-01" \
  -H "x-api-key: <MAAS_API_KEY>" \
  -d '{
    "model": "claude-opus-4-6",
    "max_tokens": 100,
    "messages": [{"role": "user", "content": "What is MaaS?"}]
  }'
```

### Viewing Usage in Grafana

**Dashboard URL:** `https://grafana-openshift-ingress.apps.ocp.4fnz2.sandbox2228.opentlc.com/d/dfnds1uockh6oe/maas-claude-usage-dashboard`

Panels:
- **Requests by User** — time series of request rate per user
- **Total Requests per User** — stat counters
- **Prompt Tokens by User** — input token consumption over time
- **Completion Tokens by User** — output token consumption over time
- **Total Token Usage** — bar gauge of total tokens per user

**Note:** Streaming requests (default for Claude Code) are tracked for request count but token usage is only captured for non-streaming responses. Non-streaming curl requests will show full token breakdowns.

### Rolling Back to Vertex AI

Close the MaaS terminal tab. In a fresh terminal:
```bash
export CLAUDE_CODE_USE_VERTEX=1
export ANTHROPIC_VERTEX_PROJECT_ID=itpc-gcp-ai-eng-claude
unset ANTHROPIC_BASE_URL
unset ANTHROPIC_API_KEY
claude
```

Or simply open a new terminal — your shell profile already has the Vertex config.

### Restoring Cluster Operators

After the demo, restore the scaled-down operators:
```bash
oc scale deployment maas-controller -n redhat-ods-applications --replicas=1
oc scale deployment kuadrant-operator-controller-manager -n kuadrant-system --replicas=1
```

## Known Limitations

1. **Streaming responses** — Claude Code uses streaming by default. The BBR framework can't parse SSE responses, so usage-tracking only captures token counts for non-streaming requests. Request counts are tracked for all requests.

2. **User identity for streaming** — The `X-MaaS-Username` header is correctly injected and read by the usage-tracking plugin during request processing. Token counts require response parsing which fails for streaming.

3. **Vertex AI OAuth** — The OAuthAuthGenerator code is ready but untested E2E. Requires a GCP service account key for `itpc-gcp-ai-eng-claude` project (org policy blocks creation, existing keys need to be obtained from team admin).

4. **AuthConfig patches** — The `x-api-key` authentication support and `X-MaaS-Username` header injection are manual patches on the Authorino AuthConfig. The MaaS controller and Kuadrant operator must be scaled to 0 to prevent overwriting. Production fix requires upstream changes to the MaaS AuthPolicy template.

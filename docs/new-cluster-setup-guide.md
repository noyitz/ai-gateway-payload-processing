# MaaS Claude/Codex Passthrough — New Cluster Setup Guide

This guide is for setting up the MaaS passthrough PoC on a fresh cluster after MaaS is deployed.

## Prerequisites

- MaaS deployed and running (maas-controller, maas-api, gateway, Kuadrant/RHCL)
- `oc` access as admin

## Branch

```bash
git clone https://github.com/noyitz/ai-gateway-payload-processing.git
cd ai-gateway-payload-processing
git checkout feature/maas-claude-passthrough-poc
```

## Step 1: Build and Deploy IPP (Payload Processing)

The IPP binary needs our custom plugins. Build and deploy via OCP binary build:

```bash
# Create BuildConfig
oc new-build --binary --name=payload-processing-test --strategy=docker -n <gateway-namespace>

# Build (only sends the needed files, not the full repo)
rm -rf /tmp/pp-build-src && mkdir -p /tmp/pp-build-src
cp Dockerfile go.mod go.sum /tmp/pp-build-src/
cp -r api cmd pkg /tmp/pp-build-src/
oc start-build payload-processing-test --from-dir=/tmp/pp-build-src --follow -n <gateway-namespace>
```

### IPP Deployment Args (passthrough mode)

The deployment should have these args (no api-translation — passthrough mode):

```
--streaming
--plugin body-field-to-header:model-extractor:{"fieldName":"model","headerName":"X-Gateway-Model-Name"}
--plugin external-metering:metering-check:{"meteringURL":"http://metering-service.<gateway-namespace>.svc:8080","timeoutSeconds":5,"featureKey":"inference-tokens","source":"maas-gateway","failOpen":true}
--plugin model-provider-resolver:model-provider-resolver
--plugin apikey-injection:apikey-injection
```

## Step 2: EnvoyFilter

The EnvoyFilter must point to the IPP service with streaming mode:

```yaml
cluster_name: payload-processing.<gateway-namespace>.svc.cluster.local
processing_mode:
  request_body_mode: FULL_DUPLEX_STREAMED
  response_body_mode: STREAMED
  request_header_mode: SEND
  response_header_mode: SEND
```

## Step 3: Create ExternalModels

### Anthropic (Claude Code)
```bash
# Secret with Anthropic API key
oc create secret generic anthropic-api-key \
  --from-literal=api-key=<ANTHROPIC_API_KEY> \
  -n llm --dry-run=client -o yaml | \
  oc label --local -f - inference.networking.k8s.io/bbr-managed=true -o yaml --dry-run=client | \
  oc apply -f -

# ExternalModel
cat <<EOF | oc apply -f -
apiVersion: maas.opendatahub.io/v1alpha1
kind: ExternalModel
metadata:
  name: ext-claude-sonnet
  namespace: llm
spec:
  provider: anthropic
  targetModel: claude-opus-4-6
  endpoint: api.anthropic.com
  credentialRef:
    name: anthropic-api-key
EOF
```

### OpenAI (Codex)
```bash
# Secret with OpenAI API key
oc create secret generic openai-api-key \
  --from-literal=api-key=<OPENAI_API_KEY> \
  -n llm --dry-run=client -o yaml | \
  oc label --local -f - inference.networking.k8s.io/bbr-managed=true -o yaml --dry-run=client | \
  oc apply -f -

# ExternalModel
cat <<EOF | oc apply -f -
apiVersion: maas.opendatahub.io/v1alpha1
kind: ExternalModel
metadata:
  name: ext-openai
  namespace: llm
spec:
  provider: openai
  targetModel: gpt-5.5
  endpoint: api.openai.com
  credentialRef:
    name: openai-api-key
EOF
```

## Step 4: MaaSModelRef + Subscription + AuthPolicy

```bash
# MaaSModelRef for each model
for model in ext-claude-sonnet ext-openai; do
  cat <<EOF | oc apply -f -
apiVersion: maas.opendatahub.io/v1alpha1
kind: MaaSModelRef
metadata:
  name: $model
  namespace: llm
spec:
  modelRef:
    kind: ExternalModel
    name: $model
EOF
done

# Add to MaaS Subscription
oc patch maassubscription <subscription-name> -n <maas-namespace> --type=json -p '[
  {"op": "add", "path": "/spec/modelRefs/-", "value": {"name": "ext-claude-sonnet", "namespace": "llm", "tokenRateLimits": [{"limit": 10000, "window": "1m"}]}},
  {"op": "add", "path": "/spec/modelRefs/-", "value": {"name": "ext-openai", "namespace": "llm", "tokenRateLimits": [{"limit": 10000, "window": "1m"}]}}
]'

# Add to MaaS AuthPolicy
oc patch maasauthpolicy <authpolicy-name> -n <maas-namespace> --type=json -p '[
  {"op": "add", "path": "/spec/modelRefs/-", "value": {"name": "ext-claude-sonnet", "namespace": "llm"}},
  {"op": "add", "path": "/spec/modelRefs/-", "value": {"name": "ext-openai", "namespace": "llm"}}
]'
```

## Step 5: HTTPRoute URL Rewrite

Each ExternalModel auto-creates an HTTPRoute. Add URL rewrite to strip the path prefix:

```bash
for route in ext-claude-sonnet ext-openai; do
  oc patch httproute $route -n llm --type=json -p '[
    {"op": "add", "path": "/spec/rules/0/filters/-", "value": {
      "type": "URLRewrite",
      "urlRewrite": {"path": {"type": "ReplacePrefixMatch", "replacePrefixMatch": "/"}}
    }}
  ]'
done
```

## Step 6: AuthConfig Patches

Claude Code sends API key in `x-api-key` header (not `Authorization: Bearer`). Patch ALL AuthConfigs for the models:

```bash
# Find AuthConfigs for our models
AUTH_CONFIGS=$(oc get authconfig -n <kuadrant-namespace> -o json | python3 -c "
import sys, json
data = json.load(sys.stdin)
for item in data['items']:
    spec = json.dumps(item.get('spec', {}))
    if 'ext-claude-sonnet' in spec or 'ext-openai' in spec:
        print(item['metadata']['name'])
")

for ac in $AUTH_CONFIGS; do
  # Add x-api-key auth method
  oc patch authconfig $ac -n <kuadrant-namespace> --type=json -p '[
    {"op": "add", "path": "/spec/authentication/api-keys-via-xapikey", "value": {
      "plain": {"selector": "request.headers.x-api-key"},
      "metrics": false, "priority": 0,
      "when": [{"operator": "matches", "selector": "request.headers.x-api-key", "value": "^sk-oai-.*"}]
    }}
  ]'
  
  # Fix key extraction for x-api-key
  oc patch authconfig $ac -n <kuadrant-namespace> --type=json -p "[
    {\"op\": \"replace\", \"path\": \"/spec/metadata/apiKeyValidation/when\", \"value\": [{\"operator\": \"neq\", \"selector\": \"auth.identity\", \"value\": \"\"}]},
    {\"op\": \"replace\", \"path\": \"/spec/metadata/apiKeyValidation/http/body/expression\", \"value\": \"{\\\"key\\\": (\\\"x-api-key\\\" in request.headers && request.headers[\\\"x-api-key\\\"].startsWith(\\\"sk-oai-\\\")) ? request.headers[\\\"x-api-key\\\"] : request.headers.authorization.replace(\\\"Bearer \\\", \\\"\\\")}\"}
  ]"
  
  # Add X-MaaS-Username header injection
  oc patch authconfig $ac -n <kuadrant-namespace> --type=json -p '[
    {"op": "add", "path": "/spec/response/success/headers/X-MaaS-Username", "value": {
      "metrics": false,
      "plain": {"expression": "(has(auth.metadata) && has(auth.metadata.apiKeyValidation)) ? auth.metadata.apiKeyValidation.username : \"\""},
      "priority": 0
    }}
  ]'
done
```

**IMPORTANT:** If the MaaS controller or Kuadrant operator reconciles, it will overwrite these patches. Scale them to 0 to prevent this:
```bash
oc scale deployment maas-controller -n <maas-namespace> --replicas=0
oc scale deployment kuadrant-operator-controller-manager -n <kuadrant-namespace> --replicas=0
```

## Step 7: Create MaaS API Keys

```bash
oc exec -n <maas-namespace> deployment/maas-api -- curl -sk \
  -X POST "https://localhost:8443/v1/api-keys" \
  -H "Content-Type: application/json" \
  -H "X-MaaS-Username: noy" \
  -H 'X-MaaS-Group: ["system:authenticated"]' \
  -d '{"name": "noy-claude"}'
```

## Step 8: Deploy Metering Service + PostgreSQL (Optional)

Only needed if NOT using native Limitador metrics.

```bash
# PostgreSQL
oc apply -f deploy/metering/postgresql-*.yaml

# Metering Service (build from metering-service/ directory)
oc new-build --binary --name=metering-service --strategy=docker -n <gateway-namespace>
oc start-build metering-service --from-dir=metering-service --follow -n <gateway-namespace>
# Deploy the metering-service Deployment + Service
```

## Step 9: Enable Tenant Telemetry (Native Limitador Metrics)

If the cluster has a clean MaaS install with Kuadrant 1.4.2+:

```bash
oc patch tenant default-tenant -n <maas-namespace> --type=merge \
  -p '{"spec":{"telemetry":{"enabled":true,"metrics":{"captureUser":true,"captureOrganization":true,"captureModelUsage":true,"captureGroup":true}}}}'
```

This creates a TelemetryPolicy that adds `user`, `subscription`, `model` labels to Limitador metrics (`authorized_hits`, `authorized_calls`).

## Step 10: Deploy Grafana

```bash
# Grafana Deployment + Service + Route
oc apply -f - <<EOF
apiVersion: apps/v1
kind: Deployment
metadata:
  name: grafana
  namespace: <gateway-namespace>
spec:
  replicas: 1
  selector:
    matchLabels:
      app: grafana
  template:
    metadata:
      labels:
        app: grafana
    spec:
      containers:
      - name: grafana
        image: docker.io/grafana/grafana:11.6.0
        ports:
        - containerPort: 3000
        env:
        - name: GF_SECURITY_ADMIN_PASSWORD
          value: admin
        - name: GF_AUTH_ANONYMOUS_ENABLED
          value: "true"
        - name: GF_AUTH_ANONYMOUS_ORG_ROLE
          value: Admin
---
apiVersion: v1
kind: Service
metadata:
  name: grafana
  namespace: <gateway-namespace>
spec:
  selector:
    app: grafana
  ports:
  - port: 3000
---
apiVersion: route.openshift.io/v1
kind: Route
metadata:
  name: grafana
  namespace: <gateway-namespace>
spec:
  to:
    kind: Service
    name: grafana
  port:
    targetPort: 3000
  tls:
    termination: edge
EOF
```

Then configure datasource (PostgreSQL or Prometheus) and create dashboard via API.

## Step 11: Test

### Claude Code
```bash
export ANTHROPIC_BASE_URL=https://<gateway-host>/llm/ext-claude-sonnet
export ANTHROPIC_API_KEY=<MAAS_API_KEY>
export NODE_TLS_REJECT_UNAUTHORIZED=0
unset CLAUDE_CODE_USE_VERTEX
unset ANTHROPIC_VERTEX_PROJECT_ID
claude
```

### Codex
Add to `~/.codex/config.toml`:
```toml
model = "gpt-5.5"
model_provider = "maas"

[model_providers.maas]
name = "MaaS Gateway"
base_url = "https://<gateway-host>/llm/ext-openai/v1"
wire_api = "responses"
env_key = "MAAS_API_KEY"
```

```bash
export MAAS_API_KEY=<MAAS_API_KEY>
export NODE_TLS_REJECT_UNAUTHORIZED=0
codex
```

## Key Code Changes in the Branch

| File | Change |
|------|--------|
| `pkg/plugins/model-provider-resolver/plugin.go` | Accept `/v1/messages` + `/v1/responses`, model override with parameter stripping |
| `pkg/plugins/external-metering/plugin.go` | CloudEvents metering with SSE usage extraction |
| `pkg/plugins/plugins.go` | Register external-metering |
| `go.mod` | Replace directive to framework fork with SSE streaming fix |
| `metering-service/` | Go HTTP server for PostgreSQL-backed usage tracking |
| `scripts/demo-swap-*.sh` | Demo scripts for model swapping |

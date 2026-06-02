# Deploy and Test the Payload Processor (IPP)

The payload processor is an Envoy ext_proc service that handles API translation, credential injection, and routing for external AI providers. It sits in the data path between the gateway and the upstream provider.

This guide picks up where the [MaaS Internal Development guide](https://docs.google.com/document/d/1atYZV5MfrzS0wWQnhBwnr62m8qdcrRqzzITkFwpt-QI) leaves off.

## Prerequisites

1. MaaS API + Controller deployed and running.
2. Gateway programmed and accessible.
3. Switch to your local payload processor repo.

```
cd <your-path>/ai-gateway-payload-processing
```

## Prepare the payload processor image

There are two ways to do it:

1. Raise a PR and let the CI build it for you. Your image would be `quay.io/opendatahub/ai-gateway-payload-processing:odh-pr-<sha>`.

2. Build locally as shown below.

```
IMAGE_TAG=$(git branch --show-current)
docker build --platform linux/amd64 \
  -t quay.io/<your-quay-user>/odh-ai-gateway-payload-processing:$IMAGE_TAG .
docker push quay.io/<your-quay-user>/odh-ai-gateway-payload-processing:$IMAGE_TAG
```

## Deploy the payload processor

```
export GATEWAY_NAME="maas-default-gateway"
export GATEWAY_NAMESPACE="openshift-ingress"

helm install payload-processing ./deploy/payload-processing \
  --namespace ${GATEWAY_NAMESPACE} \
  --dependency-update \
  -f ./test/e2e/scripts/e2e-values.yaml \
  --set upstreamBbr.inferenceGateway.name=${GATEWAY_NAME} \
  --set upstreamBbr.provider.name=istio \
  --set upstreamBbr.provider.istio.envoyFilter.operation=INSERT_FIRST
```

If using your own image:

```
helm install payload-processing ./deploy/payload-processing \
  --namespace ${GATEWAY_NAMESPACE} \
  --dependency-update \
  -f ./test/e2e/scripts/e2e-values.yaml \
  --set upstreamBbr.inferenceGateway.name=${GATEWAY_NAME} \
  --set upstreamBbr.provider.name=istio \
  --set upstreamBbr.provider.istio.envoyFilter.operation=INSERT_FIRST \
  --set upstreamBbr.image.repository=quay.io/<your-quay-user>/odh-ai-gateway-payload-processing \
  --set upstreamBbr.image.tag=$IMAGE_TAG
```

Disable sidecar injection (ext_proc uses self-signed TLS, sidecar breaks it):

```
kubectl patch deployment payload-processing -n ${GATEWAY_NAMESPACE} \
  --type=merge \
  -p='{"spec":{"template":{"metadata":{"annotations":{"sidecar.istio.io/inject":"false"}}}}}'
```

Verify:

```
kubectl rollout status deployment/payload-processing -n ${GATEWAY_NAMESPACE} --timeout=120s
```

## What the E2E tests validate

The E2E tests deploy ExternalModel CRs pointing to an LLM simulator and validate the full request/response flow through the plugin chain for all supported providers:

| Provider | Plugin chain path |
|----------|------------------|
| OpenAI | passthrough (no translation) |
| Anthropic | OpenAI → Anthropic Messages API → OpenAI |
| Azure OpenAI | passthrough + `api-key` header |
| Bedrock (OpenAI-compatible) | passthrough via Mantle endpoint |
| Vertex AI (OpenAI-compatible) | path rewrite to `/v1beta1/projects/.../endpoints/openapi/chat/completions` |

For each provider, the tests verify:
- **HTTP 200** — request reaches the provider and returns successfully
- **OpenAI format response** — response body contains `choices`, `model` fields regardless of provider
- **Key validation** — wrong API key returns 401 (when simulator has `--validate-keys`)

## Run E2E tests locally (Kind)

```
./test/e2e/scripts/setup-kind.sh
make test-e2e
```

## Run E2E tests via container (same as Jenkins nightly)

```
docker build --platform linux/amd64 -t payload-processing-e2e:local -f Dockerfile.e2e .

podman run --rm \
  -v /path/to/kubeconfig:/kubeconfig:ro \
  -e KUBECONFIG=/kubeconfig \
  -e E2E_GATEWAY_NAME=${GATEWAY_NAME} \
  -e E2E_GATEWAY_NAMESPACE=${GATEWAY_NAMESPACE} \
  -e E2E_GATEWAY_SVC_NAME=${GATEWAY_NAME}-openshift-default \
  -e E2E_LABEL_FILTER=tier1 \
  -v $(pwd)/results:/results \
  payload-processing-e2e:local
```

Results will be in `./results/results_e2e_xunit.xml`.

## Next Step

See [provider-specific guides](./providers/) for creating ExternalModel resources for each provider.

## Uninstallation

```
helm uninstall payload-processing -n ${GATEWAY_NAMESPACE}
```

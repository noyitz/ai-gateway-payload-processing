# Deploy

RBAC manifests required by the AI Gateway Payload Processing BBR plugins.

These are applied **in addition to** the upstream
[body-based-routing Helm chart](https://github.com/kubernetes-sigs/gateway-api-inference-extension/tree/main/config/charts/body-based-routing)
which provides the BBR Deployment and Service.

## Prerequisites

- A Kubernetes cluster with `kubectl` configured
- `envsubst` (part of `gettext`, usually pre-installed on Linux/macOS)

## Apply RBAC

1. Set the `NAMESPACE` environment variable to the namespace where the BBR pod will run:
    ```
    export NAMESPACE=<your-namespace>
    ```

2. Deploy the RBAC resources:
    ```
    envsubst < deploy/rbac.yaml | kubectl apply -f -
    ```

## Cleanup

```
envsubst < deploy/rbac.yaml | kubectl delete -f -
```

/*
Copyright 2026 The opendatahub.io Authors.

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
*/

package aws_sigv4_signer

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"net/http"
	"time"

	"github.com/aws/aws-sdk-go-v2/aws"
	v4 "github.com/aws/aws-sdk-go-v2/aws/signer/v4"
	corev1 "k8s.io/api/core/v1"
	"k8s.io/apimachinery/pkg/types"
	"sigs.k8s.io/controller-runtime/pkg/client"
	"sigs.k8s.io/controller-runtime/pkg/log"
	"sigs.k8s.io/gateway-api-inference-extension/pkg/bbr/framework"
	"sigs.k8s.io/gateway-api-inference-extension/pkg/epp/framework/interface/plugin"

	"github.com/opendatahub-io/ai-gateway-payload-processing/pkg/plugins/common/provider"
	"github.com/opendatahub-io/ai-gateway-payload-processing/pkg/plugins/common/state"
)

const (
	AWSSigV4SignerPluginType = "aws-sigv4-signer"
	bedrockService          = "bedrock"
	defaultRegion           = "us-east-1"
)

// compile-time type validation
var _ framework.RequestProcessor = &AWSSigV4SignerPlugin{}

// AWSSigV4SignerFactory creates a new AWSSigV4SignerPlugin.
func AWSSigV4SignerFactory(name string, _ json.RawMessage, handle framework.Handle) (framework.BBRPlugin, error) {
	return &AWSSigV4SignerPlugin{
		typedName: plugin.TypedName{Type: AWSSigV4SignerPluginType, Name: name},
		reader:    handle.ClientReader(),
		signer:    v4.NewSigner(),
	}, nil
}

// AWSSigV4SignerPlugin signs requests to AWS services using SigV4.
// It reads AWS credentials from a K8s Secret referenced in CycleState
// and computes a per-request signature.
type AWSSigV4SignerPlugin struct {
	typedName plugin.TypedName
	reader    client.Reader
	signer    *v4.Signer
}

func (p *AWSSigV4SignerPlugin) TypedName() plugin.TypedName {
	return p.typedName
}

// ProcessRequest signs the request with AWS SigV4 if the provider is bedrock-openai.
// For non-AWS providers, it's a no-op.
func (p *AWSSigV4SignerPlugin) ProcessRequest(ctx context.Context, cycleState *framework.CycleState, request *framework.InferenceRequest) error {
	if request == nil || request.Headers == nil {
		return fmt.Errorf("request or headers is nil")
	}

	providerName, _ := framework.ReadCycleStateKey[string](cycleState, state.ProviderKey)
	if providerName != provider.BedrockOpenAI {
		return nil // not a Bedrock provider, skip
	}

	logger := log.FromContext(ctx)

	// Read credential ref from CycleState
	credsName, err := framework.ReadCycleStateKey[string](cycleState, state.CredsRefName)
	if err != nil || credsName == "" {
		return fmt.Errorf("bedrock model missing credentialRef")
	}
	credsNamespace, err := framework.ReadCycleStateKey[string](cycleState, state.CredsRefNamespace)
	if err != nil || credsNamespace == "" {
		return fmt.Errorf("bedrock model missing credentialRef namespace")
	}

	// Fetch the Secret containing AWS credentials
	secret := &corev1.Secret{}
	if err := p.reader.Get(ctx, types.NamespacedName{Name: credsName, Namespace: credsNamespace}, secret); err != nil {
		return fmt.Errorf("failed to get AWS credentials secret %s/%s: %w", credsNamespace, credsName, err)
	}

	accessKey := string(secret.Data["aws-access-key-id"])
	secretKey := string(secret.Data["aws-secret-access-key"])
	sessionToken := string(secret.Data["aws-session-token"])

	if accessKey == "" || secretKey == "" {
		return fmt.Errorf("AWS credentials secret %s/%s missing aws-access-key-id or aws-secret-access-key", credsNamespace, credsName)
	}

	// Determine region from the host header
	host := request.Headers["host"]
	if host == "" {
		host = request.Headers[":authority"]
	}
	region := regionFromEndpoint(host)

	// Build the HTTP request for signing
	path := request.Headers[":path"]
	if path == "" {
		path = "/v1/chat/completions"
	}

	bodyBytes, err := json.Marshal(request.Body)
	if err != nil {
		return fmt.Errorf("failed to marshal request body for signing: %w", err)
	}

	url := fmt.Sprintf("https://%s%s", host, path)
	httpReq, err := http.NewRequestWithContext(ctx, "POST", url, bytes.NewReader(bodyBytes))
	if err != nil {
		return fmt.Errorf("failed to create HTTP request for signing: %w", err)
	}
	httpReq.Header.Set("Content-Type", "application/json")
	httpReq.Header.Set("Host", host)

	// Compute payload hash
	payloadHash := sha256Hash(bodyBytes)

	// Create AWS credentials
	creds := aws.Credentials{
		AccessKeyID:    accessKey,
		SecretAccessKey: secretKey,
		SessionToken:   sessionToken,
		Source:         "MaaSSecret",
	}

	// Sign the request
	err = p.signer.SignHTTP(ctx, creds, httpReq, payloadHash, bedrockService, region, time.Now())
	if err != nil {
		return fmt.Errorf("failed to sign request with SigV4: %w", err)
	}

	// Extract signed headers and inject into the inference request
	request.SetHeader("Authorization", httpReq.Header.Get("Authorization"))
	request.SetHeader("X-Amz-Date", httpReq.Header.Get("X-Amz-Date"))
	if sessionToken != "" {
		request.SetHeader("X-Amz-Security-Token", httpReq.Header.Get("X-Amz-Security-Token"))
	}
	request.SetHeader("X-Amz-Content-Sha256", payloadHash)

	// Remove the original authorization header (if present from the client)
	request.RemoveHeader("authorization")

	logger.Info("AWS SigV4 signature applied", "region", region, "service", bedrockService)
	return nil
}

// sha256Hash computes the hex-encoded SHA256 hash of data.
func sha256Hash(data []byte) string {
	h := sha256.Sum256(data)
	return hex.EncodeToString(h[:])
}

// regionFromEndpoint extracts the AWS region from a Bedrock endpoint FQDN.
// e.g., "bedrock-runtime.us-west-2.amazonaws.com" → "us-west-2"
func regionFromEndpoint(endpoint string) string {
	// Expected format: bedrock-runtime.{region}.amazonaws.com
	parts := splitDots(endpoint)
	if len(parts) >= 3 && parts[0] == "bedrock-runtime" {
		return parts[1]
	}
	return defaultRegion
}

func splitDots(s string) []string {
	result := []string{}
	start := 0
	for i := 0; i < len(s); i++ {
		if s[i] == '.' {
			result = append(result, s[start:i])
			start = i + 1
		}
	}
	result = append(result, s[start:])
	return result
}

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

package awsbedrock

import (
	"fmt"
	"strings"
)

const (
	bedrockOpenAIPath = "/openai/v1/chat/completions"
	defaultRegion     = "us-east-1"
)

// BedrockTranslator translates OpenAI Chat Completions requests for AWS Bedrock's
// OpenAI-compatible endpoint. The request body is not mutated since Bedrock accepts
// the same OpenAI format. Only the path and host headers are rewritten.
type BedrockTranslator struct{}

// NewBedrockTranslator creates a new AWS Bedrock translator.
func NewBedrockTranslator() *BedrockTranslator {
	return &BedrockTranslator{}
}

// TranslateRequest rewrites the path and host headers to target Bedrock's
// OpenAI-compatible endpoint. The region is derived from the existing host
// header (set by the ExternalModel endpoint field) or defaults to us-east-1.
func (t *BedrockTranslator) TranslateRequest(body map[string]any) (
	translatedBody map[string]any,
	headersToMutate map[string]string,
	headersToRemove []string,
	err error,
) {
	model, ok := body["model"].(string)
	if !ok || model == "" {
		return nil, nil, nil, fmt.Errorf("model field is required")
	}

	if _, hasMessages := body["messages"]; !hasMessages {
		return nil, nil, nil, fmt.Errorf("only Chat Completions API is supported — 'messages' field required")
	}

	headersToMutate = map[string]string{
		":path":        bedrockOpenAIPath,
		"content-type": "application/json",
	}

	return nil, headersToMutate, nil, nil
}

// TranslateResponse is a no-op — Bedrock's OpenAI-compatible endpoint returns
// responses in standard OpenAI format.
func (t *BedrockTranslator) TranslateResponse(body map[string]any, model string) (
	translatedBody map[string]any,
	err error,
) {
	return nil, nil
}

// RegionFromEndpoint extracts the AWS region from a Bedrock endpoint FQDN.
// e.g., "bedrock-runtime.us-west-2.amazonaws.com" → "us-west-2"
func RegionFromEndpoint(endpoint string) string {
	// Expected format: bedrock-runtime.{region}.amazonaws.com
	parts := strings.Split(endpoint, ".")
	if len(parts) >= 3 && parts[0] == "bedrock-runtime" {
		return parts[1]
	}
	return defaultRegion
}
